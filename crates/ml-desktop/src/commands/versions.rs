//! The commands about versions.
//!
//! The frontend shows a list of versions, knows which of them are
//! installed and can ask the launcher to refresh the list from Mojang.
//! These three commands are the whole of that.

use ml_core::VersionKind;
use ml_minecraft::manifest::{ManifestClient, VersionManifest};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

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
}
