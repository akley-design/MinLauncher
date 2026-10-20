//! Finding and preparing the profiling agent.
//!
//! The agent is a Java agent that is attached to the game with
//! `-javaagent`. It measures the game from the inside: how long it took
//! to start, how the frames are drawn, how much memory it uses.
//!
//! The jar of the agent is built by Gradle and is not part of the Rust
//! build — and not part of the installer either, because the installer
//! ships the launcher alone. It has to be found before the game can
//! start, and there are two places it can be:
//!
//! * next to the executable of the launcher, where a portable build
//!   can drop it;
//! * in the data directory of the launcher, where it is copied to the
//!   first time it is found.
//!
//! When it is in neither place the game still starts — it is only not
//! measured.
//!
//! The second one matters for a reason that is not obvious. The JVM
//! splits `-javaagent:jar=options` at the *first* `=`, so a jar whose
//! path has an `=` in it is cut in half and the agent does not load at
//! all. The name of the user is part of the path of the data
//! directory, and a name can have an `=` in it. Copying the jar to a
//! path that is known to be safe removes the problem entirely.

use std::path::{Path, PathBuf};

use ml_core::{AppPaths, Result};

/// The name of the jar of the agent, without the directory.
pub const AGENT_JAR_NAME: &str = "minlaucher-agent.jar";

/// The agent, ready to be attached to a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// The jar to attach.
    pub jar: PathBuf,
    /// The file the agent writes its report to.
    pub report: PathBuf,
}

/// Find the agent and prepare it for use.
///
/// Returns `None` when the agent is not available. That is not a
/// failure: the game runs perfectly well without it, it is only not
/// measured. The reason is written into the log, so that the launcher
/// can be asked about it later.
#[must_use]
pub fn prepare(paths: &AppPaths, version_id: &str) -> Option<Agent> {
    let target = paths.agent_jar();
    let report = paths.agent_report(version_id);

    if !target.is_file() {
        // The jar is not in place yet. It is looked for next to the
        // executable of the launcher, where a portable build can drop it.
        let Some(source) = find_bundled() else {
            tracing::info!("the agent is not installed, the game will not be measured");
            return None;
        };

        if let Err(error) = copy(&source, &target) {
            tracing::warn!("cannot install the agent: {error}");
            return None;
        }
        tracing::info!("the agent was installed at {}", target.display());
    }

    if !ml_core::paths::is_usable_as_agent_path(&target) {
        tracing::warn!(
            "the path of the agent has an '=' in it, the game will not be measured: {}",
            target.display()
        );
        return None;
    }

    // The report of the last run of this version is removed, so that a
    // report that is not written by this run is not mistaken for one.
    let _ = std::fs::remove_file(&report);

    Some(Agent {
        jar: target,
        report,
    })
}

/// Look for the jar of the agent next to the executable of the launcher.
///
/// The launcher is often started through a shortcut, so the directory
/// of the executable is the one that holds the jar. The current
/// directory is not looked at: it belongs to the game, not to the
/// launcher.
fn find_bundled() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;

    for name in [AGENT_JAR_NAME, "minlaucher-agent-0.1.0.jar"] {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Copy the jar of the agent into place.
///
/// The copy is done through a temporary file that is moved over the
/// target, so that a launcher that is stopped halfway does not leave a
/// half-written jar behind — the JVM would then fail to read it and the
/// game would not start.
fn copy(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ml_core::Error::io(parent, e))?;
    }

    let temporary = target.with_extension("jar.tmp");
    std::fs::copy(source, &temporary).map_err(|e| ml_core::Error::io(source, e))?;
    std::fs::rename(&temporary, target).map_err(|e| ml_core::Error::io(target, e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_no_agent_when_nothing_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        // The jar is not next to the executable of the test, so there
        // is nothing to find.
        assert_eq!(prepare(&paths, "1.20.5"), None);
    }

    #[test]
    fn the_report_of_the_last_run_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        // A jar is put in place, and a report of an earlier run is
        // left behind.
        let jar = paths.agent_jar();
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, b"not really a jar").unwrap();

        let report = paths.agent_report("1.20.5");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, b"{}").unwrap();

        let agent = prepare(&paths, "1.20.5").expect("the agent was not found");

        assert_eq!(agent.jar, jar);
        assert_eq!(agent.report, report);
        assert!(!report.exists(), "the old report was not removed");
    }

    #[test]
    fn a_jar_that_is_already_in_place_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        let jar = paths.agent_jar();
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, b"the one that is already there").unwrap();

        let agent = prepare(&paths, "1.20.5").expect("the agent was not found");
        assert_eq!(agent.jar, jar);
        assert_eq!(
            std::fs::read(&jar).unwrap(),
            b"the one that is already there"
        );
    }
}
