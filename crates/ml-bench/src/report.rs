//! The report of a comparison.
//!
//! A comparison of eight metrics is a table, not a paragraph. The
//! report renders that table in a form that can go straight into a
//! pull request or a release note, and it is also what the UI shows in
//! the tab "Performance".

use std::fmt;

use crate::compare::Comparison;
use crate::metric::{Metric, Verdict};

/// The report of a set of comparisons.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The comparisons the report holds.
    pub comparisons: Vec<Comparison>,
}

impl Report {
    /// An empty report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a comparison to the report.
    pub fn push(&mut self, comparison: Comparison) {
        self.comparisons.push(comparison);
    }

    /// Whether every comparison met its target.
    ///
    /// A report without a comparison is not a success: there is nothing
    /// to claim.
    #[must_use]
    pub fn is_success(&self) -> bool {
        !self.comparisons.is_empty()
            && self
                .comparisons
                .iter()
                .all(|comparison| comparison.verdict == Verdict::Met)
    }

    /// The comparisons that missed their target.
    #[must_use]
    pub fn failures(&self) -> Vec<&Comparison> {
        self.comparisons
            .iter()
            .filter(|comparison| comparison.verdict == Verdict::Missed)
            .collect()
    }

    /// The metrics of the report, in the order they were added.
    #[must_use]
    pub fn metrics(&self) -> Vec<Metric> {
        self.comparisons.iter().map(|c| c.metric).collect()
    }

    /// The comparison of a metric.
    #[must_use]
    pub fn get(&self, metric: Metric) -> Option<&Comparison> {
        self.comparisons
            .iter()
            .find(|comparison| comparison.metric == metric)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<6} {:<24} {:>10} {:>10} {:>9}  {:<8}",
            "ID", "metric", "ours", "reference", "ratio", "verdict"
        )?;

        for comparison in &self.comparisons {
            let metric = comparison.metric;
            let verdict = match comparison.verdict {
                Verdict::Met => "ok",
                Verdict::Missed => "MISSED",
                Verdict::Unknown => "n/a",
            };
            writeln!(
                f,
                "{:<6} {:<24} {:>10.2} {:>10.2} {:>8.1}%  {}",
                metric.code(),
                metric.as_str(),
                comparison.ours.p50,
                comparison.theirs.p50,
                comparison.ratio_percent,
                verdict
            )?;
        }

        writeln!(f)?;
        if self.is_success() {
            writeln!(f, "every target met")?;
        } else if self.comparisons.is_empty() {
            writeln!(f, "nothing was measured")?;
        } else {
            writeln!(
                f,
                "{} of {} targets missed",
                self.failures().len(),
                self.comparisons.len()
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::compare;
    use crate::stats::Series;

    fn series(values: &[f64]) -> Series {
        Series::from_samples(values.to_vec())
    }

    fn comparison(metric: Metric, ours: f64, theirs: f64) -> Comparison {
        compare(metric, &series(&[ours]), &series(&[theirs])).unwrap()
    }

    #[test]
    fn a_report_of_met_targets_is_a_success() {
        let mut report = Report::new();
        report.push(comparison(Metric::ClickToWindow, 800.0, 1000.0));
        report.push(comparison(Metric::InGameFps, 120.0, 100.0));

        assert!(report.is_success());
        assert_eq!(report.failures().len(), 0);
    }

    #[test]
    fn a_report_with_a_missed_target_is_not_a_success() {
        let mut report = Report::new();
        report.push(comparison(Metric::ClickToWindow, 800.0, 1000.0));
        report.push(comparison(Metric::FreshInstall, 1000.0, 1000.0));

        assert!(!report.is_success());
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.failures()[0].metric, Metric::FreshInstall);
    }

    #[test]
    fn an_empty_report_is_not_a_success() {
        let report = Report::new();
        assert!(!report.is_success());
    }

    #[test]
    fn the_metrics_are_in_the_order_they_were_added() {
        let mut report = Report::new();
        report.push(comparison(Metric::InGameFps, 120.0, 100.0));
        report.push(comparison(Metric::ClickToWindow, 800.0, 1000.0));

        assert_eq!(
            report.metrics(),
            vec![Metric::InGameFps, Metric::ClickToWindow]
        );
        assert!(report.get(Metric::InGameFps).is_some());
    }

    #[test]
    fn the_report_renders_a_table() {
        let mut report = Report::new();
        report.push(comparison(Metric::ClickToWindow, 800.0, 1000.0));

        let text = report.to_string();
        assert!(text.contains("M3"));
        assert!(text.contains("click_to_window"));
        assert!(text.contains("80.0%"));
        assert!(text.contains("every target met"));
    }

    #[test]
    fn the_report_says_how_many_targets_were_missed() {
        let mut report = Report::new();
        report.push(comparison(Metric::ClickToWindow, 800.0, 1000.0));
        report.push(comparison(Metric::FreshInstall, 1200.0, 1000.0));

        let text = report.to_string();
        assert!(text.contains("1 of 2 targets missed"));
        assert!(text.contains("MISSED"));
    }
}
