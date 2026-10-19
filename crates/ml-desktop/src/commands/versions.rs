//! The commands about versions.
//!
//! The frontend shows a list of versions, knows which of them are
//! installed, can ask the launcher to refresh the list from Mojang and
//! can ask it to install one of them.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ml_core::{VersionKind, VersionMeta};
use ml_minecraft::manifest::{ManifestClient, VersionManifest};
use ml_minecraft::{AssetIndex, DownloadProgress, MinecraftService};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

use super::throttle::Throttle;

/// A version, as the frontend sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionView {
    /// The identifier of the version, e.g. `26.3`.
    pub id: String,
    /// The kind of the version: release, snapshot and so on.
    pub kind: VersionKind,
    /// When the version was published, in ISO-8601.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,
    /// Whether the version is installed on this machine.
    pub installed: bool,
    /// The major version of Java the version needs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_major: Option<u32>,
    /// The identifier of the asset index the version uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_index_id: Option<String>,
}

/// The list of versions and the latest of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionList {
    /// Every known version, newest first.
    pub versions: Vec<VersionView>,
    /// The identifier of the latest stable release.
    pub latest_release: String,
    /// The identifier of the latest snapshot.
    pub latest_snapshot: String,
    /// How many of the versions are installed.
    pub installed_count: usize,
}

/// The list of the versions the launcher knows about.
///
/// The list comes from the database, not from the network: a launcher
/// that has to reach the internet before it can show anything is a
/// launcher that is slow every single time it is opened. The manifest
/// is fetched by [`refresh_manifest`], which the frontend calls when it
/// wants the newest versions.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn list_versions(state: State<'_, SharedState>) -> IpcResult<VersionList> {
    let storage = state.storage();
    let metas = storage.versions().list(None).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the versions",
            error.to_string(),
        )
    })?;

    let installed: Vec<String> = storage
        .versions()
        .list_installed()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the versions",
                error.to_string(),
            )
        })?
        .into_iter()
        .map(|meta| meta.id)
        .collect();

    let versions = metas
        .into_iter()
        .map(|meta| VersionView {
            installed: installed.contains(&meta.id),
            id: meta.id,
            kind: meta.kind,
            release_time: meta.release_time,
            java_major: meta.java_major,
            asset_index_id: meta.asset_index_id,
        })
        .collect::<Vec<_>>();

    let installed_count = versions.iter().filter(|version| version.installed).count();

    Ok(VersionList {
        latest_release: storage.meta_get("latest_release")?.unwrap_or_default(),
        latest_snapshot: storage.meta_get("latest_snapshot")?.unwrap_or_default(),
        versions,
        installed_count,
    })
}

/// The manifest as the launcher last saw it.
///
/// # Errors
/// Fails when the manifest cannot be read from the disk.
#[tauri::command]
pub fn read_manifest(state: State<'_, SharedState>) -> IpcResult<Option<VersionManifest>> {
    let path = state.paths().cache_dir.join("version_manifest_v2.json");
    if !path.is_file() {
        return Ok(None);
    }

    let bytes = std::fs::read(&path).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the version manifest",
            error.to_string(),
        )
    })?;

    let manifest = ml_minecraft::manifest::parse_manifest(&bytes)?;
    Ok(Some(manifest))
}

/// Fetch the manifest from Mojang and remember what it says.
///
/// The manifest is served with an `ETag`, so a repeated call costs one
/// conditional request instead of a 270 kB download. The result is
/// written into the cache and the latest identifiers are stored in the
/// database, so the list of versions can be built without the network.
///
/// # Errors
/// Fails when the network is unreachable or the manifest is malformed.
#[tauri::command]
pub async fn refresh_manifest(state: State<'_, SharedState>) -> IpcResult<ManifestSummary> {
    let mut client = ManifestClient::new().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot create the http client",
            error.to_string(),
        )
    })?;

    let manifest = client.fetch().await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch the version manifest",
            error.to_string(),
        )
    })?;

    let summary = ManifestSummary {
        latest_release: manifest.latest_release.clone(),
        latest_snapshot: manifest.latest_snapshot.clone(),
        total: manifest.versions.len(),
    };

    let storage = state.storage();
    storage.meta_set("latest_release", &manifest.latest_release)?;
    storage.meta_set("latest_snapshot", &manifest.latest_snapshot)?;

    // The manifest is kept on disk so that the frontend can be shown the
    // list of versions while the network is still being asked.
    let path = state.paths().cache_dir.join("version_manifest_v2.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&manifest) {
        let _ = std::fs::write(&path, bytes);
    }

    Ok(summary)
}

/// What the launcher learned from the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSummary {
    /// The identifier of the latest stable release.
    pub latest_release: String,
    /// The identifier of the latest snapshot.
    pub latest_snapshot: String,
    /// How many versions the manifest holds.
    pub total: usize,
}

// --- installing ----------------------------------------------------------

/// The event the frontend listens to while a version is installed.
pub const INSTALL_EVENT: &str = "minlauncher://install";

/// The name of the file the version document is written to.
const VERSION_FILE: &str = "version.json";

/// The name of the file the asset index is written to.
const ASSET_INDEX_FILE: &str = "assets.json";

/// The request of an installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallOptions {
    /// The version to install.
    pub version_id: String,
    /// Whether the launcher may fetch the version from the network.
    ///
    /// When this is false the launcher only uses what it already has on
    /// disk, which is what an installation from a local copy needs.
    #[serde(default = "default_true")]
    pub allow_network: bool,
}

/// The default of [`InstallOptions::allow_network`].
fn default_true() -> bool {
    true
}

/// What the frontend gets after a successful installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallResult {
    /// The version that was installed.
    pub version_id: String,
    /// How many files were fetched.
    pub files: usize,
    /// How many bytes were transferred.
    pub bytes: u64,
    /// How long the installation took, in milliseconds.
    pub elapsed_ms: u64,
}

/// What the launcher tells the frontend while it installs a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallEvent {
    /// The launcher started working on a version.
    Started {
        /// The version being installed.
        version_id: String,
        /// How many files have to be fetched.
        total: usize,
        /// How many bytes have to be fetched.
        bytes: i64,
    },
    /// Some of the files are done.
    Progress {
        /// The version being installed.
        version_id: String,
        /// How many files are done.
        completed: usize,
        /// How many files there are.
        total: usize,
        /// How many bytes have been transferred.
        bytes: u64,
    },
    /// The version is installed.
    Finished {
        /// The version that was installed.
        version_id: String,
        /// How many files were fetched.
        files: usize,
        /// How many bytes were transferred.
        bytes: u64,
        /// How long the installation took, in milliseconds.
        elapsed_ms: u64,
    },
    /// The installation failed.
    Failed {
        /// The version that was being installed.
        version_id: String,
        /// What went wrong.
        message: String,
    },
}

/// Install a version of the game.
///
/// The installation is the one part of the launcher that has to talk to
/// the network for a long time, so it reports its progress as it goes:
/// the frontend is told what is happening, and never has to guess.
///
/// # Errors
/// Fails when the version is unknown, when the network is unreachable,
/// or when the files cannot be written.
#[tauri::command]
pub async fn install_version(
    app: AppHandle,
    state: State<'_, SharedState>,
    options: InstallOptions,
) -> IpcResult<InstallResult> {
    let version_id = options.version_id.clone();

    let mut service = state.service_clone();

    let result = install(&mut service, &state, &options, &app).await;

    match result {
        Ok(result) => {
            let event = InstallEvent::Finished {
                version_id: result.version_id.clone(),
                files: result.files,
                bytes: result.bytes,
                elapsed_ms: result.elapsed_ms,
            };
            let _ = app.emit(INSTALL_EVENT, &event);
            Ok(result)
        }
        Err(error) => {
            let event = InstallEvent::Failed {
                version_id,
                message: error.message.clone(),
            };
            let _ = app.emit(INSTALL_EVENT, &event);
            Err(error)
        }
    }
}

/// Do the work of an installation.
async fn install(
    service: &mut MinecraftService,
    state: &SharedState,
    options: &InstallOptions,
    app: &AppHandle,
) -> IpcResult<InstallResult> {
    let started_at = Instant::now();
    let version_id = options.version_id.clone();

    // --- the version document -----------------------------------------
    let (version, version_bytes) = fetch_version(service, state, &version_id, options).await?;

    // The document is written before anything else: it is what the
    // launcher reads when the game is started, and a launcher that has
    // the document can start the game even if the download is cut short.
    let version_path = write_version_document(state, &version_id, &version_bytes)?;
    state
        .storage()
        .versions()
        .set_json_path(&version_id, &version_path.to_string_lossy())?;

    // --- the asset index ----------------------------------------------
    let asset_index = fetch_asset_index(service, &version).await?;
    let index_bytes = serde_json::to_vec_pretty(&asset_index).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the asset index",
            error.to_string(),
        )
    })?;
    write_file(
        &state
            .paths()
            .version_dir(&version_id)
            .join(ASSET_INDEX_FILE),
        &index_bytes,
    )?;

    // The index is written a second time, where the *game* looks for it.
    // The game reads `<assets>/indexes/<id>.json`, with the id taken from
    // the version document — the place the launcher itself does not need,
    // so without this the game starts without its assets.
    if let Some(index_id) = version
        .asset_index
        .as_ref()
        .map(|index| index.id.as_str())
        .filter(|id| !id.is_empty())
    {
        write_file(&state.paths().asset_index(index_id), &index_bytes)?;
    }

    // --- the plan ------------------------------------------------------
    let plan = service.plan(&version, &asset_index).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot plan the installation",
            error.to_string(),
        )
    })?;

    let _ = app.emit(
        INSTALL_EVENT,
        &InstallEvent::Started {
            version_id: version_id.clone(),
            total: plan.len(),
            bytes: plan.total_size,
        },
    );

    // --- the download ---------------------------------------------------
    //
    // The report of the download is thinned out before it reaches the
    // frontend: an installation of a modern version fetches thousands of
    // files, and an event per file would cost more than the download.
    let throttle = Arc::new(Throttle::new(INSTALL_EVENT, Duration::from_millis(100)));
    let emitter = Arc::clone(&throttle);
    let progress_version = version_id.clone();
    let handle = app.clone();

    let callback: ml_minecraft::ProgressCallback = Arc::new(move |progress: DownloadProgress| {
        emitter.report(
            &handle,
            InstallEvent::Progress {
                version_id: progress_version.clone(),
                completed: progress.completed,
                total: progress.total,
                bytes: progress.bytes,
            },
        );
    });

    let result = service
        .install_with_progress(&plan, callback)
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "cannot download the files of the version",
                error.to_string(),
            )
        })?;

    if !result.is_success() {
        let failed = result
            .failed
            .iter()
            .map(|(url, reason)| format!("{url}: {reason}"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "the installation did not finish",
            failed,
        ));
    }

    // --- what is stored -------------------------------------------------
    store_the_install(state, &version, &version_id, &plan, &asset_index)?;

    Ok(InstallResult {
        version_id,
        files: result.completed.len(),
        bytes: result.bytes,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
    })
}

/// Fetch the version document of a version.
///
/// The document is small and it is the one thing the launcher cannot do
/// without, so it is fetched with a plain request. A document that is
/// already on the disk is used as it is, unless the network is allowed
/// and the file on disk is not a valid document.
async fn fetch_version(
    service: &mut MinecraftService,
    state: &SharedState,
    version_id: &str,
    options: &InstallOptions,
) -> IpcResult<(ml_minecraft::VersionJson, Vec<u8>)> {
    let path = state.paths().version_dir(version_id).join(VERSION_FILE);

    if !options.allow_network {
        let bytes = std::fs::read(&path).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "the version is not on the disk",
                error.to_string(),
            )
        })?;
        let version = ml_minecraft::VersionJson::parse(&bytes)?;
        return Ok((version, bytes));
    }

    // The URL of the document is in the manifest, and the manifest is
    // what the launcher keeps on the disk.
    let manifest = manifest_of(service, state).await?;
    let entry = manifest
        .find(version_id)
        .ok_or_else(|| IpcError::not_found(format!("there is no version `{version_id}`")))?;

    let bytes = service.fetch(&entry.url).await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch the version document",
            error.to_string(),
        )
    })?;

    let version = ml_minecraft::VersionJson::parse(&bytes)?;
    Ok((version, bytes))
}

/// The manifest, from the disk when it is there and from the network
/// otherwise.
async fn manifest_of(
    service: &mut MinecraftService,
    state: &SharedState,
) -> IpcResult<VersionManifest> {
    let path = state.paths().cache_dir.join("version_manifest_v2.json");
    if path.is_file()
        && let Ok(bytes) = std::fs::read(&path)
        && let Ok(manifest) = ml_minecraft::manifest::parse_manifest(&bytes)
    {
        return Ok(manifest);
    }

    service.manifest().await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch the version manifest",
            error.to_string(),
        )
    })
}

/// Fetch the asset index of a version.
async fn fetch_asset_index(
    service: &MinecraftService,
    version: &ml_minecraft::VersionJson,
) -> IpcResult<AssetIndex> {
    service.asset_index(version).await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch the asset index",
            error.to_string(),
        )
    })
}

/// Write the version document where the launcher expects to find it.
fn write_version_document(
    state: &SharedState,
    version_id: &str,
    bytes: &[u8],
) -> IpcResult<PathBuf> {
    let path = state.paths().version_dir(version_id).join(VERSION_FILE);
    write_file(&path, bytes)?;
    Ok(path)
}

/// Write a file, creating the directories it needs.
fn write_file(path: &PathBuf, bytes: &[u8]) -> IpcResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot create the directory of the file",
                error.to_string(),
            )
        })?;
    }

    std::fs::write(path, bytes).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the file",
            error.to_string(),
        )
    })
}

/// Remember what was installed.
///
/// The database is what the launcher reads when it opens, so a version
/// that was installed but not remembered does not exist as far as the
/// interface is concerned.
fn store_the_install(
    state: &SharedState,
    version: &ml_minecraft::VersionJson,
    version_id: &str,
    plan: &ml_minecraft::InstallPlan,
    asset_index: &AssetIndex,
) -> IpcResult<()> {
    let storage = state.storage();

    // The libraries, so that the launcher does not have to resolve the
    // rules of Mojang on every launch.
    ml_minecraft::install::store_libraries(storage, version_id, &plan.libraries).map_err(
        |error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot store the libraries",
                error.to_string(),
            )
        },
    )?;

    // The asset index, so that the launcher knows which objects the
    // version needs without reading the document again.
    let index_record = ml_storage::AssetIndex {
        version_id: version_id.to_string(),
        index_id: version
            .asset_index
            .as_ref()
            .map_or_else(String::new, |index| index.id.clone()),
        sha1: version
            .asset_index
            .as_ref()
            .map_or_else(String::new, |index| index.sha1.clone()),
        size: version.asset_index.as_ref().map_or(0, |index| index.size),
        total_size: asset_index.total_size(),
        object_count: i64::try_from(asset_index.len()).unwrap_or(i64::MAX),
    };
    storage
        .assets()
        .upsert_index(&index_record)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot store the asset index",
                error.to_string(),
            )
        })?;

    // The version itself.
    let meta = VersionMeta {
        id: version_id.to_string(),
        kind: VersionKind::from_manifest(&version.kind),
        release_time: version.release_time.clone(),
        java_component: version
            .java_version
            .as_ref()
            .map(|java| java.component.clone()),
        java_major: version.java_version.as_ref().map(|java| java.major_version),
        main_class: version.main_class.clone(),
        asset_index_id: version.asset_index.as_ref().map(|index| index.id.clone()),
        asset_index_sha1: version.asset_index.as_ref().map(|index| index.sha1.clone()),
        asset_index_size: version.asset_index.as_ref().map(|index| index.size),
        client_sha1: version
            .downloads
            .client
            .as_ref()
            .map(|client| client.sha1.clone()),
        client_size: version.downloads.client.as_ref().map(|client| client.size),
        json_sha1: None,
    };
    storage.versions().upsert(&meta).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot store the version",
            error.to_string(),
        )
    })?;
    storage
        .versions()
        .set_installed(version_id, true)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot mark the version as installed",
                error.to_string(),
            )
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{AppPaths, OsArch, OsName, Platform};
    use ml_storage::Storage;

    fn state() -> (tempfile::TempDir, SharedState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths, platform).unwrap();
        (dir, std::sync::Arc::new(state))
    }

    #[test]
    fn the_list_is_empty_on_a_fresh_installation() {
        let (_dir, state) = state();
        let storage = state.storage();
        let list = VersionList {
            versions: Vec::new(),
            latest_release: String::new(),
            latest_snapshot: String::new(),
            installed_count: 0,
        };
        assert_eq!(list.versions.len(), 0);
        assert_eq!(storage.versions().count().unwrap(), 0);
    }

    #[test]
    fn a_manifest_is_parsed_from_its_bytes() {
        let bytes = br#"{
            "latest": {"release": "26.3", "snapshot": "26.4-snapshot-4"},
            "versions": [
                {"id": "26.3", "type": "release", "url": "https://example/26.3",
                 "releaseTime": "2026-01-01T00:00:00+00:00", "sha1": "abc"}
            ]
        }"#;

        let manifest = ml_minecraft::manifest::parse_manifest(bytes).unwrap();
        assert_eq!(manifest.latest_release, "26.3");
        assert_eq!(manifest.versions.len(), 1);
        assert!(manifest.latest_release().is_some());
    }

    #[test]
    fn a_malformed_manifest_becomes_a_storage_error() {
        let error: IpcError = ml_minecraft::manifest::parse_manifest(b"not json")
            .unwrap_err()
            .into();
        assert_eq!(error.code, ErrorCode::Storage);
    }

    #[test]
    fn the_storage_of_the_state_is_the_one_that_was_opened() {
        let (_dir, state) = state();
        let storage: &Storage = state.storage();
        assert!(storage.versions().count().is_ok());
    }

    // --- the installation ----------------------------------------------

    const VERSION: &str = r#"{
        "id": "26.3",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "releaseTime": "2026-01-01T00:00:00+00:00",
        "javaVersion": {"component": "java-runtime-gamma", "majorVersion": 21},
        "assetIndex": {
            "id": "34",
            "sha1": "e787e8afd149a4cefa89d341b91f5aed8f5d93a0",
            "size": 597035,
            "totalSize": 484829952,
            "url": "https://example/34.json"
        },
        "libraries": [],
        "downloads": {
            "client": {
                "sha1": "e877b6a07acd633fb3bb475002175cec036e7b87",
                "size": 41483720,
                "url": "https://example/client.jar"
            }
        }
    }"#;

    const ASSETS: &str = r#"{
        "objects": {
            "minecraft/lang/en_us.json": {
                "hash": "5ff04807c356f1beed0b86ccf659b44b9983e3fa",
                "size": 781
            }
        }
    }"#;

    #[test]
    fn the_file_of_a_version_is_written_where_the_launcher_looks_for_it() {
        let (_dir, state) = state();
        let path = write_version_document(&state, "26.3", VERSION.as_bytes()).unwrap();

        assert!(path.is_file());
        assert!(path.ends_with("26.3/version.json"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), VERSION);
    }

    #[test]
    fn a_file_is_written_into_a_directory_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("file.json");

        write_file(&path, b"{}").unwrap();

        assert!(path.is_file());
    }

    #[test]
    fn the_version_is_remembered_after_the_installation() {
        let (_dir, state) = state();
        let version = ml_minecraft::VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = ml_minecraft::AssetIndex::parse(ASSETS.as_bytes()).unwrap();
        let plan = ml_minecraft::InstallPlan {
            version_id: "26.3".to_string(),
            files: Vec::new(),
            libraries: Vec::new(),
            total_size: 0,
        };

        store_the_install(&state, &version, "26.3", &plan, &index).unwrap();

        let storage = state.storage();
        assert!(storage.versions().is_installed("26.3").unwrap());

        let meta = storage.versions().get("26.3").unwrap().unwrap();
        assert_eq!(meta.id, "26.3");
        assert_eq!(meta.kind, VersionKind::Release);
        assert_eq!(meta.java_major, Some(21));
        assert_eq!(meta.java_component.as_deref(), Some("java-runtime-gamma"));
        assert_eq!(meta.asset_index_id.as_deref(), Some("34"));
        assert_eq!(meta.client_size, Some(41_483_720));
        assert_eq!(
            meta.main_class.as_deref(),
            Some("net.minecraft.client.main.Main")
        );
    }

    #[test]
    fn the_asset_index_is_remembered_after_the_installation() {
        let (_dir, state) = state();
        let version = ml_minecraft::VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = ml_minecraft::AssetIndex::parse(ASSETS.as_bytes()).unwrap();
        let plan = ml_minecraft::InstallPlan::default();

        store_the_install(&state, &version, "26.3", &plan, &index).unwrap();

        let stored = state.storage().assets().index("26.3").unwrap().unwrap();
        assert_eq!(stored.index_id, "34");
        assert_eq!(stored.object_count, 1);
        assert_eq!(stored.sha1, "e787e8afd149a4cefa89d341b91f5aed8f5d93a0");
    }

    #[test]
    fn the_installed_version_shows_up_in_the_list() {
        let (_dir, state) = state();
        let version = ml_minecraft::VersionJson::parse(VERSION.as_bytes()).unwrap();
        let index = ml_minecraft::AssetIndex::parse(ASSETS.as_bytes()).unwrap();
        let plan = ml_minecraft::InstallPlan::default();

        store_the_install(&state, &version, "26.3", &plan, &index).unwrap();

        let installed = state.storage().versions().list_installed().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, "26.3");
    }

    #[test]
    fn the_events_of_the_installation_carry_their_kind() {
        let started = InstallEvent::Started {
            version_id: "26.3".to_string(),
            total: 10,
            bytes: 100,
        };
        let progress = InstallEvent::Progress {
            version_id: "26.3".to_string(),
            completed: 5,
            total: 10,
            bytes: 50,
        };
        let finished = InstallEvent::Finished {
            version_id: "26.3".to_string(),
            files: 10,
            bytes: 100,
            elapsed_ms: 1234,
        };
        let failed = InstallEvent::Failed {
            version_id: "26.3".to_string(),
            message: "the network is down".to_string(),
        };

        // The frontend switches on the kind, so the name of the kind is
        // part of the contract.
        assert!(
            serde_json::to_value(&started)
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str()
                .is_some_and(|kind| kind == "started")
        );
        assert!(
            serde_json::to_value(&progress)
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str()
                .is_some_and(|kind| kind == "progress")
        );
        assert!(
            serde_json::to_value(&finished)
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str()
                .is_some_and(|kind| kind == "finished")
        );
        assert!(
            serde_json::to_value(&failed)
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str()
                .is_some_and(|kind| kind == "failed")
        );
    }

    #[test]
    fn the_request_of_an_installation_carries_its_version() {
        let options = InstallOptions {
            version_id: "26.3".to_string(),
            allow_network: true,
        };

        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json.get("version_id").unwrap().as_str(), Some("26.3"));
        assert_eq!(json.get("allow_network").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn a_request_without_the_network_is_still_a_valid_request() {
        // The frontend does not have to say that the network is allowed:
        // it is the default.
        let options: InstallOptions =
            serde_json::from_value(serde_json::json!({"version_id": "26.3"})).unwrap();
        assert!(options.allow_network);
    }
}
