//! Repository for the download queue.
//!
//! The queue is persisted so that an interrupted installation resumes
//! instead of starting over — a meaningful part of the "faster than
//! the reference launcher" promise, since a fresh install is dominated by
//! downloads.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::storage_error;

/// Lifecycle state of a queued download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// Waiting for a free slot.
    Queued,
    /// In flight.
    Downloading,
    /// Finished and verified.
    Completed,
    /// Gave up after exhausting the retry budget.
    Failed,
}

impl DownloadState {
    /// The value stored in the database.
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parse a value read back from the database.
    #[must_use]
    pub fn from_db_str(raw: &str) -> Self {
        match raw {
            "downloading" => Self::Downloading,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

/// One entry of the download queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadEntry {
    /// Source URL, used as the primary key.
    pub url: String,
    /// Expected SHA-1 of the payload, if known.
    pub sha1: Option<String>,
    /// Expected payload size in bytes, if known.
    pub size: Option<i64>,
    /// Where the payload is written.
    pub target_path: String,
    /// Current state.
    pub state: DownloadState,
    /// Bytes transferred so far.
    pub bytes_done: i64,
    /// How many attempts have been made.
    pub attempts: i64,
    /// Error message of the last failed attempt.
    pub error: Option<String>,
    /// When the entry was last touched, ISO-8601.
    pub updated_at: String,
}

/// The new state of one entry of the queue.
///
/// The bytes and the attempts are only written when the update knows
/// them: `None` leaves what is in the database alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadUpdate {
    /// The URL of the entry.
    pub url: String,
    /// The state the entry moves to.
    pub state: DownloadState,
    /// The error of the last attempt, when there was one.
    pub error: Option<String>,
    /// How many bytes of the payload are on the disk, when it is known.
    pub bytes_done: Option<i64>,
    /// How many attempts were made, when it is known.
    pub attempts: Option<i64>,
}

/// Reads and writes the download queue.
#[derive(Debug, Clone)]
pub struct DownloadRepo {
    pool: SqlitePool,
}

impl DownloadRepo {
    /// Create a repository backed by the given pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn conn(&self) -> Result<PooledConnection> {
        self.pool
            .get()
            .map_err(|e| Error::Storage(format!("cannot acquire connection: {e}")))
    }

    /// Add an entry to the queue, or refresh the metadata of an existing one.
    ///
    /// A re-queue of a *completed* entry is ignored: the file is already on
    /// disk and re-downloading it would waste bandwidth.
    pub fn enqueue(&self, entry: &DownloadEntry) -> Result<()> {
        self.enqueue_all(std::slice::from_ref(entry))
    }

    /// Add a whole batch to the queue, or refresh the metadata of the
    /// entries that are already there.
    ///
    /// The batch is written in one transaction: an install plan holds
    /// thousands of files, and one transaction per file would cost more
    /// than the download itself.
    ///
    /// A re-queue of a *completed* entry is ignored: the file is already on
    /// disk and re-downloading it would waste bandwidth.
    pub fn enqueue_all(&self, entries: &[DownloadEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot begin the transaction", &e))?;

        // One timestamp for the whole batch: the entries are written
        // together, so they were touched together.
        let now = now_iso8601();

        for entry in entries {
            tx.execute(
                "INSERT INTO downloads (url, sha1, size, target_path, state, bytes_done, attempts, error, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(url) DO UPDATE SET
                     sha1 = excluded.sha1,
                     size = excluded.size,
                     target_path = excluded.target_path,
                     state = excluded.state,
                     bytes_done = excluded.bytes_done,
                     attempts = excluded.attempts,
                     error = excluded.error,
                     updated_at = excluded.updated_at
                 WHERE downloads.state != 'completed'",
                params![
                    entry.url,
                    entry.sha1,
                    entry.size,
                    entry.target_path,
                    entry.state.as_db_str(),
                    entry.bytes_done,
                    entry.attempts,
                    entry.error,
                    now,
                ],
            )
            .map_err(|e| storage_error("cannot enqueue download", &e))?;
        }

        tx.commit()
            .map_err(|e| storage_error("cannot commit the queue", &e))?;
        Ok(())
    }

    /// Move a whole batch of entries into their new state.
    ///
    /// The bytes and the attempts are only written when the update knows
    /// them: `None` leaves what is in the database alone.
    ///
    /// A *completed* entry is not moved back to work: the file is already
    /// on the disk. It may still be written again, so that the size on
    /// the disk is refreshed.
    pub fn update_all(&self, updates: &[DownloadUpdate]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot begin the transaction", &e))?;

        let now = now_iso8601();

        for update in updates {
            tx.execute(
                "UPDATE downloads SET
                     state = ?2,
                     error = ?3,
                     bytes_done = COALESCE(?4, bytes_done),
                     attempts = COALESCE(?5, attempts),
                     updated_at = ?6
                 WHERE url = ?1 AND (state != 'completed' OR ?2 = 'completed')",
                params![
                    update.url,
                    update.state.as_db_str(),
                    update.error,
                    update.bytes_done,
                    update.attempts,
                    now,
                ],
            )
            .map_err(|e| storage_error("cannot update download", &e))?;
        }

        tx.commit()
            .map_err(|e| storage_error("cannot commit the queue", &e))?;
        Ok(())
    }

    /// Record how many bytes have been transferred.
    pub fn update_progress(&self, url: &str, bytes_done: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE downloads SET bytes_done = ?2, updated_at = ?3 WHERE url = ?1",
            params![url, bytes_done, now_iso8601()],
        )
        .map_err(|e| storage_error("cannot update download progress", &e))?;
        Ok(())
    }

    /// Move an entry into a new state.
    ///
    /// The bytes and the attempts are left alone.
    pub fn set_state(&self, url: &str, state: DownloadState, error: Option<&str>) -> Result<()> {
        self.update_all(&[DownloadUpdate {
            url: url.to_string(),
            state,
            error: error.map(str::to_string),
            bytes_done: None,
            attempts: None,
        }])
    }

    /// Increment the attempt counter of an entry.
    pub fn increment_attempts(&self, url: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE downloads SET attempts = attempts + 1, updated_at = ?2 WHERE url = ?1",
            params![url, now_iso8601()],
        )
        .map_err(|e| storage_error("cannot update download attempts", &e))?;
        Ok(())
    }

    /// Fetch a single entry.
    pub fn get(&self, url: &str) -> Result<Option<DownloadEntry>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT url, sha1, size, target_path, state, bytes_done, attempts, error, updated_at
                 FROM downloads WHERE url = ?1",
                params![url],
                row_to_entry,
            )
            .optional()
            .map_err(|e| storage_error("cannot read download", &e))?;
        Ok(row)
    }

    /// List queue entries, optionally filtered by state.
    pub fn list(&self, state: Option<DownloadState>) -> Result<Vec<DownloadEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT url, sha1, size, target_path, state, bytes_done, attempts, error, updated_at
                 FROM downloads
                 WHERE (?1 IS NULL OR state = ?1)
                 ORDER BY updated_at ASC, url ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map(params![state.map(DownloadState::as_db_str)], row_to_entry)
            .map_err(|e| storage_error("cannot query downloads", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read downloads", &e))
    }

    /// Remove every completed entry and report how many were removed.
    pub fn clear_completed(&self) -> Result<usize> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM downloads WHERE state = 'completed'", [])
            .map_err(|e| storage_error("cannot clear completed downloads", &e))
    }

    /// Remove every failed entry and report how many were removed.
    pub fn clear_failed(&self) -> Result<usize> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM downloads WHERE state = 'failed'", [])
            .map_err(|e| storage_error("cannot clear failed downloads", &e))
    }

    /// Remove an entry from the queue.
    pub fn remove(&self, url: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute("DELETE FROM downloads WHERE url = ?1", params![url])
            .map_err(|e| storage_error("cannot remove download", &e))?;
        Ok(changed > 0)
    }
}

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<DownloadEntry> {
    let state: String = row.get(4)?;
    Ok(DownloadEntry {
        url: row.get(0)?,
        sha1: row.get(1)?,
        size: row.get(2)?,
        target_path: row.get(3)?,
        state: DownloadState::from_db_str(&state),
        bytes_done: row.get(5)?,
        attempts: row.get(6)?,
        error: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, DownloadRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = DownloadRepo::new(db.pool().clone());
        (db, repo)
    }

    fn entry(url: &str) -> DownloadEntry {
        DownloadEntry {
            url: url.to_string(),
            sha1: Some("abc".to_string()),
            size: Some(100),
            target_path: format!("/tmp/{url}"),
            state: DownloadState::Queued,
            bytes_done: 0,
            attempts: 0,
            error: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn enqueue_and_read_back() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        let read = repo.get("a.jar").unwrap().unwrap();
        assert_eq!(read.state, DownloadState::Queued);
        assert_eq!(read.sha1.as_deref(), Some("abc"));
    }

    #[test]
    fn progress_and_state_transitions() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.update_progress("a.jar", 42).unwrap();
        repo.increment_attempts("a.jar").unwrap();
        repo.set_state("a.jar", DownloadState::Completed, None)
            .unwrap();

        let read = repo.get("a.jar").unwrap().unwrap();
        assert_eq!(read.bytes_done, 42);
        assert_eq!(read.attempts, 1);
        assert_eq!(read.state, DownloadState::Completed);
    }

    #[test]
    fn completed_entries_are_not_requeued() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.set_state("a.jar", DownloadState::Completed, None)
            .unwrap();

        let mut again = entry("a.jar");
        again.sha1 = Some("different".to_string());
        repo.enqueue(&again).unwrap();

        assert_eq!(
            repo.get("a.jar").unwrap().unwrap().sha1.as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn listing_filters_by_state() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.enqueue(&entry("b.jar")).unwrap();
        repo.set_state("b.jar", DownloadState::Failed, Some("boom"))
            .unwrap();

        assert_eq!(repo.list(None).unwrap().len(), 2);
        assert_eq!(repo.list(Some(DownloadState::Failed)).unwrap().len(), 1);
        assert_eq!(repo.clear_failed().unwrap(), 1);
        assert_eq!(repo.list(None).unwrap().len(), 1);
    }

    #[test]
    fn a_batch_is_written_in_one_go() {
        let (_db, repo) = repo();
        let entries: Vec<DownloadEntry> = ["a.jar", "b.jar", "c.jar"]
            .iter()
            .map(|url| entry(url))
            .collect();

        repo.enqueue_all(&entries).unwrap();

        let listed = repo.list(None).unwrap();
        assert_eq!(listed.len(), 3);
        assert!(
            listed
                .iter()
                .all(|entry| entry.state == DownloadState::Queued)
        );
    }

    #[test]
    fn an_empty_batch_is_a_no_op() {
        let (_db, repo) = repo();
        repo.enqueue_all(&[]).unwrap();
        repo.update_all(&[]).unwrap();
        assert_eq!(repo.list(None).unwrap().len(), 0);
    }

    #[test]
    fn a_batch_is_moved_at_once() {
        let (_db, repo) = repo();
        repo.enqueue_all(&[entry("a.jar"), entry("b.jar")]).unwrap();

        repo.update_all(&[
            DownloadUpdate {
                url: "a.jar".to_string(),
                state: DownloadState::Completed,
                error: None,
                bytes_done: Some(42),
                attempts: Some(2),
            },
            DownloadUpdate {
                url: "b.jar".to_string(),
                state: DownloadState::Failed,
                error: Some("boom".to_string()),
                bytes_done: None,
                attempts: None,
            },
        ])
        .unwrap();

        let done = repo.get("a.jar").unwrap().unwrap();
        assert_eq!(done.state, DownloadState::Completed);
        assert_eq!(done.bytes_done, 42);
        assert_eq!(done.attempts, 2);

        let failed = repo.get("b.jar").unwrap().unwrap();
        assert_eq!(failed.state, DownloadState::Failed);
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert_eq!(failed.bytes_done, 0, "the bytes were not touched");
    }

    #[test]
    fn a_requeued_entry_goes_back_to_work() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.set_state("a.jar", DownloadState::Failed, Some("boom"))
            .unwrap();

        let mut again = entry("a.jar");
        again.state = DownloadState::Downloading;
        repo.enqueue(&again).unwrap();

        let read = repo.get("a.jar").unwrap().unwrap();
        assert_eq!(read.state, DownloadState::Downloading);
        assert_eq!(read.error, None, "the old reason was dropped");
    }

    #[test]
    fn a_completed_entry_stays_completed() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.set_state("a.jar", DownloadState::Completed, None)
            .unwrap();

        let mut again = entry("a.jar");
        again.state = DownloadState::Downloading;
        repo.enqueue(&again).unwrap();

        assert_eq!(
            repo.get("a.jar").unwrap().unwrap().state,
            DownloadState::Completed
        );
    }

    #[test]
    fn a_completed_entry_is_not_moved_back_to_work() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.set_state("a.jar", DownloadState::Completed, None)
            .unwrap();

        repo.update_all(&[DownloadUpdate {
            url: "a.jar".to_string(),
            state: DownloadState::Downloading,
            error: None,
            bytes_done: None,
            attempts: None,
        }])
        .unwrap();

        assert_eq!(
            repo.get("a.jar").unwrap().unwrap().state,
            DownloadState::Completed
        );
    }

    #[test]
    fn a_completed_entry_is_written_again() {
        let (_db, repo) = repo();
        repo.enqueue(&entry("a.jar")).unwrap();
        repo.set_state("a.jar", DownloadState::Completed, None)
            .unwrap();

        // The file was fetched again and is bigger now: the record has
        // to follow the disk.
        repo.update_all(&[DownloadUpdate {
            url: "a.jar".to_string(),
            state: DownloadState::Completed,
            error: None,
            bytes_done: Some(4242),
            attempts: Some(1),
        }])
        .unwrap();

        let read = repo.get("a.jar").unwrap().unwrap();
        assert_eq!(read.bytes_done, 4242);
        assert_eq!(read.attempts, 1);
    }
}
