//! Minecraft metadata, assets, libraries and Java runtimes.
//!
//! This crate knows everything about *what* a Minecraft installation
//! consists of and *where* each piece comes from. It does not know how to
//! start the game — that is [`ml_launch`]'s job — and it does not know how
//! the data is stored — that is [`ml_storage`]'s job. It sits in between:
//!
//! ```text
//!   Mojang servers ──> manifest / version JSON / asset index / runtime manifest
//!          │
//!          v
//!   ml-minecraft ──> resolved libraries, asset objects, download plan
//!          │
//!          v
//!   ml-storage   ──> the database records of what is installed
//! ```
//!
//! # What lives where
//!
//! * [`manifest`] — the list of published versions.
//! * [`version`] — the document that describes one version, including the
//!   argument rules.
//! * [`library`] — Maven coordinates, artifact paths, native libraries.
//! * [`assets`] — the asset index and the content-addressed object store.
//! * [`jre`] — the Java runtime manifest.
//! * [`download`] — the download engine.
//! * [`install`] — the planner that turns a version into a list of files.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use ml_core::AppPaths;
//! use ml_minecraft::MinecraftService;
//! use ml_storage::Storage;
//!
//! # async fn run() -> ml_core::Result<()> {
//! let paths = AppPaths::discover()?;
//! let storage = Arc::new(Storage::open(&paths)?);
//! let mut service = MinecraftService::new(paths, storage)?;
//!
//! // Which versions exist?
//! let manifest = service.manifest().await?;
//! println!("{} versions, latest {}", manifest.versions.len(), manifest.latest_release);
//!
//! // What does the latest version need?
//! let version = service.version(&manifest, &manifest.latest_release).await?;
//! println!("{} libraries", version.libraries.len());
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod download;
pub mod fabric;
pub mod install;
pub mod jre;
pub mod library;
pub mod manifest;
pub mod version;

use std::sync::Arc;

use ml_core::{AppPaths, Error, Platform, Result};
use ml_storage::Storage;

pub use assets::{AssetIndex, AssetObject, AssetRoots, AssetStore};
pub use download::{
    BatchResult, DownloadOutcome, DownloadProgress, DownloadTask, Downloader, ProgressCallback,
};
pub use fabric::{
    FabricArguments, FabricLaunch, FabricLauncherMeta, FabricLibrary, FabricLoader,
    FabricLoaderEntry, FabricLoaderList, FabricMetaLibraries, FabricMetaMainClass, FabricProfile,
};
pub use install::{FileKind, InstallLayout, InstallPlan, PlannedFile};
pub use jre::{
    PlannedRuntimeFile, RuntimeEntry, RuntimeFileList, RuntimeManifest, RuntimePlan,
};
pub use library::{Coordinate, ResolvedLibrary};
pub use manifest::{ManifestClient, VersionManifest, VersionSummary};
pub use version::{Argument, ArgumentValue, Arguments, VersionJson};

/// The entry point of the crate.
///
/// The service owns the HTTP client, the paths and the storage handle, and
/// exposes the operations the rest of the launcher needs. Everything it
/// returns is either plain data or a [`Result`]; there is no shared mutable
/// state, so the service is cheap to clone and safe to use from several
/// tasks.
#[derive(Debug, Clone)]
pub struct MinecraftService {
    paths: AppPaths,
    platform: Platform,
    http: Arc<reqwest::Client>,
    storage: Arc<Storage>,
    manifest: ManifestClient,
    downloader: Downloader,
}

impl MinecraftService {
    /// Create a service for the given paths.
    ///
    /// The storage layer is opened by the caller, so that the Tauri layer
    /// can keep a single database handle.
    ///
    /// # Errors
    /// Fails if the HTTP client cannot be built.
    pub fn new(paths: AppPaths, storage: Arc<Storage>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::network("http client", e))?;

        let downloader = Downloader::new()?;
        let manifest = ManifestClient::new()?;

        Ok(Self {
            paths,
            platform: Platform::current(),
            http: Arc::new(http),
            storage,
            manifest,
            downloader,
        })
    }

    /// The launcher's paths.
    #[must_use]
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// The platform the launcher runs on.
    #[must_use]
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// The storage layer.
    #[must_use]
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// The HTTP client.
    #[must_use]
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The download engine.
    #[must_use]
    pub fn downloader(&self) -> &Downloader {
        &self.downloader
    }

    /// The manifest client.
    #[must_use]
    pub fn manifest_client(&self) -> &ManifestClient {
        &self.manifest
    }

    /// Fetch the version manifest.
    ///
    /// # Errors
    /// Fails on a transport error, a non-success status, or a malformed
    /// document.
    pub async fn manifest(&mut self) -> Result<VersionManifest> {
        self.manifest.fetch().await
    }

    /// Fetch the version JSON of a version.
    ///
    /// The URL comes from the manifest entry, because Mojang serves the
    /// document from a content-addressed path that cannot be derived from
    /// the version id.
    ///
    /// # Errors
    /// Fails when the version is unknown, on a transport error, or on a
    /// malformed document.
    pub async fn version(
        &self,
        manifest: &VersionManifest,
        version_id: &str,
    ) -> Result<VersionJson> {
        let entry = manifest
            .find(version_id)
            .ok_or_else(|| Error::UnknownVersion(version_id.to_string()))?;

        let bytes = self.fetch_bytes(&entry.url).await?;
        VersionJson::parse(&bytes)
    }

    /// Fetch the asset index of a version.
    ///
    /// # Errors
    /// Fails on a transport error or a malformed document.
    pub async fn asset_index(&self, version: &VersionJson) -> Result<AssetIndex> {
        let Some(index) = &version.asset_index else {
            return Err(Error::InvalidInput(format!(
                "version {} has no asset index",
                version.id
            )));
        };
        let bytes = self.fetch_bytes(&index.url).await?;
        AssetIndex::parse(&bytes)
    }

    /// Fetch the runtime manifest.
    ///
    /// # Errors
    /// Fails on a transport error or a malformed document.
    pub async fn runtime_manifest(&self) -> Result<RuntimeManifest> {
        let bytes = self.fetch_bytes(jre::JRE_MANIFEST_URL).await?;
        RuntimeManifest::parse(&bytes)
    }

    /// Fetch the file list of a Java runtime.
    ///
    /// The outer manifest names the document that holds the files; the
    /// document is fetched from there.
    ///
    /// # Errors
    /// Fails when the platform or the component is unknown, on a
    /// transport error, or on a malformed document.
    pub async fn runtime_files(&self, component: &str) -> Result<RuntimeFileList> {
        let manifest = self.runtime_manifest().await?;
        let entry = manifest
            .entry(&jre::platform_key(), component)
            .ok_or_else(|| Error::InvalidInput(format!(
                "there is no runtime `{component}` for this platform"
            )))?;

        let bytes = self.fetch_bytes(&entry.manifest.url).await?;
        RuntimeFileList::parse(&bytes)
    }

    /// Build the plan of a Java runtime.
    ///
    /// The plan says which files have to be fetched and where each one
    /// goes. It is built before the download, so that the caller can tell
    /// the user how much there is to fetch.
    ///
    /// # Errors
    /// Fails when the runtime does not exist for this platform, when the
    /// manifest is malformed, or when a path of the manifest is not safe.
    pub async fn runtime_plan(&self, component: &str) -> Result<RuntimePlan> {
        let list = self.runtime_files(component).await?;
        let runtime_dir = self.paths.jre_dir_for(component);
        jre::plan_runtime(&list, &runtime_dir)
    }

    /// Download a Java runtime and unpack it.
    ///
    /// The runtime ends up in the directory [`AppPaths::jre_dir_for`]
    /// names, laid out like a Java installation, so that `bin/java.exe`
    /// is where the game expects it.
    ///
    /// # Errors
    /// Fails when the runtime does not exist for this platform, when the
    /// manifest is malformed, or when the files cannot be fetched.
    pub async fn install_runtime(&self, component: &str) -> Result<BatchResult> {
        self.install_runtime_with_progress(component, Arc::new(|_| {})).await
    }

    /// Download a Java runtime, unpack it and report the progress.
    ///
    /// # Errors
    /// Fails when the runtime does not exist for this platform, when the
    /// manifest is malformed, or when the files cannot be fetched.
    pub async fn install_runtime_with_progress(
        &self,
        component: &str,
        on_progress: ProgressCallback,
    ) -> Result<BatchResult> {
        let plan = self.runtime_plan(component).await?;
        self.install_runtime_plan(&plan, on_progress).await
    }

    /// Download the files of a plan of a runtime and unpack it.
    ///
    /// # Errors
    /// Fails when the files cannot be fetched.
    pub async fn install_runtime_plan(
        &self,
        plan: &RuntimePlan,
        on_progress: ProgressCallback,
    ) -> Result<BatchResult> {
        let result = self
            .downloader
            .download_all_with_progress(jre::runtime_tasks(plan), on_progress)
            .await?;

        if !result.is_success() {
            return Err(Error::InvalidInput(format!(
                "the runtime did not download: {}",
                result
                    .failed
                    .iter()
                    .map(|(url, reason)| format!("{url}: {reason}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }

        jre::unpack_runtime(plan)?;
        Ok(result)
    }

    /// Fetch the list of the Fabric loaders that exist for a version of
    /// the game.
    ///
    /// # Errors
    /// Fails on a transport error or a malformed document.
    pub async fn fabric_loaders(&self, minecraft_version: &str) -> Result<FabricLoaderList> {
        let url = fabric::loader_list_url(minecraft_version);
        let bytes = self.fetch_bytes(&url).await?;
        FabricLoaderList::parse(&bytes)
    }

    /// Fetch the profile of a Fabric loader.
    ///
    /// The profile is the document that says which libraries the loader
    /// needs, which class the game is started with, and which class of the
    /// game the loader starts itself.
    ///
    /// # Errors
    /// Fails on a transport error or a malformed document.
    pub async fn fabric_profile(
        &self,
        minecraft_version: &str,
        loader_version: &str,
    ) -> Result<FabricProfile> {
        let url = fabric::profile_url(minecraft_version, loader_version);
        let bytes = self.fetch_bytes(&url).await?;
        FabricProfile::parse(&bytes)
    }

    /// Build the install plan of a version.
    ///
    /// # Errors
    /// Fails if the version carries no client download or no asset index.
    pub fn plan(&self, version: &VersionJson, asset_index: &AssetIndex) -> Result<InstallPlan> {
        let layout = InstallLayout::new(self.paths.clone(), self.platform);
        install::plan_install(version, &layout, asset_index)
    }

    /// Download every file of a plan.
    ///
    /// # Errors
    /// Fails only when the batch cannot be started; individual failures are
    /// reported in the result.
    pub async fn install(&self, plan: &InstallPlan) -> Result<BatchResult> {
        self.downloader.download_all(plan.to_tasks()).await
    }

    /// Download every file of a plan and report the progress.
    ///
    /// # Errors
    /// Fails only when the batch cannot be started; individual failures are
    /// reported in the result.
    pub async fn install_with_progress(
        &self,
        plan: &InstallPlan,
        on_progress: ProgressCallback,
    ) -> Result<BatchResult> {
        self.downloader
            .download_all_with_progress(plan.to_tasks(), on_progress)
            .await
    }

    /// Fetch a URL and return the body.
    ///
    /// The client of the service is the one the rest of the launcher
    /// uses, so a document that is fetched here shares the connection
    /// pool with the downloads.
    ///
    /// # Errors
    /// Fails on a transport error or a non-success status.
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        self.fetch_bytes(url).await
    }

    /// Fetch a URL and return the body.
    async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| Error::network(url, e))?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: response.status().as_u16(),
            });
        }

        let bytes = response.bytes().await.map_err(|e| Error::network(url, e))?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(dir: &tempfile::TempDir) -> MinecraftService {
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let storage = Arc::new(Storage::open(&paths).unwrap());
        MinecraftService::new(paths, storage).unwrap()
    }

    #[test]
    fn service_opens_the_storage() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(&dir);
        assert_eq!(service.storage().versions().count().unwrap(), 0);
    }

    #[test]
    fn service_reports_the_current_platform() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(&dir);
        assert_eq!(service.platform(), Platform::current());
    }

    #[test]
    fn service_is_cloneable() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(&dir);
        let clone = service.clone();
        assert_eq!(clone.paths().data_dir, service.paths().data_dir);
    }
}
