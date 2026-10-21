//! Tauri shell for `MinLauncher`.
//!
//! This crate owns everything that is specific to the desktop application:
//! the shared state, the IPC commands, the plugin wiring and the tray
//! icon. It glues the other crates together but contains no business logic
//! of its own.
//!
//! # The shape of the shell
//!
//! ```text
//!   apps/ui  ──invoke──▶  commands  ──▶  ml-core / ml-storage / ml-minecraft / ml-launch
//!       ▲                    │
//!       └──────event─────────┘
//! ```
//!
//! A command is a thin translation: it takes the arguments of the
//! frontend, calls the crate that knows how to do the work, and turns the
//! result into something that can be serialised to JSON. Everything that
//! is worth a test lives below this layer, in the crate that owns the
//! rule.
//!
//! # Errors
//!
//! The error of the domain crosses the IPC boundary as an
//! [`error::IpcError`]: a stable code, a message for the user and the
//! cause. The frontend switches on the code and shows the message.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod commands;
pub mod error;
pub mod state;

use std::sync::Arc;

use ml_core::{AppPaths, OsArch, OsName, Platform};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, State, WindowEvent};

use crate::state::{AppState, SharedState};

/// The identifier of the tray icon.
const TRAY_ID: &str = "minlaucher-tray";

/// Start the desktop application.
///
/// # Panics
/// Panics if the Tauri runtime cannot be created, which happens when the
/// event loop cannot be initialised.
pub fn run() {
    let platform = current_platform();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch focuses the window that is already running
            // instead of opening a second one.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            let handle = app.handle();

            // The state is created here, because it needs the paths of
            // the application, which Tauri only knows once it runs.
            let state = AppState::discover(platform).map_err(|error| {
                // There is no frontend to show a message to yet, so the
                // failure is written into the log and the application
                // keeps running with a state that has no storage.
                eprintln!("MinLauncher: {error}");
                error
            })?;

            let shared: SharedState = Arc::new(state);

            // The log of the launcher is written only when it is asked
            // for, and it is asked for before anything else happens.
            install_log(shared.paths());

            // The autostart of the machine is brought in line with the
            // settings every time the launcher starts: a record that
            // was removed by hand comes back.
            if let Ok(config) = shared.storage().load_settings()
                && let Err(error) = commands::settings::apply_autostart(&config)
            {
                tracing::warn!("cannot change the autostart: {error}");
            }

            handle.manage(Arc::clone(&shared));

            build_tray(handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::versions::list_versions,
            commands::versions::read_manifest,
            commands::versions::refresh_manifest,
            commands::versions::install_version,
            commands::runtimes::list_runtimes,
            commands::runtimes::install_runtime,
            commands::downloads::list_downloads,
            commands::downloads::clear_completed_downloads,
            commands::downloads::clear_failed_downloads,
            commands::downloads::retry_download,
            commands::accounts::list_accounts,
            commands::accounts::selected_account,
            commands::accounts::select_account,
            commands::accounts::add_offline_account,
            commands::accounts::remove_account,
            commands::instances::list_instances,
            commands::instances::create_instance,
            commands::instances::remove_instance,
            commands::launch::launch_game,
            commands::launch::stop_game,
            commands::settings::read_settings,
            commands::settings::write_settings,
            commands::settings::read_directories,
            commands::updater::check_for_update,
            commands::updater::install_update,
            commands::benchmarks::latest_benchmarks,
            commands::benchmarks::list_benchmarks,
            commands::benchmarks::compare_in_game,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state: State<'_, SharedState> = window.state();
                let minimize = state
                    .storage()
                    .load_settings()
                    .is_ok_and(|config| config.minimize_to_tray);

                if minimize {
                    // The launcher hides instead of closing, so that the
                    // game keeps running and the tray icon is still
                    // there to bring it back.
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build the MinLauncher desktop application")
        .run(move |app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // The game is a child process of the launcher. When the
                // launcher is closed for good, the game is closed with
                // it, so that no orphaned JVM is left behind.
                if let Some(state) = app.try_state::<SharedState>()
                    && state.running().is_some()
                {
                    api.prevent_exit();
                    return;
                }
                tracing::info!("MinLauncher is closing with code {code:?}");
            }
        });
}

/// The name of the file the launcher writes its log into.
const LAUNCHER_LOG: &str = "launcher.log";

/// Install the log of the launcher, when it is asked for.
///
/// The launcher has no console in a release build, so everything it has
/// to say about a launch goes nowhere unless it is written into a file.
/// The file is written only when the environment asks for it: a launcher
/// that writes a log on every launch pays for it in the time of the
/// launch, and the promise of the launcher is the speed of the launch.
///
/// The variable `MINLAUNCHER_LOG` holds the path of the file. The value
/// `1` means the file of the launcher in the directory of its logs.
///
/// # Limits
///
/// The log is not written by default, so a launcher that was never asked
/// to log says nothing about what it did. The log is also not rotated:
/// the file grows for as long as the launcher writes into it, and a
/// player who leaves the variable set for months pays for the disk.
fn install_log(paths: &AppPaths) {
    let Ok(value) = std::env::var("MINLAUNCHER_LOG") else {
        return;
    };

    let value = value.trim();
    let path = if value.is_empty() || value == "1" {
        paths.logs_dir.join(LAUNCHER_LOG)
    } else {
        std::path::PathBuf::from(value)
    };

    if let Some(directory) = path.parent()
        && let Err(error) = std::fs::create_dir_all(directory)
    {
        eprintln!("MinLauncher: cannot create the directory of the log: {error}");
        return;
    }

    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "MinLauncher: cannot open the log {}: {error}",
                path.display()
            );
            return;
        }
    };

    // The subscriber is global and can be set only once, so the failure
    // of a second attempt is not a failure of the launcher.
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::sync::Mutex::new(file))
        .try_init();
}

/// The platform the launcher runs on.
fn current_platform() -> Platform {
    let os = match std::env::consts::OS {
        "windows" => OsName::Windows,
        "macos" => OsName::Osx,
        // Every other system the launcher runs on is treated as Linux.
        _ => OsName::Linux,
    };

    let arch = match std::env::consts::ARCH {
        "aarch64" => OsArch::Arm64,
        "x86" => OsArch::X86,
        _ => OsArch::X86_64,
    };

    Platform::new(os, arch).with_os_version(ml_core::sys::os_version())
}

/// Build the tray icon of the launcher.
///
/// The tray is what makes the launcher useful while the game runs: the
/// window can be closed, and the game keeps running.
fn build_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Показать окно", true, None::<&str>)?;
    let launch = MenuItem::with_id(app, "launch", "Запустить игру", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&show, &launch, &separator, &quit])?;

    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("MinLauncher");
    // The tray reuses the icon of the window (the neon cube from
    // `bundle.icon`), so no old glyph can survive here either.
    let builder = match app.default_window_icon() {
        Some(icon) => builder.icon(icon.clone()),
        None => builder,
    };
    builder
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
                && let Some(window) = tray.app_handle().get_webview_window("main")
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_is_the_one_of_the_process() {
        let platform = current_platform();
        assert_ne!(platform.os.as_str().len(), 0);
        assert_ne!(platform.arch.as_str().len(), 0);
    }

    #[test]
    fn the_platform_matches_the_compilation_target() {
        let platform = current_platform();
        match std::env::consts::OS {
            "windows" => assert_eq!(platform.os, OsName::Windows),
            "macos" => assert_eq!(platform.os, OsName::Osx),
            "linux" => assert_eq!(platform.os, OsName::Linux),
            _ => {}
        }
    }
}
