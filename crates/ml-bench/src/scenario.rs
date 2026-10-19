//! Running a scenario and keeping the samples.
//!
//! A scenario is anything that can be measured once: starting the
//! launcher, building a command line, unpacking the natives. The runner
//! does the boring part — it repeats the measurement N times, discards
//! the warm-up runs, and keeps the rest.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::stats::Series;

/// The default number of samples of a series.
///
/// The plan fixes N=30: a single measurement is not a result, and
/// thirty runs take long enough to average the noise away and short
/// enough to run on a normal machine.
pub const DEFAULT_SAMPLES: usize = 30;

/// The default number of warm-up runs.
///
/// The first run of anything is slow: the page cache is cold, the CPU
/// is still ramping up its frequency. The warm-up runs are thrown away.
pub const DEFAULT_WARMUP: usize = 1;

/// How a scenario is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// The number of measured runs.
    pub samples: usize,
    /// The number of warm-up runs that are thrown away.
    pub warmup: usize,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            samples: DEFAULT_SAMPLES,
            warmup: DEFAULT_WARMUP,
        }
    }
}

impl Scenario {
    /// A scenario with the given number of measured runs.
    #[must_use]
    pub fn with_samples(samples: usize) -> Self {
        Self {
            samples,
            warmup: DEFAULT_WARMUP,
        }
    }

    /// A scenario without warm-up runs.
    #[must_use]
    pub fn cold(samples: usize) -> Self {
        Self { samples, warmup: 0 }
    }
}

/// The result of running a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// The measured samples, in milliseconds.
    pub series: Series,
    /// How long the whole run took.
    pub duration: Duration,
}

/// Measure a scenario.
///
/// The closure is called once per warm-up run and once per sample. The
/// value it returns is the measured duration in milliseconds.
pub fn measure<F>(scenario: Scenario, mut measurement: F) -> Run
where
    F: FnMut() -> Duration,
{
    let started = Instant::now();

    for _ in 0..scenario.warmup {
        let _ = measurement();
    }

    let mut series = Series::new();
    for _ in 0..scenario.samples {
        series.push(measurement().as_secs_f64() * 1000.0);
    }

    Run {
        series,
        duration: started.elapsed(),
    }
}

/// Measure a scenario, taking the value in milliseconds instead of a
/// [`Duration`].
pub fn measure_ms<F>(scenario: Scenario, mut measurement: F) -> Run
where
    F: FnMut() -> f64,
{
    measure(scenario, || Duration::from_secs_f64(measurement() / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warm_up_runs_are_thrown_away() {
        let mut calls = 0;
        let run = measure(Scenario::with_samples(5), || {
            calls += 1;
            Duration::from_millis(10)
        });

        assert_eq!(calls, 6, "five samples and one warm-up");
        assert_eq!(run.series.len(), 5);
    }

    #[test]
    fn a_cold_scenario_has_no_warm_up() {
        let mut calls = 0;
        let run = measure(Scenario::cold(3), || {
            calls += 1;
            Duration::from_millis(1)
        });

        assert_eq!(calls, 3);
        assert_eq!(run.series.len(), 3);
    }

    #[test]
    fn the_samples_are_the_measured_milliseconds() {
        let run = measure_ms(Scenario::cold(3), || 42.5);
        assert_eq!(run.series.samples, vec![42.5, 42.5, 42.5]);
    }

    #[test]
    fn the_default_scenario_matches_the_plan() {
        let scenario = Scenario::default();
        assert_eq!(scenario.samples, 30);
        assert_eq!(scenario.warmup, 1);
    }
}
