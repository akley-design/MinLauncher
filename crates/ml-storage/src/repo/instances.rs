//! Repository for game instances and their mods.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::Result;

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::{storage_error, to_bool};

/// A user-created game instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// Stable identifier.
    pub id: String,
    /// Version the instance runs.
    pub version_id: String,
    /// Display name.
    pub name: String,
    /// Directory holding `saves`, `resourcepacks` and friends.
    pub game_dir: String,
    /// Explicit Java executable, overriding the bundled runtime.
    pub java_path: Option<String>,
    /// Extra JVM arguments for this instance.
    pub jvm_args: Option<String>,
    /// When the instance was created, ISO-8601.
    pub created_at: String,
}

/// A mod installed into an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceMod {
    /// Owning instance.
    pub instance_id: String,
    /// Mod identifier (project id or file name).
    pub mod_id: String,
    /// Mod version.
    pub version: Option<String>,
    /// Whether the mod is active.
    pub enabled: bool,
    /// Where the mod came from.
    pub source: Option<String>,
}

/// Reads and writes game instances.
#[derive(Debug, Clone)]
pub struct InstanceRepo {
    pool: SqlitePool,
}

impl InstanceRepo {
    /// Create a repository backed by the given pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn conn(&self) -> Result<PooledConnection> {
        crate::db::connection(&self.pool)
    }

    /// Insert or update an instance.
    pub fn upsert(&self, instance: &Instance) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO instances (id, version_id, name, game_dir, java_path, jvm_args, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                 version_id = excluded.version_id,
                 name = excluded.name,
                 game_dir = excluded.game_dir,
                 java_path = excluded.java_path,
                 jvm_args = excluded.jvm_args",
            params![
                instance.id,
                instance.version_id,
                instance.name,
                instance.game_dir,
                instance.java_path,
                instance.jvm_args,
                if instance.created_at.is_empty() { now_iso8601() } else { instance.created_at.clone() },
            ],
        )
        .map_err(|e| storage_error("cannot upsert instance", &e))?;
        Ok(())
    }

    /// Fetch a single instance.
    pub fn get(&self, id: &str) -> Result<Option<Instance>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, version_id, name, game_dir, java_path, jvm_args, created_at
                 FROM instances WHERE id = ?1",
                params![id],
                row_to_instance,
            )
            .optional()
            .map_err(|e| storage_error("cannot read instance", &e))?;
        Ok(row)
    }

    /// List every instance.
    pub fn list(&self) -> Result<Vec<Instance>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, version_id, name, game_dir, java_path, jvm_args, created_at
                 FROM instances ORDER BY created_at ASC, name ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map([], row_to_instance)
            .map_err(|e| storage_error("cannot query instances", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read instances", &e))
    }

    /// Delete an instance and its mods.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute("DELETE FROM instances WHERE id = ?1", params![id])
            .map_err(|e| storage_error("cannot delete instance", &e))?;
        Ok(changed > 0)
    }

    /// Add or update a mod of an instance.
    pub fn upsert_mod(&self, mod_entry: &InstanceMod) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO instance_mods (instance_id, mod_id, version, enabled, source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(instance_id, mod_id) DO UPDATE SET
                 version = excluded.version,
                 enabled = excluded.enabled,
                 source = excluded.source",
            params![
                mod_entry.instance_id,
                mod_entry.mod_id,
                mod_entry.version,
                i64::from(mod_entry.enabled),
                mod_entry.source,
            ],
        )
        .map_err(|e| storage_error("cannot upsert mod", &e))?;
        Ok(())
    }

    /// List the mods of an instance.
    pub fn list_mods(&self, instance_id: &str) -> Result<Vec<InstanceMod>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT instance_id, mod_id, version, enabled, source
                 FROM instance_mods WHERE instance_id = ?1 ORDER BY mod_id ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map(params![instance_id], row_to_mod)
            .map_err(|e| storage_error("cannot query mods", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read mods", &e))
    }

    /// Remove a mod from an instance.
    pub fn remove_mod(&self, instance_id: &str, mod_id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "DELETE FROM instance_mods WHERE instance_id = ?1 AND mod_id = ?2",
                params![instance_id, mod_id],
            )
            .map_err(|e| storage_error("cannot remove mod", &e))?;
        Ok(changed > 0)
    }
}

fn row_to_instance(row: &Row<'_>) -> rusqlite::Result<Instance> {
    Ok(Instance {
        id: row.get(0)?,
        version_id: row.get(1)?,
        name: row.get(2)?,
        game_dir: row.get(3)?,
        java_path: row.get(4)?,
        jvm_args: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_to_mod(row: &Row<'_>) -> rusqlite::Result<InstanceMod> {
    Ok(InstanceMod {
        instance_id: row.get(0)?,
        mod_id: row.get(1)?,
        version: row.get(2)?,
        enabled: to_bool(row.get(3)?),
        source: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, InstanceRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = InstanceRepo::new(db.pool().clone());
        (db, repo)
    }

    fn instance(id: &str) -> Instance {
        Instance {
            id: id.to_string(),
            version_id: "26.3".to_string(),
            name: format!("instance {id}"),
            game_dir: format!("C:/instances/{id}"),
            java_path: None,
            jvm_args: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn upsert_and_read_back() {
        let (_db, repo) = repo();
        repo.upsert(&instance("a")).unwrap();
        let read = repo.get("a").unwrap().unwrap();
        assert_eq!(read.version_id, "26.3");
        assert_eq!(read.name, "instance a");
    }

    #[test]
    fn mods_are_kept_per_instance() {
        let (_db, repo) = repo();
        repo.upsert(&instance("a")).unwrap();
        repo.upsert_mod(&InstanceMod {
            instance_id: "a".into(),
            mod_id: "sodium".into(),
            version: Some("0.5.8".into()),
            enabled: true,
            source: Some("modrinth".into()),
        })
        .unwrap();

        let mods = repo.list_mods("a").unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].mod_id, "sodium");
        assert!(mods[0].enabled);
    }

    #[test]
    fn deleting_an_instance_removes_its_mods() {
        let (_db, repo) = repo();
        repo.upsert(&instance("a")).unwrap();
        repo.upsert_mod(&InstanceMod {
            instance_id: "a".into(),
            mod_id: "sodium".into(),
            version: None,
            enabled: true,
            source: None,
        })
        .unwrap();

        assert!(repo.delete("a").unwrap());
        assert_eq!(repo.list_mods("a").unwrap().len(), 0);
    }
}
