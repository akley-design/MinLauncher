//! The install planner: turning a version into a list of files to fetch.
//!
//! The planner is the piece that decides *what* has to be on disk for a
//! version to start:
//!
//! * the client jar,
//! * the libraries, with their natives,
//! * every asset object the asset index refers to.
//!
//! It also decides where each file goes, and — for the assets — it uses
//! the content-addressed store, so an object that several versions share is
//! fetched once. That single decision is worth more than any amount of
//! download tuning when a user installs a second version.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ml_core::{AppPaths, Error, Platform, Result};
use ml_storage::{LibraryEntry, Storage};
use serde::{Deserialize, Serialize};

use crate::assets::{ASSET_HOST, AssetIndex, asset_url};
use crate::download::{DownloadTask, hash_file, matches_hash};
use crate::library::{ResolvedLibrary, natives_directory_name};
use crate::version::VersionJson;

/// A file that has to be present for a version to start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFile {
    /// Absolute URL of the payload.
    pub url: String,
    /// Where the payload belongs.
    pub target: PathBuf,
    /// Expected SHA-1, if known.
    pub sha1: Option<String>,
    /// Expected size in bytes, if known.
    pub size: Option<i64>,
    /// Which part of the installation the file belongs to.
    pub kind: FileKind,
}

/// The part of an installation a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// The client jar of the game.
    Client,
    /// A library jar.
    Library,
    /// A native library, extracted from a jar.
    Native,
    /// An asset object.
    Asset,
    /// The asset index document.
    AssetIndex,
}

/// The complete plan for installing one version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
    /// The version the plan belongs to.
    pub version_id: String,
    /// The files that have to be fetched.
    pub files: Vec<PlannedFile>,
    /// The libraries the plan resolved to.
    pub libraries: Vec<ResolvedLibrary>,
    /// The total number of bytes the plan needs.
    pub total_size: i64,
}

impl InstallPlan {
    /// The number of files that have to be fetched.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the plan is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The download tasks for the plan.
    ///
    /// Files that are already on disk with the expected size are skipped
    /// by the download engine, so the plan can be replayed cheaply.
    #[must_use]
    pub fn to_tasks(&self) -> Vec<DownloadTask> {
        self.files
            .iter()
            .map(|file| {
                DownloadTask::new(file.url.clone(), file.target.clone(), file.sha1.clone())
                    .with_size(file.size)
            })
            .collect()
    }
}

/// Where the parts of an installation live.
#[derive(Debug, Clone)]
pub struct InstallLayout {
    /// The launcher's paths.
    pub paths: AppPaths,
    /// The platform the plan is built for.
    pub platform: Platform,
}

impl InstallLayout {
    /// Build a layout for the given paths and platform.
    #[must_use]
    pub fn new(paths: AppPaths, platform: Platform) -> Self {
        Self { paths, platform }
    }

    /// The directory the client jar goes into.
    #[must_use]
    pub fn version_dir(&self, version_id: &str) -> PathBuf {
        self.paths.version_dir(version_id)
    }

    /// The path of the client jar.
    #[must_use]
    pub fn client_jar(&self, version_id: &str) -> PathBuf {
        self.version_dir(version_id).join("client.jar")
    }

    /// The path of a library inside the library store.
    #[must_use]
    pub fn library_path(&self, relative: &str) -> PathBuf {
        self.paths.libraries_dir.join(relative)
    }

    /// The directory the natives of a version are extracted into.
    #[must_use]
    pub fn natives_dir(&self, version_id: &str) -> PathBuf {
        self.version_dir(version_id)
            .join("natives")
            .join(natives_directory_name(self.platform))
    }

    /// The path of an asset object.
    #[must_use]
    pub fn asset_path(&self, hash: &str) -> PathBuf {
        self.paths.asset_object(hash)
    }
}

/// Build the install plan for a version.
///
/// The plan only lists what is *missing* — files that are already on disk
/// with the expected size are left out, which is what makes a re-install
/// of an existing version nearly free.
///
/// # Errors
/// Fails if the version carries no client download or no asset index.
pub fn plan_install(
    version: &VersionJson,
    layout: &InstallLayout,
    asset_index: &AssetIndex,
) -> Result<InstallPlan> {
    let mut files = Vec::new();
    let mut libraries = version.resolve_libraries(layout.platform, Some(&version.id));

    // The client jar.
    if let Some(client) = &version.downloads.client {
        let target = layout.client_jar(&version.id);
        push_if_missing(
            &mut files,
            PlannedFile {
                url: client.url.clone(),
                target,
                sha1: Some(client.sha1.clone()),
                size: Some(client.size),
                kind: FileKind::Client,
            },
        );
    }

    // The libraries, including their natives.
    for library in &libraries {
        if let Some(relative) = library.artifact_path() {
            let target = layout.library_path(&relative);
            let download = library.artifact.as_ref();
            push_if_missing(
                &mut files,
                PlannedFile {
                    url: download.map_or_else(String::new, |file| file.url.clone()),
                    target,
                    sha1: download.map(|file| file.sha1.clone()),
                    size: download.map(|file| file.size),
                    kind: FileKind::Library,
                },
            );
        }

        for (classifier, download) in &library.natives {
            let Some(relative) = library.native_path(classifier) else {
                continue;
            };
            let target = layout.library_path(&relative);
            push_if_missing(
                &mut files,
                PlannedFile {
                    url: download.url.clone(),
                    target,
                    sha1: Some(download.sha1.clone()),
                    size: Some(download.size),
                    kind: FileKind::Native,
                },
            );
        }
    }

    // The assets. The same object can be referenced by many logical paths,
    // so the objects are deduplicated by their hash.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for object in asset_index.objects.values() {
        if !seen.insert(object.hash.clone()) {
            continue;
        }
        let target = layout.asset_path(&object.hash);
        push_if_missing(
            &mut files,
            PlannedFile {
                url: asset_url(ASSET_HOST, &object.hash),
                target,
                sha1: Some(object.hash.clone()),
                size: Some(object.size),
                kind: FileKind::Asset,
            },
        );
    }

    let total_size = files.iter().filter_map(|file| file.size).sum();

    Ok(InstallPlan {
        version_id: version.id.clone(),
        files,
        libraries: std::mem::take(&mut libraries),
        total_size,
    })
}

/// Add a file to the plan unless it is already on disk.
fn push_if_missing(files: &mut Vec<PlannedFile>, file: PlannedFile) {
    if let Ok(metadata) = std::fs::metadata(&file.target)
        && let Some(size) = file.size
        && metadata.len() == size.cast_unsigned()
    {
        return;
    }
    files.push(file);
}

/// Persist the resolved libraries of a version.
///
/// The libraries are stored *after* rule evaluation, so the launcher does
/// not have to re-resolve the Mojang rule language on every launch.
///
/// # Errors
/// Fails if the database write fails.
pub fn store_libraries(
    storage: &Storage,
    version_id: &str,
    libraries: &[ResolvedLibrary],
) -> Result<()> {
    let entries: Vec<LibraryEntry> = libraries
        .iter()
        .map(|library| {
            let artifact_path = library
                .artifact_path()
                .unwrap_or_else(|| library.coordinate.artifact_path());
            LibraryEntry {
                version_id: version_id.to_string(),
                name: library.name.clone(),
                classifier: library.coordinate.classifier.clone(),
                is_native: library.is_native,
                artifact_path,
                sha1: library.artifact.as_ref().map(|file| file.sha1.clone()),
                size: library.artifact.as_ref().map(|file| file.size),
                download_url: library.artifact.as_ref().map(|file| file.url.clone()),
                rules_json: library.rules_json.clone(),
            }
        })
        .collect();

    storage.libraries().replace_all(version_id, &entries)
}

/// Verify that a file on disk matches its expected hash.
///
/// # Errors
/// Fails if the file is missing or its hash does not match.
pub fn verify_file(path: &std::path::Path, expected: &str) -> Result<()> {
    if !path.is_file() {
        return Err(Error::InvalidInput(format!(
            "{} is missing",
            path.display()
        )));
    }
    if !matches_hash(path, expected) {
        let actual = hash_file(path)?;
        return Err(Error::ChecksumMismatch {
            path: path.to_path_buf(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{OsArch, OsName};

    const VERSION: &str = r#"{
        "id": "26.3",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "34",
        "assetIndex": {
            "id": "34",
            "sha1": "e787e8afd149a4cefa89d341b91f5aed8f5d93a0",
            "size": 597035,
            "totalSize": 484829952,
            "url": "https://piston-meta.mojang.com/v1/packages/e787e8afd149a4cefa89d341b91f5aed8f5d93a0/34.json"
        },
        "libraries": [
            {
                "name": "at.yawk.lz4:lz4-java:1.10.1",
                "downloads": {
                    "artifact": {
                        "path": "at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar",
                        "sha1": "f541d7f910fe3d76f38f799c507c48cc81b12ecb",
                        "size": 910232,
                        "url": "https://libraries.minecraft.net/at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
                    }
                }
            }
        ],
        "downloads": {
            "client": {
                "sha1": "e877b6a07acd633fb3bb475002175cec036e7b87",
                "size": 41483720,
                "url": "https://piston-data.mojang.com/v1/objects/e877b6a07acd633fb3bb475002175cec036e7b87/client.jar"
            }
        }
    }"#;

    const ASSETS: &str = r#"{
        "objects": {
            "minecraft/lang/en_us.json": {
                "hash": "5ff04807c356f1beed0b86ccf659b44b9983e3fa",
                "size": 781
            },
            "minecraft/lang/de_de.json": {
                "hash": "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356",
                "size": 9101
            }
        }
    }"#;

    fn layout(dir: &tempfile::TempDir) -> InstallLayout {
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        paths.ensure_dirs().unwrap();
        InstallLayout::new(paths, Platform::new(OsName::Windows, OsArch::X86_64))
    }

    #[test]
    fn plan_lists_client_libraries_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(&dir);
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = AssetIndex::parse(ASSETS.as_bytes()).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();

        // client + 1 library + 2 assets
        assert_eq!(plan.len(), 4);
        assert_eq!(plan.total_size, 41_483_720 + 910_232 + 781 + 9101);

        let kinds: BTreeSet<FileKind> = plan.files.iter().map(|file| file.kind).collect();
        assert!(kinds.contains(&FileKind::Client));
        assert!(kinds.contains(&FileKind::Library));
        assert!(kinds.contains(&FileKind::Asset));
    }

    #[test]
    fn plan_targets_the_right_directories() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(&dir);
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = AssetIndex::parse(ASSETS.as_bytes()).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();

        let client = plan
            .files
            .iter()
            .find(|file| file.kind == FileKind::Client)
            .unwrap();
        assert!(client.target.ends_with("26.3/client.jar"));

        let library = plan
            .files
            .iter()
            .find(|file| file.kind == FileKind::Library)
            .unwrap();
        assert!(
            library
                .target
                .ends_with("libraries/at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar")
        );
    }

    #[test]
    fn existing_files_are_left_out_of_the_plan() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(&dir);
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = AssetIndex::parse(ASSETS.as_bytes()).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();
        let client = plan
            .files
            .iter()
            .find(|file| file.kind == FileKind::Client)
            .unwrap()
            .target
            .clone();

        // Write a file of exactly the expected size.
        std::fs::create_dir_all(client.parent().unwrap()).unwrap();
        std::fs::write(&client, vec![0u8; 41_483_720]).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();
        assert_eq!(plan.len(), 3);
        assert!(!plan.files.iter().any(|file| file.kind == FileKind::Client));
    }

    #[test]
    fn assets_are_deduplicated_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(&dir);
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();

        // Two logical paths, one object.
        let assets = r#"{
            "objects": {
                "a": { "hash": "5ff04807c356f1beed0b86ccf659b44b9983e3fa", "size": 10 },
                "b": { "hash": "5ff04807c356f1beed0b86ccf659b44b9983e3fa", "size": 10 }
            }
        }"#;
        let index = AssetIndex::parse(assets.as_bytes()).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();
        let assets: Vec<&PlannedFile> = plan
            .files
            .iter()
            .filter(|file| file.kind == FileKind::Asset)
            .collect();
        assert_eq!(assets.len(), 1);
    }

    #[test]
    fn tasks_carry_the_hash_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(&dir);
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = AssetIndex::parse(ASSETS.as_bytes()).unwrap();

        let plan = plan_install(&version, &layout, &index).unwrap();
        let tasks = plan.to_tasks();
        assert_eq!(tasks.len(), plan.len());

        let client = tasks
            .iter()
            .find(|task| task.target.ends_with("client.jar"))
            .unwrap();
        assert_eq!(
            client.sha1.as_deref(),
            Some("e877b6a07acd633fb3bb475002175cec036e7b87")
        );
        assert_eq!(client.size, Some(41_483_720));
    }

    #[test]
    fn libraries_are_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let storage = Storage::open(&paths).unwrap();

        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let libraries =
            version.resolve_libraries(Platform::new(OsName::Windows, OsArch::X86_64), Some("26.3"));

        store_libraries(&storage, "26.3", &libraries).unwrap();
        let stored = storage.libraries().list("26.3").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "at.yawk.lz4:lz4-java:1.10.1");
        assert_eq!(
            stored[0].artifact_path,
            "at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
        );
    }

    #[test]
    fn verify_file_detects_a_wrong_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload");
        std::fs::write(&file, b"hello").unwrap();

        assert!(verify_file(&file, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d").is_ok());
        assert!(verify_file(&file, "0000000000000000000000000000000000000000").is_err());
        assert!(verify_file(&dir.path().join("missing"), "abc").is_err());
    }
}
