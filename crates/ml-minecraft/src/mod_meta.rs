//! The manifest of a mod, and the versions of the game it asks for.
//!
//! A mod says in its manifest which versions of the game it was built
//! for: `depends.minecraft` holds the requirement, and the loader
//! refuses a mod whose requirement the running game does not meet.
//!
//! The launcher has to know that **before** the game starts. A mod that
//! is put in place and then refused by the loader would still be counted
//! as a run of "the game with the mod", and every number taken from that
//! run would claim an optimisation that was not in the game.

use std::io::Read;
use std::path::Path;

use ml_core::version::comparable;

/// The file the manifest of a mod is in.
pub const MOD_MANIFEST: &str = "fabric.mod.json";

/// What the manifest of a mod says about the versions of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    /// The mod asks for no particular version: it goes with any of them.
    Any,
    /// The mod asks for these versions of the game.
    These(String),
    /// The jar is not a mod the loader would load.
    NotAMod,
}

/// Whether the jar of a mod may go into the game of a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModFit {
    /// The mod goes into the game: the loader will load it.
    Fits,
    /// The loader will refuse the mod in this version of the game.
    Refused,
    /// There is no jar of the mod on the disk.
    Missing,
}

/// What the manifest of a mod says about the versions of the game.
///
/// `NotAMod` when the jar holds no manifest at all: a jar the loader
/// would not load is not a mod, and a mod is not put into the game.
#[must_use]
pub fn requirement_of(jar: &Path) -> Requirement {
    let Some(document) = read_manifest(jar) else {
        return Requirement::NotAMod;
    };

    match requirement_in(&document) {
        Some(predicate) => Requirement::These(predicate),
        None => Requirement::Any,
    }
}

/// Whether the mod in this jar goes with this version of the game.
///
/// A jar the loader would not load at all is not a mod, and a mod is not
/// put into the game.
#[must_use]
pub fn supports(jar: &Path, game_version: &str) -> bool {
    match requirement_of(jar) {
        Requirement::Any => true,
        Requirement::These(predicate) => satisfies(&predicate, game_version),
        Requirement::NotAMod => false,
    }
}

/// Whether the jar of a mod may go into the game of this version.
///
/// The three answers are told apart because they are told apart to the
/// player: a mod that is not there is a build that was not made, and a
/// mod the loader refuses is a mod that was built for another version.
#[must_use]
pub fn fit_of_mod(jar: &Path, game_version: &str) -> ModFit {
    if !jar.is_file() {
        return ModFit::Missing;
    }

    if supports(jar, game_version) {
        ModFit::Fits
    } else {
        ModFit::Refused
    }
}

/// Whether a Forge mod with this id is present in a mods directory.
///
/// This is used to report game measurements under the correct scenario:
/// Forge mods are installed in `mods/` and do not appear in the launcher's
/// Fabric mod list.
#[must_use]
pub fn directory_contains_forge_mod(directory: &Path, mod_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };

    entries.filter_map(std::result::Result::ok).any(|entry| {
        let path = entry.path();
        path.extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("jar"))
            && forge_jar_contains_mod(&path, mod_id)
    })
}

fn forge_jar_contains_mod(jar: &Path, expected_id: &str) -> bool {
    let Ok(file) = std::fs::File::open(jar) else {
        return false;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return false;
    };
    let Ok(mut entry) = archive.by_name("META-INF/mods.toml") else {
        return false;
    };
    let mut text = String::new();
    if entry.read_to_string(&mut text).is_err() {
        return false;
    }

    text.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        if key.trim() != "modId" {
            return false;
        }
        let value = value.split('#').next().unwrap_or_default().trim();
        value.trim_matches(['"', '\'']) == expected_id
    })
}

/// Whether a version of the game satisfies a requirement of a mod.
///
/// The requirement is written the way the loader reads it: `1.21.4` means
/// exactly that version, `~1.21.4` means the whole `1.21` line, `^1.21.4`
/// means everything below `2.0.0`, `>=1.21` means everything from `1.21`
/// on, `1.21.x` means any patch of `1.21`, and `*` means anything. Several
/// requirements may be written at once, and then all of them have to hold.
///
/// A requirement the launcher does not understand is treated as "not
/// satisfied": the price of that is a mod that stays out of a version it
/// would have worked on, and the price of the opposite is a measurement
/// that calls a vanilla game a modded one.
#[must_use]
pub fn satisfies(requirement: &str, game_version: &str) -> bool {
    let requirement = requirement.trim();
    if requirement.is_empty() {
        return false;
    }

    // The form of Mojang: "[1.0,2.0)".
    if requirement.starts_with(['[', '(']) {
        return ml_core::rules::VersionRange::parse(requirement)
            .is_some_and(|range| range.matches(game_version));
    }

    let current = comparable(game_version);
    requirement
        .split([' ', '\t', '\n', '\r', ','])
        .filter(|part| !part.is_empty())
        .all(|part| part_matches(part, current))
}

/// Whether one requirement holds for a version.
fn part_matches(part: &str, current: (u64, u64, u64)) -> bool {
    if part == "*" {
        return true;
    }

    if let Some(rest) = part.strip_prefix(">=") {
        return bound(rest).is_some_and(|limit| current >= limit);
    }
    if let Some(rest) = part.strip_prefix("<=") {
        return bound(rest).is_some_and(|limit| current <= limit);
    }
    if let Some(rest) = part.strip_prefix('>') {
        return bound(rest).is_some_and(|limit| current > limit);
    }
    if let Some(rest) = part.strip_prefix('<') {
        return bound(rest).is_some_and(|limit| current < limit);
    }
    if let Some(rest) = part.strip_prefix("!=") {
        return bound(rest).is_some_and(|limit| current != limit);
    }
    if let Some(rest) = part.strip_prefix('~') {
        return in_span(current, bound(rest), tilde_upper(rest));
    }
    if let Some(rest) = part.strip_prefix('^') {
        return in_span(current, bound(rest), caret_upper(rest));
    }

    exact_matches(part, current)
}

/// Whether a version lies in the span of a requirement.
fn in_span(
    current: (u64, u64, u64),
    lower: Option<(u64, u64, u64)>,
    upper: Option<(u64, u64, u64)>,
) -> bool {
    lower
        .zip(upper)
        .is_some_and(|(low, high)| current >= low && current < high)
}

/// The version a comparison is made against, when it can be read.
///
/// A wildcard is not a bound: `>=1.21.x` says nothing about anything, so
/// it is a requirement the launcher does not understand.
fn bound(raw: &str) -> Option<(u64, u64, u64)> {
    let pieces = pattern_parts(raw)?;
    if pieces.iter().any(Option::is_none) {
        return None;
    }

    Some((
        pieces.first().copied().flatten().unwrap_or(0),
        pieces.get(1).copied().flatten().unwrap_or(0),
        pieces.get(2).copied().flatten().unwrap_or(0),
    ))
}

/// The version above which a tilde requirement ends.
///
/// `~1.21.4` ends at `1.22.0`, `~1.21` ends at `1.22.0` as well, and `~1`
/// ends at `2.0.0`.
fn tilde_upper(raw: &str) -> Option<(u64, u64, u64)> {
    let pieces = pattern_parts(raw)?;
    let major = match pieces.first() {
        Some(Some(value)) => *value,
        _ => return None,
    };
    let minor = pieces.get(1).copied().flatten();

    Some(match minor {
        Some(minor) => (major, minor + 1, 0),
        None => (major + 1, 0, 0),
    })
}

/// The version above which a caret requirement ends.
///
/// `^1.21.4` ends at `2.0.0`, `^0.2.3` ends at `0.3.0`, and `^0.0.3` ends
/// at `0.0.4`.
fn caret_upper(raw: &str) -> Option<(u64, u64, u64)> {
    let pieces = pattern_parts(raw)?;
    let major = match pieces.first() {
        Some(Some(value)) => *value,
        _ => return None,
    };
    let minor = pieces.get(1).copied().flatten();
    let patch = pieces.get(2).copied().flatten();

    Some(if major > 0 {
        (major + 1, 0, 0)
    } else if let Some(minor) = minor {
        if minor > 0 {
            (0, minor + 1, 0)
        } else {
            (0, 0, patch.unwrap_or(0) + 1)
        }
    } else {
        (1, 0, 0)
    })
}

/// Whether a version pattern with wildcards matches a version.
///
/// A wildcard takes its part and every part after it, and the version may
/// not be longer than the pattern: `1.21` is not `1.21.4`.
fn exact_matches(pattern: &str, current: (u64, u64, u64)) -> bool {
    let Some(pieces) = pattern_parts(pattern) else {
        return false;
    };

    for (index, expected) in pieces.iter().enumerate() {
        match expected {
            None => return true,
            Some(number) => {
                if part_of(current, index) != *number {
                    return false;
                }
            }
        }
    }

    pieces.len() >= 3 || current.2 == 0
}

/// The parts of a version pattern, where `None` is a wildcard.
fn pattern_parts(raw: &str) -> Option<Vec<Option<u64>>> {
    let core = raw.split(['-', '_', '+']).next().unwrap_or(raw);
    if core.is_empty() {
        return None;
    }

    let text = core.split('.').collect::<Vec<&str>>();
    if text.len() > 3 {
        // A version has three parts. More than that is not a version, and
        // a requirement the launcher does not understand does not match.
        return None;
    }

    let mut pieces = Vec::with_capacity(text.len());
    for segment in text {
        if segment.is_empty() {
            return None;
        }
        if matches!(segment, "x" | "X" | "*") {
            pieces.push(None);
        } else {
            pieces.push(Some(segment.parse::<u64>().ok()?));
        }
    }
    Some(pieces)
}

/// The part of a version with the given index.
const fn part_of(version: (u64, u64, u64), index: usize) -> u64 {
    match index {
        0 => version.0,
        1 => version.1,
        _ => version.2,
    }
}

/// The requirement of the game a manifest holds.
fn requirement_in(document: &serde_json::Value) -> Option<String> {
    match document.get("depends")? {
        // The form of Fabric: an object of names and requirements.
        serde_json::Value::Object(map) => map.get("minecraft")?.as_str().map(str::to_string),
        // The older form: a list of objects.
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|item| item.get("minecraft").and_then(|value| value.as_str()))
            .map(str::to_string),
        _ => None,
    }
}

/// The manifest of a mod, read out of its jar.
fn read_manifest(jar: &Path) -> Option<serde_json::Value> {
    let file = std::fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(MOD_MANIFEST).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// A jar with a manifest that says what the mod needs.
    fn jar_with(manifest: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mod.jar");

        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, '_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file(MOD_MANIFEST, options).unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.finish().unwrap();

        (dir, path)
    }

    fn forge_jar_with(manifest: &str, directory: &Path) -> PathBuf {
        let path = directory.join("forge-mod.jar");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, '_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("META-INF/mods.toml", options).unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.finish().unwrap();
        path
    }

    #[test]
    fn a_forge_mod_is_found_by_its_mod_id() {
        let directory = tempfile::tempdir().unwrap();
        forge_jar_with("[[mods]]\nmodId = \"minopt\"\n", directory.path());

        assert!(directory_contains_forge_mod(directory.path(), "minopt"));
        assert!(!directory_contains_forge_mod(directory.path(), "other"));
    }

    #[test]
    fn an_exact_requirement_covers_one_version() {
        assert!(satisfies("1.21.4", "1.21.4"));
        assert!(!satisfies("1.21.4", "1.21.5"));
        assert!(!satisfies("1.21", "1.21.4"));
        assert!(satisfies("1.21", "1.21.0"));
    }

    #[test]
    fn a_tilde_requirement_covers_the_whole_line() {
        assert!(satisfies("~1.21.4", "1.21.4"));
        assert!(satisfies("~1.21.4", "1.21.9"));
        assert!(!satisfies("~1.21.4", "1.22.0"));
        assert!(!satisfies("~1.21.4", "26.3"));
    }

    #[test]
    fn a_caret_requirement_covers_the_whole_major() {
        assert!(satisfies("^1.21.4", "1.21.9"));
        assert!(satisfies("^1.21.4", "1.99.0"));
        assert!(!satisfies("^1.21.4", "2.0.0"));
        assert!(!satisfies("^1.21.4", "1.21.3"));
    }

    #[test]
    fn a_comparison_covers_a_side_of_the_line() {
        assert!(satisfies(">=1.21", "26.3"));
        assert!(!satisfies(">=1.21", "1.20.6"));
        assert!(satisfies("<1.21", "1.20.6"));
        assert!(!satisfies("<1.21", "1.21"));
        assert!(satisfies("!=1.21.4", "1.21.5"));
    }

    #[test]
    fn a_wildcard_covers_the_rest_of_the_line() {
        assert!(satisfies("*", "anything"));
        assert!(satisfies("1.21.x", "1.21.7"));
        assert!(!satisfies("1.21.x", "1.22.0"));
    }

    #[test]
    fn several_requirements_all_have_to_hold() {
        assert!(satisfies(">=1.20 <1.22", "1.21.4"));
        assert!(!satisfies(">=1.20 <1.22", "1.22.1"));
    }

    #[test]
    fn a_requirement_that_is_not_understood_does_not_match() {
        assert!(!satisfies("", "1.21.4"));
        assert!(!satisfies("not-a-version", "1.21.4"));
        assert!(!satisfies(">=1.21.x", "1.21.4"));
    }

    #[test]
    fn the_manifest_of_a_mod_says_which_game_it_wants() {
        let (_dir, jar) = jar_with(
            r#"{"id":"example","depends":{"minecraft":"~1.21.4","fabricloader":">=0.16.0"}}"#,
        );

        assert_eq!(
            requirement_of(&jar),
            Requirement::These("~1.21.4".to_string())
        );
        assert!(supports(&jar, "1.21.4"));
        assert!(!supports(&jar, "26.3"));
    }

    #[test]
    fn a_mod_without_a_requirement_goes_with_any_version() {
        let (_dir, jar) = jar_with(r#"{"id":"example","name":"Example"}"#);

        assert_eq!(requirement_of(&jar), Requirement::Any);
        assert!(supports(&jar, "26.3"));
    }

    #[test]
    fn a_jar_without_a_manifest_is_not_a_mod() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("not-a-mod.jar");
        std::fs::write(&jar, b"not a zip file").unwrap();

        assert_eq!(requirement_of(&jar), Requirement::NotAMod);
        assert!(!supports(&jar, "1.21.4"));
    }

    #[test]
    fn a_mod_of_another_version_is_refused_and_a_mod_that_is_not_there_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (_other, jar) = jar_with(r#"{"depends":{"minecraft":"~1.21.4"}}"#);

        assert_eq!(fit_of_mod(&jar, "1.21.4"), ModFit::Fits);
        assert_eq!(fit_of_mod(&jar, "26.3"), ModFit::Refused);
        assert_eq!(
            fit_of_mod(&dir.path().join("missing.jar"), "1.21.4"),
            ModFit::Missing
        );
    }
}
