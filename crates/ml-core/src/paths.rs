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
}
