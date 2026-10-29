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
//! | [`agent`] | What did the game do while it ran? |
//!
//! The pipeline is deliberately free of I/O beyond the filesystem, so
//! that a command can be built and inspected without starting anything.
//! That is what makes the launch time measurable: the launcher can show
//! the exact command line before it runs it, and the benchmark harness
//! can measure the construction without the noise of a running game.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod agent;
pub mod args;
pub mod command;
pub mod gameoptions;
pub mod jvm;
pub mod natives;
pub mod supervisor;

pub use agent::{AgentReport, read_report};
pub use args::{Session, build_classpath, substitute, substitute_all};
pub use command::{LaunchCommand, LaunchRequest, build_command};
pub use jvm::{JavaInstallation, JavaOrigin, find_runtime, probe_version};
pub use natives::{ExtractionReport, NativeJar, NativeLayout, extract_all, extract_jar};
pub use supervisor::{ExitStatus, GameProcess, RestartPolicy, Supervisor};

use ml_core::{FeatureSet, Platform, Result};

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
    let phase_started = std::time::Instant::now();
    let command = build_command(request, platform, features, version_type)?;
    tracing::debug!(target: "minlaucher::performance", phase = "build_game_command", version = %request.version.id, elapsed_us = phase_started.elapsed().as_micros());

    let phase_started = std::time::Instant::now();
    crate::args::ensure_game_directory(&request.session)?;
    tracing::debug!(target: "minlaucher::performance", phase = "ensure_game_directory", version = %request.version.id, elapsed_us = phase_started.elapsed().as_micros());

    // The natives are unpacked into the directories the version asks for.
    let jars = request.native_jars(platform);
    let phase_started = std::time::Instant::now();
    let extraction = crate::natives::extract_all(&jars, &command.natives_layout)?;
    tracing::debug!(target: "minlaucher::performance", phase = "prepare_natives", version = %request.version.id, elapsed_us = phase_started.elapsed().as_micros(), cache_hit = extraction.skipped, native_files = extraction.files.len());

    Ok(PreparedLaunch {
        command,
        extraction,
    })
}
