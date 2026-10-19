//! Tauri shell for `MinLauncher`.
//!
//! This crate owns everything that is specific to the desktop application:
//! the shared state, the IPC commands, the plugin wiring and the tray
//! icon. It glues the other crates together but contains no business logic
//! of its own.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Start the desktop application.
///
/// # Panics
/// Panics if the Tauri runtime cannot be created, which happens when the
/// event loop cannot be initialised.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            // A second launch focuses the window that is already running.
        }))
        .run(tauri::generate_context!())
        .expect("failed to start the MinLauncher desktop application");
}
