//! The commands about the measurements.
//!
//! The results of the benchmark harness are stored in the same database
//! the UI reads, so the tab "Performance" shows what was measured
//! rather than a number that somebody typed in.

use ml_storage::BenchmarkRecord;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// One stored measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkView {
    /// The suite the measurement belongs to.
    pub suite: String,
    /// The name of the metric.
    pub metric: String,
    /// The scenario the metric was measured in.
    pub scenario: String,
    /// How many samples the series contains.
    pub samples: i64,
    /// The median.
    pub p50: f64,
    /// The 95th percentile.
    pub p95: f64,
    /// The 99th percentile.
    pub p99: f64,
    /// The arithmetic mean.
    pub mean: f64,
    /// The standard deviation.
    pub stddev: f64,
    /// The unit of the measurement.
    pub unit: String,
    /// When the measurement was recorded.
    pub created_at: String,
}

impl From<&BenchmarkRecord> for BenchmarkView {
    fn from(record: &BenchmarkRecord) -> Self {
        Self {
            suite: record.suite.clone(),
            metric: record.metric.clone(),
            scenario: record.scenario.clone(),
            samples: record.samples,
            p50: record.p50,
            p95: record.p95,
            p99: record.p99,
            mean: record.mean,
            stddev: record.stddev,
            unit: record.unit.clone(),
            created_at: record.created_at.clone(),
        }
    }
}

/// The stored measurements, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkList {
    /// The measurements.
    pub records: Vec<BenchmarkView>,
}

/// The newest measurement of every metric.
///
/// The measurements of the other scenarios are kept in the database and
/// can be asked for with [`list_benchmarks`].
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn latest_benchmarks(state: State<'_, SharedState>) -> IpcResult<BenchmarkList> {
    let storage = state.storage();

    let mut records = Vec::new();
    for metric in ml_bench::metric::Metric::all() {
        let record = storage
            .benchmarks()
            .latest(metric.as_str(), "")
            .map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot read the measurements",
                    error.to_string(),
                )
            })?;

        if let Some(record) = record {
            records.push(BenchmarkView::from(&record));
        }
    }

    Ok(BenchmarkList { records })
}

/// The measurements of one metric, newest first.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn list_benchmarks(
    metric: String,
    scenario: String,
    state: State<'_, SharedState>,
) -> IpcResult<BenchmarkList> {
    let records = state
        .storage()
        .benchmarks()
        .list(&metric, &scenario)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the measurements",
                error.to_string(),
            )
        })?;

    Ok(BenchmarkList {
        records: records.iter().map(BenchmarkView::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{AppPaths, OsArch, OsName, Platform};

    fn state() -> (tempfile::TempDir, SharedState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths, platform).unwrap();
        (dir, std::sync::Arc::new(state))
    }

    #[test]
    fn there_are_no_measurements_on_a_fresh_installation() {
        let (_dir, state) = state();
        let storage = state.storage();
        assert!(
            storage
                .benchmarks()
                .latest("click_to_window", "")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_measurement_can_be_stored_and_read_back() {
        let (_dir, state) = state();
        let storage = state.storage();

        let record = BenchmarkRecord {
            id: "install/click_to_window/26.3".into(),
            suite: "install".into(),
            metric: "click_to_window".into(),
            scenario: "26.3".into(),
            samples: 30,
            p50: 800.0,
            p95: 900.0,
            p99: 950.0,
            mean: 820.0,
            stddev: 40.0,
            unit: "ms".into(),
            created_at: "2026-10-19T00:00:00Z".into(),
            meta_json: None,
        };

        storage.benchmarks().insert(&record).unwrap();

        let read = storage
            .benchmarks()
            .latest("click_to_window", "26.3")
            .unwrap()
            .unwrap();
        assert_eq!(read.p50, 800.0);
        assert_eq!(read.samples, 30);
    }
}
