//! Repository for benchmark results.
//!
//! Measurements are kept so that the "at least 20% faster than `TLauncher`"
//! claim can be re-checked on demand instead of being a one-off assertion
//! in a pull request description.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::storage_error;

/// One stored measurement series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    /// Unique identifier of the series.
    pub id: String,
    /// Benchmark suite the measurement belongs to.
    pub suite: String,
    /// Metric name, e.g. `launcher_cold_start`.
    pub metric: String,
    /// Scenario the metric was measured in, e.g. `26.3`.
    pub scenario: String,
    /// How many samples the series contains.
    pub samples: i64,
    /// Median.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Standard deviation.
    pub stddev: f64,
    /// Unit of the measurement, e.g. `ms`.
    pub unit: String,
    /// When the series was recorded, ISO-8601.
    pub created_at: String,
    /// Free-form JSON metadata (host details, git revision, ...).
    pub meta_json: Option<String>,
}

impl BenchmarkRecord {
    /// Create a record with the current timestamp.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        suite: impl Into<String>,
        metric: impl Into<String>,
        scenario: impl Into<String>,
        samples: i64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            suite: suite.into(),
            metric: metric.into(),
            scenario: scenario.into(),
            samples,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            mean: 0.0,
            stddev: 0.0,
            unit: unit.into(),
            created_at: now_iso8601(),
            meta_json: None,
        }
    }
}

/// Reads and writes benchmark results.
#[derive(Debug, Clone)]
pub struct BenchmarkRepo {
    pool: SqlitePool,
}

impl BenchmarkRepo {
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

    /// Store a measurement series.
    pub fn insert(&self, record: &BenchmarkRecord) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO benchmarks (
                 id, suite, metric, scenario, samples,
                 p50, p95, p99, mean, stddev, unit, created_at, meta_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 suite = excluded.suite,
                 metric = excluded.metric,
                 scenario = excluded.scenario,
                 samples = excluded.samples,
                 p50 = excluded.p50,
                 p95 = excluded.p95,
                 p99 = excluded.p99,
                 mean = excluded.mean,
                 stddev = excluded.stddev,
                 unit = excluded.unit,
                 meta_json = excluded.meta_json",
            params![
                record.id,
                record.suite,
                record.metric,
                record.scenario,
                record.samples,
                record.p50,
                record.p95,
                record.p99,
                record.mean,
                record.stddev,
                record.unit,
                record.created_at,
                record.meta_json,
            ],
        )
        .map_err(|e| storage_error("cannot insert benchmark", &e))?;
        Ok(())
    }

    /// List the series recorded for a metric and scenario, newest first.
    pub fn list(&self, metric: &str, scenario: &str) -> Result<Vec<BenchmarkRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, suite, metric, scenario, samples, p50, p95, p99, mean, stddev, unit, created_at, meta_json
                 FROM benchmarks WHERE metric = ?1 AND scenario = ?2
                 ORDER BY created_at DESC, id ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map(params![metric, scenario], row_to_record)
            .map_err(|e| storage_error("cannot query benchmarks", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read benchmarks", &e))
    }

    /// The most recent series recorded for a metric and scenario.
    pub fn latest(&self, metric: &str, scenario: &str) -> Result<Option<BenchmarkRecord>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, suite, metric, scenario, samples, p50, p95, p99, mean, stddev, unit, created_at, meta_json
                 FROM benchmarks WHERE metric = ?1 AND scenario = ?2
                 ORDER BY created_at DESC, id ASC LIMIT 1",
                params![metric, scenario],
                row_to_record,
            )
            .optional()
            .map_err(|e| storage_error("cannot read benchmark", &e))?;
        Ok(row)
    }

    /// Keep only the `keep` most recent series of a metric, deleting the rest.
    pub fn prune(&self, metric: &str, keep: usize) -> Result<usize> {
        let conn = self.conn()?;
        let removed = conn
            .execute(
                "DELETE FROM benchmarks WHERE id IN (
                     SELECT id FROM benchmarks WHERE metric = ?1
                     ORDER BY created_at DESC, id ASC LIMIT -1 OFFSET ?2
                 )",
                params![metric, i64::try_from(keep).unwrap_or(i64::MAX)],
            )
            .map_err(|e| storage_error("cannot prune benchmarks", &e))?;
        Ok(removed)
    }
}

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<BenchmarkRecord> {
    Ok(BenchmarkRecord {
        id: row.get(0)?,
        suite: row.get(1)?,
        metric: row.get(2)?,
        scenario: row.get(3)?,
        samples: row.get(4)?,
        p50: row.get(5)?,
        p95: row.get(6)?,
        p99: row.get(7)?,
        mean: row.get(8)?,
        stddev: row.get(9)?,
        unit: row.get(10)?,
        created_at: row.get(11)?,
        meta_json: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, BenchmarkRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = BenchmarkRepo::new(db.pool().clone());
        (db, repo)
    }

    fn record(id: &str) -> BenchmarkRecord {
        let mut r = BenchmarkRecord::new(id, "launcher", "cold_start", "26.3", 30, "ms");
        r.p50 = 120.0;
        r.p95 = 180.0;
        r.p99 = 200.0;
        r.mean = 130.0;
        r.stddev = 15.0;
        r
    }

    #[test]
    fn insert_and_read_back() {
        let (_db, repo) = repo();
        repo.insert(&record("a")).unwrap();
        let read = repo.latest("cold_start", "26.3").unwrap().unwrap();
        assert_eq!(read.id, "a");
        assert_eq!(read.p50, 120.0);
        assert_eq!(read.unit, "ms");
    }

    #[test]
    fn scenarios_are_independent() {
        let (_db, repo) = repo();
        repo.insert(&record("a")).unwrap();
        assert!(repo.latest("cold_start", "1.20.5").unwrap().is_none());
    }

    #[test]
    fn prune_keeps_the_newest() {
        let (_db, repo) = repo();
        for index in 0..5 {
            let mut r = record(&format!("r{index}"));
            r.created_at = format!("2026-10-{:02}T00:00:00+00:00", index + 1);
            repo.insert(&r).unwrap();
        }
        assert_eq!(repo.list("cold_start", "26.3").unwrap().len(), 5);
        assert_eq!(repo.prune("cold_start", 2).unwrap(), 3);
        assert_eq!(repo.list("cold_start", "26.3").unwrap().len(), 2);
    }
}
