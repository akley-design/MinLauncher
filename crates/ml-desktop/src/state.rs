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
use ml_storage::Storage;
use parking_lot::Mutex;

use crate::error::{IpcError, IpcResult};

/// The state the commands share.
#[derive(Debug)]
pub struct AppState {
    /// The storage layer.
    storage: Storage,
    /// The paths the launcher uses.
    paths: AppPaths,
    /// The platform the launcher runs on.
    platform: Platform,
    /// The game that is currently running, if any.
    running: Mutex<Option<crate::commands::launch::RunningGame>>,
}

impl AppState {
    /// Build the state from a set of paths.
    ///
    /// # Errors
    /// Fails when the directories cannot be created or the database
    /// cannot be opened.
    pub fn new(paths: AppPaths, platform: Platform) -> IpcResult<Self> {
        let storage = Storage::open(&paths).map_err(|error| {
            IpcError::with_detail(
                crate::error::ErrorCode::Storage,
                "cannot open the storage of the launcher",
                error.to_string(),
            )
        })?;

        Ok(Self {
            storage,
            paths,
            platform,
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
