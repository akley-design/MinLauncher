//! The report of a comparison.
//!
//! A comparison of eight metrics is a table, not a paragraph. The
//! report renders that table in a form that can go straight into a
//! pull request or a release note, and it is also what the UI shows in
//! the tab "Performance".
//!
//! The table always has a row for every metric of the plan, M1 to M8.
//! A metric that was not measured gets a row that says so: a missing
//! measurement and a measurement of zero are two different things, and
//! the table must not pretend that the second one is the first.

use std::fmt;

use crate::compare::{Comparison, check_absolute, compare, not_measured, uncompared};
use crate::metric::{Direction, Metric, Verdict};
use crate::samples::Side;

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

    /// The report of two sides of a comparison.
    ///
    /// Every metric of the plan gets a row. A metric that was measured
    /// on our side and on the reference side is compared; a metric with
    /// a limit of its own is judged against that limit; a metric that
    /// was measured on one side only is shown, but not judged.
    #[must_use]
    pub fn between(ours: &Side, theirs: &Side) -> Self {
        let mut report = Report::new();

        for metric in Metric::all().iter().copied() {
            let comparison = match ours.series_of(metric) {
                None => Some(not_measured(metric)),
                Some(our_series) => match metric.absolute_target() {
                    Some(_) => check_absolute(metric, our_series),
                    None => match theirs.series_of(metric) {
                        Some(their_series) => compare(metric, our_series, their_series)
                            .or_else(|| uncompared(metric, our_series, Some(their_series))),
                        None => uncompared(metric, our_series, None),
                    },
                },
            };

            if let Some(comparison) = comparison {
                report.push(comparison);
            }
        }

        report
    }

    /// Whether every comparison met its target.
    ///
    /// A report without a comparison is not a success: there is nothing
    /// to claim. A comparison that could not be judged is not a success
    /// either — a metric that was measured but not judged says nothing
    /// about the promise.
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

    /// The comparisons that could not be judged.
    #[must_use]
    pub fn unknown(&self) -> Vec<&Comparison> {
        self.comparisons
            .iter()
            .filter(|comparison| comparison.verdict == Verdict::Unknown)
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

/// The value of a measurement, with its unit.
fn value(amount: f64, unit: &str) -> String {
    format!("{amount:.2} {unit}")
}

/// The target of a metric, as it is written in the table.
fn target(comparison: &Comparison) -> String {
    match comparison.metric.absolute_target() {
        Some(limit) => format!("≤ {limit:.1} {}", comparison.metric.unit()),
        None => match comparison.target_ratio {
            Some(ratio) => match comparison.metric.direction() {
                Direction::Lower => format!("≤ {:.1}%", ratio * 100.0),
                Direction::Higher => format!("≥ {:.1}%", ratio * 100.0),
            },
            None => "—".to_string(),
        },
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<4} {:<22} {:>16} {:>16} {:>9}  {:<12}  {}",
            "ID", "metric", "ours", "reference", "ratio", "target", "verdict"
        )?;

        for comparison in &self.comparisons {
            let metric = comparison.metric;
            let unit = metric.unit();

            let verdict = match comparison.verdict {
                Verdict::Met => "ok",
                Verdict::Missed => "MISSED",
                Verdict::Unknown => "n/a",
            };

            let ours = if comparison.ours.samples == 0 {
                "—".to_string()
            } else {
                value(comparison.ours.p50, unit)
            };

            let theirs = if comparison.theirs.samples == 0 {
                "—".to_string()
            } else {
                value(comparison.theirs.p50, unit)
            };

            let ratio = if comparison.theirs.samples == 0 {
                "—".to_string()
            } else {
                format!("{:.1}%", comparison.ratio_percent)
            };

            writeln!(
                f,
                "{:<4} {:<22} {:>16} {:>16} {:>9}  {:<12}  {}",
                metric.code(),
                metric.as_str(),
                ours,
                theirs,
                ratio,
                target(comparison),
                verdict
            )?;
        }

        writeln!(f)?;
        let missed = self.failures().len();
        let unknown = self.unknown().len();

        if self.comparisons.is_empty() {
            writeln!(f, "nothing was measured")?;
        } else if missed > 0 {
            writeln!(f, "{} of {} targets missed", missed, self.comparisons.len())?;
        } else if unknown > 0 {
            writeln!(
                f,
                "every judged target met, {} of {} could not be judged",
                unknown,
                self.comparisons.len()
            )?;
        } else {
            writeln!(f, "every target met")?;
        }

        if self
            .comparisons
            .iter()
            .any(|comparison| comparison.metric.absolute_target().is_some())
        {
            writeln!(
                f,
                "a metric with a limit of its own is judged against that limit, not against the reference"
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Series;

    fn series(values: &[f64]) -> Series {
        Series::from_samples(values.to_vec())
    }

    fn comparison(metric: Metric, ours: f64, theirs: f64) -> Comparison {
        compare(metric, &series(&[ours]), &series(&[theirs])).unwrap()
    }

    fn side(metrics: &[(Metric, &[f64])]) -> Side {
        let mut side = Side::new();
        for (metric, values) in metrics {
            side.put(*metric, series(values));
        }
        side
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
        report.push(comparison(Metric::InGameFps, 120.0, 1000.0));
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

    #[test]
    fn the_table_has_a_row_for_every_metric_of_the_plan() {
        let ours = side(&[(Metric::ClickToWindow, &[800.0, 810.0, 790.0])]);
        let theirs = side(&[(Metric::ClickToWindow, &[1000.0, 1010.0, 990.0])]);

        let report = Report::between(&ours, &theirs);

        assert_eq!(report.metrics(), Metric::all().to_vec());
    }

    #[test]
    fn a_metric_that_was_measured_on_one_side_only_is_not_judged() {
        let ours = side(&[(Metric::ClickToWindow, &[800.0, 810.0, 790.0])]);
        let theirs = Side::new();

        let report = Report::between(&ours, &theirs);
        let comparison = report.get(Metric::ClickToWindow).unwrap();

        assert_eq!(comparison.verdict, Verdict::Unknown);
        assert!((comparison.ours.p50 - 800.0).abs() < 0.01);
        assert_eq!(comparison.theirs.samples, 0);
    }

    #[test]
    fn a_metric_that_was_not_measured_at_all_is_shown_as_not_measured() {
        let report = Report::between(&Side::new(), &Side::new());

        let comparison = report.get(Metric::InGameFps).unwrap();
        assert_eq!(comparison.verdict, Verdict::Unknown);
        assert_eq!(comparison.ours.samples, 0);
        assert_eq!(comparison.theirs.samples, 0);
    }

    #[test]
    fn the_memory_of_the_launcher_is_judged_against_its_own_limit() {
        // M7 has no reference: it is judged against the 60 mib limit.
        let ours = side(&[(Metric::LauncherRss, &[41.2, 42.0, 40.8])]);
        let report = Report::between(&ours, &Side::new());

        let comparison = report.get(Metric::LauncherRss).unwrap();
        assert_eq!(comparison.verdict, Verdict::Met);
        assert_eq!(comparison.theirs.samples, 0);
    }

    #[test]
    fn a_launcher_that_uses_too_much_memory_misses_the_limit() {
        let ours = side(&[(Metric::LauncherRss, &[80.0, 81.0, 79.0])]);
        let report = Report::between(&ours, &Side::new());

        assert_eq!(
            report.get(Metric::LauncherRss).unwrap().verdict,
            Verdict::Missed
        );
        assert_eq!(report.failures().len(), 1);
    }

    #[test]
    fn a_noisy_measurement_is_shown_but_not_judged() {
        let ours = side(&[(Metric::ClickToWindow, &[500.0, 1500.0, 700.0, 1300.0])]);
        let theirs = side(&[(Metric::ClickToWindow, &[1000.0, 1010.0, 990.0])]);

        let report = Report::between(&ours, &theirs);
        let comparison = report.get(Metric::ClickToWindow).unwrap();

        assert_eq!(comparison.verdict, Verdict::Unknown);
        assert_eq!(comparison.theirs.samples, 3);
    }

    #[test]
    fn the_table_shows_a_dash_where_there_is_nothing_to_show() {
        let ours = side(&[(Metric::ClickToWindow, &[800.0, 810.0, 790.0])]);
        let report = Report::between(&ours, &Side::new());

        let text = report.to_string();
        assert!(text.contains('—'), "the missing reference is not shown");
        assert!(text.contains("n/a"));
    }

    #[test]
    fn the_report_says_how_many_metrics_could_not_be_judged() {
        let ours = side(&[(Metric::ClickToWindow, &[800.0, 810.0, 790.0])]);
        let report = Report::between(&ours, &Side::new());

        let text = report.to_string();
        assert!(text.contains("could not be judged"), "{text}");
    }
}
