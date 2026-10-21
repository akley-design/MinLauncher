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
//! * [`mod_meta`] — the manifest of a mod and the versions it asks for.
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
pub mod corpus;
pub mod download;
pub mod fabric;
pub mod install;
pub mod jre;
pub mod library;
pub mod manifest;
pub mod mod_meta;
pub mod version;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ml_core::{AppPaths, Error, Platform, Result};
use ml_storage::{DownloadEntry, DownloadState, DownloadUpdate, Storage};

pub use assets::{
    AssetIndex, AssetLayout, AssetObject, AssetRoots, AssetStore, LEGACY_ASSET_INDEX,
    PRE_1_6_ASSET_INDEX, layout_of, plain_tree_root,
};
pub use download::{
    BatchResult, DownloadOutcome, DownloadProgress, DownloadTask, Downloader, ProgressCallback,
};
pub use fabric::{
    FabricArguments, FabricLaunch, FabricLauncherMeta, FabricLibrary, FabricLoader,
    FabricLoaderEntry, FabricLoaderList, FabricMetaLibraries, FabricMetaMainClass, FabricProfile,
};
pub use install::{FileKind, InstallLayout, InstallPlan, PlannedFile};
pub use jre::{PlannedRuntimeFile, RuntimeEntry, RuntimeFileList, RuntimeManifest, RuntimePlan};
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
            .ok_or_else(|| {
                Error::InvalidInput(format!(
                    "there is no runtime `{component}` for this platform"
                ))
            })?;

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
        jre::plan_runtime(&list, &runtime_dir, component)
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
        self.install_runtime_with_progress(component, Arc::new(|_| {}))
            .await
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
        // The runtime is a task of its own: its files are not the files
        // of a version, and the screen of the queue tells them apart.
        let task = format!("runtime:{}", plan.component);
        let result = self
            .download_and_record(&task, jre::runtime_tasks(plan), on_progress)
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

    /// The directory the plain tree of a version lives in.
    ///
    /// The versions before 1.7 read their assets from an ordinary tree
    /// of files and not from the content-addressed store. The tree of
    /// the versions from 1.6.4 to 1.7.2 is the directory the game is
    /// given as `--assetsDir`, and the tree of the older ones is
    /// `~/.minecraft/resources` — the only place the game of 1.5.2 reads
    /// them from.
    ///
    /// There is no tree for a version that reads the store.
    #[must_use]
    pub fn plain_tree_root(&self, version: &VersionJson) -> Option<PathBuf> {
        let id = version.asset_index_id()?;
        assets::plain_tree_root(&self.paths, id)
    }

    /// Materialise the plain tree of a version.
    ///
    /// The tree is a view of the store: every file of the index is
    /// linked into it, so that the game finds its sounds, its music and
    /// its icons. A version that reads the store needs no tree, and the
    /// number returned is then zero.
    ///
    /// # Errors
    /// Fails if the tree cannot be written.
    pub fn expand_plain_tree(&self, version: &VersionJson, index: &AssetIndex) -> Result<usize> {
        let Some(root) = self.plain_tree_root(version) else {
            return Ok(0);
        };
        let store = AssetStore::new(self.paths.clone());
        store.expand_plain_tree(index, &root)
    }

    /// Download every file of a plan.
    ///
    /// # Errors
    /// Fails only when the batch cannot be started; individual failures are
    /// reported in the result.
    pub async fn install(&self, plan: &InstallPlan) -> Result<BatchResult> {
        self.download_and_record(&plan.version_id, plan.to_tasks(), Arc::new(|_| {}))
            .await
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
        self.download_and_record(&plan.version_id, plan.to_tasks(), on_progress)
            .await
    }

    /// Download a batch of files and record what happened in the queue.
    ///
    /// The queue is a note about the download, not the download itself: a
    /// database that cannot be written is logged and ignored, because the
    /// user is waiting for the files, not for the record.
    ///
    /// The name of the task is what the queue groups the files by: the
    /// screen of the queue shows one row per task instead of one row per
    /// file.
    async fn download_and_record(
        &self,
        task: &str,
        tasks: Vec<DownloadTask>,
        on_progress: ProgressCallback,
    ) -> Result<BatchResult> {
        self.start_queue(task, &tasks);

        let result = self
            .downloader
            .download_all_with_progress(tasks, on_progress)
            .await?;

        self.settle_queue(&result);
        Ok(result)
    }

    /// Put a batch of tasks into the queue and move them to work.
    fn start_queue(&self, task: &str, tasks: &[DownloadTask]) {
        let entries: Vec<DownloadEntry> =
            tasks.iter().map(|file| queue_entry(file, task)).collect();
        if let Err(error) = self.storage.downloads().enqueue_all(&entries) {
            tracing::warn!("cannot write the download queue: {error}");
        }

        // The engine fetches the files at the same time, so there is no
        // point in telling apart the ones that wait for a slot from the
        // ones that are already in flight.
        let updates: Vec<DownloadUpdate> = tasks
            .iter()
            .map(|task| DownloadUpdate {
                url: task.url.clone(),
                state: DownloadState::Downloading,
                error: None,
                bytes_done: None,
                attempts: None,
            })
            .collect();
        if let Err(error) = self.storage.downloads().update_all(&updates) {
            tracing::warn!("cannot mark the downloads as running: {error}");
        }
    }

    /// Move the entries of a finished batch to their final state.
    fn settle_queue(&self, result: &BatchResult) {
        let mut updates: Vec<DownloadUpdate> =
            Vec::with_capacity(result.completed.len() + result.failed.len());

        for outcome in &result.completed {
            updates.push(DownloadUpdate {
                url: outcome.url.clone(),
                state: DownloadState::Completed,
                error: None,
                // The size on the disk is the truth; the size the server
                // promised is only a guess.
                bytes_done: Some(file_size(&outcome.target)),
                attempts: Some(i64::from(outcome.attempts)),
            });
        }

        for (url, reason) in &result.failed {
            updates.push(DownloadUpdate {
                url: url.clone(),
                state: DownloadState::Failed,
                error: Some(reason.clone()),
                bytes_done: None,
                attempts: None,
            });
        }

        if let Err(error) = self.storage.downloads().update_all(&updates) {
            tracing::warn!("cannot settle the download queue: {error}");
        }
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

/// The queue entry of a task that is about to be fetched.
///
/// The repository writes the timestamp itself, so the field is left empty.
fn queue_entry(file: &DownloadTask, task: &str) -> DownloadEntry {
    DownloadEntry {
        url: file.url.clone(),
        sha1: file.sha1.clone(),
        size: file.size,
        target_path: file.target.to_string_lossy().into_owned(),
        state: DownloadState::Queued,
        bytes_done: 0,
        attempts: 0,
        error: None,
        updated_at: String::new(),
        task: Some(task.to_string()),
    }
}

/// The size of a file on the disk, or zero when it is not there.
///
/// The size on the disk is the truth about a file: what the server
/// promised may be wrong, and a file that was only half fetched is
/// bigger than nothing and smaller than the whole.
#[must_use]
pub fn file_size(path: &Path) -> i64 {
    std::fs::metadata(path).map_or(0, |metadata| {
        i64::try_from(metadata.len()).unwrap_or(i64::MAX)
    })
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

    #[test]
    fn the_queue_remembers_a_finished_download() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(&dir);
        let target = dir.path().join("payload.jar");
        std::fs::write(&target, b"0123456789").unwrap();

        let task = DownloadTask::new("https://example.invalid/payload.jar", &target, None);
        service.start_queue("26.3", std::slice::from_ref(&task));

        let entry = service
            .storage()
            .downloads()
            .get(&task.url)
            .unwrap()
            .unwrap();
        assert_eq!(entry.state, DownloadState::Downloading);
        assert_eq!(entry.target_path, target.to_string_lossy());
        assert_eq!(
            entry.task.as_deref(),
            Some("26.3"),
            "the entry belongs to the task it was queued for"
        );

        service.settle_queue(&BatchResult {
            completed: vec![DownloadOutcome {
                url: task.url.clone(),
                target: target.clone(),
                bytes: 10,
                skipped: false,
                attempts: 1,
            }],
            failed: Vec::new(),
            bytes: 10,
        });

        let entry = service
            .storage()
            .downloads()
            .get(&task.url)
            .unwrap()
            .unwrap();
        assert_eq!(entry.state, DownloadState::Completed);
        assert_eq!(entry.bytes_done, 10, "the size on the disk");
        assert_eq!(entry.attempts, 1);
    }

    #[test]
    fn the_queue_remembers_a_failed_download() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(&dir);
        let target = dir.path().join("payload.jar");

        let task = DownloadTask::new("https://example.invalid/payload.jar", &target, None);
        service.start_queue("26.3", std::slice::from_ref(&task));
        service.settle_queue(&BatchResult {
            completed: Vec::new(),
            failed: vec![(task.url.clone(), "the network is down".to_string())],
            bytes: 0,
        });

        let entry = service
            .storage()
            .downloads()
            .get(&task.url)
            .unwrap()
            .unwrap();
        assert_eq!(entry.state, DownloadState::Failed);
        assert_eq!(entry.error.as_deref(), Some("the network is down"));
    }
}
