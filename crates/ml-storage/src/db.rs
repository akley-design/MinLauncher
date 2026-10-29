//! Database connection management.
//!
//! The launcher opens exactly one SQLite file and keeps a small connection
//! pool in front of it. SQLite is an excellent fit here: the working set is
//! small and entirely local, reads dominate, and a single file with WAL
//! journaling gives the UI instant reads while a download is writing.
//!
//! Pragmas come in two flavours, and the difference matters:
//!
//! * `journal_mode` is a property of the *database file*. It is set once,
//!   right after the pool is built.
//! * `foreign_keys`, `synchronous`, `busy_timeout`, `cache_size` and
//!   `temp_store` are properties of a *connection*. They are applied through
//!   the manager's init hook, so every connection the pool ever hands out
//!   carries them — not just the first one.

use std::path::{Path, PathBuf};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use ml_core::{Error, Result};

use crate::migrations::{MIGRATION_TABLE, MIGRATIONS};

/// Type alias for the connection pool used across the storage layer.
pub type SqlitePool = Pool<SqliteConnectionManager>;

/// A connection borrowed from the pool.
///
/// The type parameter of `PooledConnection` is the *manager*, not the
/// connection; the connection itself is reached through `Deref`.
pub type PooledConnection = r2d2::PooledConnection<SqliteConnectionManager>;

/// Borrow a connection, flattening the pool error into a storage error.
///
/// # Errors
/// Fails when the pool cannot hand out a connection.
pub fn connection(pool: &SqlitePool) -> Result<PooledConnection> {
    pool.get()
        .map_err(|e| Error::Storage(format!("cannot acquire connection: {e}")))
}

/// Handle to the launcher's database.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    path: Option<PathBuf>,
}

impl Database {
    /// Open (or create) a database file and apply any pending migrations.
    ///
    /// # Errors
    /// Fails if the file cannot be opened, if the schema cannot be migrated,
    /// or if the directory does not exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }

        let manager = SqliteConnectionManager::file(path).with_init(apply_connection_pragmas);
        let db = Self::build(manager, Some(path.to_path_buf()))?;
        db.enable_wal()?;
        db.migrate()?;
        Ok(db)
    }

    /// Open a private in-memory database, used by tests.
    ///
    /// # Errors
    /// Fails if the pool cannot be created or the schema cannot be applied.
    pub fn open_in_memory() -> Result<Self> {
        // `SqliteConnectionManager::memory` keeps one connection alive for
        // the lifetime of the manager, so every pooled connection sees the
        // same in-memory database instead of an empty one of its own.
        let manager = SqliteConnectionManager::memory().with_init(apply_connection_pragmas);
        let db = Self::build(manager, None)?;
        db.migrate()?;
        Ok(db)
    }

    fn build(manager: SqliteConnectionManager, path: Option<PathBuf>) -> Result<Self> {
        let pool = Pool::builder()
            .max_size(8)
            .connection_timeout(std::time::Duration::from_secs(10))
            .build(manager)
            .map_err(|e| Error::Storage(format!("cannot create connection pool: {e}")))?;
        Ok(Self { pool, path })
    }

    /// Borrow a connection from the pool.
    ///
    /// # Errors
    /// Fails if no connection becomes available within the pool timeout.
    pub fn connection(&self) -> Result<PooledConnection> {
        self.pool
            .get()
            .map_err(|e| Error::Storage(format!("cannot acquire connection: {e}")))
    }

    /// The underlying connection pool.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Path of the database file, or `None` for in-memory databases.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Switch the database file to WAL journaling.
    ///
    /// WAL lets readers proceed while a writer holds the lock, which is what
    /// keeps the UI responsive while assets are being downloaded.
    fn enable_wal(&self) -> Result<()> {
        let conn = self.connection()?;
        // `journal_mode` returns the resulting mode, so it must be read with
        // `query_row` rather than set with `pragma_update`.
        let _: String = conn
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|e| Error::Storage(format!("cannot enable WAL: {e}")))?;
        Ok(())
    }

    /// Apply every migration that has not been applied yet.
    fn migrate(&self) -> Result<()> {
        let mut conn = self.connection()?;
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATION_TABLE} (
                version    INTEGER PRIMARY KEY,
                name       TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );"
        ))
        .map_err(|e| Error::Storage(format!("cannot create migration table: {e}")))?;

        let current: i64 = conn
            .query_row(
                &format!("SELECT COALESCE(MAX(version), 0) FROM {MIGRATION_TABLE}"),
                [],
                |row| row.get(0),
            )
            .map_err(|e| Error::Storage(format!("cannot read schema version: {e}")))?;

        for migration in MIGRATIONS.iter().filter(|m| m.version > current) {
            let tx = conn
                .transaction()
                .map_err(|e| Error::Storage(format!("cannot start transaction: {e}")))?;
            tx.execute_batch(migration.sql).map_err(|e| {
                Error::Storage(format!("migration `{}` failed: {e}", migration.name))
            })?;
            tx.execute(
                &format!(
                    "INSERT INTO {MIGRATION_TABLE} (version, name, applied_at) VALUES (?1, ?2, ?3)"
                ),
                rusqlite::params![migration.version, migration.name, now_iso8601()],
            )
            .map_err(|e| Error::Storage(format!("cannot record migration: {e}")))?;
            tx.commit()
                .map_err(|e| Error::Storage(format!("cannot commit migration: {e}")))?;
            tracing::info!(
                version = migration.version,
                name = migration.name,
                "applied migration"
            );
        }
        Ok(())
    }

    /// The schema version currently stored in the database.
    ///
    /// # Errors
    /// Fails if the migration table cannot be read.
    pub fn schema_version(&self) -> Result<i64> {
        let conn = self.connection()?;
        conn.query_row(
            &format!("SELECT COALESCE(MAX(version), 0) FROM {MIGRATION_TABLE}"),
            [],
            |row| row.get(0),
        )
        .map_err(|e| Error::Storage(format!("cannot read schema version: {e}")))
    }
}

/// Apply the per-connection pragmas to a freshly opened connection.
fn apply_connection_pragmas(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5_000i64)?;
    // Keep temporary structures in RAM; the launcher never spills large
    // intermediates to disk.
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    // A negative cache size is expressed in kibibytes.
    conn.pragma_update(None, "cache_size", -16_384i64)?;
    Ok(())
}

/// Current UTC time in RFC 3339 format, used for all timestamp columns.
#[must_use]
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::latest_version;

    #[test]
    fn in_memory_database_is_migrated() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), latest_version());
    }

    #[test]
    fn opening_twice_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("test.db");
        let first = Database::open(&path).unwrap();
        assert_eq!(first.schema_version().unwrap(), latest_version());
        drop(first);
        let second = Database::open(&path).unwrap();
        assert_eq!(second.schema_version().unwrap(), latest_version());
    }

    #[test]
    fn tables_are_created() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.connection().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'versions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn every_pooled_connection_carries_the_pragmas() {
        // The pragmas are per-connection, so this has to hold for a
        // connection the pool hands out *later*, not just the first one.
        let db = Database::open_in_memory().unwrap();
        for _ in 0..4 {
            let conn = db.connection().unwrap();
            let foreign_keys: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 1);
        }
    }

    #[test]
    fn file_database_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("wal.db")).unwrap();
        let conn = db.connection().unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn the_new_column_lands_on_an_old_database_without_losing_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");

        // A database of the previous schema: the first migration, one row
        // in the queue, and nothing else.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {MIGRATION_TABLE} (
                    version    INTEGER PRIMARY KEY,
                    name       TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );"
            ))
            .unwrap();
            conn.execute_batch(MIGRATIONS[0].sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, name, applied_at)
                 VALUES (1, 'core_schema', '2026-10-20T10:00:00Z')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO downloads
                     (url, sha1, size, target_path, state, bytes_done, attempts, error, updated_at)
                 VALUES ('a.jar', NULL, NULL, '/tmp/a.jar', 'completed', 10, 1, NULL, '2026-10-20T10:00:00Z')",
                [],
            )
            .unwrap();
        }

        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), latest_version());

        let conn = db.connection().unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "the row of the old database survived");

        let task: Option<String> = conn
            .query_row(
                "SELECT task FROM downloads WHERE url = 'a.jar'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(task, None, "the old row belongs to no task");
    }
}
