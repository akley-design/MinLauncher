//! Comparing a measurement with the one of `TLauncher`.
//!
//! The comparison is deliberately conservative:
//!
//! * it compares the **medians**, not the minima: the fastest run of
//!   the two is the one that happened to get a warm page cache, and it
//!   says nothing about the experience of a user;
//! * it compares the **same** statistic on both sides. Comparing our
//!   worst run against their best one would make the verdict depend on
//!   how many samples were taken, and would reward a launcher whose
//!   median is slow but whose luck is good;
//! * a series that is too noisy is not compared at all — a result that
//!   is not repeatable is not a result.
//!
//! The tails are not thrown away: both summaries carry the 95th and the
//! 99th percentile, so a reader of the report sees the whole
//! distribution and not only the number the verdict was taken from.

use serde::{Deserialize, Serialize};

use crate::metric::{Direction, Metric, Verdict};
use crate::stats::{Series, Summary, is_stable};

/// The default limit of the coefficient of variation, in percent.
///
/// Above this the measurement is considered too noisy to compare.
pub const DEFAULT_NOISE_LIMIT: f64 = 5.0;

/// Two measurements of the same metric, and the verdict on them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The metric that was compared.
    pub metric: Metric,
    /// The measurement of `MinLauncher`.
    pub ours: Summary,
    /// The measurement of `TLauncher`.
    pub theirs: Summary,
    /// The ratio of ours to theirs, in percent.
    pub ratio_percent: f64,
    /// The verdict on the target.
    pub verdict: Verdict,
    /// The ratio the target asks for, if it has one.
    pub target_ratio: Option<f64>,
}

impl Comparison {
    /// Whether the target was reached.
    #[must_use]
    pub fn is_met(&self) -> bool {
        self.verdict.is_met()
    }

    /// How much faster we are, in percent.
    ///
    /// A positive number means `MinLauncher` is faster. For a metric
    /// where a bigger value is better the sign is flipped.
    #[must_use]
    pub fn improvement_percent(&self) -> f64 {
        match self.metric.direction() {
            Direction::Lower => 100.0 - self.ratio_percent,
            Direction::Higher => self.ratio_percent - 100.0,
        }
    }
}

/// Compare two series of the same metric.
///
/// Returns `None` when the comparison cannot be made: either series is
/// empty, or one of them is too noisy to be trusted.
#[must_use]
pub fn compare(metric: Metric, ours: &Series, theirs: &Series) -> Option<Comparison> {
    compare_with_limit(metric, ours, theirs, DEFAULT_NOISE_LIMIT)
}

/// Compare two series, with a custom limit of noise.
///
/// Returns `None` when the comparison cannot be made.
#[must_use]
pub fn compare_with_limit(
    metric: Metric,
    ours: &Series,
    theirs: &Series,
    noise_limit: f64,
) -> Option<Comparison> {
    if !is_stable(ours, noise_limit) || !is_stable(theirs, noise_limit) {
        return None;
    }

    let ours = ours.summary()?;
    let theirs = theirs.summary()?;

    // The value that represents the measurement. The same statistic is
    // taken from both series, so that the verdict does not depend on
    // how many samples were measured: the median of ours against the
    // median of theirs.
    let (our_value, their_value) = (ours.p50, theirs.p50);

    if their_value <= 0.0 {
        return None;
    }
    let ratio = our_value / their_value;

    let target_ratio = metric.target_ratio();
    let verdict = match target_ratio {
        Some(target) => {
            let reached = match metric.direction() {
                Direction::Lower => ratio <= target,
                Direction::Higher => ratio >= target,
            };
            if reached {
                Verdict::Met
            } else {
                Verdict::Missed
            }
        }
        None => Verdict::Unknown,
    };

    Some(Comparison {
        metric,
        ours,
        theirs,
        ratio_percent: ratio * 100.0,
        verdict,
        target_ratio,
    })
}

/// Compare a measurement against an absolute target.
///
/// Used for the metrics that have a limit of their own, such as the
/// memory of the launcher.
#[must_use]
pub fn check_absolute(metric: Metric, ours: &Series) -> Option<Comparison> {
    let target = metric.absolute_target()?;
    let summary = ours.summary()?;
    let verdict = if summary.p99 <= target {
        Verdict::Met
    } else {
        Verdict::Missed
    };

    Some(Comparison {
        metric,
        ours: summary,
        theirs: Summary {
            samples: 0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            mean: 0.0,
            stddev: 0.0,
        },
        ratio_percent: 0.0,
        verdict,
        target_ratio: Some(target),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(values: &[f64]) -> Series {
        Series::from_samples(values.to_vec())
    }

    /// A quiet series around `centre`.
    fn quiet(centre: f64) -> Series {
        series(&[centre - 1.0, centre, centre + 1.0])
    }

    #[test]
    fn a_twenty_percent_faster_launch_meets_the_target() {
        let ours = quiet(800.0);
        let theirs = quiet(1000.0);

        let comparison = compare(Metric::ClickToWindow, &ours, &theirs).unwrap();

        assert!(comparison.is_met());
        assert_eq!(comparison.verdict, Verdict::Met);
        assert!((comparison.ratio_percent - 80.0).abs() < 0.01);
        assert!((comparison.improvement_percent() - 20.0).abs() < 0.01);
    }

    #[test]
    fn a_launch_that_is_only_ten_percent_faster_misses_the_target() {
        let ours = quiet(900.0);
        let theirs = quiet(1000.0);

        let comparison = compare(Metric::ClickToWindow, &ours, &theirs).unwrap();

        assert!(!comparison.is_met());
        assert_eq!(comparison.verdict, Verdict::Missed);
    }

    #[test]
    fn a_slower_launch_misses_the_target() {
        let comparison = compare(Metric::ClickToWindow, &quiet(1200.0), &quiet(1000.0)).unwrap();
        assert!(!comparison.is_met());
        assert!(comparison.improvement_percent() < 0.0);
    }

    #[test]
    fn a_noisy_series_is_not_compared() {
        let noisy = series(&[500.0, 1500.0, 700.0, 1300.0]);
        assert!(compare(Metric::ClickToWindow, &noisy, &quiet(1000.0)).is_none());
        assert!(compare(Metric::ClickToWindow, &quiet(800.0), &noisy).is_none());
    }

    #[test]
    fn an_empty_series_is_not_compared() {
        assert!(compare(Metric::ClickToWindow, &Series::new(), &quiet(1000.0)).is_none());
        assert!(compare(Metric::ClickToWindow, &quiet(800.0), &Series::new()).is_none());
    }

    #[test]
    fn a_delayed_run_does_not_hide_the_median() {
        // One run of ours was delayed, the rest were not. The verdict
        // follows the median, and the delay stays visible in the
        // summary instead of deciding the outcome.
        let ours = series(&[700.0, 710.0, 720.0, 730.0, 1400.0]);
        let theirs = series(&[1000.0, 1010.0, 1020.0, 1030.0, 1040.0]);

        let comparison = compare(Metric::ClickToWindow, &ours, &theirs).unwrap();

        assert!(comparison.is_met());
        assert!((comparison.ours.p50 - 720.0).abs() < 0.01);
        assert!((comparison.theirs.p50 - 1020.0).abs() < 0.01);
        assert!((comparison.ours.p99 - 1400.0).abs() < 0.01);
    }

    #[test]
    fn the_verdict_does_not_depend_on_the_number_of_samples() {
        // The same measurement, measured twice with a different number of
        // samples, has to reach the same verdict.
        let ours = quiet(800.0);
        let theirs = quiet(1000.0);

        let three = compare(Metric::ClickToWindow, &ours, &theirs).unwrap();

        let mut ours_many: Vec<f64> = (0..30).map(|_| 800.0).collect();
        ours_many.push(900.0);
        let mut theirs_many: Vec<f64> = (0..30).map(|_| 1000.0).collect();
        theirs_many.push(1100.0);

        let many = compare(
            Metric::ClickToWindow,
            &series(&ours_many),
            &series(&theirs_many),
        )
        .unwrap();

        assert_eq!(three.verdict, many.verdict);
        assert!((three.ratio_percent - many.ratio_percent).abs() < 0.01);
    }

    #[test]
    fn a_faster_frame_rate_is_an_improvement() {
        let ours = quiet(120.0);
        let theirs = quiet(100.0);

        let comparison = compare(Metric::InGameFps, &ours, &theirs).unwrap();

        assert!(comparison.is_met());
        assert!(comparison.improvement_percent() > 0.0);
    }

    #[test]
    fn a_slower_frame_rate_misses_the_target() {
        let comparison = compare(Metric::InGameFps, &quiet(90.0), &quiet(100.0)).unwrap();
        assert!(!comparison.is_met());
    }

    #[test]
    fn the_memory_of_the_launcher_is_checked_against_its_own_limit() {
        let comparison = check_absolute(Metric::LauncherRss, &quiet(50.0)).unwrap();
        assert!(comparison.is_met());

        let comparison = check_absolute(Metric::LauncherRss, &quiet(80.0)).unwrap();
        assert!(!comparison.is_met());
    }

    #[test]
    fn a_metric_without_a_relative_target_cannot_be_compared_relatively() {
        let comparison = compare(Metric::LauncherRss, &quiet(50.0), &quiet(100.0)).unwrap();
        assert_eq!(comparison.verdict, Verdict::Unknown);
        assert_eq!(comparison.target_ratio, None);
    }
}
