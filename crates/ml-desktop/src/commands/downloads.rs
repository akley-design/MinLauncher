//! The commands about the queue of the downloads.
//!
//! The queue is what the launcher remembers about the files it fetched.
//! It is written by the installation itself, so that an interrupted
//! installation leaves a trace of what was already there; the commands
//! below read that trace, clean it up and fetch one of its files again.

use std::path::PathBuf;
use std::sync::Arc;

use ml_minecraft::{DownloadTask, file_size};
use ml_storage::{DownloadEntry, DownloadState, DownloadUpdate};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::{AppState, SharedState};

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

/// Remove the finished entries from the queue.
///
/// The number of the removed entries is returned, so that the screen
/// can say what it did.
///
/// # Errors
/// Fails when the database cannot be written.
#[tauri::command]
pub fn clear_completed_downloads(state: State<'_, SharedState>) -> IpcResult<usize> {
    state
        .storage()
        .downloads()
        .clear_completed()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot clear the finished downloads",
                error.to_string(),
            )
        })
}

/// Remove the failed entries from the queue.
///
/// The number of the removed entries is returned, so that the screen
/// can say what it did.
///
/// # Errors
/// Fails when the database cannot be written.
#[tauri::command]
pub fn clear_failed_downloads(state: State<'_, SharedState>) -> IpcResult<usize> {
    state.storage().downloads().clear_failed().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot clear the failed downloads",
            error.to_string(),
        )
    })
}

/// Fetch one file of the queue again.
///
/// The entry keeps everything that is needed to fetch the file: the
/// address, the place it is written to and the checksum it has to
/// match. The payload is written to a `.part` next to its target and is
/// moved into place only when it is whole, so a retry of a file that
/// was half-fetched continues where it stopped.
///
/// # Errors
/// Fails when the entry is unknown, when the queue cannot be written,
/// or when the file cannot be fetched.
#[tauri::command]
pub async fn retry_download(
    url: String,
    state: State<'_, SharedState>,
) -> IpcResult<DownloadEntry> {
    retry_entry(&state, &url).await
}

/// The work of a retry, without the state of Tauri.
///
/// It is a function of its own so that it can be tested without a
/// running desktop application.
async fn retry_entry(state: &AppState, url: &str) -> IpcResult<DownloadEntry> {
    let entry = read_entry(state, url)?;

    let task = DownloadTask::new(
        entry.url.clone(),
        PathBuf::from(&entry.target_path),
        entry.sha1.clone(),
    )
    .with_size(entry.size)
    // The file is fetched again, even when something that looks like it
    // is already on the disk: a retry that quietly reuses a broken file
    // is worse than no retry at all.
    .always();

    // The entry is marked as running before the fetch starts, so that a
    // launcher that is closed in the middle leaves a trace.
    //
    // shortcut: a finished entry is not moved back to work — the
    // repository refuses that on purpose, and the screen does not offer
    // a retry for a file that is already there.
    let _ = state
        .storage()
        .downloads()
        .set_state(url, DownloadState::Downloading, None);

    let service = state.service_clone();
    let result = service
        .downloader()
        .download_all_with_progress(vec![task], Arc::new(|_| {}))
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "cannot download the file again",
                error.to_string(),
            )
        })?;

    if let Some(done) = result.completed.into_iter().next() {
        write_update(
            state,
            DownloadUpdate {
                url: url.to_string(),
                state: DownloadState::Completed,
                error: None,
                // The size on the disk is the truth; the size the
                // server promised is only a guess.
                bytes_done: Some(file_size(&done.target)),
                attempts: Some(i64::from(done.attempts)),
            },
        )?;
        read_entry(state, url)
    } else {
        let reason = result.failed.first().map_or_else(
            || "the file was not downloaded".to_string(),
            |(_, reason)| reason.clone(),
        );
        write_update(
            state,
            DownloadUpdate {
                url: url.to_string(),
                state: DownloadState::Failed,
                error: Some(reason.clone()),
                bytes_done: None,
                attempts: None,
            },
        )?;
        Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot download the file again",
            reason,
        ))
    }
}

/// Read one entry of the queue.
///
/// # Errors
/// Fails when the database cannot be read, or when the entry is not
/// there.
fn read_entry(state: &AppState, url: &str) -> IpcResult<DownloadEntry> {
    state
        .storage()
        .downloads()
        .get(url)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the download queue",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            IpcError::with_detail(
                ErrorCode::NotFound,
                "the download is not in the queue",
                url.to_string(),
            )
        })
}

/// Write one update of the queue.
///
/// # Errors
/// Fails when the database cannot be written.
fn write_update(state: &AppState, update: DownloadUpdate) -> IpcResult<()> {
    state
        .storage()
        .downloads()
        .update_all(&[update])
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot write the download queue",
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
            task: Some("26.3".to_string()),
        }
    }

    /// A server that answers every request with the same payload.
    ///
    /// The test needs a real address to fetch from and nothing else:
    /// the engine, the checksum and the resume are tested where they
    /// live, in the crate that owns them.
    fn fake_server(payload: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { continue };
                let mut buffer = [0u8; 4096];
                let mut head = Vec::new();
                while !head.windows(4).any(|window| window == b"\r\n\r\n") {
                    let Ok(read) = stream.read(&mut buffer) else {
                        break;
                    };
                    if read == 0 {
                        break;
                    }
                    head.extend_from_slice(&buffer[..read]);
                }
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(answer.as_bytes());
                let _ = stream.write_all(payload);
                let _ = stream.flush();
            }
        });

        (format!("http://{address}/payload"), handle)
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

    #[test]
    fn the_finished_entries_are_cleared() {
        let (_dir, state) = state();
        let storage = state.storage();
        storage.downloads().enqueue(&entry("a.jar")).unwrap();
        storage
            .downloads()
            .set_state("a.jar", DownloadState::Completed, None)
            .unwrap();
        storage.downloads().enqueue(&entry("b.jar")).unwrap();

        assert_eq!(storage.downloads().clear_completed().unwrap(), 1);
        assert_eq!(storage.downloads().list(None).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_failed_download_is_fetched_again() {
        let (dir, state) = state();
        let (url, _server) = fake_server(b"hello");

        let target = dir.path().join("payload.jar");
        let mut failed = entry(&url);
        failed.target_path = target.to_string_lossy().into_owned();
        failed.sha1 = Some("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".to_string());
        failed.size = Some(5);
        failed.state = DownloadState::Failed;
        failed.error = Some("the network is down".to_string());
        state.storage().downloads().enqueue(&failed).unwrap();

        let result = retry_entry(&state, &url).await.unwrap();

        assert_eq!(result.state, DownloadState::Completed);
        assert_eq!(result.bytes_done, 5);
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn a_download_that_is_not_in_the_queue_is_not_found() {
        let (_dir, state) = state();

        let error = retry_entry(&state, "https://example.invalid/a.jar")
            .await
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
