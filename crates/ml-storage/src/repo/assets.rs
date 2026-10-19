//! Repository for the asset store.
//!
//! Assets are content-addressed: the same object is shared between versions
//! and never downloaded twice. The database records which objects are already
//! on disk, so the launcher can plan a download without touching the disk
//! first.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::storage_error;

/// The asset index of a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    /// Version the index belongs to.
    pub version_id: String,
    /// Index identifier, e.g. `26.3`.
    pub index_id: String,
    /// SHA-1 of the index document.
    pub sha1: String,
    /// Size of the index document in bytes.
    pub size: i64,
    /// Total size of all objects referenced by the index.
    pub total_size: i64,
    /// Number of objects referenced by the index.
    pub object_count: i64,
}

/// One asset object known to the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    /// Version the object was resolved for.
    pub version_id: String,
    /// SHA-1 of the object's content.
    pub hash: String,
    /// Size of the object in bytes.
    pub size: i64,
    /// Where the object is stored on disk.
    pub local_path: String,
    /// When the object was last verified, ISO-8601.
    pub verified_at: String,
}

/// Reads and writes asset indexes and objects.
#[derive(Debug, Clone)]
pub struct AssetRepo {
    pool: SqlitePool,
}

impl AssetRepo {
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

    /// Insert or update an asset index.
    pub fn upsert_index(&self, index: &AssetIndex) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO asset_indexes (version_id, index_id, sha1, size, total_size, object_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(version_id, index_id) DO UPDATE SET
                 sha1 = excluded.sha1,
                 size = excluded.size,
                 total_size = excluded.total_size,
                 object_count = excluded.object_count",
            params![
                index.version_id,
                index.index_id,
                index.sha1,
                index.size,
                index.total_size,
                index.object_count,
            ],
        )
        .map_err(|e| storage_error("cannot upsert asset index", e))?;
        Ok(())
    }

    /// Fetch the asset index of a version.
    pub fn index(&self, version_id: &str) -> Result<Option<AssetIndex>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT version_id, index_id, sha1, size, total_size, object_count
                 FROM asset_indexes WHERE version_id = ?1 LIMIT 1",
                params![version_id],
                row_to_index,
            )
            .optional()
            .map_err(|e| storage_error("cannot read asset index", e))?;
        Ok(row)
    }

    /// Record that an asset object is present on disk.
    pub fn put_object(&self, object: &AssetObject) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO asset_objects (version_id, hash, size, local_path, verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(version_id, hash) DO UPDATE SET
                 size = excluded.size,
                 local_path = excluded.local_path,
                 verified_at = excluded.verified_at",
            params![
                object.version_id,
                object.hash,
                object.size,
                object.local_path,
                now_iso8601()
            ],
        )
        .map_err(|e| storage_error("cannot record asset object", e))?;
        Ok(())
    }

    /// Whether an asset object is already recorded as present.
    pub fn has_object(&self, version_id: &str, hash: &str) -> Result<bool> {
        let conn = self.conn()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_objects WHERE version_id = ?1 AND hash = ?2",
                params![version_id, hash],
                |row| row.get(0),
            )
            .map_err(|e| storage_error("cannot query asset object", e))?;
        Ok(count > 0)
    }

    /// List the recorded objects of a version.
    pub fn list_objects(&self, version_id: &str) -> Result<Vec<AssetObject>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT version_id, hash, size, local_path, verified_at
                 FROM asset_objects WHERE version_id = ?1 ORDER BY hash ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", e))?;
        let rows = stmt
            .query_map(params![version_id], row_to_object)
            .map_err(|e| storage_error("cannot query asset objects", e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read asset objects", e))
    }

    /// Total number of recorded objects of a version.
    pub fn object_count(&self, version_id: &str) -> Result<i64> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM asset_objects WHERE version_id = ?1",
            params![version_id],
            |row| row.get(0),
        )
        .map_err(|e| storage_error("cannot count asset objects", e))
    }

    /// Forget every object recorded for a version.
    pub fn clear_objects(&self, version_id: &str) -> Result<usize> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM asset_objects WHERE version_id = ?1",
            params![version_id],
        )
        .map_err(|e| storage_error("cannot clear asset objects", e))
    }
}

fn row_to_index(row: &Row<'_>) -> rusqlite::Result<AssetIndex> {
    Ok(AssetIndex {
        version_id: row.get(0)?,
        index_id: row.get(1)?,
        sha1: row.get(2)?,
        size: row.get(3)?,
        total_size: row.get(4)?,
        object_count: row.get(5)?,
    })
}

fn row_to_object(row: &Row<'_>) -> rusqlite::Result<AssetObject> {
    Ok(AssetObject {
        version_id: row.get(0)?,
        hash: row.get(1)?,
        size: row.get(2)?,
        local_path: row.get(3)?,
        verified_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, AssetRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = AssetRepo::new(db.pool().clone());
        (db, repo)
    }

    fn object(hash: &str) -> AssetObject {
        AssetObject {
            version_id: "26.3".to_string(),
            hash: hash.to_string(),
            size: 100,
            local_path: format!("C:/assets/objects/{hash}"),
            verified_at: String::new(),
        }
    }

    #[test]
    fn index_round_trip() {
        let (_db, repo) = repo();
        repo.upsert_index(&AssetIndex {
            version_id: "26.3".into(),
            index_id: "26.3".into(),
            sha1: "abc".into(),
            size: 1000,
            total_size: 460_000_000,
            object_count: 5147,
        })
        .unwrap();

        let read = repo.index("26.3").unwrap().unwrap();
        assert_eq!(read.object_count, 5147);
        assert_eq!(read.total_size, 460_000_000);
    }

    #[test]
    fn objects_are_deduplicated_by_hash() {
        let (_db, repo) = repo();
        repo.put_object(&object("aaa")).unwrap();
        repo.put_object(&object("aaa")).unwrap();
        repo.put_object(&object("bbb")).unwrap();

        assert_eq!(repo.object_count("26.3").unwrap(), 2);
        assert!(repo.has_object("26.3", "aaa").unwrap());
        assert!(!repo.has_object("26.3", "ccc").unwrap());
    }

    #[test]
    fn clearing_objects_keeps_the_index() {
        let (_db, repo) = repo();
        repo.upsert_index(&AssetIndex {
            version_id: "26.3".into(),
            index_id: "26.3".into(),
            sha1: "abc".into(),
            size: 1,
            total_size: 1,
            object_count: 1,
        })
        .unwrap();
        repo.put_object(&object("aaa")).unwrap();

        assert_eq!(repo.clear_objects("26.3").unwrap(), 1);
        assert!(repo.index("26.3").unwrap().is_some());
    }
}
