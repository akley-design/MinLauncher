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
}
