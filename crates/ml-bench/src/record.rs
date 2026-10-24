//! Storing and reading back a measurement.
//!
//! The database already knows how to hold a measurement; this module
//! turns a [`Series`] into a record and adds the metadata that makes a
//! comparison reproducible: what was measured, on what, with what.
//!
//! Without that metadata a result is worthless — "the launch took 1.2
//! seconds" says nothing without the machine, the version of the game
//! and the build of the launcher.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};
use ml_storage::Storage;
use ml_storage::repo::benchmarks::BenchmarkRecord;

use crate::metric::Metric;
use crate::stats::Series;

/// The metadata of a measurement.
///
/// It is stored as JSON next to the numbers, and it is what makes two
/// measurements comparable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// The launcher that was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<String>,
    /// The revision of the launcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The version of the game that was launched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    /// The version of the Java runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<String>,
    /// The name of the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The operating system of the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// The processor of the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    /// When the measurement was taken.
    ///
    /// A measurement without a date says nothing about whether it still
    /// describes the machine it was taken on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<String>,
    /// Anything else worth keeping.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl Meta {
    /// The metadata of the current machine.
    ///
    /// The parts that can be read without spawning a process are read
    /// from the system; the processor is not, because on Windows it
    /// needs the registry and a `wmic` call, and a benchmark should not
    /// pay for that.
    #[must_use]
    pub fn of_this_machine() -> Self {
        Self {
            os: Some(std::env::consts::OS.to_string()),
            host: std::env::var("COMPUTERNAME").ok(),
            launcher: Some(format!("MinLauncher {}", env!("CARGO_PKG_VERSION"))),
            recorded_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Self::default()
        }
    }

    /// Add a value to the free-form part.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.extra.insert(key.into(), value.into());
    }

    /// The metadata as a JSON string.
    ///
    /// # Errors
    /// Fails when the metadata cannot be serialised, which cannot
    /// happen for the types it holds.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| Error::InvalidInput(format!("cannot serialise the metadata: {e}")))
    }
}

/// The identifier of a measurement.
///
/// The identifier has to be unique but stable, so that a repeated
/// measurement of the same metric and scenario replaces the previous
/// one instead of piling up.
#[must_use]
pub fn record_id(metric: Metric, scenario: &str, suite: &str) -> String {
    format!("{suite}/{metric}/{scenario}")
}

/// Turn a series into a record.
#[must_use]
pub fn to_record(
    metric: Metric,
    scenario: &str,
    series: &Series,
    meta: &Meta,
) -> Option<BenchmarkRecord> {
    let summary = series.summary()?;

    let mut record = BenchmarkRecord::new(
        record_id(metric, scenario, metric.suite()),
        metric.suite(),
        metric.as_str(),
        scenario,
        i64::try_from(summary.samples).unwrap_or(i64::MAX),
        metric.unit(),
    );
    record.p50 = summary.p50;
    record.p95 = summary.p95;
    record.p99 = summary.p99;
    record.mean = summary.mean;
    record.stddev = summary.stddev;
    record.meta_json = meta.to_json().ok();

    Some(record)
}

/// Store a series in the database.
#[must_use]
pub fn store(
    storage: &Storage,
    metric: Metric,
    scenario: &str,
    series: &Series,
    meta: &Meta,
) -> Option<BenchmarkRecord> {
    let record = to_record(metric, scenario, series, meta)?;
    storage.benchmarks().insert(&record).ok()?;
    Some(record)
}

/// Read back the series of a metric, from the stored record.
///
/// The stored record holds the summary, not the samples, so the series
/// that comes back holds the percentiles as if they were samples. It
/// is enough to compare, and it is honest about what it is.
#[must_use]
pub fn load(storage: &Storage, metric: Metric, scenario: &str) -> Option<Series> {
    let record = storage
        .benchmarks()
        .latest(metric.as_str(), scenario)
        .ok()??;
    Some(Series::from_samples(vec![
        record.p50,
        record.p95,
        record.p99,
        record.mean,
    ]))
}

/// Read back the metadata of the last measurement of a metric.
#[must_use]
pub fn load_meta(storage: &Storage, metric: Metric, scenario: &str) -> Option<Meta> {
    let record = storage
        .benchmarks()
        .latest(metric.as_str(), scenario)
        .ok()??;
    let json = record.meta_json?;
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_storage::db::Database;

    fn series() -> Series {
        Series::from_samples(vec![100.0, 110.0, 120.0, 130.0])
    }

    #[test]
    fn the_identifier_is_stable_and_names_the_metric() {
        let id = record_id(Metric::ClickToWindow, "26.3", "install");
        assert_eq!(id, "install/click_to_window/26.3");
        assert_eq!(id, record_id(Metric::ClickToWindow, "26.3", "install"));
    }

    #[test]
    fn a_record_carries_the_summary_of_the_series() {
        let record = to_record(Metric::ClickToWindow, "26.3", &series(), &Meta::default()).unwrap();
        assert_eq!(record.metric, "click_to_window");
        assert_eq!(record.scenario, "26.3");
        assert_eq!(record.suite, "install");
        assert_eq!(record.samples, 4);
        assert_eq!(record.unit, "ms");
        assert_eq!(record.p50, 110.0);
        assert_eq!(record.p99, 130.0);
    }

    #[test]
    fn an_empty_series_makes_no_record() {
        assert!(
            to_record(
                Metric::ClickToWindow,
                "26.3",
                &Series::new(),
                &Meta::default()
            )
            .is_none()
        );
    }

    #[test]
    fn the_metadata_travels_with_the_record() {
        let mut meta = Meta::default();
        meta.set("gpu", "test");
        meta.launcher = Some("MinLauncher 0.1.0".into());

        let record = to_record(Metric::ClickToWindow, "26.3", &series(), &meta).unwrap();
        let json = record.meta_json.unwrap();
        assert!(json.contains("MinLauncher"));
        assert!(json.contains("gpu"));
    }

    #[test]
    fn a_stored_measurement_can_be_read_back() {
        let db = Database::open_in_memory().unwrap();
        let storage = Storage::from_database(db);
        let mut meta = Meta::default();
        meta.set("gpu", "test");

        store(&storage, Metric::ClickToWindow, "26.3", &series(), &meta).unwrap();

        let read = load(&storage, Metric::ClickToWindow, "26.3").unwrap();
        assert_eq!(read.len(), 4);
        assert!(read.samples.contains(&110.0));

        let meta = load_meta(&storage, Metric::ClickToWindow, "26.3").unwrap();
        assert_eq!(meta.extra.get("gpu").map(String::as_str), Some("test"));
    }

    #[test]
    fn a_metric_that_was_never_measured_has_no_record() {
        let db = Database::open_in_memory().unwrap();
        let storage = Storage::from_database(db);
        assert!(load(&storage, Metric::InGameFps, "26.3").is_none());
    }
}
