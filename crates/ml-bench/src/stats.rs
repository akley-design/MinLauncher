//! The statistics of a measurement series.
//!
//! A single measurement says nothing: the machine is noisy, the disk
//! caches, the CPU throttles. A **series** of N measurements does, and
//! the summary of a series is what gets compared and stored.
//!
//! The percentiles are computed with the *nearest rank* method on the
//! sorted samples: the p-th percentile is the smallest sample that at
//! least p percent of the samples are below. The method is exact, does
//! not interpolate, and — importantly for a benchmark — does not invent
//! a value that was never measured.

use serde::{Deserialize, Serialize};

/// A series of measurements, in milliseconds or another unit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// The samples, in the order they were measured.
    pub samples: Vec<f64>,
}

impl Series {
    /// An empty series.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A series built from a list of samples.
    #[must_use]
    pub fn from_samples(samples: Vec<f64>) -> Self {
        Self { samples }
    }

    /// Add a sample.
    pub fn push(&mut self, sample: f64) {
        self.samples.push(sample);
    }

    /// Whether the series holds no sample.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// The smallest sample.
    #[must_use]
    pub fn min(&self) -> Option<f64> {
        self.samples.iter().copied().reduce(f64::min)
    }

    /// The largest sample.
    #[must_use]
    pub fn max(&self) -> Option<f64> {
        self.samples.iter().copied().reduce(f64::max)
    }

    /// The arithmetic mean.
    ///
    /// Returns `None` for an empty series.
    #[must_use]
    pub fn mean(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        Some(self.samples.iter().sum::<f64>() / self.samples.len() as f64)
    }

    /// The standard deviation of the samples.
    ///
    /// The *population* standard deviation is used, because the series is
    /// the whole measurement and not a sample of a larger population.
    /// Returns `None` for an empty series.
    #[must_use]
    pub fn stddev(&self) -> Option<f64> {
        let mean = self.mean()?;
        if self.samples.len() < 2 {
            return Some(0.0);
        }
        let variance = self
            .samples
            .iter()
            .map(|sample| (sample - mean).powi(2))
            .sum::<f64>()
            / self.samples.len() as f64;
        Some(variance.sqrt())
    }

    /// The nearest-rank percentile of the series.
    ///
    /// `percentile` is a number between 0 and 100. The result is the
    /// smallest sample that at least `percentile` percent of the samples
    /// are at or below.
    ///
    /// Returns `None` for an empty series.
    #[must_use]
    pub fn percentile(&self, percentile: f64) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(f64::total_cmp);

        // The rank is one-based and is rounded up, so that the median of
        // an odd number of samples is the sample in the middle.
        //
        // The arithmetic is done with integers: the percentile is turned
        // into a whole number of hundredths first, and the rank is then
        // `count * hundredths / 10_000` rounded up. Working in integers
        // keeps the result exact.
        //
        // The value that is cast is the result of `clamp`, so it is
        // between zero and ten thousand and never negative.
        let clamped = if percentile.is_nan() {
            0.0
        } else {
            percentile.clamp(0.0, 100.0)
        };
        #[allow(clippy::cast_sign_loss)]
        let hundredths = (clamped * 100.0).round() as u64;
        let count = sorted.len() as u64;
        let rank = (count * hundredths).div_ceil(10_000).max(1);
        let index = rank.min(count) as usize;
        Some(sorted[index - 1])
    }

    /// The median of the series.
    #[must_use]
    pub fn median(&self) -> Option<f64> {
        self.percentile(50.0)
    }

    /// The interquartile range of the series.
    ///
    /// The distance between the 75th and the 25th percentile. It covers
    /// the half of the samples in the middle and ignores the tails, which
    /// is what makes it usable as a measure of how quiet a machine was:
    /// a single run that was delayed by the antivirus does not widen it.
    ///
    /// Returns `None` for an empty series.
    #[must_use]
    pub fn interquartile_range(&self) -> Option<f64> {
        let low = self.percentile(25.0)?;
        let high = self.percentile(75.0)?;
        Some(high - low)
    }

    /// The summary of the series, as it gets stored.
    ///
    /// Returns `None` for an empty series, because there is nothing
    /// meaningful to record.
    #[must_use]
    pub fn summary(&self) -> Option<Summary> {
        Some(Summary {
            samples: self.samples.len(),
            p50: self.percentile(50.0)?,
            p95: self.percentile(95.0)?,
            p99: self.percentile(99.0)?,
            mean: self.mean()?,
            stddev: self.stddev()?,
        })
    }
}

/// The summary of a series.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// The number of samples.
    pub samples: usize,
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
}

/// The coefficient of variation, in percent.
///
/// The classic measure of how much a measurement wiggles around its
/// mean. It is kept because it describes the series as a whole, but it
/// is *not* the measure the noise gate uses: a single run that was
/// delayed by the antivirus or by a disk flush drags it up, and that is
/// exactly the run the tail of the series is supposed to describe.
#[must_use]
pub fn variation(series: &Series) -> Option<f64> {
    let mean = series.mean()?;
    if mean == 0.0 {
        return None;
    }
    Some(series.stddev()? / mean * 100.0)
}

/// The relative interquartile spread of a series, in percent.
///
/// The interquartile range divided by the median. Unlike the
/// coefficient of variation it ignores the tails, so it measures how
/// tightly the *typical* run of a measurement repeats — which is the
/// question the noise gate is actually asking.
#[must_use]
pub fn spread(series: &Series) -> Option<f64> {
    let median = series.median()?;
    if median == 0.0 {
        return None;
    }
    Some(series.interquartile_range()? / median * 100.0)
}

/// Whether a series is quiet enough to be trusted.
///
/// A series whose relative interquartile spread is above `limit` percent
/// is noisy: the machine was busy, or something else was running.
///
/// The limit is compared against the spread and not against the
/// coefficient of variation, because a launch that was once delayed by
/// the antivirus is a launch that has to be measured, not one that has
/// to be thrown away.
#[must_use]
pub fn is_stable(series: &Series, limit: f64) -> bool {
    spread(series).is_some_and(|value| value <= limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(values: &[f64]) -> Series {
        Series::from_samples(values.to_vec())
    }

    #[test]
    fn an_empty_series_has_no_summary() {
        let empty = Series::new();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.mean().is_none());
        assert!(empty.median().is_none());
        assert!(empty.summary().is_none());
        assert!(empty.min().is_none());
        assert!(empty.max().is_none());
    }

    #[test]
    fn the_mean_is_the_arithmetic_mean() {
        let s = series(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(s.mean(), Some(2.5));
        assert_eq!(s.min(), Some(1.0));
        assert_eq!(s.max(), Some(4.0));
    }

    #[test]
    fn the_median_of_an_odd_series_is_the_middle_sample() {
        let s = series(&[3.0, 1.0, 2.0]);
        assert_eq!(s.median(), Some(2.0));
    }

    #[test]
    fn the_median_of_an_even_series_is_the_lower_of_the_two_middle_ones() {
        // The nearest-rank method takes the sample at the rank, not the
        // average of the two in the middle.
        let s = series(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(s.median(), Some(2.0));
    }

    #[test]
    fn the_percentiles_follow_the_nearest_rank() {
        let s = series(&(1..=100).map(f64::from).collect::<Vec<_>>());
        assert_eq!(s.percentile(50.0), Some(50.0));
        assert_eq!(s.percentile(95.0), Some(95.0));
        assert_eq!(s.percentile(99.0), Some(99.0));
        assert_eq!(s.percentile(100.0), Some(100.0));
        assert_eq!(s.percentile(0.0), Some(1.0));
    }

    #[test]
    fn the_percentiles_ignore_the_order_of_the_samples() {
        let sorted = series(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let shuffled = series(&[3.0, 1.0, 5.0, 2.0, 4.0]);
        assert_eq!(sorted.percentile(95.0), shuffled.percentile(95.0));
        assert_eq!(sorted.median(), shuffled.median());
    }

    #[test]
    fn the_standard_deviation_of_a_flat_series_is_zero() {
        let s = series(&[7.0, 7.0, 7.0]);
        assert_eq!(s.stddev(), Some(0.0));
        assert_eq!(variation(&s), Some(0.0));
    }

    #[test]
    fn the_standard_deviation_follows_the_population_formula() {
        // 2, 4, 4, 4, 5, 5, 7, 9 has a mean of 5 and a population
        // standard deviation of 2.
        let s = series(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert_eq!(s.mean(), Some(5.0));
        assert_eq!(s.stddev(), Some(2.0));
    }

    #[test]
    fn a_single_sample_has_no_spread() {
        let s = series(&[42.0]);
        assert_eq!(s.stddev(), Some(0.0));
        assert_eq!(s.percentile(99.0), Some(42.0));
    }

    #[test]
    fn the_summary_carries_every_statistic() {
        let s = series(&[10.0, 20.0, 30.0, 40.0]);
        let summary = s.summary().unwrap();
        assert_eq!(summary.samples, 4);
        assert_eq!(summary.p50, 20.0);
        assert_eq!(summary.p95, 40.0);
        assert_eq!(summary.p99, 40.0);
        assert_eq!(summary.mean, 25.0);
    }

    #[test]
    fn a_noisy_series_is_not_stable() {
        let quiet = series(&[100.0, 101.0, 99.0, 100.0]);
        let noisy = series(&[100.0, 200.0, 50.0, 130.0]);
        assert!(is_stable(&quiet, 5.0));
        assert!(!is_stable(&noisy, 5.0));
    }

    #[test]
    fn a_delayed_run_does_not_make_the_series_noisy() {
        // A launch that was once held up by the antivirus is a launch
        // that has to be measured: the gate looks at the middle of the
        // series, not at the one run that went wrong.
        let mut samples: Vec<f64> = (0..29).map(|_| 800.0).collect();
        samples.push(1400.0);
        let s = series(&samples);

        assert!(is_stable(&s, 5.0));
        assert_eq!(s.percentile(99.0), Some(1400.0));
    }

    #[test]
    fn the_interquartile_range_covers_the_middle_of_the_series() {
        let s = series(&(1..=100).map(f64::from).collect::<Vec<_>>());
        assert_eq!(s.interquartile_range(), Some(50.0));
        assert_eq!(s.percentile(25.0), Some(25.0));
        assert_eq!(s.percentile(75.0), Some(75.0));
    }

    #[test]
    fn the_spread_is_the_interquartile_range_over_the_median() {
        let s = series(&[99.0, 100.0, 101.0]);
        assert_eq!(spread(&s), Some(2.0));
    }

    #[test]
    fn a_series_of_zeroes_has_no_spread() {
        let s = series(&[0.0, 0.0]);
        assert_eq!(spread(&s), None);
        assert!(!is_stable(&s, 5.0));
    }

    #[test]
    fn a_series_of_zeroes_has_no_variation() {
        let s = series(&[0.0, 0.0]);
        assert_eq!(variation(&s), None);
    }
}
