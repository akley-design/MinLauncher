//! The commands about the settings of the launcher.
//!
//! The settings live in a JSON file next to the database. They are read
//! whole and written whole: the file is small, and a partial write of a
//! settings file is a corrupted settings file.

use std::sync::Arc;

use ml_core::{AppPaths, CollectorMeasurement, FlagDecision, LauncherConfig};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::install_location::{self, InstallRootEvent, InstallRootProgress};
use crate::reconcile::{self, ScanReport};
use crate::state::SharedState;

/// The event the frontend listens to while the installation root changes.
pub const INSTALL_ROOT_EVENT: &str = "minlauncher://install-root";

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
    /// What the measurement said about the flags of the virtual machine,
    /// so that the interface can show why a flag is on and not merely
    /// that it is.
    pub jvm_flags: Vec<ProfileFlags>,
    /// What the measurement said about the collectors of the garbage.
    ///
    /// The player may pick a collector instead of the one the version of
    /// the game prescribes, and that is a choice about a flag of the
    /// virtual machine. The rule of the project is the same for it as
    /// for every other flag: the choice is offered with the measurement
    /// next to it.
    pub jvm_collectors: Vec<CollectorMeasurement>,
    /// Why the configured installation directory is not in use, if it is
    /// not. The settings still name it; the launcher fell back to the
    /// historical layout because the directory could not be opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_dir_error: Option<String>,
}

/// The decisions of one profile of the flags of the virtual machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileFlags {
    /// The profile these decisions belong to.
    pub profile: String,
    /// The decisions themselves.
    pub decisions: Vec<FlagDecision>,
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
    /// The chosen root of the game files; empty for the historical layout.
    pub install: String,
    /// Where the installed versions live.
    pub versions: String,
    /// Where the libraries live.
    pub libraries: String,
    /// Where the assets live.
    pub assets: String,
    /// Where the Java runtimes live.
    pub runtimes: String,
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
    let jvm_flags = ml_core::jvmflags::every_profile()
        .into_iter()
        .map(|(profile, decisions)| ProfileFlags {
            profile: profile.as_str().to_string(),
            decisions,
        })
        .collect();

    Ok(SettingsView {
        config,
        directories: directories_of(&paths),
        autostart_supported: ml_core::autostart::is_supported(),
        jvm_flags,
        jvm_collectors: ml_core::jvmflags::collectors(),
        install_dir_error: state.install_dir_error(),
    })
}

/// The directories of one layout, as the frontend sees them.
fn directories_of(paths: &AppPaths) -> Directories {
    Directories {
        data: paths.data_dir.to_string_lossy().into_owned(),
        cache: paths.cache_dir.to_string_lossy().into_owned(),
        database: paths.database_path.to_string_lossy().into_owned(),
        install: paths
            .install_dir
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        versions: paths.versions_dir.to_string_lossy().into_owned(),
        libraries: paths.libraries_dir.to_string_lossy().into_owned(),
        assets: paths.assets_dir.to_string_lossy().into_owned(),
        runtimes: paths.jre_dir.to_string_lossy().into_owned(),
    }
}

/// Write the settings.
///
/// A changed installation directory is applied before the settings file
/// is touched: the game files are copied to the new folder, the new
/// layout is built, and only then the settings start pointing at it. A
/// copy that fails leaves both the settings and the running layout as
/// they were.
///
/// # Errors
/// Fails when the new directory is invalid, the files cannot be copied,
/// the settings file cannot be written, or the machine cannot be told to
/// start the launcher with it.
#[tauri::command]
pub async fn write_settings(
    app: AppHandle,
    config: LauncherConfig,
    state: State<'_, SharedState>,
) -> IpcResult<SettingsView> {
    let previous = state.storage().load_settings().unwrap_or_default();
    let requested = config.install_dir.trim();
    let changed = !same_directory(requested, previous.install_dir.trim());

    // The guard lives until the new root is in place, so an installation
    // cannot start in the middle of the move.
    let mut switch_guard = None;
    let mut prepared = None;

    if changed {
        switch_guard = Some(state.begin_switch()?);

        let old_paths = state.paths();
        let storage = state.storage_handle();
        let requested = requested.to_string();
        let progress_app = app.clone();
        let progress: InstallRootProgress = Arc::new(move |event| {
            let _ = progress_app.emit(INSTALL_ROOT_EVENT, &event);
        });

        let result = tokio::task::spawn_blocking(move || {
            install_location::prepare_transition(storage, &old_paths, &requested, &progress)
        })
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Internal,
                "the copy of the game files did not finish",
                error.to_string(),
            )
        })?;
        prepared = Some(result.map_err(IpcError::from)?);
    }

    // The autostart is not a setting of the file alone: it is a setting
    // of the machine. The machine is told before the file is written, so
    // that a registry that refuses leaves the file untouched.
    apply_when_changed(previous.autostart, config.autostart, |enabled| {
        ml_core::autostart::set_enabled(enabled)
    })
    .map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot change the autostart",
            error.to_string(),
        )
    })?;

    let mut to_save = config;
    to_save.install_dir = match &prepared {
        Some(prepared) => prepared
            .paths
            .install_dir
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        // The same directory written a little differently is still the
        // same directory; the canonical spelling that is stored wins.
        None => previous.install_dir.clone(),
    };

    state.storage().save_settings(&to_save).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the settings",
            error.to_string(),
        )
    })?;
    state.set_download_concurrency(to_save.downloads.max_concurrent);

    if let Some(prepared) = prepared {
        let _ = app.emit(INSTALL_ROOT_EVENT, InstallRootEvent::Scanning);
        let report = state.apply_transition(prepared.paths, prepared.service)?;
        if !report.failed.is_empty() {
            tracing::warn!(
                "the new installation root holds {} versions that cannot be read",
                report.failed.len()
            );
        }
        let _ = app.emit(
            INSTALL_ROOT_EVENT,
            InstallRootEvent::Ready {
                installed: report.installed,
                failed: report.failed.len(),
            },
        );
    }
    drop(switch_guard);

    read_settings(state)
}

/// Whether two strings name the same directory.
fn same_directory(left: &str, right: &str) -> bool {
    fn normalize(value: &str) -> String {
        let value = value.trim().trim_end_matches(['/', '\\']);
        if cfg!(windows) {
            value.replace('/', "\\").to_lowercase()
        } else {
            value.to_string()
        }
    }
    normalize(left) == normalize(right)
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
    Ok(directories_of(&state.paths()))
}

/// Ask the player for a directory with the native folder picker.
///
/// The returned path is not applied here: the settings form puts it into
/// the draft, and the player saves it like any other setting. That way a
/// canceled save or a canceled dialog changes nothing.
///
/// # Errors
/// Fails when the chosen path cannot be read back from the dialog.
#[tauri::command]
pub async fn pick_install_directory(app: AppHandle) -> IpcResult<Option<String>> {
    let Some(folder) = app
        .dialog()
        .file()
        .set_title("Папка установки версий")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };

    let path = folder.into_path().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the chosen directory",
            error.to_string(),
        )
    })?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Scan the installation directory and remember what is installed there.
///
/// # Errors
/// Fails when the directory or the database cannot be read.
#[tauri::command]
pub fn rescan_versions(state: State<'_, SharedState>) -> IpcResult<ScanReport> {
    reconcile::rescan_versions(state.storage(), &state.paths()).map_err(IpcError::from)
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
    fn the_same_directory_is_recognised() {
        assert!(same_directory("", ""));
        assert!(same_directory("E:\\MinLauncher", "E:\\MinLauncher\\"));
        assert!(!same_directory("E:\\MinLauncher", "E:\\Other"));
        if cfg!(windows) {
            assert!(same_directory("E:\\MinLauncher", "e:/minlauncher"));
        }
    }

    #[test]
    fn the_directories_show_the_chosen_layout() {
        let dir = tempfile::tempdir().unwrap();
        let base = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let game = dir.path().join("game");
        let paths = base.rebase_install_root(Some(&game));

        let directories = directories_of(&paths);

        assert_eq!(directories.install, game.to_string_lossy());
        assert_eq!(
            directories.versions,
            game.join("versions").to_string_lossy()
        );
        assert_eq!(
            directories.libraries,
            game.join("libraries").to_string_lossy()
        );
        assert_eq!(directories.assets, game.join("assets").to_string_lossy());
        assert_eq!(
            directories.runtimes,
            game.join("runtimes").to_string_lossy()
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
