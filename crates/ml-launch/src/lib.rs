//! The launch pipeline: from a version to a running game.
//!
//! The crate is split into four pieces that mirror the four questions a
//! launch asks:
//!
//! | Module | Question |
//! |--------|----------|
//! | [`jvm`] | Which Java runtime runs the game? |
//! | [`args`] | What does the command line look like? |
//! | [`natives`] | Where do the native libraries come from? |
//! | [`command`] | How are the pieces put together? |
//! | [`supervisor`] | How is the process run and watched? |
//!
//! The pipeline is deliberately free of I/O beyond the filesystem, so
//! that a command can be built and inspected without starting anything.
//! That is what makes the launch time measurable: the launcher can show
//! the exact command line before it runs it, and the benchmark harness
//! can measure the construction without the noise of a running game.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod args;
pub mod command;
pub mod jvm;
pub mod natives;
pub mod supervisor;

pub use args::{Session, build_classpath, substitute, substitute_all};
pub use command::{LaunchCommand, LaunchRequest, build_command};
pub use jvm::{JavaInstallation, JavaOrigin, find_runtime, probe_version};
pub use natives::{ExtractionReport, NativeJar, NativeLayout, extract_all, extract_jar};
pub use supervisor::{ExitStatus, GameProcess, Supervisor};

use ml_core::{Error, FeatureSet, Platform, Result};
use ml_minecraft::version::VersionJson;

/// A complete, ready to run launch.
///
/// Building it performs every step that has to happen before the game
/// can start: the natives are unpacked, the command line is assembled
/// and the working directory is created.
#[derive(Debug, Clone)]
pub struct PreparedLaunch {
    /// The command that will be run.
    pub command: LaunchCommand,
    /// What the unpacking of the natives did.
    pub extraction: ExtractionReport,
}

impl PreparedLaunch {
    /// The command line, for logging and for the benchmark harness.
    #[must_use]
    pub fn display(&self) -> String {
        self.command.display()
    }
}

/// Prepare a launch.
///
/// # Errors
/// Fails when the version carries no entry point, when the natives
/// cannot be unpacked, or when the command cannot be assembled.
pub fn prepare(
    request: &LaunchRequest,
    platform: Platform,
    features: &FeatureSet,
    version_type: &str,
) -> Result<PreparedLaunch> {
    let command = build_command(request, platform, features, version_type)?;

    crate::args::ensure_game_directory(&request.session)?;

    // The natives are unpacked into the directories the version asks for.
    let jars = request.native_jars(platform);
    let extraction = crate::natives::extract_all(&jars, &command.natives_layout)?;

    Ok(PreparedLaunch {
        command,
        extraction,
    })
}

/// The classpath a request would use, without building the whole command.
#[must_use]
pub fn classpath_of(request: &LaunchRequest) -> String {
    crate::command::request_classpath(request)
}

/// The version a request launches, for diagnostics.
#[must_use]
pub fn version_id(version: &VersionJson) -> &str {
    &version.id
}

/// The error the launcher reports when a version cannot be started.
#[must_use]
pub fn describe_failure(error: &Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_version_id_is_reported() {
        let version =
            VersionJson::parse(br#"{"id":"26.3","type":"release","mainClass":"Main"}"#).unwrap();
        assert_eq!(version_id(&version), "26.3");
    }

    #[test]
    fn a_failure_is_described() {
        let error = Error::NoJavaRuntime(25);
        assert!(describe_failure(&error).contains("25"));
    }

    #[test]
    fn the_classpath_is_available_without_a_command() {
        let request = LaunchRequest {
            version: VersionJson::parse(br#"{"id":"26.3","type":"release","mainClass":"Main"}"#)
                .unwrap(),
            libraries: Vec::new(),
            java: JavaInstallation {
                executable: PathBuf::from("java"),
                windowless_executable: None,
                major_version: 25,
                version: "25.0.1".into(),
                origin: JavaOrigin::System,
            },
            session: Session::default(),
            client_jar: PathBuf::from("C:/v/client.jar"),
            library_directory: PathBuf::from("C:/l"),
            extra_jvm_args: Vec::new(),
            agent_jar: None,
        };

        assert!(classpath_of(&request).starts_with("C:/v/client.jar"));
    }
}
