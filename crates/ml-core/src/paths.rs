//! Filesystem layout of the launcher.
//!
//! Every path the launcher touches is derived from a single [`AppPaths`]
//! value so that tests can redirect the whole tree into a temporary
//! directory and so that "where does this file live?" always has one answer.

use std::path::{Path, PathBuf};

/// Directory name used under the per-user application data directories.
pub const APP_DIR_NAME: &str = "MinLauncher";

/// The complete set of directories the launcher owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Root for persistent state (settings, database, accounts).
    pub data_dir: PathBuf,
    /// Root for regenerable data (downloaded jars, assets, runtimes).
    pub cache_dir: PathBuf,
    /// Installed game versions, one directory per version id.
    pub versions_dir: PathBuf,
    /// Content-addressed asset store.
    pub assets_dir: PathBuf,
    /// Maven-style library repository.
    pub libraries_dir: PathBuf,
    /// Downloaded Java runtimes, one directory per runtime component.
    pub jre_dir: PathBuf,
    /// User-created game instances (separate `gameDir`s).
    pub instances_dir: PathBuf,
    /// Rotating launcher logs.
    pub logs_dir: PathBuf,
    /// Benchmark result store.
    pub benchmarks_dir: PathBuf,
    /// Directory holding the jar of the profiling agent.
    pub agent_dir: PathBuf,
    /// Directory holding the jars of the mods.
    pub mods_dir: PathBuf,
    /// SQLite database file.
    pub database_path: PathBuf,
    /// JSON settings file.
    pub settings_path: PathBuf,
    /// Encrypted secrets file (access/refresh tokens).
    pub secrets_path: PathBuf,
}

impl AppPaths {
    /// Build a path layout rooted at the given data and cache directories.
    #[must_use]
    pub fn new(data_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let cache_dir = cache_dir.into();
        Self {
            versions_dir: data_dir.join("versions"),
            assets_dir: cache_dir.join("assets"),
            libraries_dir: cache_dir.join("libraries"),
            jre_dir: cache_dir.join("runtimes"),
            instances_dir: data_dir.join("instances"),
            logs_dir: data_dir.join("logs"),
            benchmarks_dir: data_dir.join("benchmarks"),
            agent_dir: data_dir.join("agent"),
            mods_dir: data_dir.join("mods"),
            database_path: data_dir.join("minlaucher.db"),
            settings_path: data_dir.join("settings.json"),
            secrets_path: data_dir.join("secrets.json"),
            data_dir,
            cache_dir,
        }
    }

    /// Discover the layout from the host operating system.
    ///
    /// Uses `%APPDATA%` (or the XDG equivalent) for persistent state and
    /// `%LOCALAPPDATA%` for regenerable caches, so that clearing the cache
    /// never destroys accounts or settings.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidInput`] when the OS does not expose a
    /// data directory.
    pub fn discover() -> crate::Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| crate::Error::InvalidInput("no OS data directory available".into()))?
            .join(APP_DIR_NAME);
        let cache_dir = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .ok_or_else(|| crate::Error::InvalidInput("no OS cache directory available".into()))?
            .join(APP_DIR_NAME);
        Ok(Self::new(data_dir, cache_dir))
    }

    /// All directories that must exist before the launcher can operate.
    #[must_use]
    pub fn all_dirs(&self) -> Vec<&Path> {
        vec![
            &self.data_dir,
            &self.cache_dir,
            &self.versions_dir,
            &self.assets_dir,
            &self.libraries_dir,
            &self.jre_dir,
            &self.instances_dir,
            &self.logs_dir,
            &self.benchmarks_dir,
            &self.agent_dir,
            &self.mods_dir,
        ]
    }

    /// Create every required directory, ignoring ones that already exist.
    ///
    /// # Errors
    /// Propagates the first filesystem error encountered.
    pub fn ensure_dirs(&self) -> crate::Result<()> {
        for dir in self.all_dirs() {
            std::fs::create_dir_all(dir).map_err(|e| crate::Error::io(dir, e))?;
        }
        Ok(())
    }

    /// Directory holding the files of a single installed version.
    #[must_use]
    pub fn version_dir(&self, version_id: &str) -> PathBuf {
        self.versions_dir.join(sanitize_component(version_id))
    }

    /// Directory holding a downloaded Java runtime.
    #[must_use]
    pub fn jre_dir_for(&self, component: &str) -> PathBuf {
        self.jre_dir.join(sanitize_component(component))
    }

    /// Directory holding one game instance.
    #[must_use]
    pub fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.instances_dir.join(sanitize_component(instance_id))
    }

    /// Path of an asset object inside the content-addressed store.
    ///
    /// Mojang addresses assets by the SHA-1 of their content, which makes the
    /// store a pure content-addressed filesystem: identical assets are stored
    /// once and never re-downloaded.
    #[must_use]
    pub fn asset_object(&self, hash: &str) -> PathBuf {
        let hash = hash.to_ascii_lowercase();
        let (prefix, rest) = hash.split_at(hash.len().min(2));
        self.assets_dir.join("objects").join(prefix).join(rest)
    }

    /// Path of the jar of the profiling agent.
    ///
    /// The jar lives under the data directory rather than next to the
    /// executable, because it is written there by the build and not
    /// carried by the installer: the launcher copies it into place when
    /// it is missing.
    #[must_use]
    pub fn agent_jar(&self) -> PathBuf {
        self.agent_dir.join("minlaucher-agent.jar")
    }

    /// Path of the jar of the mod of the launcher.
    ///
    /// The jar lives next to the other mods: it is a mod like any other,
    /// and the loader reads them all from the same place.
    #[must_use]
    pub fn mod_jar(&self) -> PathBuf {
        self.mods_dir.join("minlaucher-optimisations.jar")
    }

    /// Directory the loader reads the mods from.
    ///
    /// The loader reads the mods of the instance it runs in, so the
    /// directory is inside the instance and not in the data directory of
    /// the launcher.
    #[must_use]
    pub fn instance_mods_dir(&self, instance_id: Option<&str>) -> PathBuf {
        match instance_id {
            Some(id) => self.instance_dir(id).join("mods"),
            None => self.mods_dir.clone(),
        }
    }

    /// Path of the report the agent writes.
    ///
    /// The report is written next to the logs: it is the same kind of
    /// thing — what the launcher tells about the last run — and it is
    /// read back after the game ends.
    #[must_use]
    pub fn agent_report(&self, version_id: &str) -> PathBuf {
        self.logs_dir
            .join("agent")
            .join(format!("{}.json", sanitize_component(version_id)))
    }
}

/// Whether a path can be used for the `-javaagent:` argument.
///
/// The JVM splits `-javaagent:jar=options` at the *first* `=`, so a jar
/// whose path has an `=` in it is cut in half and the agent does not
/// load. The name of the user is part of the path of the data
/// directory, and a name can have an `=` in it, so the path has to be
/// checked before the agent is attached.
///
/// The path of the *report* has no such problem: it travels in a system
/// property, which is a separate argument and is never split.
#[must_use]
pub fn is_usable_as_agent_path(path: &Path) -> bool {
    path.to_string_lossy().chars().all(|c| c != '=')
}

/// Reduce an arbitrary string to something safe to use as a path segment.
///
/// Prevents path traversal (`..`, absolute paths, Windows reserved device
/// names) from reaching the filesystem through version ids or instance ids
/// that originate from remote metadata.
#[must_use]
pub fn sanitize_component(input: &str) -> String {
    // Windows reserves a handful of device names regardless of extension.
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    // Collapse every run of dots into a single dot followed by an
    // underscore, so the result can never contain a `..` sequence.
    let mut collapsed = String::with_capacity(out.len() + 1);
    let mut previous_was_dot = false;
    for ch in out.chars() {
        if ch == '.' {
            if previous_was_dot {
                collapsed.push('_');
            } else {
                collapsed.push('.');
            }
            previous_was_dot = true;
        } else {
            collapsed.push(ch);
            previous_was_dot = false;
        }
    }

    let trimmed = collapsed.trim_matches('.').to_string();
    if !trimmed.chars().any(|c| c.is_ascii_alphanumeric()) {
        return "default".to_string();
    }

    if RESERVED.iter().any(|r| trimmed.eq_ignore_ascii_case(r)) {
        return format!("{trimmed}_");
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_derived_from_roots() {
        let p = AppPaths::new("/data", "/cache");
        assert_eq!(p.database_path, PathBuf::from("/data/minlaucher.db"));
        assert_eq!(p.assets_dir, PathBuf::from("/cache/assets"));
        assert_eq!(p.libraries_dir, PathBuf::from("/cache/libraries"));
    }

    #[test]
    fn asset_objects_are_content_addressed() {
        let p = AppPaths::new("/data", "/cache");
        let obj = p.asset_object("ABCDEF0123456789");
        assert_eq!(
            obj,
            PathBuf::from("/cache/assets/objects/ab/cdef0123456789")
        );
    }

    #[test]
    fn traversal_is_neutralised() {
        // The result must never contain a path separator or a `..` sequence.
        let result = sanitize_component("../../etc/passwd");
        assert!(!result.contains(".."), "got {result}");
        assert!(!result.contains('/'), "got {result}");
        assert!(!result.contains('\\'), "got {result}");
        assert_eq!(result, "__.__etc_passwd");
        assert_eq!(sanitize_component(".."), "default");
        assert_eq!(sanitize_component(""), "default");
    }

    #[test]
    fn windows_device_names_are_escaped() {
        assert_eq!(sanitize_component("CON"), "CON_");
        assert_eq!(sanitize_component("con"), "con_");
        assert_eq!(sanitize_component("nul.txt"), "nul.txt");
    }

    #[test]
    fn normal_ids_survive() {
        assert_eq!(sanitize_component("1.20.5"), "1.20.5");
        assert_eq!(sanitize_component("26.4-snapshot-4"), "26.4-snapshot-4");
    }

    #[test]
    fn the_agent_jar_and_its_report_have_their_own_places() {
        let p = AppPaths::new("/data", "/cache");

        assert_eq!(
            p.agent_jar(),
            PathBuf::from("/data/agent/minlaucher-agent.jar")
        );
        assert_eq!(
            p.agent_report("1.20.5"),
            PathBuf::from("/data/logs/agent/1.20.5.json")
        );
    }

    #[test]
    fn a_path_with_an_equals_sign_cannot_carry_the_agent() {
        // The JVM splits `-javaagent:jar=options` at the first `=`, so
        // such a path would cut the jar in half.
        assert!(is_usable_as_agent_path(Path::new("/data/agent/agent.jar")));
        assert!(!is_usable_as_agent_path(Path::new("/data/a=b/agent.jar")));
        assert!(!is_usable_as_agent_path(Path::new(
            "C:\\Users\\a=b\\agent.jar"
        )));
    }
}
