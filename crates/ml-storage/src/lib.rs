//! Persistence layer for MinLauncher.
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

pub use db::{Database, SqlitePool, now_iso8601};
pub use json_store::JsonStore;
pub use migrations::{MIGRATION_TABLE, MIGRATIONS, Migration, latest_version};
pub use repo::accounts::AccountRepo;
pub use repo::assets::{AssetIndex, AssetObject, AssetRepo};
pub use repo::benchmarks::{BenchmarkRecord, BenchmarkRepo};
pub use repo::downloads::{DownloadEntry, DownloadRepo, DownloadState};
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
}
