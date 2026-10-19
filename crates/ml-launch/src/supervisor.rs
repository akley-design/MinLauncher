//! Supervising the game process.
//!
//! The supervisor starts the game, forwards its output, and reports when
//! it ends. It is deliberately thin: everything that decides *what* to
//! run lives in [`crate::command`], and everything that decides *how* to
//! run it lives here.
//!
//! The output of the game is line buffered and forwarded to the launcher
//! log. On Windows the game runs under `javaw.exe`, which has no console
//! at all, so the launcher cannot read its output — there the game writes
//! into the `logs` directory of the game itself.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use ml_core::{Error, Result};
use tokio::process::{Child, Command};

use crate::command::LaunchCommand;

/// How the game process ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatus {
    /// The exit code of the process.
    pub code: Option<i32>,
    /// The terminating signal, on POSIX systems.
    pub signal: Option<i32>,
    /// Whether the process ended because it was asked to.
    pub stopped: bool,
}

impl ExitStatus {
    /// Whether the process ended successfully.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.code == Some(0)
    }
}

/// A handle to a running game.
#[derive(Debug)]
pub struct GameProcess {
    child: Child,
    pid: Option<u32>,
}

impl GameProcess {
    /// The identifier of the process.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.pid
    }

    /// Whether the process is still running.
    #[must_use]
    pub fn is_running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    /// Ask the process to stop.
    ///
    /// # Errors
    /// Fails if the process cannot be signalled.
    pub async fn stop(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| Error::ProcessSpawn(e.to_string()))
    }

    /// Wait for the process to end.
    ///
    /// # Errors
    /// Fails if the process cannot be waited for.
    pub async fn wait(&mut self) -> Result<ExitStatus> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|e| Error::ProcessSpawn(e.to_string()))?;

        Ok(ExitStatus {
            code: status.code(),
            signal: exit_signal(&status),
            stopped: false,
        })
    }
}

/// The game process and the log it writes to.
#[derive(Debug)]
pub struct Supervisor {
    command: LaunchCommand,
    log_path: Option<PathBuf>,
}

impl Supervisor {
    /// Create a supervisor for a command.
    #[must_use]
    pub fn new(command: LaunchCommand) -> Self {
        Self {
            command,
            log_path: None,
        }
    }

    /// Write the output of the game into a file as well.
    #[must_use]
    pub fn with_log(mut self, path: PathBuf) -> Self {
        self.log_path = Some(path);
        self
    }

    /// The command that will be run.
    #[must_use]
    pub fn command(&self) -> &LaunchCommand {
        &self.command
    }

    /// Start the game.
    ///
    /// # Errors
    /// Fails when the process cannot be started.
    pub fn start(&self) -> Result<GameProcess> {
        let mut command = Command::new(&self.command.executable);
        command
            .args(&self.command.arguments)
            .current_dir(&self.command.working_directory)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &self.command.environment {
            command.env(key, value);
        }

        // The launcher itself is visible to the game, so that the game
        // can tell a `MinLauncher` session from a vanilla one.
        command.env("MINLAUNCHER_VERSION", env!("CARGO_PKG_VERSION"));

        let mut child = command
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::ProcessSpawn(e.to_string()))?;

        let pid = child.id();

        // The output is drained on background tasks, so that a chatty
        // game cannot fill the pipe buffer and block.
        if let Some(stdout) = child.stdout.take() {
            spawn_drain(stdout, self.log_path.clone(), false);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_drain(stderr, self.log_path.clone(), true);
        }

        Ok(GameProcess { child, pid })
    }

    /// Start the game and wait for it to end.
    ///
    /// # Errors
    /// Fails when the process cannot be started.
    pub async fn run(&self) -> Result<ExitStatus> {
        let mut process = self.start()?;
        process.wait().await
    }
}

/// Read a stream to the end and write it to the log.
fn spawn_drain<R>(reader: R, log_path: Option<PathBuf>, is_error: bool)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;

        let mut lines = tokio::io::BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if is_error {
                tracing::debug!("game: {line}");
            } else {
                tracing::info!("game: {line}");
            }
            if let Some(path) = &log_path
                && let Ok(mut file) = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await
            {
                use tokio::io::AsyncWriteExt;
                let _ = file.write_all(line.as_bytes()).await;
                let _ = file.write_all(b"\n").await;
            }
        }
    });
}

/// A shared handle to the supervisor, for the state of the desktop shell.
pub type SharedSupervisor = Arc<Supervisor>;

/// The signal that killed a process, on POSIX systems.
///
/// Windows has no signals in this sense, so the answer is always `None`.
#[must_use]
pub fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Session;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn command(program: &str) -> LaunchCommand {
        LaunchCommand {
            executable: PathBuf::from(program),
            arguments: Vec::new(),
            working_directory: std::env::temp_dir(),
            environment: Vec::new(),
            natives_layout: crate::natives::NativeLayout {
                natives_directory: PathBuf::from("/tmp/natives"),
                directories: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn a_missing_executable_is_reported() {
        let supervisor = Supervisor::new(command("/definitely/not/a/program"));
        let error = supervisor.start().unwrap_err();
        assert!(matches!(error, Error::ProcessSpawn(_)));
    }

    #[cfg(unix)]
    #[test]
    fn a_successful_process_reports_its_code() {
        let supervisor = Supervisor::new(command("/bin/sh"));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let status = runtime.block_on(supervisor.run()).unwrap();
        assert!(status.is_success());
    }

    #[test]
    fn the_session_directory_is_where_the_game_runs() {
        let session = Session {
            game_directory: PathBuf::from("/tmp/game"),
            ..Session::default()
        };
        assert_eq!(
            crate::command::game_directory(&session),
            Path::new("/tmp/game")
        );
    }
}
