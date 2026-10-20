//! The report of a benchmark run against its baseline.
//!
//! The benchmarks of the crate are the CPU path of a launch: parsing
//! the document of a version, resolving its libraries, building the
//! command of the game. A change that makes them slower is a change
//! everybody feels, so the run is compared with a baseline and the
//! comparison is a gate: the exit code says whether the build may pass.
//!
//! ```text
//! cargo run -p ml-bench --bin bench -- \
//!     --criterion target/criterion \
//!     --baseline benchmarks/baseline.json
//! ```
//!
//! The numbers of a benchmark are comparable within one machine only,
//! so the baseline has to be measured on the same machine. That is why
//! the CI measures the base of the branch next to the head instead of
//! trusting the committed numbers, and why `--save` writes a baseline
//! that belongs to the machine that made it.

use std::path::PathBuf;
use std::process::ExitCode;

use ml_bench::criterion::{Baseline, compare, report};
use ml_core::{Error, Result};

/// The exit code of a run with a regression in it.
const EXIT_REGRESSION: u8 = 1;
/// The exit code of a report that could not be built.
const EXIT_ERROR: u8 = 2;

/// The threshold a median may get slower by, in percent.
const DEFAULT_THRESHOLD: f64 = 10.0;

/// The options of the report.
struct Options {
    /// The directory criterion wrote the run into.
    ///
    /// `None` means the directory of the build, the one criterion itself
    /// writes into.
    criterion: Option<PathBuf>,
    /// The file of the baseline.
    baseline: PathBuf,
    /// The file the baseline is written into, when it is to be written.
    save: Option<PathBuf>,
    /// The file the report is written into.
    summary: Option<PathBuf>,
    /// The threshold a median may get slower by, in percent.
    threshold: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            criterion: None,
            baseline: PathBuf::from("benchmarks/baseline.json"),
            save: None,
            summary: None,
            threshold: DEFAULT_THRESHOLD,
        }
    }
}

impl Options {
    /// The options of the command line.
    ///
    /// # Errors
    /// Fails when an argument is unknown or misses its value.
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();

        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--criterion" => {
                    options.criterion = Some(PathBuf::from(value(&mut args, "--criterion")?));
                }
                "--baseline" => {
                    options.baseline = PathBuf::from(value(&mut args, "--baseline")?);
                }
                "--save" => options.save = Some(PathBuf::from(value(&mut args, "--save")?)),
                "--summary" => {
                    options.summary = Some(PathBuf::from(value(&mut args, "--summary")?));
                }
                "--threshold" => {
                    let raw = value(&mut args, "--threshold")?;
                    options.threshold = raw.parse().map_err(|_| {
                        Error::InvalidInput(format!("`{raw}` is not a number of percent"))
                    })?;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    return Err(Error::InvalidInput(format!("unknown argument `{other}`")));
                }
            }
        }

        Ok(options)
    }
}

/// The value that follows an argument.
fn value<'a>(args: &mut impl Iterator<Item = &'a String>, name: &str) -> Result<String> {
    args.next()
        .cloned()
        .ok_or_else(|| Error::InvalidInput(format!("`{name}` needs a value")))
}

/// The help of the command.
fn print_help() {
    println!(
        "usage: bench [--criterion <dir>] [--baseline <file>] [--save <file>]
       [--summary <file>] [--threshold <percent>]

  --criterion <dir>    the directory criterion wrote the run into
  --baseline <file>    the file of the baseline the run is compared with
  --save <file>        write the run as the baseline of this machine and stop
  --summary <file>     write the report into the file as well
  --threshold <pct>    the percent a median may get slower by (default 10)

exit codes: 0 - no regression, 1 - a scenario is a regression,
            2 - the report could not be built"
    );
}

/// Build the report.
fn run(options: &Options) -> Result<ExitCode> {
    let criterion = options
        .criterion
        .clone()
        .unwrap_or_else(ml_bench::criterion::default_directory);
    let current = Baseline::read_run(&criterion)?;

    // The baseline of a machine is written by the machine itself.
    if let Some(path) = &options.save {
        current.write(path)?;
        println!(
            "the baseline of {} scenarios is written to {}",
            current.scenarios.len(),
            path.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    // Without a baseline there is nothing to judge: the numbers are
    // shown, and the run is not a verdict.
    if !options.baseline.exists() {
        let text = ml_bench::criterion::report_without_baseline(&current);
        print!("{text}");
        write_summary(options, &text)?;
        return Ok(ExitCode::SUCCESS);
    }

    let baseline = Baseline::read(&options.baseline)?;
    let comparisons = compare(&baseline, &current, options.threshold);
    let text = report(&comparisons, options.threshold);

    print!("{text}");
    write_summary(options, &text)?;

    let missed = ml_bench::criterion::regressions(&comparisons);
    if missed.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        for comparison in missed {
            eprintln!(
                "regression: {} — {:.2} ns → {:.2} ns ({:+.1} %)",
                comparison.scenario,
                comparison.baseline.unwrap_or_default(),
                comparison.current.unwrap_or_default(),
                comparison.percent.unwrap_or_default(),
            );
        }
        Ok(ExitCode::from(EXIT_REGRESSION))
    }
}

/// Write the report into the file of the summary, when there is one.
fn write_summary(options: &Options, text: &str) -> Result<()> {
    if let Some(path) = &options.summary {
        std::fs::write(path, text).map_err(|error| ml_core::Error::io(path, error))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("bench: {error}");
            print_help();
            return ExitCode::from(EXIT_ERROR);
        }
    };

    match run(&options) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("bench: {error}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options_are_read() {
        let args = vec![
            "--criterion".to_string(),
            "target/criterion".to_string(),
            "--baseline".to_string(),
            "base.json".to_string(),
            "--save".to_string(),
            "new.json".to_string(),
            "--summary".to_string(),
            "summary.md".to_string(),
            "--threshold".to_string(),
            "5".to_string(),
        ];

        let options = Options::parse(&args).unwrap();

        assert_eq!(options.criterion, Some(PathBuf::from("target/criterion")));
        assert_eq!(options.baseline, PathBuf::from("base.json"));
        assert_eq!(options.save, Some(PathBuf::from("new.json")));
        assert_eq!(options.summary, Some(PathBuf::from("summary.md")));
        assert_eq!(options.threshold, 5.0);
    }

    #[test]
    fn the_threshold_is_ten_percent_by_default() {
        let options = Options::parse(&[]).unwrap();
        assert_eq!(options.threshold, 10.0);
        assert_eq!(options.baseline, PathBuf::from("benchmarks/baseline.json"));
        assert_eq!(options.criterion, None);
    }

    #[test]
    fn the_criterion_directory_is_the_one_of_the_build() {
        let directory = ml_bench::criterion::default_directory();
        assert!(directory.ends_with("criterion"), "{directory:?}");
    }

    #[test]
    fn an_unknown_argument_is_an_error() {
        assert!(Options::parse(&["--nonsense".to_string()]).is_err());
    }

    #[test]
    fn an_argument_without_a_value_is_an_error() {
        assert!(Options::parse(&["--baseline".to_string()]).is_err());
    }

    #[test]
    fn a_threshold_that_is_not_a_number_is_an_error() {
        assert!(Options::parse(&["--threshold".to_string(), "fast".to_string()]).is_err());
    }

    /// A run of criterion, with one scenario.
    fn run_of(dir: &std::path::Path, scenario: &str, median: f64) -> std::path::PathBuf {
        let directory = dir.join("criterion").join(scenario).join("new");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("estimates.json"),
            format!(
                r#"{{ "median": {{ "point_estimate": {median:.1},
                     "confidence_interval": {{ "lower_bound": {lower:.1}, "upper_bound": {upper:.1} }} }} }}"#,
                lower = median * 0.99,
                upper = median * 1.01
            ),
        )
        .unwrap();
        dir.join("criterion")
    }

    #[test]
    fn a_scenario_that_got_slower_breaks_the_gate() {
        let dir = tempfile::tempdir().unwrap();

        let mut baseline = ml_bench::criterion::Baseline::new();
        baseline.put(
            "parse",
            ml_bench::criterion::Estimate::of(5_000.0, 4_900.0, 5_100.0),
        );
        let baseline_path = dir.path().join("baseline.json");
        baseline.write(&baseline_path).unwrap();

        // The scenario got twice as slow.
        let criterion = run_of(dir.path(), "parse", 10_000.0);

        let options = Options {
            criterion: Some(criterion),
            baseline: baseline_path,
            save: None,
            summary: Some(dir.path().join("summary.md")),
            threshold: 10.0,
        };

        let code = run(&options).unwrap();

        assert_eq!(code, ExitCode::from(EXIT_REGRESSION));
        let summary = std::fs::read_to_string(dir.path().join("summary.md")).unwrap();
        assert!(summary.contains("regression"), "{summary}");
        assert!(summary.contains("`parse`"), "{summary}");
    }

    #[test]
    fn a_run_without_a_regression_passes_the_gate() {
        let dir = tempfile::tempdir().unwrap();

        let mut baseline = ml_bench::criterion::Baseline::new();
        baseline.put(
            "parse",
            ml_bench::criterion::Estimate::of(5_000.0, 4_900.0, 5_100.0),
        );
        let baseline_path = dir.path().join("baseline.json");
        baseline.write(&baseline_path).unwrap();

        let criterion = run_of(dir.path(), "parse", 5_000.0);

        let options = Options {
            criterion: Some(criterion),
            baseline: baseline_path,
            save: None,
            summary: Some(dir.path().join("summary.md")),
            threshold: 10.0,
        };

        let code = run(&options).unwrap();

        assert_eq!(code, ExitCode::SUCCESS);
        let summary = std::fs::read_to_string(dir.path().join("summary.md")).unwrap();
        assert!(summary.contains("No regressions."), "{summary}");
    }
}
