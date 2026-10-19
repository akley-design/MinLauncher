//! Turning the report of the profiling agent into measurements.
//!
//! The agent watches the game from the inside and writes down what it
//! saw: how the frames were drawn, how much memory the game used. The
//! metrics M5, M6 and M8 are about exactly that, so the report of the
//! agent is the raw material of those three metrics.
//!
//! The numbers go into the database as the measurements of a run. They
//! are stored the same way as any other measurement, so that the
//! comparison against `TLauncher` does not have to know where they came
//! from.

use ml_launch::agent::AgentReport;
use ml_storage::Storage;

use crate::metric::Metric;
use crate::record::{Meta, store};
use crate::stats::Series;

/// The scenario the in-game measurements are stored under.
pub const IN_GAME_SCENARIO: &str = "in_game";

/// The measurements of one run of the game.
#[derive(Debug, Clone, PartialEq)]
pub struct InGameMeasurements {
    /// The average frame rate, in frames a second.
    pub fps: f64,
    /// The frame time of the metric M6, in milliseconds.
    pub frametime_ms: f64,
    /// The memory the game used, in mebibytes.
    pub memory_mib: f64,
}

/// The measurements of a report, when the report holds any.
///
/// A report without frames says nothing about how the game runs, and
/// one whose memory is zero says nothing about how much memory it
/// takes: the game may not have allocated anything yet when the report
/// was written.
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
        memory_mib: report.heap_used_mib,
    })
}

/// Store the measurements of a run of the game.
///
/// The three metrics are stored separately, because each of them has a
/// target of its own and is compared on its own.
#[must_use]
pub fn store_measurements(
    storage: &Storage,
    measurements: &InGameMeasurements,
    meta: &Meta,
) -> usize {
    let mut stored = 0;

    let fps = Series::from_samples(vec![measurements.fps]);
    if store(storage, Metric::InGameFps, IN_GAME_SCENARIO, &fps, meta).is_some() {
        stored += 1;
    }

    let frametime = Series::from_samples(vec![measurements.frametime_ms]);
    if store(
        storage,
        Metric::InGameFrametime,
        IN_GAME_SCENARIO,
        &frametime,
        meta,
    )
    .is_some()
    {
        stored += 1;
    }

    // The memory is only stored when the agent managed to read it: a
    // zero would be a measurement of nothing, and it would drag the
    // average of the memory down.
    if measurements.memory_mib > 0.0 {
        let memory = Series::from_samples(vec![measurements.memory_mib]);
        if store(storage, Metric::GameRss, IN_GAME_SCENARIO, &memory, meta).is_some() {
            stored += 1;
        }
    }

    stored
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
        assert!((measurements.memory_mib - 512.5).abs() < 1e-9);
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

    #[test]
    fn the_measurements_are_stored() {
        let storage = storage();
        let meta = Meta::of_this_machine();

        let measurements = measurements_of(&report()).unwrap();
        let stored = store_measurements(&storage, &measurements, &meta);

        assert_eq!(stored, 3);

        // And they can be read back.
        let fps = crate::record::load(&storage, Metric::InGameFps, IN_GAME_SCENARIO).unwrap();
        assert!((fps.mean().unwrap() - 60.67).abs() < 1e-6);
    }

    #[test]
    fn the_memory_is_not_stored_when_it_was_not_measured() {
        let storage = storage();
        let meta = Meta::of_this_machine();

        let mut report = report();
        report.heap_used_mib = 0.0;

        let measurements = measurements_of(&report).unwrap();
        let stored = store_measurements(&storage, &measurements, &meta);

        assert_eq!(stored, 2);
        assert!(
            crate::record::load(&storage, Metric::GameRss, IN_GAME_SCENARIO).is_none(),
            "the memory was stored even though it was not measured"
        );
    }
}
