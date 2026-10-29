//! Persistence layer for `MinLauncher`.
//!
//! Two storage mechanisms are used side by side:
//!
//! * **SQLite** for everything that is queried, indexed or grows: versions,
//!   libraries, assets, the download queue, accounts, instances, runtimes
//!   and benchmark results.
//! * **JSON** for everything that is small, hand-editable and read once:
//!   the launcher settings, and the encrypted secrets file holding account
//!   tokens.
//!
//! The split matters for security: an access token must never end up in the
//! database, because databases get copied into bug reports. Tokens live in
//! [`secrets`], keyed by account id.
//!
//! ```no_run
//! use ml_core::AppPaths;
//! use ml_storage::Storage;
//!
//! # fn main() -> ml_core::Result<()> {
//! let paths = AppPaths::discover()?;
//! let storage = Storage::open(&paths)?;
//! println!("{} known versions", storage.versions().count()?);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod db;
pub mod json_store;
pub mod migrations;
pub mod repo;
pub mod secrets;

use ml_core::{AppPaths, Error, LauncherConfig, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

pub use db::{Database, SqlitePool, now_iso8601};
pub use json_store::JsonStore;
pub use migrations::{MIGRATION_TABLE, MIGRATIONS, Migration, latest_version};
pub use repo::accounts::AccountRepo;
pub use repo::assets::{AssetIndex, AssetRepo};
pub use repo::benchmarks::{BenchmarkRecord, BenchmarkRepo};
pub use repo::downloads::{DownloadEntry, DownloadRepo, DownloadState, DownloadUpdate};
pub use repo::instances::{Instance, InstanceMod, InstanceRepo};
pub use repo::jres::{JreEntry, JreRepo, JreStatus};
pub use repo::libraries::{LibraryEntry, LibraryRepo};
pub use repo::versions::VersionRepo;
pub use secrets::{SecretsFile, SecretsStore};

/// Everything the launcher persists, in one handle.
///
/// The Tauri layer keeps a single `Storage` in its managed state and hands
/// the repositories to the command handlers, so no layer below the UI ever
/// touches the filesystem or the database directly.
#[derive(Debug)]
pub struct Storage {
    database: Database,
    versions: VersionRepo,
    accounts: AccountRepo,
    assets: AssetRepo,
    libraries: LibraryRepo,
    downloads: DownloadRepo,
    jres: JreRepo,
    instances: InstanceRepo,
    benchmarks: BenchmarkRepo,
    settings: JsonStore<LauncherConfig>,
    secrets: SecretsStore,
}

/// Local state for a globally reserved offline nickname.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NicknameClaim {
    /// Local account identifier used to find its keyring token.
    pub account_id: String,
    /// Nickname shown to the player.
    pub nickname: String,
    /// `reserved`, `conflict`, or `release_pending`.
    pub status: String,
}

impl Storage {
    /// Open the storage layer rooted at the given paths.
    ///
    /// Every directory is created if missing, the database is migrated to the
    /// latest schema, and the settings file is created with defaults if it is
    /// missing.
    pub fn open(paths: &AppPaths) -> Result<Self> {
        paths.ensure_dirs()?;
        let database = Database::open(&paths.database_path)?;
        let pool = database.pool().clone();

        let settings = JsonStore::<LauncherConfig>::new(&paths.settings_path);
        if !paths.settings_path.exists() {
            settings.save(&LauncherConfig::default())?;
        }

        Ok(Self {
            versions: VersionRepo::new(pool.clone()),
            accounts: AccountRepo::new(pool.clone()),
            assets: AssetRepo::new(pool.clone()),
            libraries: LibraryRepo::new(pool.clone()),
            downloads: DownloadRepo::new(pool.clone()),
            jres: JreRepo::new(pool.clone()),
            instances: InstanceRepo::new(pool.clone()),
            benchmarks: BenchmarkRepo::new(pool),
            settings,
            secrets: SecretsStore::new(&paths.secrets_path),
            database,
        })
    }

    /// The underlying database handle.
    #[must_use]
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Build a storage layer around an already open database.
    ///
    /// The settings and the secrets are not touched: the caller is
    /// responsible for them. It is the way the benchmark harness works
    /// with a database it opened itself.
    #[must_use]
    pub fn from_database(database: Database) -> Self {
        let pool = database.pool().clone();

        Self {
            versions: VersionRepo::new(pool.clone()),
            accounts: AccountRepo::new(pool.clone()),
            assets: AssetRepo::new(pool.clone()),
            libraries: LibraryRepo::new(pool.clone()),
            downloads: DownloadRepo::new(pool.clone()),
            jres: JreRepo::new(pool.clone()),
            instances: InstanceRepo::new(pool.clone()),
            benchmarks: BenchmarkRepo::new(pool),
            settings: JsonStore::new(std::path::PathBuf::from("settings.json")),
            secrets: SecretsStore::new(std::path::PathBuf::from("secrets.json")),
            database,
        }
    }

    /// Repository of known Minecraft versions.
    #[must_use]
    pub fn versions(&self) -> &VersionRepo {
        &self.versions
    }

    /// Repository of player accounts.
    #[must_use]
    pub fn accounts(&self) -> &AccountRepo {
        &self.accounts
    }

    /// Repository of assets.
    #[must_use]
    pub fn assets(&self) -> &AssetRepo {
        &self.assets
    }

    /// Repository of libraries.
    #[must_use]
    pub fn libraries(&self) -> &LibraryRepo {
        &self.libraries
    }

    /// Repository of the download queue.
    #[must_use]
    pub fn downloads(&self) -> &DownloadRepo {
        &self.downloads
    }

    /// Repository of Java runtimes.
    #[must_use]
    pub fn jres(&self) -> &JreRepo {
        &self.jres
    }

    /// Repository of game instances.
    #[must_use]
    pub fn instances(&self) -> &InstanceRepo {
        &self.instances
    }

    /// Repository of benchmark results.
    #[must_use]
    pub fn benchmarks(&self) -> &BenchmarkRepo {
        &self.benchmarks
    }

    /// The launcher settings.
    #[must_use]
    pub fn settings(&self) -> &JsonStore<LauncherConfig> {
        &self.settings
    }

    /// The encrypted secrets store.
    #[must_use]
    pub fn secrets(&self) -> &SecretsStore {
        &self.secrets
    }

    /// Read the launcher settings.
    pub fn load_settings(&self) -> Result<LauncherConfig> {
        self.settings.load()
    }

    /// Write the launcher settings.
    pub fn save_settings(&self, config: &LauncherConfig) -> Result<()> {
        self.settings.save(config)
    }

    /// Forget everything that points into the game installation directory.
    ///
    /// The database stores absolute paths of the files of the game. When
    /// the player moves the installation to another folder, those paths
    /// describe files that are no longer a part of the launcher's world:
    /// the new folder is scanned after this call and the versions that
    /// are found there are written down again.
    ///
    /// The rows that only describe *metadata* — the known versions, the
    /// accounts, the instances — are left alone.
    ///
    /// # Errors
    /// Fails when the transaction cannot be executed.
    pub fn forget_local_paths(&self) -> Result<()> {
        let mut conn = self.database.connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Storage(format!("cannot start transaction: {e}")))?;

        for table in ["downloads", "libraries", "asset_objects", "jres"] {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(|e| Error::Storage(format!("cannot clear {table}: {e}")))?;
        }

        tx.execute(
            "UPDATE versions SET installed = 0, json_path = NULL, updated_at = ?1",
            [now_iso8601()],
        )
        .map_err(|e| Error::Storage(format!("cannot reset the installed versions: {e}")))?;

        tx.commit()
            .map_err(|e| Error::Storage(format!("cannot commit the reset: {e}")))
    }

    /// Read a value from the generic key/value table.
    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.database.connection()?;
        conn.query_row("SELECT value FROM app_meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| Error::Storage(format!("cannot read meta value: {e}")))
    }

    /// Write a value into the generic key/value table.
    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.database.connection()?;
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| Error::Storage(format!("cannot write meta value: {e}")))?;
        Ok(())
    }

    /// Save the local state of an offline nickname reservation.
    pub fn nickname_claim_set(&self, account_id: &str, nickname: &str, status: &str) -> Result<()> {
        if !matches!(status, "reserved" | "conflict" | "release_pending") {
            return Err(Error::Storage("invalid nickname claim status".into()));
        }
        let conn = self.database.connection()?;
        conn.execute(
            "INSERT INTO nickname_claims (account_id, nickname, status, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id) DO UPDATE SET nickname = excluded.nickname, status = excluded.status, updated_at = excluded.updated_at",
            rusqlite::params![account_id, nickname, status, now_iso8601()],
        ).map_err(|e| Error::Storage(format!("cannot save nickname claim: {e}")))?;
        Ok(())
    }

    /// Read local nickname claims, including queued releases for deleted accounts.
    pub fn nickname_claims(&self) -> Result<Vec<NicknameClaim>> {
        let conn = self.database.connection()?;
        let mut statement = conn
            .prepare("SELECT account_id, nickname, status FROM nickname_claims ORDER BY account_id")
            .map_err(|e| Error::Storage(format!("cannot read nickname claims: {e}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(NicknameClaim {
                    account_id: row.get(0)?,
                    nickname: row.get(1)?,
                    status: row.get(2)?,
                })
            })
            .map_err(|e| Error::Storage(format!("cannot read nickname claims: {e}")))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| Error::Storage(format!("cannot read nickname claims: {e}")))
    }

    /// Queue release of a claim while preserving its keyring token.
    pub fn nickname_claim_queue_release(&self, account_id: &str) -> Result<bool> {
        let conn = self.database.connection()?;
        let changed = conn.execute(
            "UPDATE nickname_claims SET status = 'release_pending', updated_at = ?2 WHERE account_id = ?1",
            rusqlite::params![account_id, now_iso8601()],
        ).map_err(|e| Error::Storage(format!("cannot queue nickname release: {e}")))?;
        Ok(changed > 0)
    }

    /// Forget a local claim after the server confirms release.
    pub fn nickname_claim_remove(&self, account_id: &str) -> Result<()> {
        let conn = self.database.connection()?;
        conn.execute(
            "DELETE FROM nickname_claims WHERE account_id = ?1",
            [account_id],
        )
        .map_err(|e| Error::Storage(format!("cannot remove nickname claim: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let storage = Storage::open(&paths).unwrap();
        (dir, storage)
    }

    #[test]
    fn opening_creates_the_whole_tree() {
        let (dir, _storage) = storage();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        for required in paths.all_dirs() {
            assert!(required.is_dir(), "missing {}", required.display());
        }
        assert!(paths.database_path.is_file());
        assert!(paths.settings_path.is_file());
    }

    #[test]
    fn settings_default_to_the_built_in_defaults() {
        let (_dir, storage) = storage();
        let config = storage.load_settings().unwrap();
        assert_eq!(config, LauncherConfig::default());
    }

    #[test]
    fn settings_round_trip() {
        let (_dir, storage) = storage();
        let mut config = storage.load_settings().unwrap();
        config.default_version = Some("26.3".to_string());
        storage.save_settings(&config).unwrap();

        assert_eq!(
            storage.load_settings().unwrap().default_version.as_deref(),
            Some("26.3")
        );
    }

    #[test]
    fn meta_values_round_trip() {
        let (_dir, storage) = storage();
        assert!(storage.meta_get("last_version").unwrap().is_none());
        storage.meta_set("last_version", "26.3").unwrap();
        assert_eq!(
            storage.meta_get("last_version").unwrap().as_deref(),
            Some("26.3")
        );
    }

    #[test]
    fn nickname_release_survives_account_deletion_until_confirmed() {
        let (_dir, storage) = storage();
        storage
            .nickname_claim_set("offline-u1", "Steve", "reserved")
            .unwrap();
        assert!(storage.nickname_claim_queue_release("offline-u1").unwrap());
        let claims = storage.nickname_claims().unwrap();
        assert_eq!(claims[0].status, "release_pending");
        storage.nickname_claim_remove("offline-u1").unwrap();
        assert_eq!(storage.nickname_claims().unwrap(), []);
    }

    #[test]
    fn schema_is_at_the_latest_version() {
        let (_dir, storage) = storage();
        assert_eq!(
            storage.database().schema_version().unwrap(),
            latest_version()
        );
    }

    #[test]
    fn paths_and_database_agree() {
        let (dir, storage) = storage();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        assert_eq!(
            storage.database().path(),
            Some(paths.database_path.as_path())
        );
        assert!(
            paths
                .database_path
                .ends_with(PathBuf::from("minlaucher.db"))
        );
    }

    #[test]
    fn forgetting_the_local_paths_keeps_the_metadata() {
        let (_dir, storage) = storage();
        let mut meta = ml_core::VersionMeta {
            id: "26.3".to_string(),
            kind: ml_core::VersionKind::Release,
            release_time: Some("2026-01-01T00:00:00+00:00".to_string()),
            java_component: Some("java-runtime-epsilon".to_string()),
            java_major: Some(25),
            main_class: Some("net.minecraft.client.main.Main".to_string()),
            asset_index_id: Some("34".to_string()),
            asset_index_sha1: Some("abc".to_string()),
            asset_index_size: Some(1),
            client_sha1: Some("def".to_string()),
            client_size: Some(1),
            json_sha1: None,
        };
        storage.versions().upsert(&meta).unwrap();
        storage
            .versions()
            .set_json_path("26.3", "C:/old/versions/26.3/version.json")
            .unwrap();
        storage.versions().set_installed("26.3", true).unwrap();
        meta.id = "1.0".to_string();
        storage.versions().upsert(&meta).unwrap();
        storage.versions().set_installed("1.0", true).unwrap();

        let conn = storage.database().connection().unwrap();
        conn.execute(
            "INSERT INTO downloads (url, state, target_path, bytes_done, attempts, updated_at)
             VALUES ('https://example/a', 'completed', 'C:/old/a', 1, 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_objects (version_id, hash, size, local_path, verified_at)
             VALUES ('26.3', 'abc', 1, 'C:/old/a', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO libraries (version_id, name, artifact_path)
             VALUES ('26.3', 'g:a:1', 'g/a/1/a.jar')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jres (component, platform, version, path, status, updated_at)
             VALUES ('jre-legacy', 'windows-x64', '8', 'C:/old/jre', 'ready', 'now')",
            [],
        )
        .unwrap();
        drop(conn);

        storage.forget_local_paths().unwrap();

        let conn = storage.database().connection().unwrap();
        for table in ["downloads", "asset_objects", "libraries", "jres"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} was not cleared");
        }
        let (installed, json_path): (i64, Option<String>) = conn
            .query_row(
                "SELECT installed, json_path FROM versions WHERE id = '26.3'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(installed, 0);
        assert!(json_path.is_none());
        assert_eq!(storage.versions().count().unwrap(), 2);
    }
}
