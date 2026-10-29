//! Repository for the resolved library set of a version.
//!
//! Libraries are stored *after* rule evaluation, so the launcher does not
//! have to re-resolve the Mojang rule language on every launch.

use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};

use ml_core::Result;

use crate::db::{PooledConnection, SqlitePool};
use crate::repo::{storage_error, to_bool};

/// One library of a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    /// Version the library belongs to.
    pub version_id: String,
    /// Maven coordinates, e.g. `com.mojang:logging:1.1.1`.
    pub name: String,
    /// Classifier, e.g. `natives-windows`; empty for plain libraries.
    pub classifier: String,
    /// Whether this is a native (extracted) library.
    pub is_native: bool,
    /// Path of the jar inside the library store.
    pub artifact_path: String,
    /// Expected SHA-1 of the artifact.
    pub sha1: Option<String>,
    /// Size of the artifact in bytes.
    pub size: Option<i64>,
    /// Where the artifact can be downloaded from.
    pub download_url: Option<String>,
    /// The original rules, kept for diagnostics.
    pub rules_json: Option<String>,
}

/// Reads and writes the library set of a version.
#[derive(Debug, Clone)]
pub struct LibraryRepo {
    pool: SqlitePool,
}

impl LibraryRepo {
    /// Create a repository backed by the given pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn conn(&self) -> Result<PooledConnection> {
        crate::db::connection(&self.pool)
    }

    /// Replace the library set of a version with the given entries.
    pub fn replace_all(&self, version_id: &str, entries: &[LibraryEntry]) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot start transaction", &e))?;
        tx.execute(
            "DELETE FROM libraries WHERE version_id = ?1",
            params![version_id],
        )
        .map_err(|e| storage_error("cannot clear libraries", &e))?;
        for entry in entries {
            tx.execute(
                "INSERT INTO libraries (
                     version_id, name, classifier, is_native, artifact_path, sha1, size, download_url, rules_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    version_id,
                    entry.name,
                    entry.classifier,
                    i64::from(entry.is_native),
                    entry.artifact_path,
                    entry.sha1,
                    entry.size,
                    entry.download_url,
                    entry.rules_json,
                ],
            )
            .map_err(|e| storage_error("cannot insert library", &e))?;
        }
        tx.commit()
            .map_err(|e| storage_error("cannot commit transaction", &e))?;
        Ok(())
    }

    /// List the libraries of a version, natives last.
    pub fn list(&self, version_id: &str) -> Result<Vec<LibraryEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT version_id, name, classifier, is_native, artifact_path, sha1, size, download_url, rules_json
                 FROM libraries WHERE version_id = ?1
                 ORDER BY is_native ASC, name ASC, classifier ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map(params![version_id], row_to_entry)
            .map_err(|e| storage_error("cannot query libraries", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read libraries", &e))
    }

    /// Number of libraries of a version.
    pub fn count(&self, version_id: &str) -> Result<i64> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM libraries WHERE version_id = ?1",
            params![version_id],
            |row| row.get(0),
        )
        .map_err(|e| storage_error("cannot count libraries", &e))
    }

    /// Forget the library set of a version.
    pub fn clear(&self, version_id: &str) -> Result<usize> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM libraries WHERE version_id = ?1",
            params![version_id],
        )
        .map_err(|e| storage_error("cannot clear libraries", &e))
    }
}

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<LibraryEntry> {
    Ok(LibraryEntry {
        version_id: row.get(0)?,
        name: row.get(1)?,
        classifier: row.get(2)?,
        is_native: to_bool(row.get(3)?),
        artifact_path: row.get(4)?,
        sha1: row.get(5)?,
        size: row.get(6)?,
        download_url: row.get(7)?,
        rules_json: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, LibraryRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = LibraryRepo::new(db.pool().clone());
        (db, repo)
    }

    fn entry(name: &str, native: bool) -> LibraryEntry {
        LibraryEntry {
            version_id: "26.3".to_string(),
            name: name.to_string(),
            classifier: if native {
                "natives-windows".to_string()
            } else {
                String::new()
            },
            is_native: native,
            artifact_path: format!("{name}.jar"),
            sha1: Some("abc".to_string()),
            size: Some(10),
            download_url: None,
            rules_json: None,
        }
    }

    #[test]
    fn replace_all_replaces_previous_entries() {
        let (_db, repo) = repo();
        repo.replace_all("26.3", &[entry("a", false), entry("b", true)])
            .unwrap();
        assert_eq!(repo.count("26.3").unwrap(), 2);

        repo.replace_all("26.3", &[entry("a", false)]).unwrap();
        assert_eq!(repo.count("26.3").unwrap(), 1);
    }

    #[test]
    fn natives_are_listed_last() {
        let (_db, repo) = repo();
        repo.replace_all("26.3", &[entry("zzz-native", true), entry("aaa", false)])
            .unwrap();

        let libs = repo.list("26.3").unwrap();
        assert_eq!(libs[0].name, "aaa");
        assert!(!libs[0].is_native);
        assert!(libs[1].is_native);
    }
}
