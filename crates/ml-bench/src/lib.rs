//! Benchmark harness for `MinLauncher`.
//!
//! The project promises to start Minecraft at least **20 % faster** than
//! the reference launcher, and to make the game itself measurably faster.
//! A promise like that is only worth something if the measurement is
//! fixed before the result is known, so the harness is built around the
//! metrics of `docs/PLAN.md` § 1:
//!
//! * [`metric`] names every metric, the direction of an improvement and
//!   the target it has to reach;
//! * [`stats`] turns a series of measurements into the summary that gets
//!   compared and stored;
//! * [`scenario`] runs a measurement the same way N times;
//! * [`compare`] decides whether the target was reached;
//! * [`record`] stores the result next to the metadata that makes it
//!   reproducible;
//! * [`ingame`] turns the report of the profiling agent into the
//!   measurements of the metrics that are about the game itself.
//!
//! # Example
//!
//! ```
//! use ml_bench::compare::compare;
//! use ml_bench::metric::{Metric, Verdict};
//! use ml_bench::stats::Series;
//!
//! let ours = Series::from_samples(vec![790.0, 800.0, 810.0]);
//! let theirs = Series::from_samples(vec![990.0, 1000.0, 1010.0]);
//!
//! let comparison = compare(Metric::ClickToWindow, &ours, &theirs).unwrap();
//! assert_eq!(comparison.verdict, Verdict::Met);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod compare;
pub mod ingame;
pub mod metric;
pub mod record;
pub mod report;
pub mod scenario;
pub mod stats;

pub use compare::{Comparison, DEFAULT_NOISE_LIMIT, check_absolute, compare, compare_with_limit};
pub use ingame::{
    IN_GAME_PREFIX, InGameMeasurements, InGamePair, PRESET_OFF, PRESETS, measurements_of, pair_of,
    scenario_of, scenario_with_mod, scenario_without_mod, store_measurements,
};
pub use metric::{Direction, Metric, Verdict};
pub use record::{Meta, load, load_meta, store, to_record};
pub use report::Report;
pub use scenario::{DEFAULT_SAMPLES, DEFAULT_WARMUP, Run, Scenario, measure, measure_ms};
pub use stats::{Series, Summary, is_stable, spread, variation};
