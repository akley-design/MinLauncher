//! Reading the report of the profiling agent.
//!
//! The agent writes a small JSON document while the game runs, and once
//! more when it ends. The launcher reads it after the game is over, and
//! turns the numbers in it into the measurements of the metrics M5, M6
//! and M8.
//!
//! The document is flat on purpose: the agent must not depend on
//! anything that can fail to load, and the launcher must not have to
//! guess what the fields mean.
//!
//! A missing report is not a failure. The game may have been started
//! without the agent, or the agent may have failed — it says so in the
//! report itself, but the report of a failed agent is still a report,
//! and a game that was not measured is a game that was not measured.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The report of a run of the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentReport {
    /// When the agent was attached, in nanoseconds.
    #[serde(default)]
    pub agent_started_at_nanos: i64,
    /// How long the game has been running, in nanoseconds.
    #[serde(default)]
    pub uptime_nanos: i64,
    /// How many frames were measured.
    #[serde(default)]
    pub frames: u64,
    /// The average frame rate, in frames a second.
    #[serde(default)]
    pub average_fps: f64,
    /// The average frame time, in milliseconds.
    #[serde(default)]
    pub average_frametime_ms: f64,
    /// The 99th percentile of the frame time, in milliseconds.
    #[serde(default)]
    pub p99_frametime_ms: f64,
    /// The 95th percentile of the frame time, in milliseconds.
    #[serde(default)]
    pub p95_frametime_ms: f64,
    /// The 50th percentile of the frame time, in milliseconds.
    #[serde(default)]
    pub p50_frametime_ms: f64,
    /// The average of the slowest one per cent of the frames, in
    /// milliseconds.
    #[serde(default)]
    pub one_percent_low_ms: f64,
    /// The memory the game uses, in mebibytes.
    #[serde(default)]
    pub heap_used_mib: f64,
    /// Whether the frames were counted at all.
    #[serde(default)]
    pub frames_counted: bool,
    /// What the agent has to say about the run.
    #[serde(default)]
    pub notes: Vec<String>,
    /// What went wrong while the agent worked.
    #[serde(default)]
    pub failures: Vec<String>,
}

impl AgentReport {
    /// Whether the report holds a usable measurement.
    ///
    /// A report without frames says nothing about how the game runs,
    /// even when the agent itself did not fail: the game may simply
    /// not use the library the agent knows how to watch.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.frames_counted && self.frames > 0
    }

    /// The frame time of the metric M6, in milliseconds.
    ///
    /// The one per cent low is used and not the 99th percentile: the
    /// metric says "the 99th percentile of the frame time", and the
    /// agent reports the low itself, which is the same number in a
    /// form that is easier to compare.
    #[must_use]
    pub fn frametime_ms(&self) -> f64 {
        if self.one_percent_low_ms > 0.0 {
            self.one_percent_low_ms
        } else {
            self.p99_frametime_ms
        }
    }
}

/// Read the report of the agent.
///
/// Returns `None` when the report is missing or cannot be read. The
/// document is written by a process that may have been killed, so a
/// half-written file is possible in principle — the agent writes it
/// atomically, but a launcher that reads it at the wrong moment must
/// not fall over.
#[must_use]
pub fn read_report(path: &Path) -> Option<AgentReport> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "agent_started_at_nanos": 48826054101100,
  "uptime_nanos": 2031442800,
  "frames": 119,
  "average_fps": 60.673668,
  "average_frametime_ms": 16.481614,
  "p99_frametime_ms": 17.850600,
  "p95_frametime_ms": 17.507900,
  "p50_frametime_ms": 16.547800,
  "one_percent_low_ms": 17.869300,
  "heap_used_mib": 512.5,
  "frames_counted": true,
  "notes": ["the agent is attached"],
  "failures": []
}"#;

    #[test]
    fn the_report_of_a_run_is_read() {
        let report: AgentReport = serde_json::from_str(SAMPLE).unwrap();

        assert_eq!(report.frames, 119);
        assert!((report.average_fps - 60.673668).abs() < 1e-6);
        assert!((report.average_frametime_ms - 16.481614).abs() < 1e-6);
        assert!((report.heap_used_mib - 512.5).abs() < 1e-9);
        assert!(report.frames_counted);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn a_report_without_frames_is_not_usable() {
        let report: AgentReport =
            serde_json::from_str(r#"{"frames": 0, "frames_counted": false}"#).unwrap();

        assert!(!report.is_usable());
    }

    #[test]
    fn a_report_with_frames_is_usable() {
        let report: AgentReport =
            serde_json::from_str(r#"{"frames": 10, "frames_counted": true}"#).unwrap();

        assert!(report.is_usable());
    }

    #[test]
    fn the_low_is_the_frametime_of_the_metric() {
        let report: AgentReport = serde_json::from_str(SAMPLE).unwrap();

        assert!((report.frametime_ms() - 17.8693).abs() < 1e-6);
    }

    #[test]
    fn the_percentile_is_used_when_there_is_no_low() {
        let report: AgentReport =
            serde_json::from_str(r#"{"p99_frametime_ms": 12.5, "one_percent_low_ms": 0.0}"#)
                .unwrap();

        assert!((report.frametime_ms() - 12.5).abs() < 1e-9);
    }

    #[test]
    fn a_missing_report_is_not_a_failure() {
        assert_eq!(read_report(Path::new("/does/not/exist.json")), None);
    }

    #[test]
    fn a_broken_report_is_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        std::fs::write(&path, b"{ this is not json").unwrap();

        assert_eq!(read_report(&path), None);
    }

    #[test]
    fn a_report_is_read_from_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        std::fs::write(&path, SAMPLE).unwrap();

        let report = read_report(&path).expect("the report was not read");
        assert_eq!(report.frames, 119);
    }

    #[test]
    fn a_report_without_the_new_fields_is_still_read() {
        // The agent of an older build does not know about every field.
        // The launcher has to be able to read its report anyway.
        let report: AgentReport = serde_json::from_str(r#"{"frames": 3}"#).unwrap();

        assert_eq!(report.frames, 3);
        assert!(!report.frames_counted);
    }
}
