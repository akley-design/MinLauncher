//! The numbers of a benchmark run.
//!
//! The benchmarks of the crate are written for criterion, and criterion
//! writes the estimates of a run into a directory of its own:
//! `target/criterion/<scenario>/new/estimates.json`. The file holds the
//! estimate of the median together with the confidence interval around
//! it, and the comparison is made of the medians: a benchmark has a
//! tail, and a median does not believe it.
//!
//! The numbers of a benchmark are comparable **within one machine**
//! only — the same code on another processor is another number. The
//! baseline a run is compared with therefore has to be measured on the
//! same machine, and that is why the CI measures the base of the branch
//! next to the head instead of trusting the committed numbers.
//!
//! # Example
//!
//! ```
//! use ml_bench::criterion::{Baseline, Change, Estimate, compare};
//!
//! let mut baseline = Baseline::default();
//! baseline.put("parse", Estimate::of(5_000.0, 4_900.0, 5_100.0));
//!
//! let mut current = Baseline::default();
//! current.put("parse", Estimate::of(6_000.0, 5_900.0, 6_100.0));
//!
//! let comparisons = compare(&baseline, &current, 10.0);
//! assert_eq!(comparisons[0].change, Change::Regression);
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::record::Meta;

/// The directory criterion writes the estimates of the last run into.
const LAST_RUN: &str = "new";

/// The file the estimates of a run are written into.
const ESTIMATES: &str = "estimates.json";

/// The directory criterion writes its numbers into.
///
/// Criterion puts them into the target directory of the build, and it
/// follows one rule — the same one this function follows, so that the
/// default is right on every machine:
///
/// 1. `$CRITERION_HOME`, used as it is;
/// 2. `$CARGO_TARGET_DIR/criterion`;
/// 3. the target directory cargo reports, plus `criterion`;
/// 4. `target/criterion`.
#[must_use]
pub fn default_directory() -> PathBuf {
    let home = std::env::var_os("CRITERION_HOME");
    let target = std::env::var_os("CARGO_TARGET_DIR");
    // The cargo metadata call spawns a process, so it is made only when
    // the answer is not already known.
    let metadata = if home.is_some() || target.is_some() {
        None
    } else {
        cargo_target_directory()
    };
    directory_of(home, target, metadata)
}

/// The directory criterion writes into, by the rule it follows.
///
/// The three arguments are the three things the rule looks at, and they
/// are taken apart from the environment so that the rule itself can be
/// tested without touching the environment of the process.
fn directory_of(
    home: Option<std::ffi::OsString>,
    target: Option<std::ffi::OsString>,
    metadata: Option<PathBuf>,
) -> PathBuf {
    if let Some(home) = home {
        return PathBuf::from(home);
    }
    if let Some(target) = target {
        return PathBuf::from(target).join("criterion");
    }
    metadata.map_or_else(
        || PathBuf::from("target").join("criterion"),
        |dir| dir.join("criterion"),
    )
}

/// The target directory of the build, as cargo reports it.
fn cargo_target_directory() -> Option<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .ok()?;
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let directory = metadata.get("target_directory")?.as_str()?;
    Some(PathBuf::from(directory))
}

/// The estimates of one benchmark run.
///
/// Every scenario of the run has its own directory, and the estimates
/// of the last run are in the `new` directory of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// The metadata of the machine the numbers were taken on.
    ///
    /// The numbers of a benchmark mean nothing without the machine they
    /// were taken on, so the metadata travels with them.
    #[serde(default)]
    pub meta: Meta,
    /// The estimate of every scenario, by the name of the scenario.
    #[serde(default)]
    pub scenarios: BTreeMap<String, Estimate>,
}

impl Baseline {
    /// An empty baseline.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The estimate of a scenario.
    #[must_use]
    pub fn estimate_of(&self, scenario: &str) -> Option<&Estimate> {
        self.scenarios.get(scenario)
    }

    /// Put the estimate of a scenario.
    pub fn put(&mut self, scenario: impl Into<String>, estimate: Estimate) {
        self.scenarios.insert(scenario.into(), estimate);
    }

    /// Whether the baseline holds no scenario at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scenarios.is_empty()
    }

    /// Read a baseline from a file.
    ///
    /// # Errors
    /// Fails when the file cannot be read or does not hold valid JSON.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|error| Error::io(path, error))?;
        let text = String::from_utf8_lossy(&bytes);
        serde_json::from_str(&text).map_err(|error| Error::json(path.display().to_string(), &error))
    }

    /// Write the baseline into a file.
    ///
    /// # Errors
    /// Fails when the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| Error::json(path.display().to_string(), &error))?;
        std::fs::write(path, text).map_err(|error| Error::io(path, error))
    }

    /// Read the estimates of a run out of a directory of criterion.
    ///
    /// The directory is the one criterion writes into — the
    /// `target/criterion` of the build. The estimates of the last run
    /// are the ones in the `new` directory of every scenario.
    ///
    /// # Errors
    /// Fails when the directory cannot be read or does not hold valid
    /// JSON.
    pub fn read_run(dir: &Path) -> Result<Self> {
        let mut baseline = Self::new();
        baseline.meta = Meta::of_this_machine();
        // The processor is the machine a benchmark number belongs to, so
        // it travels with the numbers.
        baseline.meta.cpu = ml_core::sys::cpu_name().map(str::to_string);
        collect(dir, &mut baseline)?;
        Ok(baseline)
    }
}

/// One estimate of a benchmark run.
///
/// The values are in nanoseconds, the unit criterion measures in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    /// The value of the estimate.
    pub point_estimate: f64,
    /// The confidence interval around the estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_interval: Option<Interval>,
}

impl Estimate {
    /// An estimate of a median, with the interval around it.
    #[must_use]
    pub fn of(median: f64, lower: f64, upper: f64) -> Self {
        Self {
            point_estimate: median,
            confidence_interval: Some(Interval {
                lower_bound: lower,
                upper_bound: upper,
            }),
        }
    }

    /// The value of the estimate.
    #[must_use]
    pub fn median(&self) -> f64 {
        self.point_estimate
    }

    /// Whether the estimate is certainly bigger than the other one.
    ///
    /// It is when the confidence intervals of the two do not overlap:
    /// then the difference is more than the noise of the machine. When
    /// there are no intervals to compare, the answer is `true` — the
    /// threshold alone decides.
    #[must_use]
    pub fn is_certainly_bigger_than(&self, other: &Self) -> bool {
        match (&self.confidence_interval, &other.confidence_interval) {
            (Some(ours), Some(theirs)) => ours.lower_bound > theirs.upper_bound,
            _ => true,
        }
    }
}

/// The confidence interval around an estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    /// The lower bound of the interval.
    pub lower_bound: f64,
    /// The upper bound of the interval.
    pub upper_bound: f64,
}

/// The estimates of a run, as criterion writes them.
#[derive(Debug, Clone, Deserialize)]
struct Written {
    /// The estimate of the median.
    median: Estimate,
}

/// What happened to a scenario since the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The scenario got faster.
    Faster,
    /// The scenario stayed the same.
    TheSame,
    /// The scenario got slower, but not past the threshold.
    Slower,
    /// The scenario got slower past the threshold, and the noise of the
    /// machine cannot explain it: a regression.
    Regression,
    /// The scenario is not in the baseline: it is new.
    New,
    /// The scenario of the baseline is not in the run: it is missing.
    Missing,
}

/// The comparison of one scenario with its baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The name of the scenario.
    pub scenario: String,
    /// The median of the baseline, in nanoseconds.
    pub baseline: Option<f64>,
    /// The median of the run, in nanoseconds.
    pub current: Option<f64>,
    /// How much the median changed, in percent. A positive number is a
    /// slowdown.
    pub percent: Option<f64>,
    /// What the change means.
    pub change: Change,
}

/// Compare a run with a baseline.
///
/// A scenario is a **regression** when its median got more than
/// `threshold` percent slower *and* the confidence intervals of the two
/// medians do not overlap: a slowdown that the noise of the machine can
/// explain is not a regression, and a gate that falls on noise is a
/// gate nobody reads.
///
/// Scenarios that are in one side only are reported as they are — a
/// scenario that is new is not a regression, and neither is one that
/// went missing.
#[must_use]
pub fn compare(baseline: &Baseline, current: &Baseline, threshold: f64) -> Vec<Comparison> {
    let mut comparisons = Vec::new();

    for (scenario, estimate) in &current.scenarios {
        let Some(base) = baseline.scenarios.get(scenario) else {
            comparisons.push(Comparison {
                scenario: scenario.clone(),
                baseline: None,
                current: Some(estimate.median()),
                percent: None,
                change: Change::New,
            });
            continue;
        };

        let percent = percent_of(base.median(), estimate.median());
        let change = if percent > threshold && estimate.is_certainly_bigger_than(base) {
            Change::Regression
        } else if percent > 0.0 {
            Change::Slower
        } else if percent < 0.0 {
            Change::Faster
        } else {
            Change::TheSame
        };

        comparisons.push(Comparison {
            scenario: scenario.clone(),
            baseline: Some(base.median()),
            current: Some(estimate.median()),
            percent: Some(percent),
            change,
        });
    }

    for (scenario, estimate) in &baseline.scenarios {
        if !current.scenarios.contains_key(scenario) {
            comparisons.push(Comparison {
                scenario: scenario.clone(),
                baseline: Some(estimate.median()),
                current: None,
                percent: None,
                change: Change::Missing,
            });
        }
    }

    comparisons
}

/// The scenarios of a comparison that are a regression.
#[must_use]
pub fn regressions(comparisons: &[Comparison]) -> Vec<&Comparison> {
    comparisons
        .iter()
        .filter(|comparison| comparison.change == Change::Regression)
        .collect()
}

/// The report of a comparison, as a markdown table.
#[must_use]
pub fn report(comparisons: &[Comparison], threshold: f64) -> String {
    let mut text = String::new();

    text.push_str(
        "The benchmarks of the launch path, against the baseline of the same machine.\n\n",
    );
    write!(
        text,
        "A scenario is a regression when its median got more than {threshold:.0} % slower \
         and the confidence intervals of the two medians do not overlap.\n\n"
    )
    .expect("a string does not fail to grow");
    text.push_str(
        "The metrics of the game itself (M5, M6) are not among them: they need a graphics \
         card and a live game. This is the CPU path of a launch, and only that.\n\n",
    );

    text.push_str("| Scenario | Baseline | Now | Change | Verdict |\n");
    text.push_str("|---|---|---|---|---|\n");
    for comparison in comparisons {
        writeln!(
            text,
            "| `{}` | {} | {} | {} | {} |",
            comparison.scenario,
            duration(comparison.baseline),
            duration(comparison.current),
            percent(comparison.percent),
            verdict(comparison.change),
        )
        .expect("a string does not fail to grow");
    }

    text.push('\n');
    let missed = regressions(comparisons);
    if missed.is_empty() {
        text.push_str("No regressions.\n");
    } else {
        writeln!(
            text,
            "{} of {} scenarios are a regression.",
            missed.len(),
            comparisons.len()
        )
        .expect("a string does not fail to grow");
        for comparison in missed {
            writeln!(
                text,
                "- `{}`: {} → {} ({} slower)",
                comparison.scenario,
                duration(comparison.baseline),
                duration(comparison.current),
                percent(comparison.percent),
            )
            .expect("a string does not fail to grow");
        }
    }

    text
}

/// The report of a run that has no baseline to be compared with.
///
/// The numbers are shown, but nothing is judged: a run without a
/// baseline is not a run that passed.
#[must_use]
pub fn report_without_baseline(current: &Baseline) -> String {
    let mut text = String::new();

    text.push_str("The benchmarks of the launch path.\n\n");
    text.push_str(
        "There is no baseline to compare with, so nothing is judged: the numbers are shown \
         and the run is not a verdict.\n\n",
    );

    text.push_str("| Scenario | Now |\n");
    text.push_str("|---|---|\n");
    for (scenario, estimate) in &current.scenarios {
        writeln!(
            text,
            "| `{}` | {} |",
            scenario,
            duration(Some(estimate.median()))
        )
        .expect("a string does not fail to grow");
    }

    text
}

/// The verdict of a comparison, as it is written in the table.
#[must_use]
pub fn verdict(change: Change) -> &'static str {
    match change {
        Change::Faster => "faster",
        Change::TheSame => "ok",
        Change::Slower => "slower",
        Change::Regression => "**regression**",
        Change::New => "new",
        Change::Missing => "missing",
    }
}

/// How much a median changed, in percent.
fn percent_of(base: f64, current: f64) -> f64 {
    if base <= 0.0 {
        return 0.0;
    }
    (current - base) / base * 100.0
}

/// A change of a median, as it is written in the table.
fn percent(value: Option<f64>) -> String {
    match value {
        None => "—".to_string(),
        Some(value) => format!("{value:+.1} %"),
    }
}

/// A duration in nanoseconds, the way a person reads it.
fn duration(value: Option<f64>) -> String {
    let Some(value) = value else {
        return "—".to_string();
    };
    if value < 1_000.0 {
        format!("{value:.0} ns")
    } else if value < 1_000_000.0 {
        format!("{:.2} µs", value / 1_000.0)
    } else {
        format!("{:.2} ms", value / 1_000_000.0)
    }
}

/// Collect the estimates of every scenario of a directory.
fn collect(dir: &Path, baseline: &mut Baseline) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|error| Error::io(dir, error))?;

    for entry in entries {
        let entry = entry.map_err(|error| Error::io(dir, error))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if name == LAST_RUN {
            if let Some(scenario) = scenario_of(&path) {
                let file = path.join(ESTIMATES);
                if file.is_file() {
                    let bytes = std::fs::read(&file).map_err(|error| Error::io(&file, error))?;
                    let text = String::from_utf8_lossy(&bytes);
                    let written: Written = serde_json::from_str(&text)
                        .map_err(|error| Error::json(file.display().to_string(), &error))?;
                    baseline.put(scenario, written.median);
                }
            }
            continue;
        }

        collect(&path, baseline)?;
    }

    Ok(())
}

/// The name of the scenario a directory of estimates belongs to.
///
/// Criterion writes the estimates of a scenario into the `new`
/// directory of it, so the name of the scenario is the name of the
/// directory above.
fn scenario_of(dir: &Path) -> Option<String> {
    let parent = dir.parent()?;
    let name = parent.file_name()?.to_string_lossy().to_string();
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline(scenarios: &[(&str, f64)]) -> Baseline {
        let mut baseline = Baseline::new();
        for (scenario, median) in scenarios {
            baseline.put(
                *scenario,
                Estimate::of(*median, median * 0.99, median * 1.01),
            );
        }
        baseline
    }

    #[test]
    fn a_scenario_that_got_slower_past_the_threshold_is_a_regression() {
        let base = baseline(&[("parse", 5_000.0)]);
        let current = baseline(&[("parse", 6_000.0)]);

        let comparisons = compare(&base, &current, 10.0);

        assert_eq!(comparisons[0].change, Change::Regression);
        assert_eq!(comparisons[0].percent, Some(20.0));
        assert_eq!(regressions(&comparisons).len(), 1);
    }

    #[test]
    fn a_slowdown_that_the_noise_explains_is_not_a_regression() {
        let base = baseline(&[("parse", 5_000.0)]);
        // The intervals of the two medians overlap: the machine explains
        // the difference.
        let mut current = Baseline::new();
        current.put("parse", Estimate::of(6_000.0, 4_000.0, 8_000.0));

        let comparisons = compare(&base, &current, 10.0);

        assert_eq!(comparisons[0].change, Change::Slower);
        let missed = regressions(&comparisons);
        assert_eq!(missed.len(), 0);
    }

    #[test]
    fn a_slowdown_inside_the_threshold_is_not_a_regression() {
        let base = baseline(&[("parse", 5_000.0)]);
        let current = baseline(&[("parse", 5_200.0)]);

        let comparisons = compare(&base, &current, 10.0);

        assert_eq!(comparisons[0].change, Change::Slower);
        let missed = regressions(&comparisons);
        assert_eq!(missed.len(), 0);
    }

    #[test]
    fn a_faster_scenario_is_faster() {
        let base = baseline(&[("parse", 5_000.0)]);
        let current = baseline(&[("parse", 4_000.0)]);

        let comparisons = compare(&base, &current, 10.0);

        assert_eq!(comparisons[0].change, Change::Faster);
        assert_eq!(comparisons[0].percent, Some(-20.0));
    }

    #[test]
    fn a_scenario_that_is_new_is_not_a_regression() {
        let base = baseline(&[("parse", 5_000.0)]);
        let current = baseline(&[("parse", 5_000.0), ("libraries", 9_000.0)]);

        let comparisons = compare(&base, &current, 10.0);

        let new = comparisons
            .iter()
            .find(|comparison| comparison.scenario == "libraries")
            .unwrap();
        assert_eq!(new.change, Change::New);
        let missed = regressions(&comparisons);
        assert_eq!(missed.len(), 0);
    }

    #[test]
    fn a_scenario_that_went_missing_is_not_a_regression() {
        let base = baseline(&[("parse", 5_000.0), ("libraries", 9_000.0)]);
        let current = baseline(&[("parse", 5_000.0)]);

        let comparisons = compare(&base, &current, 10.0);

        let missing = comparisons
            .iter()
            .find(|comparison| comparison.scenario == "libraries")
            .unwrap();
        assert_eq!(missing.change, Change::Missing);
        let missed = regressions(&comparisons);
        assert_eq!(missed.len(), 0);
    }

    #[test]
    fn the_estimates_of_a_run_are_read_out_of_the_directory_of_criterion() {
        let dir = tempfile::tempdir().unwrap();
        let scenario = dir.path().join("parse_version_json").join(LAST_RUN);
        std::fs::create_dir_all(&scenario).unwrap();
        std::fs::write(
            scenario.join(ESTIMATES),
            r#"{
                "mean": { "point_estimate": 5100.0 },
                "median": {
                    "point_estimate": 5000.0,
                    "confidence_interval": { "lower_bound": 4900.0, "upper_bound": 5100.0 }
                },
                "std_dev": { "point_estimate": 100.0 }
            }"#,
        )
        .unwrap();

        let run = Baseline::read_run(dir.path()).unwrap();

        let estimate = run
            .estimate_of("parse_version_json")
            .expect("nothing was read");
        assert_eq!(estimate.median(), 5000.0);
        assert!(estimate.is_certainly_bigger_than(&Estimate::of(4_000.0, 3_900.0, 4_100.0)));
    }

    #[test]
    fn the_estimates_of_the_previous_run_are_not_the_ones_of_this_one() {
        let dir = tempfile::tempdir().unwrap();
        let scenario = dir.path().join("parse_version_json");
        std::fs::create_dir_all(scenario.join(LAST_RUN)).unwrap();
        std::fs::create_dir_all(scenario.join("base")).unwrap();
        std::fs::write(
            scenario.join(LAST_RUN).join(ESTIMATES),
            r#"{ "median": { "point_estimate": 5000.0 } }"#,
        )
        .unwrap();
        std::fs::write(
            scenario.join("base").join(ESTIMATES),
            r#"{ "median": { "point_estimate": 4000.0 } }"#,
        )
        .unwrap();

        let run = Baseline::read_run(dir.path()).unwrap();

        assert_eq!(
            run.estimate_of("parse_version_json").unwrap().median(),
            5000.0
        );
    }

    #[test]
    fn the_criterion_home_is_the_directory_of_the_numbers() {
        // Criterion reads this variable before anything else, and uses it
        // as it is — without the `criterion` part.
        let directory = directory_of(
            Some(std::ffi::OsString::from("C:/somewhere/else")),
            Some(std::ffi::OsString::from("C:/build")),
            Some(PathBuf::from("C:/build")),
        );

        assert_eq!(directory, PathBuf::from("C:/somewhere/else"));
    }

    #[test]
    fn the_target_directory_of_the_build_is_the_second_choice() {
        let directory = directory_of(
            None,
            Some(std::ffi::OsString::from("C:/build")),
            Some(PathBuf::from("C:/build")),
        );

        assert_eq!(directory, PathBuf::from("C:/build/criterion"));
    }

    #[test]
    fn the_target_directory_of_cargo_is_the_last_choice() {
        let directory = directory_of(None, None, Some(PathBuf::from("C:/build")));
        assert_eq!(directory, PathBuf::from("C:/build/criterion"));

        let directory = directory_of(None, None, None);
        assert_eq!(directory, PathBuf::from("target/criterion"));
    }

    #[test]
    fn a_baseline_round_trips_through_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("baseline.json");

        let mut base = baseline(&[("parse", 5_000.0)]);
        base.meta.launcher = Some("MinLauncher 0.1.0".into());
        base.write(&path).unwrap();

        let read = Baseline::read(&path).unwrap();

        assert_eq!(read.estimate_of("parse").unwrap().median(), 5_000.0);
        assert_eq!(read.meta.launcher.as_deref(), Some("MinLauncher 0.1.0"));
    }

    #[test]
    fn the_report_names_the_regressions() {
        let base = baseline(&[("parse", 5_000.0), ("libraries", 9_000.0)]);
        let current = baseline(&[("parse", 6_000.0), ("libraries", 9_000.0)]);

        let comparisons = compare(&base, &current, 10.0);
        let text = report(&comparisons, 10.0);

        assert!(text.contains("| `parse` |"));
        assert!(text.contains("**regression**"));
        assert!(text.contains("1 of 2 scenarios are a regression."));
    }

    #[test]
    fn a_clean_run_says_that_there_are_no_regressions() {
        let base = baseline(&[("parse", 5_000.0)]);
        let current = baseline(&[("parse", 5_000.0)]);

        let comparisons = compare(&base, &current, 10.0);
        let text = report(&comparisons, 10.0);

        assert!(text.contains("No regressions."));
    }
}
