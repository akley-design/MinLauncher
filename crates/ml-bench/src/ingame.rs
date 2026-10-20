//! Turning the report of the profiling agent into measurements.
//!
//! The agent watches the game from the inside and writes down what it
//! saw: how the frames were drawn, how much of the heap the game used.
//! The metrics M5 and M6 are about exactly that, so the report of the
//! agent is the raw material of those two metrics.
//!
//! The numbers go into the database as the measurements of a run. They
//! are stored the same way as any other measurement, so that the
//! comparison against the reference launcher does not have to know where
//! they came from.
//!
//! The scenario of a run holds the preset of the mod that was in the
//! game. A run with the mod and a run without it are two different
//! measurements, and they must not overwrite each other: the whole
//! point of the pair is to compare them.

use ml_launch::agent::AgentReport;
use ml_storage::Storage;

use crate::metric::Metric;
use crate::record::{Meta, load, store};
use crate::stats::Series;

/// The prefix of the scenarios the in-game measurements are stored under.
pub const IN_GAME_PREFIX: &str = "in_game";

/// The preset of the mod that is not there at all.
pub const PRESET_OFF: &str = "off";

/// The presets the mod of the launcher knows.
pub const PRESETS: [&str; 3] = [PRESET_OFF, "balanced", "fast"];

/// The scenario the measurements of a run with a given preset are stored under.
///
/// The preset is part of the scenario: a run with the mod and a run
/// without it are two different measurements. With one scenario for
/// both of them, the second run would overwrite the first one, and the
/// comparison between them would be impossible.
#[must_use]
pub fn scenario_of(preset: &str) -> String {
    format!("{IN_GAME_PREFIX}/{preset}")
}

/// The scenario of the run without the mod.
#[must_use]
pub fn scenario_without_mod() -> String {
    scenario_of(PRESET_OFF)
}

/// The scenario of the run with the mod, when there was one.
///
/// It is the scenario of the preset that was measured, whatever preset
/// it was: the launcher does not have to know which one of them was in
/// the game, only that it was not the one without the mod.
#[must_use]
pub fn scenario_with_mod(storage: &Storage, metric: Metric) -> Option<String> {
    let prefix = format!("{IN_GAME_PREFIX}/");
    let without = scenario_without_mod();

    storage
        .benchmarks()
        .scenarios(metric.as_str())
        .ok()?
        .into_iter()
        .find(|scenario| scenario.starts_with(&prefix) && *scenario != without)
}

/// The measurements of one run of the game.
#[derive(Debug, Clone, PartialEq)]
pub struct InGameMeasurements {
    /// The average frame rate, in frames a second.
    pub fps: f64,
    /// The frame time of the metric M6, in milliseconds.
    pub frametime_ms: f64,
    /// The heap the game used, in mebibytes.
    ///
    /// It is the heap of the virtual machine and not the memory of the
    /// process: the metric M8 is the working set of the game, and it is
    /// measured from the outside. The heap is a part of the run and
    /// not a metric of its own, so it goes into the metadata.
    pub heap_mib: f64,
}

/// The measurements of a report, when the report holds any.
///
/// A report without frames says nothing about how the game runs, and
/// one whose frame rate is zero says nothing either: the game may not
/// have drawn a single frame when the report was written.
#[must_use]
pub fn measurements_of(report: &AgentReport) -> Option<InGameMeasurements> {
    if !report.is_usable() {
        return None;
    }

    let frametime_ms = report.frametime_ms();
    if report.average_fps <= 0.0 || frametime_ms <= 0.0 {
        return None;
    }

    Some(InGameMeasurements {
        fps: report.average_fps,
        frametime_ms,
        heap_mib: report.heap_used_mib,
    })
}

/// Store the measurements of a run of the game.
///
/// The two metrics are stored separately, because each of them has a
/// target of its own and is compared on its own. They go under the
/// scenario of the run, so that the runs with the mod and without it
/// are kept apart.
///
/// The heap of the run goes into the metadata, and not into a metric:
/// the memory of the process is measured from the outside, by the
/// measurer, and the heap of the virtual machine is a different
/// quantity.
///
/// # Panics
/// Never panics: the storage returns `None` when it cannot write, and
/// the run is simply not measured.
#[must_use]
pub fn store_measurements(
    storage: &Storage,
    measurements: &InGameMeasurements,
    meta: &mut Meta,
    scenario: &str,
) -> usize {
    // The heap is only kept when the agent managed to read it: a zero
    // says nothing about the memory of the game.
    if measurements.heap_mib > 0.0 {
        meta.set("heap_mib", measurements.heap_mib.to_string());
    }

    let mut stored = 0;

    let fps = Series::from_samples(vec![measurements.fps]);
    if store(storage, Metric::InGameFps, scenario, &fps, meta).is_some() {
        stored += 1;
    }

    let frametime = Series::from_samples(vec![measurements.frametime_ms]);
    if store(storage, Metric::InGameFrametime, scenario, &frametime, meta).is_some() {
        stored += 1;
    }

    stored
}

/// The two series of a metric of the game: with the mod and without it.
#[derive(Debug, Clone, PartialEq)]
pub struct InGamePair {
    /// The scenario of the run with the mod.
    pub with_mod_scenario: String,
    /// The series of the run with the mod.
    pub with_mod: Series,
    /// The scenario of the run without the mod.
    pub without_mod_scenario: String,
    /// The series of the run without the mod.
    pub without_mod: Series,
}

/// The pair of a metric of the game.
///
/// A pair needs both of its sides: a comparison against a series that
/// was never measured says nothing, so there is no pair without it.
#[must_use]
pub fn pair_of(storage: &Storage, metric: Metric) -> Option<InGamePair> {
    let with_mod_scenario = scenario_with_mod(storage, metric)?;
    let without_mod_scenario = scenario_without_mod();

    let with_mod = load(storage, metric, &with_mod_scenario)?;
    let without_mod = load(storage, metric, &without_mod_scenario)?;

    Some(InGamePair {
        with_mod_scenario,
        with_mod,
        without_mod_scenario,
        without_mod,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_storage::db::Database;

    fn storage() -> Storage {
        let database = Database::open_in_memory().unwrap();
        Storage::from_database(database)
    }

    fn report() -> AgentReport {
        AgentReport {
            agent_started_at_nanos: 1,
            uptime_nanos: 2,
            frames: 119,
            average_fps: 60.67,
            average_frametime_ms: 16.48,
            p99_frametime_ms: 17.85,
            p95_frametime_ms: 17.51,
            p50_frametime_ms: 16.55,
            one_percent_low_ms: 17.87,
            heap_used_mib: 512.5,
            frames_counted: true,
            notes: Vec::new(),
            failures: Vec::new(),
        }
    }

    #[test]
    fn the_measurements_of_a_report_are_read() {
        let measurements = measurements_of(&report()).expect("there are no measurements");

        assert!((measurements.fps - 60.67).abs() < 1e-6);
        assert!((measurements.frametime_ms - 17.87).abs() < 1e-6);
        assert!((measurements.heap_mib - 512.5).abs() < 1e-9);
    }

    #[test]
    fn a_report_without_frames_has_no_measurements() {
        let mut report = report();
        report.frames = 0;
        report.frames_counted = false;

        assert_eq!(measurements_of(&report), None);
    }

    #[test]
    fn a_report_with_nonsense_in_it_has_no_measurements() {
        let mut report = report();
        report.average_fps = 0.0;

        assert_eq!(measurements_of(&report), None);
    }

    /// Store a run of the game, and make sure that it was stored.
    fn store(storage: &Storage, report: &AgentReport, meta: &mut Meta, scenario: &str) {
        let measurements = measurements_of(report).expect("the report holds no measurement");
        let stored = store_measurements(storage, &measurements, meta, scenario);
        assert_eq!(stored, 2, "the run was not stored under {scenario}");
    }

    #[test]
    fn the_measurements_are_stored() {
        let storage = storage();
        let mut meta = Meta::of_this_machine();

        let measurements = measurements_of(&report()).unwrap();
        let stored =
            store_measurements(&storage, &measurements, &mut meta, &scenario_of("balanced"));

        assert_eq!(stored, 2);

        // And they can be read back.
        let fps =
            crate::record::load(&storage, Metric::InGameFps, &scenario_of("balanced")).unwrap();
        assert!((fps.mean().unwrap() - 60.67).abs() < 1e-6);
    }

    #[test]
    fn the_runs_with_the_mod_and_without_it_do_not_overwrite_each_other() {
        // The whole point of the preset in the scenario: two runs of one
        // build have to be able to stand next to each other.
        let storage = storage();
        let mut meta = Meta::of_this_machine();

        let mut without = report();
        without.average_fps = 60.0;
        let mut with = report();
        with.average_fps = 120.0;

        store(&storage, &without, &mut meta, &scenario_without_mod());
        store(&storage, &with, &mut meta, &scenario_of("balanced"));

        let off =
            crate::record::load(&storage, Metric::InGameFps, &scenario_without_mod()).unwrap();
        let balanced =
            crate::record::load(&storage, Metric::InGameFps, &scenario_of("balanced")).unwrap();

        assert!((off.mean().unwrap() - 60.0).abs() < 1e-6);
        assert!((balanced.mean().unwrap() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn the_memory_of_the_process_is_not_the_heap_of_the_machine() {
        // The heap of the virtual machine is not the working set of the
        // process: the metric M8 is measured from the outside, and the
        // heap is only kept as a part of the metadata of the run.
        let storage = storage();
        let mut meta = Meta::of_this_machine();

        store(&storage, &report(), &mut meta, &scenario_of("balanced"));

        assert!(
            crate::record::load(&storage, Metric::GameRss, &scenario_of("balanced")).is_none(),
            "the heap was stored as the memory of the process"
        );
        assert_eq!(
            meta.extra.get("heap_mib").map(String::as_str),
            Some("512.5"),
            "the heap was not kept in the metadata"
        );
    }

    #[test]
    fn the_pair_of_the_game_is_the_two_runs() {
        let storage = storage();
        let mut meta = Meta::of_this_machine();

        let mut without = report();
        without.average_fps = 60.0;
        let mut with = report();
        with.average_fps = 120.0;

        store(&storage, &without, &mut meta, &scenario_without_mod());
        store(&storage, &with, &mut meta, &scenario_of("balanced"));

        let pair = pair_of(&storage, Metric::InGameFps).expect("there is no pair");

        assert_eq!(pair.without_mod_scenario, scenario_without_mod());
        assert_eq!(pair.with_mod_scenario, scenario_of("balanced"));
        assert!((pair.without_mod.mean().unwrap() - 60.0).abs() < 1e-6);
        assert!((pair.with_mod.mean().unwrap() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn there_is_no_pair_without_both_sides() {
        let storage = storage();
        let mut meta = Meta::of_this_machine();

        store(&storage, &report(), &mut meta, &scenario_of("balanced"));

        assert!(pair_of(&storage, Metric::InGameFps).is_none());
    }
}
