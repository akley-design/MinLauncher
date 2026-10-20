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
use std::time::Duration;

use ml_core::{Error, Result};
use tokio::process::{Child, Command};
use tokio::time::Instant;

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

/// The rules of the restart of a game that ended on its own.
///
/// The rules answer two questions: may the game be started again, and
/// how many times. The second one matters as much as the first: a game
/// that falls over every time it is started would otherwise be started
/// again forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Whether the game may be started again at all.
    pub enabled: bool,
    /// How many restarts are allowed inside the window.
    pub max_restarts: u32,
    /// How long to wait before the next start.
    pub backoff: Duration,
    /// The window the restarts are counted in.
    pub window: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_restarts: 3,
            backoff: Duration::from_secs(5),
            window: Duration::from_secs(300),
        }
    }
}

/// A handle to a running game.
#[derive(Debug)]
pub struct GameProcess {
    child: Child,
    pid: Option<u32>,
    stopped: bool,
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
    /// The process is remembered as stopped, so that the code that waits
    /// for it can tell a game the player closed from a game that fell
    /// over.
    ///
    /// # Errors
    /// Fails if the process cannot be signalled.
    pub async fn stop(&mut self) -> Result<()> {
        self.stopped = true;
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
            stopped: self.stopped,
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

        Ok(GameProcess {
            child,
            pid,
            stopped: false,
        })
    }

    /// Start the game and wait for it to end.
    ///
    /// # Errors
    /// Fails when the process cannot be started.
    pub async fn run(&self) -> Result<ExitStatus> {
        let mut process = self.start()?;
        process.wait().await
    }

    /// Start the game and start it again when it ends on its own.
    ///
    /// The two callbacks are how the caller stays in the loop:
    ///
    /// * `on_ended` is asked, every time the game ends, whether it may
    ///   be started again. The caller answers `false` for a game the
    ///   player closed — that game is not a game that fell over, and
    ///   starting it again would be exactly the wrong thing to do;
    /// * `on_started` is called after every start the supervisor made,
    ///   with the number of the attempt and the identifier of the new
    ///   process, so that the caller can show the game that is running.
    ///
    /// The supervisor keeps the rules: the number of restarts inside the
    /// window, the wait between them, and the backoff.
    ///
    /// # Errors
    /// Fails when the process cannot be started.
    pub async fn run_with_restart(
        &self,
        policy: &RestartPolicy,
        on_started: &mut (dyn FnMut(u32, Option<u32>) + Send),
        on_ended: &mut (dyn FnMut(u32, &ExitStatus) -> bool + Send),
    ) -> Result<ExitStatus> {
        let process = self.start()?;
        self.watch_with_restart(process, policy, on_started, on_ended)
            .await
    }

    /// Wait for a game that is already running and start it again when
    /// it ends on its own.
    ///
    /// It is the same thing [`Supervisor::run_with_restart`] does, but
    /// for a caller that started the game itself and needs to know its
    /// identifier before the game is watched.
    ///
    /// # Errors
    /// Fails when the process cannot be started again.
    pub async fn watch_with_restart(
        &self,
        mut process: GameProcess,
        policy: &RestartPolicy,
        on_started: &mut (dyn FnMut(u32, Option<u32>) + Send),
        on_ended: &mut (dyn FnMut(u32, &ExitStatus) -> bool + Send),
    ) -> Result<ExitStatus> {
        let mut attempt = 1u32;
        let mut restarts: Vec<Instant> = Vec::new();

        loop {
            let status = process.wait().await?;

            if !policy.enabled {
                return Ok(status);
            }

            // The restarts that are too old do not count: a game that
            // fell over three times in a week and once in the last
            // minute is not a game that is in a loop.
            let now = Instant::now();
            restarts.retain(|at| now.duration_since(*at) < policy.window);
            if restarts.len() >= policy.max_restarts as usize {
                tracing::warn!(
                    "the game is not started again: the limit of the restarts is reached"
                );
                return Ok(status);
            }

            if !on_ended(attempt, &status) {
                return Ok(status);
            }

            restarts.push(now);
            attempt += 1;

            tokio::time::sleep(policy.backoff).await;

            process = self.start()?;
            on_started(attempt, process.id());
        }
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

    /// A command that ends at once with a failure.
    fn failing_command() -> LaunchCommand {
        #[cfg(windows)]
        {
            let mut command = command("cmd");
            command.arguments = vec!["/C".into(), "exit".into(), "1".into()];
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = command("/bin/sh");
            command.arguments = vec!["-c".into(), "exit 1".into()];
            command
        }
    }

    /// A policy that waits as little as the test can afford.
    fn quick_policy(max_restarts: u32) -> RestartPolicy {
        RestartPolicy {
            enabled: true,
            max_restarts,
            backoff: Duration::from_millis(10),
            window: Duration::from_secs(600),
        }
    }

    #[tokio::test]
    async fn a_failing_game_is_started_again_until_the_limit() {
        let supervisor = Supervisor::new(failing_command());
        let policy = quick_policy(2);

        let mut started = Vec::new();
        let mut on_started = |attempt: u32, _pid: Option<u32>| started.push(attempt);
        let mut on_ended = |_attempt: u32, _status: &ExitStatus| true;

        let status = supervisor
            .run_with_restart(&policy, &mut on_started, &mut on_ended)
            .await
            .unwrap();

        assert!(!status.is_success());
        assert_eq!(started, vec![2, 3], "two restarts, and no more");
    }

    #[tokio::test]
    async fn a_game_the_player_stopped_is_not_started_again() {
        let supervisor = Supervisor::new(failing_command());
        let policy = quick_policy(3);

        let mut started = Vec::new();
        let mut on_started = |attempt: u32, _pid: Option<u32>| started.push(attempt);
        // The player closed the game: the answer is no.
        let mut on_ended = |_attempt: u32, _status: &ExitStatus| false;

        supervisor
            .run_with_restart(&policy, &mut on_started, &mut on_ended)
            .await
            .unwrap();

        assert!(started.is_empty(), "the game was not started again");
    }

    #[tokio::test]
    async fn the_wait_between_the_restarts_is_kept() {
        let supervisor = Supervisor::new(failing_command());
        let policy = RestartPolicy {
            backoff: Duration::from_millis(300),
            ..quick_policy(1)
        };

        let mut on_started = |_attempt: u32, _pid: Option<u32>| {};
        let mut on_ended = |_attempt: u32, _status: &ExitStatus| true;

        let started_at = Instant::now();
        supervisor
            .run_with_restart(&policy, &mut on_started, &mut on_ended)
            .await
            .unwrap();
        let elapsed = started_at.elapsed();

        assert!(
            elapsed >= Duration::from_millis(300),
            "the game was started again after {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn a_process_that_was_asked_to_stop_says_so() {
        let supervisor = Supervisor::new(failing_command());
        let mut process = supervisor.start().unwrap();
        process.stop().await.unwrap();
        let status = process.wait().await.unwrap();

        assert!(status.stopped, "the process was stopped on purpose");
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
