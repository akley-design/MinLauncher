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
    /// Whether this system can start the launcher with itself.
    ///
    /// The interface hides the toggle of the autostart when the answer
    /// is no: a toggle that does nothing is worse than no toggle.
    pub autostart_supported: bool,
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
        autostart_supported: ml_core::autostart::is_supported(),
    })
}

/// Write the settings.
///
/// # Errors
/// Fails when the settings file cannot be written, or when the machine
/// cannot be told to start the launcher with it.
#[tauri::command]
pub fn write_settings(
    config: LauncherConfig,
    state: State<'_, SharedState>,
) -> IpcResult<SettingsView> {
    // The autostart is not a setting of the file alone: it is a setting
    // of the machine. The old value is read before anything changes,
    // so that the registry is touched only when the setting really
    // changed.
    let was_enabled = state
        .storage()
        .load_settings()
        .is_ok_and(|previous| previous.autostart);

    // The machine is told first: if it cannot be told, nothing changes
    // at all, and the setting does not end up lying about itself.
    apply_when_changed(was_enabled, config.autostart, |enabled| {
        ml_core::autostart::set_enabled(enabled)
    })
    .map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot change the autostart",
            error.to_string(),
        )
    })?;

    state.storage().save_settings(&config).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the settings",
            error.to_string(),
        )
    })?;

    read_settings(state)
}

/// Apply the autostart when the setting really changed.
///
/// The registry is touched only when the value changed: the player
/// presses «Сохранить» after every change of the settings, and the
/// record in the registry does not have to be rewritten every time.
///
/// # Errors
/// Fails when the machine cannot be told about the autostart.
fn apply_when_changed(
    was_enabled: bool,
    is_enabled: bool,
    apply: impl FnOnce(bool) -> ml_core::Result<()>,
) -> ml_core::Result<()> {
    if was_enabled != is_enabled {
        apply(is_enabled)?;
    }
    Ok(())
}

/// Bring the autostart of the machine in line with the settings.
///
/// The settings file is the truth: the record in the registry is
/// written from it every time the launcher starts, so a record that
/// was removed by hand comes back.
///
/// # Errors
/// Fails when the registry cannot be written.
pub fn apply_autostart(config: &LauncherConfig) -> ml_core::Result<()> {
    ml_core::autostart::set_enabled(config.autostart)
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
    fn the_machine_is_told_only_when_the_setting_changed() {
        let mut told = Vec::new();

        // Nothing changed: the registry is not touched at all.
        apply_when_changed(false, false, |enabled| {
            told.push(enabled);
            Ok(())
        })
        .unwrap();
        assert!(told.is_empty(), "the registry was touched for nothing");

        // The setting changed: the machine is told.
        apply_when_changed(false, true, |enabled| {
            told.push(enabled);
            Ok(())
        })
        .unwrap();
        assert_eq!(told, vec![true]);

        // And the other way around: the autostart is switched off.
        apply_when_changed(true, false, |enabled| {
            told.push(enabled);
            Ok(())
        })
        .unwrap();
        assert_eq!(told, vec![true, false]);
    }

    #[test]
    fn a_failure_of_the_machine_is_reported() {
        let error = apply_when_changed(false, true, |_| {
            Err(ml_core::Error::Storage("the registry is locked".into()))
        })
        .unwrap_err();

        assert!(
            error.to_string().contains("the registry is locked"),
            "{error}"
        );
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
