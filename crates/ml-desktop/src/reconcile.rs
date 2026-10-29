//! Bringing the database in line with the versions on the disk.
//!
//! The database is the list the interface reads, but the files are the
//! truth. The two drift apart when the player moves the installation
//! folder, copies versions from another machine, or deletes a version
//! directory by hand. This module scans the version directory and writes
//! what it finds into the database, so that the launcher knows what is
//! really installed.

use std::collections::HashSet;

use ml_core::{AppPaths, Error, Result};
use ml_minecraft::VersionJson;
use ml_storage::Storage;
use serde::{Deserialize, Serialize};

/// One version directory that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFailure {
    /// The version id, taken from the directory name when the document
    /// itself cannot be read.
    pub version_id: String,
    /// What went wrong.
    pub message: String,
}

/// What a scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    /// How many version directories had a `version.json` to read.
    pub scanned: usize,
    /// How many versions are installed and complete.
    pub installed: usize,
    /// The directories that could not be adopted.
    pub failed: Vec<ScanFailure>,
}

/// Scan the versions of the current installation and update the database.
///
/// A version counts as installed only when its document parses and its
/// client jar is present and not empty: a version whose download was cut
/// short is not a version the player can start.
///
/// Versions the database calls installed but that are not in the scanned
/// directory are marked as not installed. Their metadata is kept: the
/// manifest still knows them, and switching back to the old folder will
/// find them again.
///
/// # Errors
/// Fails when the directory cannot be read or the database cannot be
/// written. Unreadable individual version directories do not abort the
/// scan; they are reported in [`ScanReport::failed`].
pub fn rescan_versions(storage: &Storage, paths: &AppPaths) -> Result<ScanReport> {
    let previously_installed = storage.versions().list_installed()?;
    let mut report = ScanReport::default();
    let mut found: HashSet<String> = HashSet::new();

    let entries = match std::fs::read_dir(&paths.versions_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            for version in previously_installed {
                storage.versions().set_installed(&version.id, false)?;
            }
            return Ok(report);
        }
        Err(error) => return Err(Error::io(&paths.versions_dir, error)),
    };

    for entry in entries {
        let entry = entry.map_err(|error| Error::io(&paths.versions_dir, error))?;
        if !entry
            .file_type()
            .map_err(|error| Error::io(entry.path(), error))?
            .is_dir()
        {
            continue;
        }

        let directory = entry.path();
        let document = directory.join("version.json");
        if !document.is_file() {
            continue;
        }
        let directory_id = entry.file_name().to_string_lossy().into_owned();
        report.scanned += 1;

        let bytes = match std::fs::read(&document) {
            Ok(bytes) => bytes,
            Err(error) => {
                report.failed.push(ScanFailure {
                    version_id: directory_id,
                    message: error.to_string(),
                });
                continue;
            }
        };
        let version = match VersionJson::parse(&bytes) {
            Ok(version) => version,
            Err(error) => {
                report.failed.push(ScanFailure {
                    version_id: directory_id,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if version.id.is_empty() {
            report.failed.push(ScanFailure {
                version_id: directory_id,
                message: "the version document has no id".into(),
            });
            continue;
        }

        let client = directory.join("client.jar");
        if !client.is_file() {
            report.failed.push(ScanFailure {
                version_id: version.id.clone(),
                message: "the client jar is missing".into(),
            });
            continue;
        }
        let size = std::fs::metadata(&client)
            .map_err(|error| Error::io(&client, error))?
            .len();
        if size == 0 {
            report.failed.push(ScanFailure {
                version_id: version.id.clone(),
                message: "the client jar is empty".into(),
            });
            continue;
        }

        let meta = version.to_meta(None);
        storage.versions().upsert(&meta)?;
        storage
            .versions()
            .set_json_path(&meta.id, &document.to_string_lossy())?;
        storage.versions().set_installed(&meta.id, true)?;
        found.insert(meta.id);
    }

    for version in previously_installed {
        if !found.contains(&version.id) {
            storage.versions().set_installed(&version.id, false)?;
        }
    }

    report.installed = found.len();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{VersionKind, VersionMeta};

    const VERSION: &str = r#"{
        "id": "26.3",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "releaseTime": "2026-01-01T00:00:00+00:00",
        "javaVersion": {"component": "java-runtime-epsilon", "majorVersion": 25},
        "assetIndex": {
            "id": "34",
            "sha1": "e787e8afd149a4cefa89d341b91f5aed8f5d93a0",
            "size": 1,
            "totalSize": 1,
            "url": "https://example/34.json"
        },
        "downloads": {
            "client": {
                "sha1": "e877b6a07acd633fb3bb475002175cec036e7b87",
                "size": 3,
                "url": "https://example/client.jar"
            }
        }
    }"#;

    fn setup() -> (tempfile::TempDir, AppPaths, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let storage = Storage::open(&paths).unwrap();
        (dir, paths, storage)
    }

    fn meta(id: &str) -> VersionMeta {
        VersionMeta {
            id: id.to_string(),
            kind: VersionKind::Release,
            release_time: Some("2026-01-01T00:00:00+00:00".into()),
            java_component: None,
            java_major: None,
            main_class: None,
            asset_index_id: None,
            asset_index_sha1: None,
            asset_index_size: None,
            client_sha1: None,
            client_size: None,
            json_sha1: None,
        }
    }

    fn write_version(paths: &AppPaths, id: &str, document: &str, client: Option<&[u8]>) {
        let dir = paths.version_dir(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("version.json"), document).unwrap();
        if let Some(bytes) = client {
            std::fs::write(dir.join("client.jar"), bytes).unwrap();
        }
    }

    #[test]
    fn a_complete_version_is_adopted_and_its_metadata_recorded() {
        let (_dir, paths, storage) = setup();
        write_version(&paths, "26.3", VERSION, Some(b"jar"));

        let report = rescan_versions(&storage, &paths).unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.installed, 1);
        assert_eq!(report.failed, []);
        assert!(storage.versions().is_installed("26.3").unwrap());
        let meta = storage.versions().get("26.3").unwrap().unwrap();
        assert_eq!(meta.java_major, Some(25));
        assert_eq!(meta.java_component.as_deref(), Some("java-runtime-epsilon"));
        assert_eq!(meta.asset_index_id.as_deref(), Some("34"));
        let json_path = storage.versions().json_path("26.3").unwrap().unwrap();
        assert!(
            json_path.ends_with("26.3/version.json") || json_path.ends_with("26.3\\version.json")
        );
    }

    #[test]
    fn a_version_without_a_client_is_not_installed() {
        let (_dir, paths, storage) = setup();
        storage.versions().upsert(&meta("1.0")).unwrap();
        storage.versions().set_installed("1.0", true).unwrap();
        write_version(&paths, "1.0", VERSION.replace("26.3", "1.0").as_str(), None);

        let report = rescan_versions(&storage, &paths).unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.installed, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(!storage.versions().is_installed("1.0").unwrap());
    }

    #[test]
    fn a_version_that_disappeared_is_marked_not_installed() {
        let (_dir, paths, storage) = setup();
        storage.versions().upsert(&meta("1.0")).unwrap();
        storage.versions().set_installed("1.0", true).unwrap();

        let report = rescan_versions(&storage, &paths).unwrap();

        assert_eq!(report.installed, 0);
        assert!(!storage.versions().is_installed("1.0").unwrap());
        assert!(storage.versions().get("1.0").unwrap().is_some());
    }

    #[test]
    fn a_broken_document_is_reported_and_does_not_stop_the_scan() {
        let (_dir, paths, storage) = setup();
        write_version(&paths, "broken", "{ not json", Some(b"jar"));
        write_version(&paths, "26.3", VERSION, Some(b"jar"));

        let report = rescan_versions(&storage, &paths).unwrap();

        assert_eq!(report.scanned, 2);
        assert_eq!(report.installed, 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].version_id, "broken");
        assert!(storage.versions().is_installed("26.3").unwrap());
    }
}
