//! Entry point of the MinLauncher desktop application.
//!
//! The Tauri application itself lives in [`ml_desktop`]; this binary only
//! starts it.

// Release builds must not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ml_desktop::run();
}
