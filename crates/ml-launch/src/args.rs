//! Placeholder substitution and command line construction.
//!
//! A version JSON describes the command line with placeholders such as
//! `${auth_player_name}` or `${classpath}`. The launcher replaces them
//! with the concrete values of the session it is about to start.
//!
//! The list of placeholders is fixed by Mojang and has not changed since
//! 1.13; new ones appear occasionally (`${clientid}`, `${auth_xuid}`).
//! An unknown placeholder is **left untouched** rather than replaced with
//! an empty string: an empty argument changes the meaning of the command
//! line, whereas an unresolved `${...}` makes the game complain loudly,
//! which is much easier to diagnose.
//!
//! # Classpath
//!
//! The classpath is the client jar followed by every library that the
//! rules allowed, in the order the version JSON lists them. The natives
//! jars are part of it: LWJGL finds its native inside the jar on the
//! classpath and unpacks it itself, so the launcher must not hide it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ml_core::{Error, Result};

/// The values a launch session substitutes into the version's arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// The player's name.
    pub player_name: String,
    /// The player's UUID, with hyphens.
    pub uuid: String,
    /// The player's UUID without hyphens.
    pub uuid_prefix: String,
    /// The access token handed to the game.
    pub access_token: String,
    /// The XUID of the signed-in Microsoft account.
    pub xuid: String,
    /// The client id the launcher identifies itself with.
    pub client_id: String,
    /// The version being launched.
    pub version_name: String,
    /// The kind of the version, e.g. `release`.
    pub version_type: String,
    /// The directory the game runs in.
    pub game_directory: PathBuf,
    /// The root of the content-addressed asset store.
    pub assets_root: PathBuf,
    /// The id of the asset index.
    pub assets_index_name: String,
    /// The directory the client jar lives in.
    pub library_directory: PathBuf,
    /// The directory the native libraries are unpacked into.
    pub natives_directory: PathBuf,
    /// The name the launcher identifies itself with.
    pub launcher_name: String,
    /// The version of the launcher.
    pub launcher_version: String,
    /// The width of the game window, when a custom resolution was asked for.
    pub resolution_width: Option<u32>,
    /// The height of the game window, when a custom resolution was asked for.
    pub resolution_height: Option<u32>,
    /// The path a quick-play single-player save is loaded from.
    pub quick_play_path: Option<PathBuf>,
    /// The identifier of a quick-play single-player world.
    pub quick_play_singleplayer: Option<String>,
    /// The address of a quick-play multiplayer server.
    pub quick_play_multiplayer: Option<String>,
    /// The identifier of a quick-play Realms world.
    pub quick_play_realms: Option<String>,
}

impl Session {
    /// The map of placeholder to value used for substitution.
    ///
    /// Placeholders that are not applicable to this session are omitted
    /// rather than mapped to an empty string.
    #[must_use]
    pub fn placeholders(&self) -> BTreeMap<&'static str, String> {
        let mut map = BTreeMap::new();
        map.insert("auth_player_name", self.player_name.clone());
        map.insert("auth_uuid", self.uuid.clone());
        map.insert("auth_access_token", self.access_token.clone());
        map.insert("auth_xuid", self.xuid.clone());
        map.insert("clientid", self.client_id.clone());
        map.insert("version_name", self.version_name.clone());
        map.insert("version_type", self.version_type.clone());
        map.insert("game_directory", path_to_string(&self.game_directory));
        map.insert("assets_root", path_to_string(&self.assets_root));
        map.insert("assets_index_name", self.assets_index_name.clone());
        map.insert("library_directory", path_to_string(&self.library_directory));
        map.insert("natives_directory", path_to_string(&self.natives_directory));
        map.insert("launcher_name", self.launcher_name.clone());
        map.insert("launcher_version", self.launcher_version.clone());

        // The legacy servers expect the UUID without hyphens.
        map.insert("uuid", self.uuid_prefix.clone());
        map.insert("user_properties", String::new());
        map.insert("game_assets", path_to_string(&self.assets_root));

        if let Some(width) = self.resolution_width {
            map.insert("resolution_width", width.to_string());
        }
        if let Some(height) = self.resolution_height {
            map.insert("resolution_height", height.to_string());
        }
        if let Some(path) = &self.quick_play_path {
            map.insert("quickPlayPath", path_to_string(path));
        }
        if let Some(id) = &self.quick_play_singleplayer {
            map.insert("quickPlaySingleplayer", id.clone());
        }
        if let Some(address) = &self.quick_play_multiplayer {
            map.insert("quickPlayMultiplayer", address.clone());
        }
        if let Some(id) = &self.quick_play_realms {
            map.insert("quickPlayRealms", id.clone());
        }

        map
    }
}

/// Render a path the way the game expects it.
///
/// Windows paths contain backslashes, which the game's own argument
/// parser treats as escape characters. Mojang's launcher therefore writes
/// them with forward slashes, which the JVM accepts on every platform.
#[must_use]
pub fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Replace every `${placeholder}` in a template.
///
/// A placeholder that is not in the map is left as it is.
#[must_use]
pub fn substitute(template: &str, values: &BTreeMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            // An unterminated placeholder is not a placeholder at all.
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        if let Some(value) = values.get(name) {
            out.push_str(value);
        } else {
            out.push_str("${");
            out.push_str(name);
            out.push('}');
        }
        rest = &after[end + 1..];
    }

    out.push_str(rest);
    out
}

/// Replace every `${placeholder}` in a list of templates.
#[must_use]
pub fn substitute_all(templates: &[String], values: &BTreeMap<&str, String>) -> Vec<String> {
    templates
        .iter()
        .map(|template| substitute(template, values))
        .collect()
}

/// The classpath of a launch session.
///
/// The client jar comes first, followed by the libraries in the order the
/// version JSON lists them. The separator is the platform's own path
/// separator, because the JVM accepts it on every platform.
#[must_use]
pub fn build_classpath(client_jar: &Path, libraries: &[PathBuf]) -> String {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut parts = Vec::with_capacity(libraries.len() + 1);
    parts.push(path_to_string(client_jar));
    for library in libraries {
        parts.push(path_to_string(library));
    }
    parts.join(&separator.to_string())
}

/// The separator used between classpath entries.
#[must_use]
pub fn classpath_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

/// Split a classpath into its entries.
#[must_use]
pub fn split_classpath(classpath: &str) -> Vec<&str> {
    classpath
        .split(classpath_separator())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The `-D` argument that points a system property at a directory.
///
/// The launcher uses it to describe the native layout to the game.
#[must_use]
pub fn system_property(name: &str, value: &Path) -> String {
    format!("-D{name}={}", path_to_string(value))
}

/// Extract the value of a `-D` system property from a list of arguments.
///
/// Returns `None` when the property is not present.
#[must_use]
pub fn property_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    let prefix = format!("-D{name}=");
    arguments
        .iter()
        .find_map(|argument| argument.strip_prefix(prefix.as_str()))
}

/// The directories the version's arguments point the native libraries at.
///
/// The version JSON names them explicitly:
///
/// ```text
/// -Djava.library.path=${natives_directory}/java
/// -Djna.tmpdir=${natives_directory}/jna
/// -Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}/lwjgl
/// -Dio.netty.native.workdir=${natives_directory}/netty
/// ```
///
/// Older versions point all four at the natives directory itself. The
/// launcher reads the arguments rather than guessing, so the two layouts
/// are handled without a version check.
#[must_use]
pub fn native_directories(arguments: &[String]) -> BTreeMap<String, String> {
    const PROPERTIES: &[&str] = &[
        "java.library.path",
        "jna.tmpdir",
        "org.lwjgl.system.SharedLibraryExtractPath",
        "io.netty.native.workdir",
    ];

    let mut directories = BTreeMap::new();
    for property in PROPERTIES {
        if let Some(value) = property_value(arguments, property) {
            directories.insert((*property).to_string(), value.to_string());
        }
    }
    directories
}

/// The `-javaagent:` argument for a jar, or `None` when the jar is absent.
#[must_use]
pub fn java_agent(jar: &Path) -> Option<String> {
    if jar.is_file() {
        Some(format!("-javaagent:{}", path_to_string(jar)))
    } else {
        None
    }
}

/// The classpath entry of a jar, as the JVM expects it.
#[must_use]
pub fn classpath_entry(jar: &Path) -> String {
    path_to_string(jar)
}

/// Whether a path looks like a Java archive.
#[must_use]
pub fn is_jar(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
}

/// Build the environment of a launch session.
///
/// The launcher passes the JVM's own environment through untouched and
/// adds the variables the game reads.
#[must_use]
pub fn build_environment(
    base: &[(String, String)],
    overrides: &[(String, String)],
) -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = base.to_vec();
    for (key, value) in overrides {
        if let Some(slot) = environment
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(key))
        {
            slot.1.clone_from(value);
        } else {
            environment.push((key.clone(), value.clone()));
        }
    }
    environment
}

/// The working directory of the game process.
#[must_use]
pub fn working_directory(session: &Session) -> PathBuf {
    session.game_directory.clone()
}

/// Ensure the game directory exists, so the game does not fail on a
/// read-only or missing directory.
///
/// # Errors
/// Fails if the directory cannot be created.
pub fn ensure_game_directory(session: &Session) -> Result<()> {
    if session.game_directory.as_os_str().is_empty() {
        return Err(Error::InvalidInput("no game directory configured".into()));
    }
    std::fs::create_dir_all(&session.game_directory)
        .map_err(|e| Error::io(&session.game_directory, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            player_name: "Steve".into(),
            uuid: "1234-5678-9abc-def0-1234-56789abc0000".into(),
            uuid_prefix: "123456789abcdef0123456789abc0000".into(),
            access_token: "token".into(),
            xuid: "1234567890".into(),
            client_id: "client".into(),
            version_name: "26.3".into(),
            version_type: "release".into(),
            game_directory: PathBuf::from("C:/games/mc"),
            assets_root: PathBuf::from("C:/cache/assets"),
            assets_index_name: "34".into(),
            library_directory: PathBuf::from("C:/cache/libraries"),
            natives_directory: PathBuf::from("C:/games/mc/natives"),
            launcher_name: "MinLauncher".into(),
            launcher_version: "0.1.0".into(),
            resolution_width: Some(1920),
            resolution_height: Some(1080),
            ..Session::default()
        }
    }

    #[test]
    fn placeholders_are_substituted() {
        let values = session().placeholders();
        assert_eq!(substitute("${auth_player_name}", &values), "Steve");
        assert_eq!(
            substitute("--version ${version_name}", &values),
            "--version 26.3"
        );
    }

    #[test]
    fn an_unknown_placeholder_is_left_alone() {
        let values = session().placeholders();
        assert_eq!(
            substitute("${not_a_placeholder}", &values),
            "${not_a_placeholder}"
        );
    }

    #[test]
    fn an_unterminated_placeholder_is_left_alone() {
        let values = session().placeholders();
        assert_eq!(
            substitute("${auth_player_name", &values),
            "${auth_player_name"
        );
    }

    #[test]
    fn several_placeholders_in_one_string() {
        let values = session().placeholders();
        assert_eq!(substitute("${a}", &values), "${a}");
        let mut custom = BTreeMap::new();
        custom.insert("a", "1".to_string());
        custom.insert("b", "2".to_string());
        assert_eq!(substitute("${a}-${b}-${a}", &custom), "1-2-1");
    }

    #[test]
    fn windows_paths_use_forward_slashes() {
        let values = session().placeholders();
        assert_eq!(substitute("${game_directory}", &values), "C:/games/mc");
    }

    #[test]
    fn the_classpath_starts_with_the_client_jar() {
        let classpath = build_classpath(
            Path::new("C:/v/26.3/client.jar"),
            &[PathBuf::from("C:/l/a.jar"), PathBuf::from("C:/l/b.jar")],
        );
        let parts = split_classpath(&classpath);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "C:/v/26.3/client.jar");
        assert_eq!(parts[1], "C:/l/a.jar");
        assert_eq!(parts[2], "C:/l/b.jar");
    }

    #[test]
    fn the_classpath_separator_is_platform_specific() {
        let separator = classpath_separator();
        if cfg!(windows) {
            assert_eq!(separator, ';');
        } else {
            assert_eq!(separator, ':');
        }
    }

    #[test]
    fn system_properties_are_read_back() {
        let arguments = vec![
            "-Djava.library.path=C:/natives/java".to_string(),
            "-Xmx4G".to_string(),
        ];
        assert_eq!(
            property_value(&arguments, "java.library.path"),
            Some("C:/natives/java")
        );
        assert_eq!(property_value(&arguments, "Xmx4G"), None);
    }

    #[test]
    fn native_directories_are_collected() {
        let arguments = vec![
            "-Djava.library.path=C:/n/java".to_string(),
            "-Djna.tmpdir=C:/n/jna".to_string(),
            "-Dorg.lwjgl.system.SharedLibraryExtractPath=C:/n/lwjgl".to_string(),
            "-Dio.netty.native.workdir=C:/n/netty".to_string(),
        ];
        let directories = native_directories(&arguments);
        assert_eq!(directories.len(), 4);
        assert_eq!(directories.get("java.library.path").unwrap(), "C:/n/java");
        assert_eq!(directories.get("jna.tmpdir").unwrap(), "C:/n/jna");
    }

    #[test]
    fn old_versions_have_a_single_natives_directory() {
        let arguments = vec![
            "-Djava.library.path=C:/n".to_string(),
            "-Djna.tmpdir=C:/n".to_string(),
            "-Dorg.lwjgl.system.SharedLibraryExtractPath=C:/n".to_string(),
            "-Dio.netty.native.workdir=C:/n".to_string(),
        ];
        let directories = native_directories(&arguments);
        assert_eq!(directories.len(), 4);
        for value in directories.values() {
            assert_eq!(value, "C:/n");
        }
    }

    #[test]
    fn a_missing_jar_makes_no_agent_argument() {
        assert_eq!(java_agent(Path::new("/does/not/exist.jar")), None);
    }

    #[test]
    fn environment_overrides_win() {
        let base = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("HOME".to_string(), "/home/x".to_string()),
        ];
        let overrides = vec![("PATH".to_string(), "/opt/bin".to_string())];
        let environment = build_environment(&base, &overrides);

        let path = environment
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.as_str());
        assert_eq!(path, Some("/opt/bin"));
        assert_eq!(environment.len(), 2);
    }

    #[test]
    fn the_game_directory_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = session();
        session.game_directory = dir.path().join("game");
        ensure_game_directory(&session).unwrap();
        assert!(session.game_directory.is_dir());
    }
}
