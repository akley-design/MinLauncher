//! The commands about the measurements.
//!
//! The results of the benchmark harness are stored in the same database
//! the UI reads, so the tab "Performance" shows what was measured
//! rather than a number that somebody typed in.

use ml_bench::SampleFile;
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
    pub optimized_scenario: String,
    /// The median of the run with the mod.
    pub optimized: f64,
    /// How many runs contribute to the measurement with the optimization.
    pub optimized_samples: i64,
    /// The scenario of the run without the mod.
    pub reference_scenario: String,
    /// The median of the run without the mod.
    pub reference: f64,
    /// How many reference runs contribute to the comparison.
    pub reference_samples: i64,
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

/// The result of importing the TLauncher side of an in-game comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TLauncherBaselineImport {
    /// The canonical scenario used for future comparisons.
    pub scenario: String,
    /// The number of complete game sessions in each metric.
    pub runs_per_metric: usize,
}

/// Import the TLauncher FPS file produced by `scripts/measure-tlauncher-fps.ps1`.
///
/// The file is validated for the same Minecraft version and machine, then
/// stored under a launcher-owned scenario. The source `.minecraft` directory
/// is never read or written by this command.
#[tauri::command]
pub fn import_tlauncher_baseline(
    samples_json: String,
    state: State<'_, SharedState>,
) -> IpcResult<TLauncherBaselineImport> {
    if samples_json.len() > 1_000_000 {
        return Err(IpcError::invalid("the benchmark file is too large"));
    }
    let mut file: SampleFile = serde_json::from_str(&samples_json).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "invalid benchmark JSON",
            error.to_string(),
        )
    })?;

    if !file
        .meta
        .launcher
        .as_deref()
        .is_some_and(|launcher| launcher.to_ascii_lowercase().contains("tlauncher"))
    {
        return Err(IpcError::invalid(
            "the benchmark file is not marked as TLauncher data",
        ));
    }
    if file.meta.game_version.as_deref() != Some("1.20.1") {
        return Err(IpcError::invalid(
            "the TLauncher reference must be measured on Minecraft 1.20.1",
        ));
    }
    let current_host = std::env::var("COMPUTERNAME").unwrap_or_default();
    if current_host.is_empty() || file.meta.host.as_deref() != Some(current_host.as_str()) {
        return Err(IpcError::invalid(
            "the TLauncher reference must come from this same PC",
        ));
    }

    let fps = file.samples.get(Metric::InGameFps.as_str());
    let frametime = file.samples.get(Metric::InGameFrametime.as_str());
    let (Some(fps), Some(frametime)) = (fps, frametime) else {
        return Err(IpcError::invalid(
            "the TLauncher file must contain FPS and frame-time samples",
        ));
    };
    if fps.len() < ml_bench::DEFAULT_SAMPLES || fps.len() != frametime.len() {
        return Err(IpcError::invalid(format!(
            "the TLauncher file must contain at least {} matched runs for both metrics",
            ml_bench::DEFAULT_SAMPLES
        )));
    }
    if fps
        .iter()
        .chain(frametime)
        .any(|sample| !sample.is_finite() || *sample <= 0.0)
    {
        return Err(IpcError::invalid(
            "the TLauncher file contains an invalid FPS or frame-time value",
        ));
    }

    let runs_per_metric = fps.len();
    file.samples.retain(|metric, _| {
        metric == Metric::InGameFps.as_str() || metric == Metric::InGameFrametime.as_str()
    });
    let scenario = ml_bench::scenario_of("tlauncher");
    file.scenario = Some(scenario.clone());
    std::fs::create_dir_all(&state.paths().benchmarks_dir).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot prepare the benchmark directory",
            error.to_string(),
        )
    })?;
    let path = ml_bench::samples::scenario_file(state.paths(), &scenario);
    file.write(&path).map_err(IpcError::from)?;

    let stored = file.import(state.storage());
    if stored != 2 {
        return Err(IpcError::new(
            ErrorCode::Storage,
            "the TLauncher samples could not be stored",
        ));
    }

    Ok(TLauncherBaselineImport {
        scenario,
        runs_per_metric,
    })
}

/// The comparison of the game with the mod and without it.
///
/// A pair that is not there is not a failure: a run that has not
/// happened yet is a state of the launcher, not a fault of it.
fn compare_in_game_of(state: &SharedState) -> InGameComparisons {
    let storage = state.storage();
    let mut comparisons = Vec::new();

    for metric in [Metric::InGameFps, Metric::InGameFrametime] {
        let with_mod_scenario = if state.storage().load_settings().is_ok_and(|config| {
            config.optimization.profile == ml_core::GamePerformanceProfile::MaxFps
        }) {
            ml_bench::scenario_of("max_fps")
        } else if let Some(scenario) = ml_bench::scenario_with_mod(storage, metric) {
            scenario
        } else {
            continue;
        };
        let reference_scenario = ml_bench::scenario_of("tlauncher");
        let Ok(Some(with_mod)) = storage
            .benchmarks()
            .latest(metric.as_str(), &with_mod_scenario)
        else {
            continue;
        };
        let Ok(Some(reference)) = storage
            .benchmarks()
            .latest(metric.as_str(), &reference_scenario)
        else {
            continue;
        };

        let with_mod_samples = series_from_samples(state, metric, &with_mod_scenario);
        let reference_samples = series_from_samples(state, metric, &reference_scenario);
        let comparison = with_mod_samples
            .as_ref()
            .zip(reference_samples.as_ref())
            .filter(|(with, without)| {
                with.len() >= ml_bench::DEFAULT_SAMPLES
                    && without.len() >= ml_bench::DEFAULT_SAMPLES
            })
            .and_then(|(with, without)| ml_bench::compare(metric, with, without));

        comparisons.push(InGameComparison {
            metric: metric.as_str().to_string(),
            unit: metric.unit().to_string(),
            optimized_scenario: with_mod_scenario,
            optimized: with_mod.p50,
            optimized_samples: with_mod.samples,
            reference_scenario,
            reference: reference.p50,
            reference_samples: reference.samples,
            improvement_percent: comparison
                .as_ref()
                .map(ml_bench::Comparison::improvement_percent),
            verdict: comparison.map(|comparison| comparison.verdict),
        });
    }

    InGameComparisons { comparisons }
}

fn series_from_samples(
    state: &SharedState,
    metric: Metric,
    scenario: &str,
) -> Option<ml_bench::Series> {
    let path = ml_bench::samples::scenario_file(state.paths(), scenario);
    ml_bench::SampleFile::read(&path).ok()?.series_of(metric)
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
        store_run_values(
            state,
            metric,
            scenario,
            vec![value; ml_bench::DEFAULT_SAMPLES],
        );
    }

    fn store_run_values(state: &SharedState, metric: Metric, scenario: &str, values: Vec<f64>) {
        let meta = ml_bench::Meta::of_this_machine();
        for value in &values {
            ml_bench::samples::append(state.paths(), scenario, metric, *value, &meta).unwrap();
        }
        ml_bench::store(
            state.storage(),
            metric,
            scenario,
            &ml_bench::Series::from_samples(values),
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
    fn minlauncher_is_compared_with_tlauncher_samples() {
        let (_dir, state) = state();
        let mut config = state.storage().load_settings().unwrap();
        config.optimization.profile = ml_core::GamePerformanceProfile::Original;
        state.storage().save_settings(&config).unwrap();

        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("tlauncher"),
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

        assert!((fps.optimized - 120.0).abs() < 1e-6);
        assert!((fps.reference - 60.0).abs() < 1e-6);
        assert_eq!(fps.optimized_scenario, "in_game/balanced");
        assert_eq!(fps.reference_scenario, "in_game/tlauncher");

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

    #[test]
    fn the_game_target_is_not_judged_before_thirty_runs_per_side() {
        let (_dir, state) = state();
        store_run(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("tlauncher"),
            60.0,
        );
        store_run_values(
            &state,
            Metric::InGameFps,
            &ml_bench::scenario_of("max_fps"),
            vec![120.0],
        );

        let comparisons = compare_in_game_of(&state);
        let fps = comparisons
            .comparisons
            .iter()
            .find(|comparison| comparison.metric == "in_game_fps")
            .expect("the measurements should still be visible");

        assert_eq!(fps.optimized_samples, 1);
        assert_eq!(fps.reference_samples, 30);
        assert_eq!(fps.improvement_percent, None);
        assert_eq!(fps.verdict, None);
    }
}
