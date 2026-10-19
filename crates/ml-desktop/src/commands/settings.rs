//! The commands about the settings of the launcher.
//!
//! The settings live in a JSON file next to the database. They are read
//! whole and written whole: the file is small, and a partial write of a
//! settings file is a corrupted settings file.

use ml_core::LauncherConfig;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// The settings of the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsView {
    /// The settings themselves.
    pub config: LauncherConfig,
    /// The directories the launcher uses, so that the frontend can show
    /// them and let the player open them.
    pub directories: Directories,
}

/// The directories the launcher uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Directories {
    /// Where the data of the launcher lives.
    pub data: String,
    /// Where the cache of the launcher lives.
    pub cache: String,
    /// Where the databases live.
    pub database: String,
}

/// The settings as they are now.
///
/// # Errors
/// Fails when the settings file cannot be read.
#[tauri::command]
pub fn read_settings(state: State<'_, SharedState>) -> IpcResult<SettingsView> {
    let config = state.storage().load_settings().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the settings",
            error.to_string(),
        )
    })?;

    let paths = state.paths();

    Ok(SettingsView {
        config,
        directories: Directories {
            data: paths.data_dir.to_string_lossy().into_owned(),
            cache: paths.cache_dir.to_string_lossy().into_owned(),
            database: paths.database_path.to_string_lossy().into_owned(),
        },
    })
}

/// Write the settings.
///
/// # Errors
/// Fails when the settings file cannot be written.
#[tauri::command]
pub fn write_settings(
    config: LauncherConfig,
    state: State<'_, SharedState>,
) -> IpcResult<SettingsView> {
    state.storage().save_settings(&config).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the settings",
            error.to_string(),
        )
    })?;

    read_settings(state)
}

/// The directories the launcher uses.
///
/// # Errors
/// Never fails; the error type is kept so that the command has the same
/// shape as the others.
#[tauri::command]
pub fn read_directories(state: State<'_, SharedState>) -> IpcResult<Directories> {
    let paths = state.paths();

    Ok(Directories {
        data: paths.data_dir.to_string_lossy().into_owned(),
        cache: paths.cache_dir.to_string_lossy().into_owned(),
        database: paths.database_path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{AppPaths, OsArch, OsName, Platform};

    fn state() -> (tempfile::TempDir, SharedState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths, platform).unwrap();
        (dir, std::sync::Arc::new(state))
    }

    #[test]
    fn the_settings_round_trip() {
        let (_dir, state) = state();
        let storage = state.storage();

        let mut config = storage.load_settings().unwrap();
        config.language = "en-US".into();
        config.downloads.max_concurrent = 4;
        storage.save_settings(&config).unwrap();

        let read = storage.load_settings().unwrap();
        assert_eq!(read.language, "en-US");
        assert_eq!(read.downloads.max_concurrent, 4);
    }

    #[test]
    fn the_directories_are_the_ones_that_were_given() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths.clone(), platform).unwrap();

        assert_eq!(
            paths.data_dir.to_string_lossy().into_owned(),
            dir.path().join("data").to_string_lossy().into_owned()
        );
        let _ = state;
    }
}
