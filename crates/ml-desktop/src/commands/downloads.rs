//! The commands about the queue of the downloads.
//!
//! The queue is what the launcher remembers about the files it fetched.
//! It is written by the installation itself, so that an interrupted
//! installation leaves a trace of what was already there; the command
//! below only reads that trace.

use ml_storage::DownloadEntry;
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// The entries of the download queue.
///
/// The entries carry their own state, so the caller can tell the
/// finished files from the ones that failed without asking twice.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn list_downloads(state: State<'_, SharedState>) -> IpcResult<Vec<DownloadEntry>> {
    state.storage().downloads().list(None).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the download queue",
            error.to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{AppPaths, OsArch, OsName, Platform};
    use ml_storage::DownloadState;

    fn state() -> (tempfile::TempDir, SharedState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths, platform).unwrap();
        (dir, std::sync::Arc::new(state))
    }

    fn entry(url: &str) -> DownloadEntry {
        DownloadEntry {
            url: url.to_string(),
            sha1: Some("abc".to_string()),
            size: Some(100),
            target_path: "/tmp/a.jar".to_string(),
            state: DownloadState::Queued,
            bytes_done: 0,
            attempts: 0,
            error: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn the_queue_is_read_as_it_was_written() {
        let (_dir, state) = state();
        let storage = state.storage();
        storage
            .downloads()
            .enqueue(&entry("https://example.invalid/a.jar"))
            .unwrap();

        let listed = storage.downloads().list(None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].url, "https://example.invalid/a.jar");
        assert_eq!(listed[0].state, DownloadState::Queued);
    }

    #[test]
    fn an_entry_survives_the_trip_to_the_frontend() {
        let entry = entry("https://example.invalid/a.jar");
        let json = serde_json::to_string(&entry).unwrap();
        let back: DownloadEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }
}
