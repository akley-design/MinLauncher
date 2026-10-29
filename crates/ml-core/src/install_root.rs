//! Recognition and validation of a directory that holds the game files.
//!
//! A player may keep the versions, libraries, assets and runtimes on any
//! disk. The launcher remembers that choice in its settings, and writes a
//! small marker into the directory so that the directory itself says what
//! it is: a folder that `MinLauncher` owns.
//!
//! The marker is deliberately not a hidden database. It carries the
//! schema revision and the name of the application, and nothing the
//! launcher cannot rebuild from the game files themselves.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Name of the marker file in the root of a chosen installation directory.
pub const MARKER_FILE: &str = ".minlauncher.json";

/// The marker schema this version of the launcher writes and understands.
pub const MARKER_SCHEMA: u32 = 1;

/// The name of the application the marker belongs to.
const MARKER_APP: &str = "MinLauncher";

/// The contents of the marker file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRootMarker {
    /// Schema revision of the marker itself.
    pub schema: u32,
    /// The application that owns the directory.
    pub app: String,
    /// When the marker was written, in ISO-8601.
    #[serde(default)]
    pub created_at: String,
}

impl InstallRootMarker {
    /// A marker for a directory that is being adopted now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema: MARKER_SCHEMA,
            app: MARKER_APP.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl Default for InstallRootMarker {
    fn default() -> Self {
        Self::new()
    }
}

/// The marker file of a directory.
#[must_use]
pub fn marker_path(root: &Path) -> PathBuf {
    root.join(MARKER_FILE)
}

/// Read the marker of a directory, when it has one.
///
/// A marker that is present but cannot be read is an error: the directory
/// was written by something, and silently treating it as empty would put
/// the game's files next to whatever that something is.
///
/// # Errors
/// Fails when the marker is malformed, belongs to another application,
/// or was written by a newer schema.
pub fn read_marker(root: &Path) -> Result<Option<InstallRootMarker>> {
    let path = marker_path(root);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(&path, error)),
    };

    let marker: InstallRootMarker =
        serde_json::from_slice(&bytes).map_err(|error| Error::json(MARKER_FILE, &error))?;

    if marker.app != MARKER_APP {
        return Err(Error::InvalidInput(format!(
            "the directory `{}` belongs to `{}`, not to {MARKER_APP}",
            root.display(),
            marker.app
        )));
    }
    if marker.schema > MARKER_SCHEMA {
        return Err(Error::InvalidInput(format!(
            "the directory `{}` was written by a newer MinLauncher (marker schema {}, this launcher understands {MARKER_SCHEMA})",
            root.display(),
            marker.schema
        )));
    }

    Ok(Some(marker))
}

/// Whether a directory carries the marker of this launcher.
#[must_use]
pub fn is_install_root(root: &Path) -> bool {
    read_marker(root).is_ok_and(|marker| marker.is_some())
}

/// Validate and prepare a directory as the root of the game files.
///
/// The directory is created when it does not exist. An empty directory is
/// adopted and marked; a directory with the marker is accepted as it is;
/// a non-empty directory without the marker is refused, so that a typo in
/// the folder picker cannot scatter the game over an unrelated folder.
///
/// The returned path is absolute and is what should be stored in the
/// settings: it is the same directory the caller passed, without the
/// verbatim `\\?\` prefix that canonicalisation would add on Windows.
///
/// # Errors
/// Fails when the path is relative, points at a file, names a filesystem
/// root, cannot be created or written, or is an unmarked non-empty folder.
pub fn prepare_install_root(path: impl AsRef<Path>) -> Result<PathBuf> {
    let requested = path.as_ref();
    if requested.as_os_str().is_empty() {
        return Err(Error::InvalidInput(
            "the installation directory is empty".into(),
        ));
    }
    if !requested.is_absolute() {
        return Err(Error::InvalidInput(format!(
            "the installation directory `{}` is not an absolute path",
            requested.display()
        )));
    }

    let absolute = std::path::absolute(requested)
        .map_err(|error| Error::io(requested.to_path_buf(), error))?;

    // A filesystem root is never a game directory: the marker and the
    // four trees would sit next to everything else on the disk.
    if absolute.parent().is_none() {
        return Err(Error::InvalidInput(format!(
            "`{}` is a drive root; choose a folder inside it",
            absolute.display()
        )));
    }

    if absolute.exists() && !absolute.is_dir() {
        return Err(Error::InvalidInput(format!(
            "`{}` is a file, not a directory",
            absolute.display()
        )));
    }

    std::fs::create_dir_all(&absolute).map_err(|error| Error::io(&absolute, error))?;
    probe_writable(&absolute)?;

    if read_marker(&absolute)?.is_none() {
        if directory_is_empty(&absolute)? {
            write_marker(&absolute, &InstallRootMarker::new())?;
        } else {
            return Err(Error::InvalidInput(format!(
                "the directory `{}` is not empty and has no MinLauncher marker; choose an empty folder",
                absolute.display()
            )));
        }
    }

    for directory in ["versions", "libraries", "assets", "runtimes"] {
        let path = absolute.join(directory);
        std::fs::create_dir_all(&path).map_err(|error| Error::io(&path, error))?;
    }

    Ok(absolute)
}

/// Whether `path` is the same directory as `root` or lives inside it.
///
/// The comparison is case-insensitive on Windows, where the filesystem is,
/// and keeps the comparison textual so that a path that does not exist yet
/// can be checked before it is created.
#[must_use]
pub fn is_same_or_inside(path: &Path, root: &Path) -> bool {
    let path = normalized(path);
    let root = normalized(root);

    if path == root {
        return true;
    }

    let prefix = if root.ends_with(std::path::MAIN_SEPARATOR) {
        root
    } else {
        format!("{root}{}", std::path::MAIN_SEPARATOR)
    };
    path.starts_with(&prefix)
}

/// Refuse a root that overlaps one of the directories the caller owns.
///
/// # Errors
/// Fails when the root is inside a forbidden directory, or a forbidden
/// directory is inside the root. Both directions matter: the copy must
/// never walk into itself.
pub fn ensure_disjoint(root: &Path, forbidden: &[&Path]) -> Result<()> {
    for other in forbidden {
        if is_same_or_inside(root, other) || is_same_or_inside(other, root) {
            return Err(Error::InvalidInput(format!(
                "the installation directory `{}` overlaps `{}`; choose another folder",
                root.display(),
                other.display()
            )));
        }
    }
    Ok(())
}

fn normalized(path: &Path) -> String {
    let text = path
        .to_string_lossy()
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    let text = text.trim_end_matches(std::path::MAIN_SEPARATOR);
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.to_string()
    }
}

fn directory_is_empty(path: &Path) -> Result<bool> {
    let mut entries = std::fs::read_dir(path).map_err(|error| Error::io(path, error))?;
    Ok(entries.next().is_none())
}

fn probe_writable(path: &Path) -> Result<()> {
    let probe = path.join(format!(
        ".minlauncher-write-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&probe, b"ok").map_err(|error| Error::io(&probe, error))?;
    std::fs::remove_file(&probe).map_err(|error| Error::io(&probe, error))?;
    Ok(())
}

fn write_marker(root: &Path, marker: &InstallRootMarker) -> Result<()> {
    let path = marker_path(root);
    let temp = root.join(format!(".minlauncher-{}.tmp", std::process::id()));
    let bytes =
        serde_json::to_vec_pretty(marker).map_err(|error| Error::json(MARKER_FILE, &error))?;

    std::fs::write(&temp, bytes).map_err(|error| Error::io(&temp, error))?;
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(Error::io(&path, error));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_directory_is_adopted_and_marked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");

        let prepared = prepare_install_root(&root).unwrap();

        assert_eq!(prepared, root);
        assert!(marker_path(&prepared).is_file());
        for name in ["versions", "libraries", "assets", "runtimes"] {
            assert!(prepared.join(name).is_dir(), "{name} is missing");
        }
        let marker = read_marker(&prepared).unwrap().unwrap();
        assert_eq!(marker.schema, MARKER_SCHEMA);
        assert_eq!(marker.app, MARKER_APP);
    }

    #[test]
    fn a_marked_directory_is_accepted_even_when_it_is_not_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        std::fs::create_dir_all(root.join("versions").join("26.3")).unwrap();
        let marker = InstallRootMarker::new();
        write_marker(&root, &marker).unwrap();

        let prepared = prepare_install_root(&root).unwrap();
        assert_eq!(prepared, root);
        assert_eq!(read_marker(&prepared).unwrap(), Some(marker));
    }

    #[test]
    fn an_unmarked_non_empty_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("documents");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("thesis.txt"), b"not a game").unwrap();

        let error = prepare_install_root(&root).unwrap_err();
        assert!(error.to_string().contains("is not empty"), "{error}");
    }

    #[test]
    fn a_marker_of_a_newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        std::fs::create_dir_all(&root).unwrap();
        let marker = InstallRootMarker {
            schema: MARKER_SCHEMA + 1,
            ..InstallRootMarker::new()
        };
        std::fs::write(marker_path(&root), serde_json::to_vec(&marker).unwrap()).unwrap();

        let error = prepare_install_root(&root).unwrap_err();
        assert!(error.to_string().contains("newer MinLauncher"), "{error}");
    }

    #[test]
    fn a_relative_path_is_refused() {
        let error = prepare_install_root("relative/game").unwrap_err();
        assert!(error.to_string().contains("not an absolute"), "{error}");
    }

    #[test]
    fn a_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("game");
        std::fs::write(&file, b"x").unwrap();

        let error = prepare_install_root(&file).unwrap_err();
        assert!(error.to_string().contains("is a file"), "{error}");
    }

    #[test]
    fn a_drive_root_is_refused() {
        let root = if cfg!(windows) {
            Path::new("C:\\")
        } else {
            Path::new("/")
        };
        let error = prepare_install_root(root).unwrap_err();
        assert!(error.to_string().contains("drive root"), "{error}");
    }

    #[test]
    fn nesting_is_detected_in_both_directions() {
        let inside = Path::new("/games/MinLauncher/versions");
        let root = Path::new("/games/MinLauncher");

        assert!(is_same_or_inside(inside, root));
        assert!(is_same_or_inside(root, root));
        assert!(!is_same_or_inside(root, inside));
        assert!(!is_same_or_inside(Path::new("/games/MinLauncher2"), root));
    }

    #[test]
    fn overlapping_directories_are_refused() {
        let root = Path::new("/games/MinLauncher");
        let forbidden = Path::new("/games");
        assert!(ensure_disjoint(root, &[forbidden]).is_err());

        let root = Path::new("/other/game");
        assert!(ensure_disjoint(root, &[forbidden]).is_ok());
    }
}
