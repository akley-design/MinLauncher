//! The commands about the measurements.
//!
//! The results of the benchmark harness are stored in the same database
//! the UI reads, so the tab "Performance" shows what was measured
//! rather than a number that somebody typed in.

use ml_bench::metric::{Metric, Verdict};
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

/// The newest measurement of every metric and every scenario.
///
/// A metric can be measured in more than one scenario — the game is
/// measured with the mod and without it — so the newest measurement of
/// the metric is not one row but one row per scenario. The older
/// measurements of the same scenario are kept in the database and can be
/// asked for with [`list_benchmarks`].
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn latest_benchmarks(state: State<'_, SharedState>) -> IpcResult<BenchmarkList> {
    latest_benchmarks_of(state.inner())
}

/// The newest measurement of every metric and every scenario.
///
/// # Errors
/// Fails when the database cannot be read.
fn latest_benchmarks_of(state: &SharedState) -> IpcResult<BenchmarkList> {
    let storage = state.storage();

    let mut records = Vec::new();
    for metric in Metric::all() {
        let scenarios = storage
            .benchmarks()
            .scenarios(metric.as_str())
            .map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot read the measurements",
                    error.to_string(),
                )
            })?;

        for scenario in scenarios {
            let record = storage
                .benchmarks()
                .latest(metric.as_str(), &scenario)
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
    }

    Ok(BenchmarkList { records })
}

/// One metric of the game, measured with the mod and without it.
///
/// The run without the mod is the base, and the run with it is what the
/// mod of the launcher changed. Both of them are the medians of their
/// own series, so that the verdict does not depend on how many samples
/// a run had.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InGameComparison {
    /// The name of the metric.
    pub metric: String,
    /// The unit of the measurement.
    pub unit: String,
    /// The scenario of the run with the mod.
    pub with_mod_scenario: String,
    /// The median of the run with the mod.
    pub with_mod: f64,
    /// The scenario of the run without the mod.
    pub without_mod_scenario: String,
    /// The median of the run without the mod.
    pub without_mod: f64,
    /// How much the mod changed the game, in percent.
    ///
    /// A positive number means the game got better. It is absent when
    /// the two runs are not comparable — a series that is too noisy is
    /// not a result.
    pub improvement_percent: Option<f64>,
    /// The verdict on the target of the metric.
    pub verdict: Option<Verdict>,
}

/// The comparisons of the game: the metrics M5 and M6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InGameComparisons {
    /// The comparisons, one per metric that has both of its runs.
    pub comparisons: Vec<InGameComparison>,
}

/// The comparison of the game with the mod and without it.
///
/// The two runs of one build stand side by side: the run without the
/// mod is the base, and the run with it is what the mod changed. A
/// metric whose two sides are not both measured is not in the list —
/// there is nothing to compare it against, and a comparison against a
/// run that never happened would be a number nobody measured.
#[tauri::command]
pub fn compare_in_game(state: State<'_, SharedState>) -> InGameComparisons {
    compare_in_game_of(state.inner())
}

/// The comparison of the game with the mod and without it.
///
/// A pair that is not there is not a failure: a run that has not
/// happened yet is a state of the launcher, not a fault of it.
fn compare_in_game_of(state: &SharedState) -> InGameComparisons {
    let storage = state.storage();
    let mut comparisons = Vec::new();

    for metric in [Metric::InGameFps, Metric::InGameFrametime] {
        let Some(pair) = ml_bench::pair_of(storage, metric) else {
            continue;
        };

        let Some(with_mod) = pair.with_mod.summary() else {
            continue;
        };
        let Some(without_mod) = pair.without_mod.summary() else {
            continue;
        };

        let comparison = ml_bench::compare(metric, &pair.with_mod, &pair.without_mod);

        comparisons.push(InGameComparison {
            metric: metric.as_str().to_string(),
            unit: metric.unit().to_string(),
            with_mod_scenario: pair.with_mod_scenario,
            with_mod: with_mod.p50,
            without_mod_scenario: pair.without_mod_scenario,
            without_mod: without_mod.p50,
            improvement_percent: comparison
                .as_ref()
                .map(ml_bench::Comparison::improvement_percent),
            verdict: comparison.map(|comparison| comparison.verdict),
        });
    }

    InGameComparisons { comparisons }
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

    /// Store one run of the game, the way the launcher stores it.
    ///
    /// A run of the game is a single measurement, and a single
    /// measurement is a series of one sample: the record holds the same
    /// number in every percentile.
    fn store_run(state: &SharedState, metric: Metric, scenario: &str, value: f64) {
        let meta = ml_bench::Meta::of_this_machine();
        ml_bench::store(
            state.storage(),
            metric,
            scenario,
            &ml_bench::Series::from_samples(vec![value]),
            &meta,
        )
        .unwrap();
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

    #[test]
    fn the_table_shows_every_scenario_of_a_metric() {
        // The bug this closes: the table asked for the measurements of
        // the empty scenario, and every measurement of the game lives
        // under a scenario of its own.
        let (_dir, state) = state();

        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_without_mod(),
            60.0,
        );
        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("balanced"),
            120.0,
        );

        let list = latest_benchmarks_of(&state).unwrap();

        assert_eq!(list.records.len(), 2, "the table is not empty");
        let scenarios: Vec<&str> = list
            .records
            .iter()
            .map(|record| record.scenario.as_str())
            .collect();
        assert!(scenarios.contains(&"in_game/off"));
        assert!(scenarios.contains(&"in_game/balanced"));
    }

    #[test]
    fn the_pair_of_the_game_is_compared() {
        let (_dir, state) = state();

        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_without_mod(),
            60.0,
        );
        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("balanced"),
            120.0,
        );

        let comparisons = compare_in_game_of(&state);
        let fps = comparisons
            .comparisons
            .iter()
            .find(|comparison| comparison.metric == "in_game_fps")
            .expect("there is no comparison of the frame rate");

        assert!((fps.with_mod - 120.0).abs() < 1e-6);
        assert!((fps.without_mod - 60.0).abs() < 1e-6);
        assert_eq!(fps.with_mod_scenario, "in_game/balanced");
        assert_eq!(fps.without_mod_scenario, "in_game/off");

        // Twice the frame rate is a hundred percent better.
        let improvement = fps.improvement_percent.expect("there is no difference");
        assert!((improvement - 100.0).abs() < 1e-6);
        assert_eq!(fps.verdict, Some(Verdict::Met));
    }

    #[test]
    fn there_is_no_comparison_without_both_runs() {
        // A run with the mod alone says nothing: there is no base to
        // compare it against.
        let (_dir, state) = state();

        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("balanced"),
            120.0,
        );

        let comparisons = compare_in_game_of(&state);
        assert_eq!(comparisons.comparisons.len(), 0, "{comparisons:?}");
    }
}
