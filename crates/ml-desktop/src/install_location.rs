//! Moving the game files to a directory chosen by the player.
//!
//! The player may keep the versions, libraries, assets and runtimes on
//! any disk. Moving the root is not a rename of one folder: the database
//! stores absolute paths, and the launcher itself lives elsewhere. This
//! module copies the four game trees into the new place, checks that the
//! destination has room for them, and builds the service that will work
//! with the new layout. Nothing is deleted from the old place.
//!
//! The copy is resumable: a file that is already in the destination with
//! the same size is left alone, so a copy that was interrupted can be
//! started again. A `.part` file is never copied: it is a download that
//! was cut short, and the installer will fetch it again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ml_core::{AppPaths, Error, Result, ensure_disjoint, prepare_install_root};
use ml_minecraft::MinecraftService;
use ml_storage::Storage;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

/// How much free space must remain after the copy.
const SPACE_MARGIN: u64 = 1024 * 1024 * 1024;

/// How often the copy reports its progress.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// What the launcher tells the interface while the root is changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallRootEvent {
    /// The game files are being copied.
    Copying {
        /// Bytes written so far.
        copied_bytes: u64,
        /// Bytes that have to be written in total.
        total_bytes: u64,
    },
    /// The new directory is being scanned for installed versions.
    Scanning,
    /// The new directory is in use.
    Ready {
        /// How many versions were found in the new directory.
        installed: usize,
        /// How many version directories could not be read.
        failed: usize,
    },
}

/// The receiver of the progress of a root change.
pub type InstallRootProgress = Arc<dyn Fn(InstallRootEvent) + Send + Sync>;

/// The result of preparing a new installation root.
#[derive(Debug)]
pub struct PreparedTransition {
    /// The paths of the new root.
    pub paths: AppPaths,
    /// The service that works with the new root.
    pub service: MinecraftService,
    /// How many bytes were copied.
    pub copied_bytes: u64,
}

/// A source and a destination of one game tree.
#[derive(Debug, Clone)]
struct CopyRoot {
    source: PathBuf,
    target: PathBuf,
}

/// Validate a new root and copy the game files into it.
///
/// An empty `requested` means the historical split layout: versions go
/// back under the data directory and the other trees under the cache
/// directory. A non-empty value is validated as an absolute, writable
/// directory with the `MinLauncher` marker.
///
/// # Errors
/// Fails when the destination is invalid, overlaps the old installation
/// or the launcher's own directories, has too little free space, or when
/// a file cannot be copied.
pub fn prepare_transition(
    storage: Arc<Storage>,
    old_paths: &AppPaths,
    requested: &str,
    progress: &InstallRootProgress,
) -> Result<PreparedTransition> {
    let requested = requested.trim();
    let new_paths = if requested.is_empty() {
        old_paths.rebase_install_root(None)
    } else {
        let path = PathBuf::from(requested);
        if !path.is_absolute() {
            return Err(Error::InvalidInput(format!(
                "the installation directory `{requested}` is not an absolute path"
            )));
        }
        let absolute =
            std::path::absolute(&path).map_err(|error| Error::io(path.clone(), error))?;
        ensure_disjoint(
            &absolute,
            &[
                &old_paths.data_dir,
                &old_paths.cache_dir,
                &old_paths.versions_dir,
                &old_paths.assets_dir,
                &old_paths.libraries_dir,
                &old_paths.jre_dir,
            ],
        )?;
        let root = prepare_install_root(&absolute)?;
        old_paths.rebase_install_root(Some(&root))
    };
    new_paths.ensure_dirs()?;

    let roots = [
        CopyRoot {
            source: old_paths.versions_dir.clone(),
            target: new_paths.versions_dir.clone(),
        },
        CopyRoot {
            source: old_paths.assets_dir.clone(),
            target: new_paths.assets_dir.clone(),
        },
        CopyRoot {
            source: old_paths.libraries_dir.clone(),
            target: new_paths.libraries_dir.clone(),
        },
        CopyRoot {
            source: old_paths.jre_dir.clone(),
            target: new_paths.jre_dir.clone(),
        },
    ];

    let (total_bytes, needed_by_volume) = measure_copy(&roots)?;
    check_space(&needed_by_volume)?;
    let copied_bytes = copy_files(&roots, total_bytes, progress)?;

    let service = MinecraftService::new(new_paths.clone(), storage).map_err(|error| {
        Error::InvalidInput(format!(
            "cannot build the service for the new root: {error}"
        ))
    })?;

    Ok(PreparedTransition {
        paths: new_paths,
        service,
        copied_bytes,
    })
}

/// Measure what has to be copied and group the bytes by the volume.
fn measure_copy(roots: &[CopyRoot]) -> Result<(u64, BTreeMap<PathBuf, u64>)> {
    let mut total = 0u64;
    let mut by_volume: BTreeMap<PathBuf, u64> = BTreeMap::new();

    for root in roots {
        if !root.source.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&root.source) {
            let entry = entry.map_err(|error| {
                Error::InvalidInput(format!("cannot walk `{}`: {error}", root.source.display()))
            })?;
            if !entry.file_type().is_file() || is_partial(entry.path()) {
                continue;
            }
            let size = entry
                .metadata()
                .map_err(|error| Error::io(entry.path(), error.into()))?
                .len();
            let relative = entry
                .path()
                .strip_prefix(&root.source)
                .map_err(|error| Error::InvalidInput(error.to_string()))?;
            let target = root.target.join(relative);
            let already_there =
                std::fs::metadata(&target).is_ok_and(|metadata| metadata.len() == size);
            if already_there {
                continue;
            }

            total += size;
            *by_volume.entry(volume_root(&root.target)).or_default() += size;
        }
    }

    Ok((total, by_volume))
}

/// Refuse a copy that would not fit on the destination volume.
fn check_space(needed: &BTreeMap<PathBuf, u64>) -> Result<()> {
    for (volume, bytes) in needed {
        let available = fs2::available_space(volume).map_err(|error| Error::io(volume, error))?;
        if available < bytes.saturating_add(SPACE_MARGIN) {
            return Err(Error::InvalidInput(format!(
                "there is not enough room on `{}`: the copy needs {} GiB, {} GiB are free",
                volume.display(),
                bytes.saturating_add(SPACE_MARGIN) / (1024 * 1024 * 1024),
                available / (1024 * 1024 * 1024)
            )));
        }
    }
    Ok(())
}

/// Copy the files that are missing or have a different size.
fn copy_files(roots: &[CopyRoot], total_bytes: u64, progress: &InstallRootProgress) -> Result<u64> {
    let mut copied = 0u64;
    let mut last_report = Instant::now();
    progress(InstallRootEvent::Copying {
        copied_bytes: 0,
        total_bytes,
    });

    for root in roots {
        if !root.source.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&root.source) {
            let entry = entry.map_err(|error| {
                Error::InvalidInput(format!("cannot walk `{}`: {error}", root.source.display()))
            })?;
            if !entry.file_type().is_file() || is_partial(entry.path()) {
                continue;
            }
            let source = entry.path();
            let size = entry
                .metadata()
                .map_err(|error| Error::io(source, error.into()))?
                .len();
            let relative = source
                .strip_prefix(&root.source)
                .map_err(|error| Error::InvalidInput(error.to_string()))?;
            let target = root.target.join(relative);

            if std::fs::metadata(&target).is_ok_and(|metadata| metadata.len() == size) {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
            }
            std::fs::copy(source, &target).map_err(|error| Error::io(&target, error))?;
            copied += size;

            if last_report.elapsed() >= PROGRESS_INTERVAL {
                progress(InstallRootEvent::Copying {
                    copied_bytes: copied,
                    total_bytes,
                });
                last_report = Instant::now();
            }
        }
    }

    progress(InstallRootEvent::Copying {
        copied_bytes: copied,
        total_bytes,
    });
    Ok(copied)
}

fn is_partial(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".part"))
}

fn volume_root(path: &Path) -> PathBuf {
    path.ancestors()
        .last()
        .map_or_else(|| path.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    fn paths() -> (tempfile::TempDir, AppPaths, Arc<Storage>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let storage = Arc::new(Storage::open(&paths).unwrap());
        (dir, paths, storage)
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_new_root_receives_the_game_files() {
        let (dir, old, storage) = paths();
        write(
            &old.version_dir("26.3").join("version.json"),
            br#"{"id":"26.3"}"#,
        );
        write(&old.version_dir("26.3").join("client.jar"), b"client");
        write(&old.version_dir("26.3").join("client.jar.part"), b"partial");
        write(
            &old.assets_dir.join("objects").join("ab").join("abc"),
            b"asset",
        );
        write(&old.libraries_dir.join("g").join("a.jar"), b"library");
        write(
            &old.jre_dir_for("jre-legacy").join("bin").join("java.exe"),
            b"java",
        );

        let target = dir.path().join("game");
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&events);
        let progress: InstallRootProgress = Arc::new(move |event| {
            recorder.lock().push(event);
        });

        let prepared = prepare_transition(
            Arc::clone(&storage),
            &old,
            &target.to_string_lossy(),
            &progress,
        )
        .unwrap();

        assert_eq!(
            prepared.paths.install_dir.as_deref(),
            Some(target.as_path())
        );
        assert!(
            prepared
                .paths
                .versions_dir
                .join("26.3")
                .join("client.jar")
                .is_file()
        );
        assert!(
            prepared
                .paths
                .assets_dir
                .join("objects")
                .join("ab")
                .join("abc")
                .is_file()
        );
        assert!(
            prepared
                .paths
                .libraries_dir
                .join("g")
                .join("a.jar")
                .is_file()
        );
        assert!(
            prepared
                .paths
                .jre_dir_for("jre-legacy")
                .join("bin")
                .join("java.exe")
                .is_file()
        );
        assert!(
            !prepared
                .paths
                .version_dir("26.3")
                .join("client.jar.part")
                .exists()
        );
        assert!(ml_core::marker_path(&target).is_file());
        assert!(prepared.copied_bytes > 0);
        assert!(matches!(
            events.lock().last(),
            Some(InstallRootEvent::Copying {
                copied_bytes,
                total_bytes
            }) if copied_bytes == total_bytes
        ));
    }

    #[test]
    fn a_non_empty_directory_without_a_marker_is_refused() {
        let (dir, old, storage) = paths();
        let target = dir.path().join("documents");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("thesis.txt"), b"not a game").unwrap();

        let progress: InstallRootProgress = Arc::new(|_| {});
        let error =
            prepare_transition(storage, &old, &target.to_string_lossy(), &progress).unwrap_err();
        assert!(error.to_string().contains("is not empty"), "{error}");
    }

    #[test]
    fn a_target_inside_the_old_game_tree_is_refused() {
        let (_dir, old, storage) = paths();
        let target = old.versions_dir.join("game");

        let progress: InstallRootProgress = Arc::new(|_| {});
        let error =
            prepare_transition(storage, &old, &target.to_string_lossy(), &progress).unwrap_err();
        assert!(error.to_string().contains("overlaps"), "{error}");
    }

    #[test]
    fn an_existing_file_of_the_same_size_is_not_copied_again() {
        let (dir, old, storage) = paths();
        write(&old.versions_dir.join("a.jar"), b"same");
        let target = dir.path().join("game");
        ml_core::prepare_install_root(&target).unwrap();
        std::fs::write(target.join("versions").join("a.jar"), b"same").unwrap();

        let progress: InstallRootProgress = Arc::new(|_| {});
        let prepared =
            prepare_transition(storage, &old, &target.to_string_lossy(), &progress).unwrap();

        assert_eq!(prepared.copied_bytes, 0);
    }

    #[test]
    fn a_disk_without_room_is_refused_before_anything_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let volume = volume_root(dir.path());
        let free = fs2::available_space(&volume).unwrap();

        // A copy that needs everything that is free would leave the disk
        // without room for the game to run; the margin refuses it.
        let error = check_space(&BTreeMap::from([(volume, free)])).unwrap_err();

        assert!(error.to_string().contains("not enough room"), "{error}");
    }

    #[test]
    fn a_folder_with_spaces_and_cyrillic_receives_the_game_files() {
        let (dir, old, storage) = paths();
        write(
            &old.version_dir("26.3").join("version.json"),
            br#"{"id":"26.3"}"#,
        );
        write(&old.version_dir("26.3").join("client.jar"), b"client");

        // A path the player is likely to type: two words, one of them in
        // Russian. It has to work exactly as a path of one word does.
        let target = dir.path().join("Мои игры").join("Minecraft Launcher");
        let progress: InstallRootProgress = Arc::new(|_| {});
        let prepared =
            prepare_transition(storage, &old, &target.to_string_lossy(), &progress).unwrap();

        assert_eq!(
            prepared.paths.install_dir.as_deref(),
            Some(target.as_path())
        );
        assert!(
            prepared
                .paths
                .versions_dir
                .join("26.3")
                .join("client.jar")
                .is_file()
        );
    }
}
