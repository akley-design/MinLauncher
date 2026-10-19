//! Database schema and migrations.
//!
//! Migrations are plain SQL statements applied in order. Each migration has a
//! monotonically increasing version; the applied version is recorded in
//! `schema_migrations`, so opening an existing database is cheap (one query)
//! and opening a fresh one replays the whole history.

/// Name of the table that records which migrations have been applied.
pub const MIGRATION_TABLE: &str = "schema_migrations";

/// A single forward-only migration.
pub struct Migration {
    /// Version number, applied in ascending order.
    pub version: i64,
    /// Human readable name, stored for diagnostics.
    pub name: &'static str,
    /// SQL statements executed inside a single transaction.
    pub sql: &'static str,
}

/// Every migration, in the order they must be applied.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "core_schema",
    sql: r#"
    -- Generic key/value store for launcher state that does not deserve a
    -- table of its own (last selected version, cached manifest ETag, ...).
    CREATE TABLE app_meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    -- Known Minecraft versions. One row per version id, whether or not the
    -- version has been installed locally.
    CREATE TABLE versions (
        id               TEXT PRIMARY KEY,
        kind             TEXT NOT NULL,
        release_time     TEXT,
        java_component   TEXT,
        java_major       INTEGER,
        main_class       TEXT,
        asset_index_id   TEXT,
        asset_index_sha1 TEXT,
        asset_index_size INTEGER,
        client_sha1      TEXT,
        client_size      INTEGER,
        json_sha1        TEXT,
        json_path        TEXT,
        installed        INTEGER NOT NULL DEFAULT 0,
        updated_at       TEXT NOT NULL
    );
    CREATE INDEX idx_versions_kind ON versions(kind);

    -- Asset indexes, one row per (version, index) pair.
    CREATE TABLE asset_indexes (
        version_id   TEXT NOT NULL,
        index_id     TEXT NOT NULL,
        sha1         TEXT NOT NULL,
        size         INTEGER NOT NULL,
        total_size   INTEGER NOT NULL,
        object_count INTEGER NOT NULL,
        PRIMARY KEY (version_id, index_id)
    );

    -- Individual asset objects. The primary key is the content hash, which
    -- makes the store content-addressed: the same object is never stored
    -- twice, and a partially completed download can be resumed safely.
    CREATE TABLE asset_objects (
        version_id  TEXT NOT NULL,
        hash        TEXT NOT NULL,
        size        INTEGER NOT NULL,
        local_path  TEXT NOT NULL,
        verified_at TEXT NOT NULL,
        PRIMARY KEY (version_id, hash)
    );
    CREATE INDEX idx_asset_objects_version ON asset_objects(version_id);

    -- Resolved libraries for a version, after rule evaluation.
    CREATE TABLE libraries (
        version_id    TEXT NOT NULL,
        name          TEXT NOT NULL,
        classifier    TEXT NOT NULL DEFAULT '',
        is_native     INTEGER NOT NULL DEFAULT 0,
        artifact_path TEXT NOT NULL,
        sha1          TEXT,
        size          INTEGER,
        download_url  TEXT,
        rules_json    TEXT,
        PRIMARY KEY (version_id, name, classifier)
    );
    CREATE INDEX idx_libraries_version ON libraries(version_id);

    -- Download queue and its progress. Survives a launcher restart, so an
    -- interrupted installation can resume instead of starting over.
    CREATE TABLE downloads (
        url         TEXT PRIMARY KEY,
        sha1        TEXT,
        size        INTEGER,
        target_path TEXT NOT NULL,
        state       TEXT NOT NULL,
        bytes_done  INTEGER NOT NULL DEFAULT 0,
        attempts    INTEGER NOT NULL DEFAULT 0,
        error       TEXT,
        updated_at  TEXT NOT NULL
    );
    CREATE INDEX idx_downloads_state ON downloads(state);

    -- Player accounts. Access tokens live in the encrypted secrets file,
    -- never in this table.
    CREATE TABLE accounts (
        id          TEXT PRIMARY KEY,
        uuid        TEXT NOT NULL,
        username    TEXT NOT NULL,
        kind        TEXT NOT NULL,
        uuid_prefix TEXT NOT NULL,
        selected    INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL,
        last_used   TEXT
    );

    -- User-created game instances.
    CREATE TABLE instances (
        id         TEXT PRIMARY KEY,
        version_id TEXT NOT NULL,
        name       TEXT NOT NULL,
        game_dir   TEXT NOT NULL,
        java_path  TEXT,
        jvm_args   TEXT,
        created_at TEXT NOT NULL
    );
    CREATE INDEX idx_instances_version ON instances(version_id);

    -- Mods installed into an instance. The rows belong to the instance, so
    -- deleting an instance takes its mods with it.
    CREATE TABLE instance_mods (
        instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
        mod_id      TEXT NOT NULL,
        version     TEXT,
        enabled     INTEGER NOT NULL DEFAULT 1,
        source      TEXT,
        PRIMARY KEY (instance_id, mod_id)
    );

    -- Downloaded Java runtimes.
    CREATE TABLE jres (
        component  TEXT NOT NULL,
        platform   TEXT NOT NULL,
        version    TEXT NOT NULL,
        path       TEXT NOT NULL,
        status     TEXT NOT NULL,
        size       INTEGER,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (component, platform)
    );

    -- Benchmark results, one row per measurement series.
    CREATE TABLE benchmarks (
        id         TEXT PRIMARY KEY,
        suite      TEXT NOT NULL,
        metric     TEXT NOT NULL,
        scenario   TEXT NOT NULL,
        samples    INTEGER NOT NULL,
        p50        REAL NOT NULL,
        p95        REAL NOT NULL,
        p99        REAL NOT NULL,
        mean       REAL NOT NULL,
        stddev     REAL NOT NULL,
        unit       TEXT NOT NULL,
        created_at TEXT NOT NULL,
        meta_json  TEXT
    );
    CREATE INDEX idx_benchmarks_metric ON benchmarks(metric, scenario);
    CREATE INDEX idx_benchmarks_created ON benchmarks(created_at);
    "#,
}];

/// The schema version a freshly created database ends up at.
#[must_use]
pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_contiguous_and_ordered() {
        let mut expected = 1;
        for migration in MIGRATIONS {
            assert_eq!(
                migration.version, expected,
                "migration `{}` is out of order",
                migration.name
            );
            expected += 1;
        }
    }

    #[test]
    fn latest_version_matches_last_migration() {
        assert_eq!(latest_version(), MIGRATIONS.last().unwrap().version);
    }
}
