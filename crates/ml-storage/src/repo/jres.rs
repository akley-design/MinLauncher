//! Repository for downloaded Java runtimes.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::storage_error;

/// How a Java runtime was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JreStatus {
    /// Queued for download.
    Pending,
    /// Downloaded and unpacked.
    Ready,
    /// Download failed.
    Failed,
    /// Provided by the user, not downloaded.
    External,
}

impl JreStatus {
    /// The value stored in the database.
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::External => "external",
        }
    }

    /// Parse a value read back from the database.
    #[must_use]
    pub fn from_db_str(raw: &str) -> Self {
        match raw {
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            "external" => Self::External,
            _ => Self::Pending,
        }
    }
}

/// A Java runtime known to the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JreEntry {
    /// Runtime family, e.g. `java-runtime-epsilon`.
    pub component: String,
    /// Platform the runtime was built for, e.g. `windows-x64`.
    pub platform: String,
    /// Exact runtime version.
    pub version: String,
    /// Directory the runtime was unpacked into.
    pub path: String,
    /// Current status.
    pub status: JreStatus,
    /// Size on disk in bytes, if known.
    pub size: Option<i64>,
    /// When the entry was last touched, ISO-8601.
    pub updated_at: String,
}

/// Reads and writes Java runtime records.
#[derive(Debug, Clone)]
pub struct JreRepo {
    pool: SqlitePool,
}

impl JreRepo {
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

    /// Insert or update a runtime record.
    pub fn upsert(&self, entry: &JreEntry) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO jres (component, platform, version, path, status, size, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(component, platform) DO UPDATE SET
                 version = excluded.version,
                 path = excluded.path,
                 status = excluded.status,
                 size = excluded.size,
                 updated_at = excluded.updated_at",
            params![
                entry.component,
                entry.platform,
                entry.version,
                entry.path,
                entry.status.as_db_str(),
                entry.size,
                now_iso8601(),
            ],
        )
        .map_err(|e| storage_error("cannot upsert java runtime", &e))?;
        Ok(())
    }

    /// Fetch a runtime record.
    pub fn get(&self, component: &str, platform: &str) -> Result<Option<JreEntry>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT component, platform, version, path, status, size, updated_at
                 FROM jres WHERE component = ?1 AND platform = ?2",
                params![component, platform],
                row_to_entry,
            )
            .optional()
            .map_err(|e| storage_error("cannot read java runtime", &e))?;
        Ok(row)
    }

    /// List every known runtime.
    pub fn list(&self) -> Result<Vec<JreEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT component, platform, version, path, status, size, updated_at
                 FROM jres ORDER BY component ASC, platform ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map([], row_to_entry)
            .map_err(|e| storage_error("cannot query java runtimes", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read java runtimes", &e))
    }

    /// List the runtimes that are ready to be used.
    pub fn list_ready(&self) -> Result<Vec<JreEntry>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|entry| entry.status == JreStatus::Ready)
            .collect())
    }

    /// Forget a runtime record.
    pub fn delete(&self, component: &str, platform: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "DELETE FROM jres WHERE component = ?1 AND platform = ?2",
                params![component, platform],
            )
            .map_err(|e| storage_error("cannot delete java runtime", &e))?;
        Ok(changed > 0)
    }
}

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<JreEntry> {
    let status: String = row.get(4)?;
    Ok(JreEntry {
        component: row.get(0)?,
        platform: row.get(1)?,
        version: row.get(2)?,
        path: row.get(3)?,
        status: JreStatus::from_db_str(&status),
        size: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, JreRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = JreRepo::new(db.pool().clone());
        (db, repo)
    }

    fn entry() -> JreEntry {
        JreEntry {
            component: "java-runtime-epsilon".to_string(),
            platform: "windows-x64".to_string(),
            version: "25.0.1".to_string(),
            path: "C:/runtimes/epsilon".to_string(),
            status: JreStatus::Ready,
            size: Some(1024),
            updated_at: String::new(),
        }
    }

    #[test]
    fn upsert_and_read_back() {
        let (_db, repo) = repo();
        repo.upsert(&entry()).unwrap();
        let read = repo
            .get("java-runtime-epsilon", "windows-x64")
            .unwrap()
            .unwrap();
        assert_eq!(read.version, "25.0.1");
        assert_eq!(read.status, JreStatus::Ready);
    }

    #[test]
    fn upsert_replaces_the_existing_record() {
        let (_db, repo) = repo();
        repo.upsert(&entry()).unwrap();
        let mut changed = entry();
        changed.status = JreStatus::Failed;
        repo.upsert(&changed).unwrap();

        let read = repo
            .get("java-runtime-epsilon", "windows-x64")
            .unwrap()
            .unwrap();
        assert_eq!(read.status, JreStatus::Failed);
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn ready_listing_filters_by_status() {
        let (_db, repo) = repo();
        repo.upsert(&entry()).unwrap();
        let mut pending = entry();
        pending.component = "java-runtime-delta".to_string();
        pending.status = JreStatus::Pending;
        repo.upsert(&pending).unwrap();

        assert_eq!(repo.list().unwrap().len(), 2);
        assert_eq!(repo.list_ready().unwrap().len(), 1);
    }
}
