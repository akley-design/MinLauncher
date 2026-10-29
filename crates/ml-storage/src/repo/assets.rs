//! Repository for the asset indexes.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::Result;

use crate::db::{PooledConnection, SqlitePool};
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

/// Reads and writes asset indexes.
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
        crate::db::connection(&self.pool)
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
        .map_err(|e| storage_error("cannot upsert asset index", &e))?;
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
            .map_err(|e| storage_error("cannot read asset index", &e))?;
        Ok(row)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, AssetRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = AssetRepo::new(db.pool().clone());
        (db, repo)
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
}
