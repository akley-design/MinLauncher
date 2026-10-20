//! The state every command works with.
//!
//! One [`AppState`] is created when the application starts and is handed
//! to every command through the managed state of Tauri. It owns the
//! storage layer, the paths and the platform the launcher runs on.
//!
//! The state is behind an `Arc` and never blocks: the storage layer
//! opens its own pool of connections, and the parts of the state that
//! are changed while the game runs are behind a lock of their own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ml_core::{AppPaths, Platform};
use ml_minecraft::MinecraftService;
use ml_storage::Storage;
use parking_lot::Mutex;

use crate::error::{IpcError, IpcResult};

/// The state the commands share.
#[derive(Debug)]
pub struct AppState {
    /// The storage layer.
    ///
    /// It is behind an `Arc` because the service that talks to Mojang
    /// needs a handle of its own, and opening a second database would
    /// mean a second pool of connections for the same file.
    storage: Arc<Storage>,
    /// The paths the launcher uses.
    paths: AppPaths,
    /// The platform the launcher runs on.
    platform: Platform,
    /// The service that knows about the metadata of the game.
    ///
    /// It holds the HTTP client and the download engine, and both of
    /// them are expensive to build, so they are built once and kept.
    service: MinecraftService,
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

        let service =
            MinecraftService::new(paths.clone(), Arc::clone(&storage)).map_err(|error| {
                IpcError::with_detail(
                    crate::error::ErrorCode::Network,
                    "cannot create the http client of the launcher",
                    error.to_string(),
                )
            })?;

        Ok(Self {
            storage,
            paths,
            platform,
            service,
            running: Mutex::new(None),
            current_run: Mutex::new(None),
        })
    }

    /// Build the state from the paths the operating system suggests.
    ///
    /// # Errors
    /// Fails when the directories cannot be found or created.
    pub fn discover(platform: Platform) -> IpcResult<Self> {
        let paths = AppPaths::discover().map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot find the directories of the launcher",
                error.to_string(),
            )
        })?;
        Self::new(paths, platform)
    }

    /// The storage layer.
    #[must_use]
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// The paths the launcher uses.
    #[must_use]
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// The platform the launcher runs on.
    #[must_use]
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// The service that knows about the metadata of the game.
    #[must_use]
    pub fn service(&self) -> &MinecraftService {
        &self.service
    }

    /// A copy of the service, which can be moved into a task.
    #[must_use]
    pub fn service_clone(&self) -> MinecraftService {
        self.service.clone()
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
}
