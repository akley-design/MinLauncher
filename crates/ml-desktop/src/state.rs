//! The state every command works with.
//!
//! One [`AppState`] is created when the application starts and is handed
//! to every command through the managed state of Tauri. It owns the
//! storage layer, the paths and the platform the launcher runs on.
//!
//! The state is behind an `Arc` and never blocks: the storage layer
//! opens its own pool of connections, and the parts of the state that
//! are changed while the game runs are behind a lock of their own.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ml_core::{AppPaths, Platform};
use ml_minecraft::MinecraftService;
use ml_storage::Storage;
use parking_lot::{Mutex, RwLock};

use crate::error::{IpcError, IpcResult};
use crate::reconcile;

/// The paths and the service that go together.
///
/// They are replaced as a pair when the player moves the installation:
/// a service with the old paths next to the new paths would download
/// into the old folder and start the game from the new one.
#[derive(Debug, Clone)]
struct Runtime {
    paths: AppPaths,
    service: MinecraftService,
}

/// A guard that keeps the installation from being moved while it runs.
#[derive(Debug)]
pub struct ActivityGuard {
    activity: Arc<AtomicUsize>,
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.activity.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A guard that keeps other operations out while the root is changed.
#[derive(Debug)]
pub struct SwitchGuard {
    switching: Arc<AtomicBool>,
}

impl Drop for SwitchGuard {
    fn drop(&mut self) {
        self.switching.store(false, Ordering::SeqCst);
    }
}

/// The state the commands share.
#[derive(Debug)]
pub struct AppState {
    /// The storage layer.
    ///
    /// It is behind an `Arc` because the service that talks to Mojang
    /// needs a handle of its own, and opening a second database would
    /// mean a second pool of connections for the same file.
    storage: Arc<Storage>,
    /// The paths and the service of the current installation.
    runtime: RwLock<Runtime>,
    /// The platform the launcher runs on.
    platform: Platform,
    /// Why the configured installation directory could not be used.
    install_dir_error: Mutex<Option<String>>,
    /// How many installations or runtime downloads are running.
    activity: Arc<AtomicUsize>,
    /// Whether the installation root is being changed right now.
    switching: Arc<AtomicBool>,
    /// The game that is currently running, if any.
    running: Mutex<Option<crate::commands::launch::RunningGame>>,
    /// The flag that tells the watcher of the game that the player
    /// stopped it.
    ///
    /// A game the player closed is not a game that fell over, and the
    /// one that watches the game must not start it again.
    ///
    /// Every run gets a flag of its own. The flag of a run that has
    /// been replaced must not be touched by the run that came after it,
    /// and the watcher of a run reads the flag of that run only — that
    /// is what keeps a game that the player stopped from being started
    /// again by a run that came after it.
    current_run: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    /// Build the state from a set of paths.
    ///
    /// # Errors
    /// Fails when the directories cannot be created, the database cannot
    /// be opened, or the HTTP client cannot be built.
    pub fn new(paths: AppPaths, platform: Platform) -> IpcResult<Self> {
        let storage = Arc::new(Storage::open(&paths).map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot open the storage of the launcher",
                error.to_string(),
            )
        })?);
        let config = storage.load_settings().unwrap_or_default();

        let service =
            MinecraftService::new(paths.clone(), Arc::clone(&storage)).map_err(|error| {
                IpcError::with_detail(
                    crate::error::ErrorCode::Network,
                    "cannot create the http client of the launcher",
                    error.to_string(),
                )
            })?;
        service.set_download_concurrency(config.downloads.max_concurrent);

        Ok(Self {
            storage,
            runtime: RwLock::new(Runtime { paths, service }),
            platform,
            install_dir_error: Mutex::new(None),
            activity: Arc::new(AtomicUsize::new(0)),
            switching: Arc::new(AtomicBool::new(false)),
            running: Mutex::new(None),
            current_run: Mutex::new(None),
        })
    }

    /// Build the state from the paths the operating system suggests.
    ///
    /// The installation directory chosen by the player is applied here,
    /// before anything else can use the paths. A directory that cannot
    /// be used does not stop the launcher: the historical layout takes
    /// over, the reason is remembered for the settings screen, and the
    /// database is brought in line with the layout that is really in use.
    ///
    /// # Errors
    /// Fails when the directories cannot be found or the storage cannot
    /// be opened.
    pub fn discover(platform: Platform) -> IpcResult<Self> {
        let base = AppPaths::discover().map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot find the directories of the launcher",
                error.to_string(),
            )
        })?;
        let storage = Arc::new(Storage::open(&base).map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot open the storage of the launcher",
                error.to_string(),
            )
        })?);
        let config = storage.load_settings().unwrap_or_default();

        let (paths, install_dir_error) = resolve_install_root(&base, config.install_dir.trim());
        paths.ensure_dirs().map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot create the directories of the launcher",
                error.to_string(),
            )
        })?;

        let service =
            MinecraftService::new(paths.clone(), Arc::clone(&storage)).map_err(|error| {
                IpcError::with_detail(
                    crate::error::ErrorCode::Network,
                    "cannot create the http client of the launcher",
                    error.to_string(),
                )
            })?;
        service.set_download_concurrency(config.downloads.max_concurrent);

        let state = Self {
            storage,
            runtime: RwLock::new(Runtime {
                paths: paths.clone(),
                service,
            }),
            platform,
            install_dir_error: Mutex::new(install_dir_error),
            activity: Arc::new(AtomicUsize::new(0)),
            switching: Arc::new(AtomicBool::new(false)),
            running: Mutex::new(None),
            current_run: Mutex::new(None),
        };
        state.reconcile_after_start(&config);
        Ok(state)
    }

    /// The storage layer.
    #[must_use]
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// A copy of the storage handle, for work that runs on another thread.
    #[must_use]
    pub fn storage_handle(&self) -> Arc<Storage> {
        Arc::clone(&self.storage)
    }

    /// The paths the launcher uses.
    #[must_use]
    pub fn paths(&self) -> AppPaths {
        self.runtime.read().paths.clone()
    }

    /// The platform the launcher runs on.
    #[must_use]
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// The service that knows about the metadata of the game.
    #[must_use]
    pub fn service(&self) -> MinecraftService {
        self.runtime.read().service.clone()
    }

    /// A copy of the service, which can be moved into a task.
    #[must_use]
    pub fn service_clone(&self) -> MinecraftService {
        self.runtime.read().service.clone()
    }

    /// Apply the saved download concurrency to the current service.
    pub fn set_download_concurrency(&self, concurrency: usize) {
        self.runtime
            .read()
            .service
            .set_download_concurrency(concurrency);
    }

    /// Why the configured installation directory is not in use.
    #[must_use]
    pub fn install_dir_error(&self) -> Option<String> {
        self.install_dir_error.lock().clone()
    }

    /// Whether the installation directory is being changed right now.
    #[must_use]
    pub fn is_switching(&self) -> bool {
        self.switching.load(Ordering::SeqCst)
    }

    /// Reserve the right to work with the current installation.
    ///
    /// An installation or a runtime download may not run while the root
    /// is changed: the work would continue in the old folder after the
    /// settings already name the new one.
    ///
    /// # Errors
    /// Fails while the installation root is being changed.
    pub fn begin_activity(&self) -> IpcResult<ActivityGuard> {
        if self.is_switching() {
            return Err(IpcError::invalid(
                "the installation folder is being changed",
            ));
        }
        self.activity.fetch_add(1, Ordering::SeqCst);
        if self.is_switching() {
            self.activity.fetch_sub(1, Ordering::SeqCst);
            return Err(IpcError::invalid(
                "the installation folder is being changed",
            ));
        }
        Ok(ActivityGuard {
            activity: Arc::clone(&self.activity),
        })
    }

    /// Take the exclusive right to change the installation root.
    ///
    /// # Errors
    /// Fails when the game runs, another change is under way, or an
    /// installation is in progress.
    pub fn begin_switch(&self) -> IpcResult<SwitchGuard> {
        if self.running().is_some() {
            return Err(IpcError::invalid(
                "the game is running: close it before changing the installation folder",
            ));
        }
        if self
            .switching
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(IpcError::invalid(
                "the installation folder is already being changed",
            ));
        }
        if self.activity.load(Ordering::SeqCst) != 0 {
            self.switching.store(false, Ordering::SeqCst);
            return Err(IpcError::invalid(
                "an installation is in progress: wait for it to finish before changing the installation folder",
            ));
        }
        Ok(SwitchGuard {
            switching: Arc::clone(&self.switching),
        })
    }

    /// Replace the paths and the service after the root has been changed.
    ///
    /// The database is rebuilt for the new folder: every absolute path it
    /// holds describes the old folder, and the versions that are really
    /// in the new one are scanned in.
    ///
    /// # Errors
    /// Fails when the database cannot be reset or scanned.
    pub fn apply_transition(
        &self,
        paths: AppPaths,
        service: MinecraftService,
    ) -> IpcResult<reconcile::ScanReport> {
        let config = self.storage.load_settings().unwrap_or_default();
        service.set_download_concurrency(config.downloads.max_concurrent);
        *self.runtime.write() = Runtime { paths, service };
        *self.install_dir_error.lock() = None;

        let paths = self.paths();
        self.storage.forget_local_paths().map_err(IpcError::from)?;
        let report = reconcile::rescan_versions(&self.storage, &paths).map_err(IpcError::from)?;
        self.storage
            .meta_set("install_root.applied", &root_key(&paths))
            .map_err(IpcError::from)?;

        Ok(report)
    }

    /// Bring the database in line with the folder that is actually used.
    fn reconcile_after_start(&self, config: &ml_core::LauncherConfig) {
        let paths = self.paths();
        let key = root_key(&paths);
        let applied = self.storage.meta_get("install_root.applied").ok().flatten();

        if applied.as_deref() == Some(key.as_str()) {
            return;
        }

        let configured = !config.install_dir.trim().is_empty();
        if applied.is_some() || configured || self.install_dir_error().is_some() {
            if let Err(error) = self.storage.forget_local_paths() {
                tracing::warn!("cannot forget the local paths of the previous root: {error}");
            }
            match reconcile::rescan_versions(&self.storage, &paths) {
                Ok(report) => {
                    if !report.failed.is_empty() {
                        tracing::warn!(
                            "the scan of the installation found {} unreadable versions",
                            report.failed.len()
                        );
                    }
                }
                Err(error) => tracing::warn!("cannot scan the installation: {error}"),
            }
        }

        if let Err(error) = self.storage.meta_set("install_root.applied", &key) {
            tracing::warn!("cannot remember the installation root: {error}");
        }
    }

    /// The game that is running, if any.
    #[must_use]
    pub fn running(&self) -> Option<crate::commands::launch::RunningGame> {
        self.running.lock().clone()
    }

    /// Remember that a game was started.
    pub fn set_running(&self, game: Option<crate::commands::launch::RunningGame>) {
        *self.running.lock() = game;
    }

    /// Begin a run of the game and return the flag of that run.
    ///
    /// The flag starts out false: a game that has just been started was
    /// not stopped by the player.
    pub fn begin_run(&self) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        *self.current_run.lock() = Some(Arc::clone(&flag));
        flag
    }

    /// Remember that the player stopped the game that is running.
    ///
    /// The flag of the run that is running now is set, and only that
    /// one: a run that has already ended keeps the answer it had.
    pub fn stop_current_run(&self) {
        if let Some(flag) = self.current_run.lock().as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// The layout and the error to show, for the settings as they are.
///
/// A chosen directory that cannot be used — an unplugged disk, a folder
/// that became a file, a path that is not absolute — must not keep the
/// launcher from starting: the launcher opens on the historical layout,
/// tells the player what is wrong, and keeps the choice in the settings
/// so that the files are found again when the disk returns.
fn resolve_install_root(base: &AppPaths, requested: &str) -> (AppPaths, Option<String>) {
    match configured_paths(base, requested) {
        Ok(paths) => (paths, None),
        Err(error) => (base.rebase_install_root(None), Some(error.to_string())),
    }
}

/// The layout named by the settings, with the historical one as a fallback.
fn configured_paths(base: &AppPaths, requested: &str) -> IpcResult<AppPaths> {
    if requested.is_empty() {
        return Ok(base.rebase_install_root(None));
    }

    let path = PathBuf::from(requested);
    if !path.is_absolute() {
        return Err(IpcError::invalid(format!(
            "the installation directory `{requested}` is not an absolute path"
        )));
    }
    if path.exists() && !path.is_dir() {
        return Err(IpcError::invalid(format!(
            "the installation directory `{requested}` is a file"
        )));
    }
    if path.parent().is_none() {
        return Err(IpcError::invalid(format!(
            "`{requested}` is a drive root; choose a folder inside it"
        )));
    }

    std::fs::create_dir_all(&path).map_err(|error| {
        IpcError::with_detail(
            crate::error::ErrorCode::Storage,
            "cannot create the installation directory",
            format!("{}: {error}", path.display()),
        )
    })?;
    let absolute = std::path::absolute(&path).map_err(|error| {
        IpcError::with_detail(
            crate::error::ErrorCode::Storage,
            "cannot resolve the installation directory",
            format!("{}: {error}", path.display()),
        )
    })?;

    let paths = base.rebase_install_root(Some(&absolute));
    paths.ensure_dirs().map_err(|error| {
        IpcError::with_detail(
            crate::error::ErrorCode::Storage,
            "cannot create the game directories",
            error.to_string(),
        )
    })?;
    Ok(paths)
}

/// The identity of the installation root, for the applied-root bookmark.
fn root_key(paths: &AppPaths) -> String {
    let root = paths
        .install_dir
        .clone()
        .unwrap_or_else(|| paths.data_dir.clone());
    let text = root.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// The state as it is stored in the managed state of Tauri.
pub type SharedState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::OsArch;

    fn state() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(ml_core::OsName::Windows, OsArch::X86_64);
        let state = AppState::new(paths, platform).unwrap();
        (dir, state)
    }

    #[test]
    fn the_state_opens_the_storage() {
        let (_dir, state) = state();
        assert!(state.storage().versions().count().unwrap() >= 0);
        assert!(state.paths().database_path.is_file());
    }

    #[test]
    fn the_platform_is_kept() {
        let (_dir, state) = state();
        assert_eq!(state.platform().os, ml_core::OsName::Windows);
    }

    #[test]
    fn no_game_is_running_at_the_start() {
        let (_dir, state) = state();
        assert!(state.running().is_none());

        state.set_running(None);
        assert!(state.running().is_none());
    }

    #[test]
    fn every_run_gets_a_flag_of_its_own() {
        let (_dir, state) = state();

        let first = state.begin_run();
        state.stop_current_run();
        assert!(first.load(Ordering::Relaxed), "the run was stopped");

        // The run that came after it does not inherit the answer of the
        // one before it, and the old run keeps the answer it had.
        let second = state.begin_run();
        assert!(
            !second.load(Ordering::Relaxed),
            "the run that came after was not stopped"
        );
        assert!(
            first.load(Ordering::Relaxed),
            "the old run keeps its answer"
        );
    }

    #[test]
    fn the_installation_root_can_be_replaced() {
        let (dir, state) = state();
        let new_paths = state
            .paths()
            .rebase_install_root(Some(&dir.path().join("game")));
        new_paths.ensure_dirs().unwrap();
        let service = MinecraftService::new(new_paths.clone(), Arc::clone(&state.storage)).unwrap();

        state.apply_transition(new_paths.clone(), service).unwrap();

        assert_eq!(state.paths(), new_paths);
        assert_eq!(state.paths().install_dir, Some(dir.path().join("game")));
        assert_eq!(
            state.storage().meta_get("install_root.applied").unwrap(),
            Some(root_key(&new_paths))
        );
    }

    #[test]
    fn an_installation_and_a_root_change_exclude_each_other() {
        let (_dir, state) = state();

        let activity = state.begin_activity().unwrap();
        assert!(state.begin_switch().is_err(), "a switch slipped in");
        drop(activity);

        let switching = state.begin_switch().unwrap();
        assert!(state.begin_activity().is_err(), "an install slipped in");
        drop(switching);

        assert!(state.begin_activity().is_ok());
    }

    #[test]
    fn a_running_game_blocks_the_root_change() {
        let (_dir, state) = state();
        state.set_running(Some(crate::commands::launch::RunningGame {
            pid: Some(1),
            version_id: "26.3".into(),
            command: "java".into(),
            started_at: 0,
        }));

        assert!(state.begin_switch().is_err());
        state.set_running(None);
        assert!(state.begin_switch().is_ok());
    }

    #[test]
    fn a_configured_directory_becomes_the_game_root() {
        let dir = tempfile::tempdir().unwrap();
        let base = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let requested = dir.path().join("game");

        let paths = configured_paths(&base, &requested.to_string_lossy()).unwrap();

        assert_eq!(paths.versions_dir, requested.join("versions"));
        assert_eq!(paths.assets_dir, requested.join("assets"));
        assert_eq!(paths.libraries_dir, requested.join("libraries"));
        assert_eq!(paths.jre_dir, requested.join("runtimes"));
        assert_eq!(paths.instances_dir, base.instances_dir);
        assert!(paths.versions_dir.is_dir());
    }

    #[test]
    fn a_directory_that_cannot_be_made_falls_back_to_the_historical_layout() {
        let dir = tempfile::tempdir().unwrap();
        let base = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let file = dir.path().join("not-a-folder");
        std::fs::write(&file, b"x").unwrap();

        let (paths, error) = resolve_install_root(&base, &file.join("games").to_string_lossy());

        // The launcher must start even when the folder it was pointed at
        // cannot be made: the error is shown, and the game files are taken
        // from the historical layout.
        assert!(error.is_some(), "a directory under a file cannot be made");
        assert_eq!(paths.versions_dir, base.versions_dir);
    }

    #[test]
    fn a_relative_directory_falls_back_to_the_historical_layout() {
        let dir = tempfile::tempdir().unwrap();
        let base = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        let (paths, error) = resolve_install_root(&base, "games");

        assert!(
            error.unwrap().contains("not an absolute path"),
            "the player has to be told why the folder is not used"
        );
        assert_eq!(paths.versions_dir, base.versions_dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_root_falls_back_to_the_historical_layout() {
        let dir = tempfile::tempdir().unwrap();
        let base = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        let (paths, error) = resolve_install_root(&base, "C:\\");

        assert!(error.unwrap().contains("drive root"));
        assert_eq!(paths.versions_dir, base.versions_dir);
    }
}
