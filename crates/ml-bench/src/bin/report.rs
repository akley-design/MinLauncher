//! The report of a comparison between `MinLauncher` and the reference
//! launcher.
//!
//! The report reads the file of our measurements, the file of the
//! reference measurements and the database of the launcher, and prints
//! the table of the metrics M1 to M8 with the verdict on each of them.
//!
//! ```text
//! cargo run -p ml-bench --bin report -- \
//!     --ours benchmarks/ours.json \
//!     --reference benchmarks/reference.json
//! ```
//!
//! The exit code says what happened: zero when no target was missed,
//! one when a target was missed, two when the report could not be
//! built at all. A metric that could not be judged — it was not
//! measured, or it was too noisy — is neither a success nor a failure:
//! the table says so, and the exit code does not pretend otherwise.

use std::path::PathBuf;
use std::process::ExitCode;

use ml_bench::report::Report;
use ml_bench::samples::{DEFAULT_SCENARIO, SampleFile, side_of_storage};
use ml_core::{AppPaths, Error, Result};
use ml_storage::Storage;
use serde_json::{Value, json};

/// The exit code of a report whose target was missed.
const EXIT_MISSED: u8 = 1;
/// The exit code of a report that could not be built.
const EXIT_ERROR: u8 = 2;

/// The options of the report.
struct Options {
    /// The file of our measurements.
    ours: Option<PathBuf>,
    /// The file of the measurements of the reference launcher.
    reference: Option<PathBuf>,
    /// The scenario the measurements belong to.
    scenario: Option<String>,
    /// Whether to print the report as JSON.
    json: bool,
}

impl Options {
    /// The options of the command line.
    ///
    /// # Errors
    /// Fails when an argument is unknown or misses its value.
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self {
            ours: None,
            reference: None,
            scenario: None,
            json: false,
        };

        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ours" => options.ours = Some(PathBuf::from(value(&mut args, "--ours")?)),
                "--reference" => {
                    options.reference = Some(PathBuf::from(value(&mut args, "--reference")?));
                }
                "--scenario" => {
                    options.scenario = Some(value(&mut args, "--scenario")?);
                }
                "--json" => options.json = true,
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
        "usage: report [--ours <file>] [--reference <file>] [--scenario <name>] [--json]

  --ours <file>        the file of our measurements
  --reference <file>   the file of the measurements of the reference launcher
  --scenario <name>    the scenario the measurements belong to
  --json               print the report as JSON

exit codes: 0 - no target was missed, 1 - a target was missed,
            2 - the report could not be built"
    );
}

/// Where a side of the comparison came from.
fn describe(file: Option<&SampleFile>) -> String {
    match file {
        None => "not measured".to_string(),
        Some(file) => {
            let launcher = file.meta.launcher.as_deref().unwrap_or("unknown launcher");
            match &file.meta.recorded_at {
                Some(date) => format!("{launcher}, {date}"),
                None => launcher.to_string(),
            }
        }
    }
}

/// The report of the two sides, as a JSON document.
fn to_json(report: &Report, scenario: &str, ours: &str, theirs: &str) -> Value {
    let metrics: Vec<Value> = report
        .comparisons
        .iter()
        .map(|comparison| {
            let reference = if comparison.theirs.samples == 0 {
                Value::Null
            } else {
                serde_json::to_value(&comparison.theirs).unwrap_or(Value::Null)
            };

            json!({
                "id": comparison.metric.code(),
                "metric": comparison.metric.as_str(),
                "unit": comparison.metric.unit(),
                "ours": serde_json::to_value(&comparison.ours).unwrap_or(Value::Null),
                "reference": reference,
                "ratio_percent": comparison.ratio_percent,
                "verdict": comparison.verdict,
                "target_ratio": comparison.target_ratio,
                "absolute_target": comparison.metric.absolute_target(),
            })
        })
        .collect();

    json!({
        "scenario": scenario,
        "ours": ours,
        "reference": theirs,
        "metrics": metrics,
    })
}

/// Build the report.
fn run(options: &Options) -> Result<ExitCode> {
    let ours_file = match &options.ours {
        Some(path) => Some(SampleFile::read(path)?),
        None => None,
    };
    let reference_file = match &options.reference {
        Some(path) => Some(SampleFile::read(path)?),
        None => None,
    };

    let scenario = options
        .scenario
        .clone()
        .or_else(|| ours_file.as_ref().map(SampleFile::scenario_of))
        .unwrap_or_else(|| DEFAULT_SCENARIO.to_string());

    // The database of the launcher holds the measurements it took
    // itself: the length of an installation and the behaviour of the
    // game.
    let paths = AppPaths::discover()?;
    let storage = Storage::open(&paths)?;
    let mut ours = side_of_storage(&storage, &scenario);

    // The file holds the raw samples, and it wins over the summary the
    // database keeps. It is also imported, so that the tab
    // "Performance" of the launcher shows the same numbers.
    if let Some(file) = &ours_file {
        file.import(&storage);
        ours.merge(&file.side());
    }

    let theirs = reference_file
        .as_ref()
        .map(SampleFile::side)
        .unwrap_or_default();

    let report = Report::between(&ours, &theirs);

    if options.json {
        let document = to_json(
            &report,
            &scenario,
            &describe(ours_file.as_ref()),
            &describe(reference_file.as_ref()),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&document)
                .map_err(|error| Error::Other(Box::new(error)))?
        );
    } else {
        println!("scenario: {scenario}");
        println!("ours:      {}", describe(ours_file.as_ref()));
        println!("reference: {}", describe(reference_file.as_ref()));
        println!();
        println!("{report}");
    }

    let missed = report.failures();
    if missed.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        for comparison in missed {
            eprintln!(
                "missed: {} {} — ours {:.2} {}, reference {:.2} {}",
                comparison.metric.code(),
                comparison.metric.as_str(),
                comparison.ours.p50,
                comparison.metric.unit(),
                comparison.theirs.p50,
                comparison.metric.unit(),
            );
        }
        Ok(ExitCode::from(EXIT_MISSED))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("report: {error}");
            print_help();
            return ExitCode::from(EXIT_ERROR);
        }
    };

    match run(&options) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("report: {error}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

/// The metric of the report, for the tests.
#[cfg(test)]
mod tests {
    use super::*;
    use ml_bench::metric::Metric;
    use ml_bench::samples::Side;

    #[test]
    fn the_options_are_read() {
        let args = vec![
            "--ours".to_string(),
            "ours.json".to_string(),
            "--reference".to_string(),
            "reference.json".to_string(),
            "--scenario".to_string(),
            "1.21.4".to_string(),
            "--json".to_string(),
        ];

        let options = Options::parse(&args).unwrap();

        assert_eq!(options.ours, Some(PathBuf::from("ours.json")));
        assert_eq!(options.reference, Some(PathBuf::from("reference.json")));
        assert_eq!(options.scenario.as_deref(), Some("1.21.4"));
        assert!(options.json);
    }

    #[test]
    fn an_unknown_argument_is_an_error() {
        let args = vec!["--nonsense".to_string()];
        assert!(Options::parse(&args).is_err());
    }

    #[test]
    fn an_argument_without_a_value_is_an_error() {
        let args = vec!["--ours".to_string()];
        assert!(Options::parse(&args).is_err());
    }

    #[test]
    fn a_report_without_a_side_is_still_a_report() {
        let report = Report::between(&Side::new(), &Side::new());
        let document = to_json(&report, "default", "not measured", "not measured");

        assert_eq!(document["scenario"], "default");
        assert_eq!(
            document["metrics"].as_array().unwrap().len(),
            Metric::all().len()
        );
    }
}
