//! Repository for known Minecraft versions.

use rusqlite::{OptionalExtension, Row, params};

use ml_core::{Error, Result, VersionKind, VersionMeta};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::{storage_error, to_u32};

const SELECT_COLUMNS: &str = "id, kind, release_time, java_component, java_major, main_class, \
     asset_index_id, asset_index_sha1, asset_index_size, client_sha1, client_size, json_sha1";

/// Reads and writes known Minecraft versions.
#[derive(Debug, Clone)]
pub struct VersionRepo {
    pool: SqlitePool,
}

impl VersionRepo {
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

    /// Insert or update the metadata of a version.
    ///
    /// The `installed` flag and the on-disk path of the version JSON are
    /// deliberately *not* touched: they describe local state, not metadata.
    pub fn upsert(&self, meta: &VersionMeta) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO versions (
                 id, kind, release_time, java_component, java_major, main_class,
                 asset_index_id, asset_index_sha1, asset_index_size,
                 client_sha1, client_size, json_sha1, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 kind = excluded.kind,
                 release_time = excluded.release_time,
                 java_component = excluded.java_component,
                 java_major = excluded.java_major,
                 main_class = excluded.main_class,
                 asset_index_id = excluded.asset_index_id,
                 asset_index_sha1 = excluded.asset_index_sha1,
                 asset_index_size = excluded.asset_index_size,
                 client_sha1 = excluded.client_sha1,
                 client_size = excluded.client_size,
                 json_sha1 = excluded.json_sha1,
                 updated_at = excluded.updated_at",
            params![
                meta.id,
                meta.kind.as_manifest_str(),
                meta.release_time,
                meta.java_component,
                meta.java_major.map(i64::from),
                meta.main_class,
                meta.asset_index_id,
                meta.asset_index_sha1,
                meta.asset_index_size,
                meta.client_sha1,
                meta.client_size,
                meta.json_sha1,
                now_iso8601(),
            ],
        )
        .map_err(|e| storage_error("cannot upsert version", &e))?;
        Ok(())
    }

    /// Insert or update many versions in one transaction.
    ///
    /// The fields that come from the document of the version — the Java it
    /// needs, its asset index, the client — are *not* touched when the
    /// version is already known. The manifest does not carry them, so
    /// writing them would erase what the installation has learned about
    /// the versions that are already on the disk.
    pub fn upsert_many(&self, metas: &[VersionMeta]) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot start transaction", &e))?;
        for meta in metas {
            tx.execute(
                "INSERT INTO versions (
                     id, kind, release_time, java_component, java_major, main_class,
                     asset_index_id, asset_index_sha1, asset_index_size,
                     client_sha1, client_size, json_sha1, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                     kind = excluded.kind,
                     release_time = excluded.release_time,
                     updated_at = excluded.updated_at",
                params![
                    meta.id,
                    meta.kind.as_manifest_str(),
                    meta.release_time,
                    meta.java_component,
                    meta.java_major.map(i64::from),
                    meta.main_class,
                    meta.asset_index_id,
                    meta.asset_index_sha1,
                    meta.asset_index_size,
                    meta.client_sha1,
                    meta.client_size,
                    meta.json_sha1,
                    now_iso8601(),
                ],
            )
            .map_err(|e| storage_error("cannot upsert version", &e))?;
        }
        tx.commit()
            .map_err(|e| storage_error("cannot commit transaction", &e))?;
        Ok(())
    }

    /// Fetch the metadata of a single version.
    pub fn get(&self, id: &str) -> Result<Option<VersionMeta>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM versions WHERE id = ?1"),
                params![id],
                row_to_meta,
            )
            .optional()
            .map_err(|e| storage_error("cannot read version", &e))?;
        Ok(row)
    }

    /// List known versions, newest release first.
    ///
    /// Passing `None` for `kind` returns every version.
    pub fn list(&self, kind: Option<VersionKind>) -> Result<Vec<VersionMeta>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM versions
                 WHERE (?1 IS NULL OR kind = ?1)
                 ORDER BY release_time IS NULL, release_time DESC, id ASC"
            ))
            .map_err(|e| storage_error("cannot prepare statement", &e))?;

        let rows = stmt
            .query_map(params![kind.map(VersionKind::as_manifest_str)], row_to_meta)
            .map_err(|e| storage_error("cannot query versions", &e))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read versions", &e))
    }

    /// List the versions that are installed locally.
    pub fn list_installed(&self) -> Result<Vec<VersionMeta>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM versions WHERE installed = 1
                 ORDER BY release_time IS NULL, release_time DESC, id ASC"
            ))
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map([], row_to_meta)
            .map_err(|e| storage_error("cannot query versions", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read versions", &e))
    }

    /// Mark a version as installed or not.
    pub fn set_installed(&self, id: &str, installed: bool) -> Result<()> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE versions SET installed = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, i64::from(installed), now_iso8601()],
            )
            .map_err(|e| storage_error("cannot update install state", &e))?;
        if changed == 0 {
            return Err(Error::UnknownVersion(id.to_string()));
        }
        Ok(())
    }

    /// Whether a version is installed locally.
    pub fn is_installed(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let value: Option<i64> = conn
            .query_row(
                "SELECT installed FROM versions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| storage_error("cannot read install state", &e))?;
        Ok(value.is_some_and(|v| v != 0))
    }

    /// Remember where the version JSON was written on disk.
    pub fn set_json_path(&self, id: &str, path: &str) -> Result<()> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE versions SET json_path = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, path, now_iso8601()],
            )
            .map_err(|e| storage_error("cannot update json path", &e))?;
        if changed == 0 {
            return Err(Error::UnknownVersion(id.to_string()));
        }
        Ok(())
    }

    /// The on-disk path of the version JSON, if it was recorded.
    pub fn json_path(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT json_path FROM versions WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| storage_error("cannot read json path", &e))
    }

    /// Number of known versions.
    pub fn count(&self) -> Result<i64> {
        let conn = self.conn()?;
        conn.query_row("SELECT COUNT(*) FROM versions", [], |row| row.get(0))
            .map_err(|e| storage_error("cannot count versions", &e))
    }

    /// Forget a version entirely.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute("DELETE FROM versions WHERE id = ?1", params![id])
            .map_err(|e| storage_error("cannot delete version", &e))?;
        Ok(changed > 0)
    }
}

fn row_to_meta(row: &Row<'_>) -> rusqlite::Result<VersionMeta> {
    let kind: String = row.get(1)?;
    Ok(VersionMeta {
        id: row.get(0)?,
        kind: VersionKind::from_manifest(&kind),
        release_time: row.get(2)?,
        java_component: row.get(3)?,
        java_major: to_u32(row.get(4)?),
        main_class: row.get(5)?,
        asset_index_id: row.get(6)?,
        asset_index_sha1: row.get(7)?,
        asset_index_size: row.get(8)?,
        client_sha1: row.get(9)?,
        client_size: row.get(10)?,
        json_sha1: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, VersionRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = VersionRepo::new(db.pool().clone());
        (db, repo)
    }

    fn meta(id: &str, kind: VersionKind) -> VersionMeta {
        VersionMeta {
            id: id.to_string(),
            kind,
            release_time: Some("2026-10-01T00:00:00+00:00".to_string()),
            java_component: Some("java-runtime-epsilon".to_string()),
            java_major: Some(25),
            main_class: Some("net.minecraft.client.main.Main".to_string()),
            asset_index_id: Some("26.3".to_string()),
            asset_index_sha1: Some("abc".to_string()),
            asset_index_size: Some(1024),
            client_sha1: Some("def".to_string()),
            client_size: Some(2048),
            json_sha1: Some("123".to_string()),
        }
    }

    #[test]
    fn upsert_and_read_back() {
        let (_db, repo) = repo();
        let m = meta("26.3", VersionKind::Release);
        repo.upsert(&m).unwrap();
        let read = repo.get("26.3").unwrap().unwrap();
        assert_eq!(read, m);
    }

    #[test]
    fn upsert_replaces_metadata_but_keeps_install_state() {
        let (_db, repo) = repo();
        repo.upsert(&meta("26.3", VersionKind::Release)).unwrap();
        repo.set_installed("26.3", true).unwrap();

        let mut changed = meta("26.3", VersionKind::Snapshot);
        changed.main_class = None;
        repo.upsert(&changed).unwrap();

        assert!(repo.is_installed("26.3").unwrap());
        assert_eq!(
            repo.get("26.3").unwrap().unwrap().kind,
            VersionKind::Snapshot
        );
    }

    #[test]
    fn the_bulk_store_keeps_what_the_document_of_the_version_taught() {
        let (_db, repo) = repo();
        repo.upsert(&meta("26.3", VersionKind::Release)).unwrap();
        repo.set_installed("26.3", true).unwrap();

        // What the manifest knows: the identifier, the kind and the date.
        let from_manifest = VersionMeta {
            id: "26.3".to_string(),
            kind: VersionKind::Release,
            release_time: Some("2026-10-20T00:00:00+00:00".to_string()),
            java_component: None,
            java_major: None,
            main_class: None,
            asset_index_id: None,
            asset_index_sha1: None,
            asset_index_size: None,
            client_sha1: None,
            client_size: None,
            json_sha1: None,
        };
        repo.upsert_many(&[from_manifest]).unwrap();

        let read = repo.get("26.3").unwrap().unwrap();
        assert_eq!(
            read.java_component,
            Some("java-runtime-epsilon".to_string())
        );
        assert_eq!(read.java_major, Some(25));
        assert_eq!(
            read.main_class,
            Some("net.minecraft.client.main.Main".to_string())
        );
        assert!(repo.is_installed("26.3").unwrap());
    }

    #[test]
    fn the_bulk_store_puts_the_versions_into_the_list() {
        let (_db, repo) = repo();
        let metas = vec![
            meta("26.3", VersionKind::Release),
            meta("26.4-snapshot-4", VersionKind::Snapshot),
        ];
        repo.upsert_many(&metas).unwrap();

        assert_eq!(repo.count().unwrap(), 2);
        assert_eq!(repo.list(Some(VersionKind::Snapshot)).unwrap().len(), 1);
    }

    #[test]
    fn list_filters_by_kind() {
        let (_db, repo) = repo();
        repo.upsert(&meta("26.3", VersionKind::Release)).unwrap();
        repo.upsert(&meta("26.4-snapshot-4", VersionKind::Snapshot))
            .unwrap();

        assert_eq!(repo.list(None).unwrap().len(), 2);
        assert_eq!(repo.list(Some(VersionKind::Release)).unwrap().len(), 1);
        assert_eq!(repo.list(Some(VersionKind::Snapshot)).unwrap().len(), 1);
        assert_eq!(repo.list(Some(VersionKind::OldBeta)).unwrap().len(), 0);
    }

    #[test]
    fn unknown_version_is_reported() {
        let (_db, repo) = repo();
        assert!(matches!(
            repo.set_installed("nope", true),
            Err(Error::UnknownVersion(_))
        ));
        assert!(repo.get("nope").unwrap().is_none());
    }

    #[test]
    fn installed_listing_only_returns_installed() {
        let (_db, repo) = repo();
        repo.upsert(&meta("a", VersionKind::Release)).unwrap();
        repo.upsert(&meta("b", VersionKind::Release)).unwrap();
        repo.set_installed("b", true).unwrap();

        let installed = repo.list_installed().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, "b");
    }
}
