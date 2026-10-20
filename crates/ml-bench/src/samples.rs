//! A file of measurements.
//!
//! The measurer writes its samples into a JSON file, and the file is
//! the shape that both sides of the comparison share: the file of the
//! reference launcher has exactly the same layout, so the report does
//! not have to know where a number came from.
//!
//! The format is deliberately dull:
//!
//! ```json
//! {
//!   "meta": { "launcher": "MinLauncher 0.1.0", "recorded_at": "2026-10-20T12:00:00Z" },
//!   "scenario": "1.21.4",
//!   "samples": { "click_to_window": [812.4, 801.9] }
//! }
//! ```
//!
//! The samples of a metric are named after the metric itself, so a file
//! that does not hold a metric is a file in which that metric was not
//! measured — and not a file in which the metric is zero.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ml_core::paths::sanitize_component;
use ml_core::{AppPaths, Error, Result};
use ml_storage::Storage;

use crate::ingame::{scenario_with_mod, scenario_without_mod};
use crate::metric::Metric;
use crate::record::{Meta, load, store};
use crate::stats::Series;

/// The scenario the measurements are stored under when the file does
/// not name one.
pub const DEFAULT_SCENARIO: &str = "default";

/// The measurements of one launcher.
///
/// A side is a set of series, one per metric. The metrics that were not
/// measured are simply not in it: a missing metric is not a metric with
/// a zero in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Side {
    /// The series of every metric, by the name of the metric.
    pub series: BTreeMap<String, Series>,
}

impl Side {
    /// An empty side.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The series of a metric.
    #[must_use]
    pub fn series_of(&self, metric: Metric) -> Option<&Series> {
        self.series.get(metric.as_str())
    }

    /// Add the series of a metric.
    pub fn put(&mut self, metric: Metric, series: Series) {
        self.series.insert(metric.as_str().to_string(), series);
    }

    /// Whether the side holds no measurement at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    /// Add the measurements of another side.
    ///
    /// The other side wins: the file of the samples is closer to the
    /// measurement than the summary the database keeps.
    pub fn merge(&mut self, other: &Self) {
        for (metric, series) in &other.series {
            self.series.insert(metric.clone(), series.clone());
        }
    }
}

/// A file of measurements.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SampleFile {
    /// The metadata of the measurement.
    #[serde(default)]
    pub meta: Meta,
    /// The scenario the measurements belong to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The samples of every metric, by the name of the metric.
    #[serde(default)]
    pub samples: BTreeMap<String, Vec<f64>>,
}

impl SampleFile {
    /// An empty file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a file of measurements.
    ///
    /// # Errors
    /// Fails when the file cannot be read or does not hold valid JSON.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|error| Error::io(path, error))?;
        let text = String::from_utf8_lossy(&bytes);
        serde_json::from_str(&text).map_err(|error| Error::json(path.display().to_string(), &error))
    }

    /// Write the file.
    ///
    /// # Errors
    /// Fails when the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| Error::json(path.display().to_string(), &error))?;
        std::fs::write(path, text).map_err(|error| Error::io(path, error))
    }

    /// Add a sample to a metric.
    pub fn add(&mut self, metric: Metric, sample: f64) {
        self.samples
            .entry(metric.as_str().to_string())
            .or_default()
            .push(sample);
    }

    /// The series of a metric.
    ///
    /// A metric without samples has no series: it was not measured.
    #[must_use]
    pub fn series_of(&self, metric: Metric) -> Option<Series> {
        let samples = self.samples.get(metric.as_str())?;
        if samples.is_empty() {
            return None;
        }
        Some(Series::from_samples(samples.clone()))
    }

    /// The metrics the file holds.
    #[must_use]
    pub fn metrics(&self) -> Vec<Metric> {
        Metric::all()
            .iter()
            .copied()
            .filter(|metric| self.samples.contains_key(metric.as_str()))
            .collect()
    }

    /// The side of the comparison the file describes.
    #[must_use]
    pub fn side(&self) -> Side {
        let mut side = Side::new();
        for metric in self.metrics() {
            if let Some(series) = self.series_of(metric) {
                side.put(metric, series);
            }
        }
        side
    }

    /// The scenario the measurements belong to.
    #[must_use]
    pub fn scenario_of(&self) -> String {
        self.scenario
            .clone()
            .or_else(|| self.meta.game_version.clone())
            .unwrap_or_else(|| DEFAULT_SCENARIO.to_string())
    }

    /// Store the measurements of the file in the database.
    ///
    /// The database is where the launcher keeps what it measured, and
    /// the interface reads it from there. Returns the number of metrics
    /// that were stored.
    pub fn import(&self, storage: &Storage) -> usize {
        let scenario = self.scenario_of();
        let mut stored = 0;

        for metric in self.metrics() {
            let Some(series) = self.series_of(metric) else {
                continue;
            };
            if store(storage, metric, &scenario, &series, &self.meta).is_some() {
                stored += 1;
            }
        }

        stored
    }
}

/// The file of the measurements of a scenario.
///
/// The launcher writes the measurements it takes itself into this file,
/// so that a measurer outside can pick them up: the file has the same
/// format the measurer writes, and the two merge into one report.
#[must_use]
pub fn scenario_file(paths: &AppPaths, scenario: &str) -> PathBuf {
    paths
        .benchmarks_dir
        .join(format!("{}.json", sanitize_component(scenario)))
}

/// The measurements of a scenario, as they are on the disk.
///
/// A scenario that was never measured has no file, and an empty one is
/// returned for it.
#[must_use]
pub fn of_scenario(paths: &AppPaths, scenario: &str) -> SampleFile {
    match SampleFile::read(&scenario_file(paths, scenario)) {
        Ok(file) => file,
        Err(_) => {
            let mut file = SampleFile::new();
            file.scenario = Some(scenario.to_string());
            file
        }
    }
}

/// Add a sample to the measurements of a scenario and write them.
///
/// The sample is written next to the samples that were measured before
/// it, so that the file of a scenario holds the whole series and not
/// only its last run.
///
/// # Errors
/// Fails when the file cannot be written.
pub fn append(
    paths: &AppPaths,
    scenario: &str,
    metric: Metric,
    sample: f64,
    meta: &Meta,
) -> Result<()> {
    std::fs::create_dir_all(&paths.benchmarks_dir)
        .map_err(|error| Error::io(&paths.benchmarks_dir, error))?;

    let mut file = of_scenario(paths, scenario);
    file.meta = meta.clone();
    file.scenario = Some(scenario.to_string());
    file.add(metric, sample);

    file.write(&scenario_file(paths, scenario))
}

/// The measurements the launcher took itself, read from the database.
///
/// The launcher measures the metrics that only it can see: the length
/// of an installation (M4) and the behaviour of the game (M5, M6).
/// They are read from the scenarios they were stored under.
#[must_use]
pub fn side_of_storage(storage: &Storage, scenario: &str) -> Side {
    let mut side = Side::new();

    // The installation is measured under the version it installed.
    if let Some(series) = load(storage, Metric::FreshInstall, scenario) {
        side.put(Metric::FreshInstall, series);
    }

    // The game is measured under the preset of the mod that was in it.
    // The run with the mod is the one that is compared with the
    // reference: it is the game the launcher promises.
    for metric in [Metric::InGameFps, Metric::InGameFrametime] {
        let scenario = scenario_with_mod(storage, metric).unwrap_or_else(scenario_without_mod);
        if let Some(series) = load(storage, metric, &scenario) {
            side.put(metric, series);
        }
    }

    side
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_storage::db::Database;

    fn storage() -> Storage {
        Storage::from_database(Database::open_in_memory().unwrap())
    }

    fn file() -> SampleFile {
        let mut file = SampleFile::new();
        file.meta.launcher = Some("MinLauncher 0.1.0".into());
        file.meta.game_version = Some("1.21.4".into());
        file.scenario = Some("1.21.4".into());
        file.add(Metric::ClickToWindow, 812.4);
        file.add(Metric::ClickToWindow, 801.9);
        file.add(Metric::LauncherRss, 41.2);
        file
    }

    #[test]
    fn a_file_round_trips_through_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ours.json");

        file().write(&path).unwrap();
        let read = SampleFile::read(&path).unwrap();

        assert_eq!(read.samples["click_to_window"], vec![812.4, 801.9]);
        assert_eq!(read.meta.launcher.as_deref(), Some("MinLauncher 0.1.0"));
        assert_eq!(read.scenario.as_deref(), Some("1.21.4"));
    }

    #[test]
    fn a_file_that_is_not_there_is_an_error() {
        assert!(SampleFile::read(Path::new("/does/not/exist.json")).is_err());
    }

    #[test]
    fn a_metric_without_samples_has_no_series() {
        let file = file();
        assert!(file.series_of(Metric::InGameFps).is_none());
        assert!(file.series_of(Metric::ClickToWindow).is_some());
    }

    #[test]
    fn the_metrics_of_a_file_are_the_ones_it_holds() {
        let metrics = file().metrics();
        assert!(metrics.contains(&Metric::ClickToWindow));
        assert!(metrics.contains(&Metric::LauncherRss));
        assert!(!metrics.contains(&Metric::InGameFps));
    }

    #[test]
    fn the_scenario_of_a_file_is_the_one_it_names() {
        assert_eq!(file().scenario_of(), "1.21.4");

        let mut nameless = file();
        nameless.scenario = None;
        nameless.meta.game_version = None;
        assert_eq!(nameless.scenario_of(), DEFAULT_SCENARIO);
    }

    #[test]
    fn a_file_is_stored_in_the_database() {
        let storage = storage();
        let file = file();

        let stored = file.import(&storage);
        assert_eq!(stored, 2, "the metrics were not stored");

        // The database keeps the summary, not the samples, so what comes
        // back is the summary read as if it were a series.
        let read = load(&storage, Metric::ClickToWindow, "1.21.4").unwrap();
        assert!((read.median().unwrap() - 807.15).abs() < 1e-6);
    }

    #[test]
    fn the_side_of_a_file_holds_the_metrics_it_measured() {
        let side = file().side();

        assert_eq!(side.series.len(), 2);
        assert!(side.series_of(Metric::ClickToWindow).is_some());
        assert!(side.series_of(Metric::InGameFps).is_none());
    }

    #[test]
    fn the_side_of_the_database_holds_what_the_launcher_measured() {
        let storage = storage();
        let meta = Meta::of_this_machine();

        // An installation of the version 1.21.4.
        store(
            &storage,
            Metric::FreshInstall,
            "1.21.4",
            &Series::from_samples(vec![120_000.0]),
            &meta,
        )
        .unwrap();

        // A run of the game with the mod in it.
        store(
            &storage,
            Metric::InGameFps,
            &crate::scenario_of("balanced"),
            &Series::from_samples(vec![120.0]),
            &meta,
        )
        .unwrap();

        let side = side_of_storage(&storage, "1.21.4");

        assert!(side.series_of(Metric::FreshInstall).is_some());
        let fps = side
            .series_of(Metric::InGameFps)
            .expect("the game was not measured");
        assert!((fps.mean().unwrap() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn the_side_of_the_database_prefers_the_run_with_the_mod() {
        let storage = storage();
        let meta = Meta::of_this_machine();

        store(
            &storage,
            Metric::InGameFps,
            &crate::scenario_without_mod(),
            &Series::from_samples(vec![60.0]),
            &meta,
        )
        .unwrap();
        store(
            &storage,
            Metric::InGameFps,
            &crate::scenario_of("balanced"),
            &Series::from_samples(vec![120.0]),
            &meta,
        )
        .unwrap();

        let side = side_of_storage(&storage, "1.21.4");
        let fps = side.series_of(Metric::InGameFps).unwrap();

        assert!((fps.mean().unwrap() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn the_side_of_the_database_falls_back_to_the_run_without_the_mod() {
        let storage = storage();
        let meta = Meta::of_this_machine();

        store(
            &storage,
            Metric::InGameFps,
            &crate::scenario_without_mod(),
            &Series::from_samples(vec![60.0]),
            &meta,
        )
        .unwrap();

        let side = side_of_storage(&storage, "1.21.4");
        let fps = side
            .series_of(Metric::InGameFps)
            .expect("the game was not measured");

        assert!((fps.mean().unwrap() - 60.0).abs() < 1e-6);
    }

    #[test]
    fn the_measurements_of_the_file_win_over_the_ones_of_the_database() {
        let mut ours = Side::new();
        ours.put(Metric::ClickToWindow, Series::from_samples(vec![800.0]));

        let mut other = Side::new();
        other.put(Metric::ClickToWindow, Series::from_samples(vec![900.0]));

        ours.merge(&other);
        let series = ours.series_of(Metric::ClickToWindow).unwrap();

        assert!((series.mean().unwrap() - 900.0).abs() < 1e-6);
    }

    /// The paths of a launcher in a temporary directory.
    fn paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        (dir, paths)
    }

    #[test]
    fn a_measurement_of_the_launcher_is_written_into_the_file_of_the_scenario() {
        let (_dir, paths) = paths();
        let meta = Meta::of_this_machine();

        append(&paths, "1.21.4", Metric::FreshInstall, 120_000.0, &meta).unwrap();
        append(&paths, "1.21.4", Metric::FreshInstall, 121_000.0, &meta).unwrap();

        let file = of_scenario(&paths, "1.21.4");
        let series = file
            .series_of(Metric::FreshInstall)
            .expect("nothing was written");

        assert_eq!(series.len(), 2);
        assert!((series.mean().unwrap() - 120_500.0).abs() < 1e-6);
        assert_eq!(file.scenario.as_deref(), Some("1.21.4"));
    }

    #[test]
    fn the_file_of_a_scenario_is_named_after_it() {
        let (_dir, paths) = paths();

        let path = scenario_file(&paths, "in_game/balanced");
        assert_eq!(path.file_name().unwrap(), "in_game_balanced.json");
        assert_eq!(path.parent().unwrap(), paths.benchmarks_dir);
    }

    #[test]
    fn a_scenario_that_was_never_measured_has_an_empty_file() {
        let (_dir, paths) = paths();

        let file = of_scenario(&paths, "1.21.4");
        assert!(file.samples.is_empty());
        assert_eq!(file.scenario.as_deref(), Some("1.21.4"));
    }
}
