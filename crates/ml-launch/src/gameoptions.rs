//! Reversible game-side graphics profiles.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ml_core::{Error, GamePerformanceProfile, Result};

const OPTIONS_FILE: &str = "options.txt";
const BACKUP_FILE: &str = "options.txt.minlaucher-original";

/// Apply or restore the graphics settings for one isolated game directory.
///
/// The original file is saved once before the FPS profile changes it. The
/// launcher never touches a Minecraft directory outside `game_dir`.
pub fn apply_profile(game_dir: &Path, profile: GamePerformanceProfile) -> Result<()> {
    let options_path = game_dir.join(OPTIONS_FILE);
    let backup_path = game_dir.join(BACKUP_FILE);

    match profile {
        GamePerformanceProfile::Original => restore(&options_path, &backup_path),
        GamePerformanceProfile::MaxFps => apply_max_fps(&options_path, &backup_path),
    }
}

fn apply_max_fps(options_path: &Path, backup_path: &Path) -> Result<()> {
    if let Some(parent) = options_path.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    }

    let original = read_options(options_path)?;
    if options_path.is_file() && !backup_path.exists() {
        fs::copy(options_path, backup_path).map_err(|error| Error::io(backup_path, error))?;
    }

    let mut options = parse_options(&original);
    for (key, value) in [
        ("renderDistance", "4"),
        ("simulationDistance", "4"),
        ("graphicsMode", "0"),
        ("ao", "false"),
        ("renderClouds", "\"false\""),
        ("particles", "0"),
        ("vsync", "false"),
        ("maxFps", "500"),
        ("mipmapLevels", "0"),
        ("entityShadows", "false"),
        ("biomeBlendRadius", "0"),
        ("entityDistanceScaling", "0.5"),
    ] {
        options.insert(key.to_string(), value.to_string());
    }

    let contents = options
        .into_iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(options_path, format!("{contents}\n")).map_err(|error| Error::io(options_path, error))
}

fn restore(options_path: &Path, backup_path: &Path) -> Result<()> {
    if !backup_path.is_file() {
        return Ok(());
    }
    fs::copy(backup_path, options_path).map_err(|error| Error::io(options_path, error))?;
    fs::remove_file(backup_path).map_err(|error| Error::io(backup_path, error))
}

fn read_options(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(Error::io(path, error)),
    }
}

fn parse_options(contents: &str) -> BTreeMap<String, String> {
    contents
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn max_fps_reduces_expensive_options_and_keeps_unrelated_options() {
        let directory = tempdir().unwrap();
        let options = directory.path().join(OPTIONS_FILE);
        fs::write(&options, "language:ru_ru\nrenderDistance:32\n").unwrap();

        apply_profile(directory.path(), GamePerformanceProfile::MaxFps).unwrap();

        let written = fs::read_to_string(options).unwrap();
        for expected in [
            "language:ru_ru",
            "renderDistance:4",
            "simulationDistance:4",
            "graphicsMode:0",
            "ao:false",
            "vsync:false",
            "maxFps:500",
            "entityDistanceScaling:0.5",
        ] {
            assert!(written.lines().any(|line| line == expected), "{expected}");
        }
        assert!(directory.path().join(BACKUP_FILE).is_file());
    }

    #[test]
    fn original_profile_restores_the_exact_previous_file() {
        let directory = tempdir().unwrap();
        let options = directory.path().join(OPTIONS_FILE);
        let original = "language:ru_ru\nrenderDistance:32\nmaxFps:144\n";
        fs::write(&options, original).unwrap();

        apply_profile(directory.path(), GamePerformanceProfile::MaxFps).unwrap();
        apply_profile(directory.path(), GamePerformanceProfile::Original).unwrap();

        assert_eq!(fs::read_to_string(options).unwrap(), original);
        assert!(!directory.path().join(BACKUP_FILE).exists());
    }

    #[test]
    fn max_fps_does_not_replace_the_first_backup_on_relaunch() {
        let directory = tempdir().unwrap();
        let options = directory.path().join(OPTIONS_FILE);
        fs::write(&options, "renderDistance:24\n").unwrap();

        apply_profile(directory.path(), GamePerformanceProfile::MaxFps).unwrap();
        fs::write(&options, "renderDistance:12\n").unwrap();
        apply_profile(directory.path(), GamePerformanceProfile::MaxFps).unwrap();
        apply_profile(directory.path(), GamePerformanceProfile::Original).unwrap();

        assert_eq!(fs::read_to_string(options).unwrap(), "renderDistance:24\n");
    }
}
