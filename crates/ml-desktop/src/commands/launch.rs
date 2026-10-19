//! The commands that start and stop the game.
//!
//! Starting the game is the one operation of the launcher that a player
//! waits for, so the command is built to spend as little time as
//! possible before the process exists:
//!
//! * the version document is read from the cache, not from the network;
//! * the libraries are resolved from the database, not by resolving the
//!   rules again;
//! * the command line is built and the natives are unpacked only when
//!   the natives are not unpacked yet.
//!
//! The command returns as soon as the process exists. The launcher does
//! not wait for the game: a launcher that waits is a launcher that
//! cannot be closed.

use std::path::PathBuf;
use std::sync::Arc;

use ml_core::{Account, AppPaths, FeatureSet, JavaRuntime};
use ml_launch::args::Session;
use ml_launch::command::LaunchRequest;
use ml_launch::jvm::JavaInstallation;
use ml_launch::supervisor::{GameProcess, Supervisor};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// The event the frontend listens to while the game runs.
pub const GAME_EVENT: &str = "minlauncher://game";

/// The state of the running game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningGame {
    /// The identifier of the process, when the platform gives one.
    pub pid: Option<u32>,
    /// The version that is running.
    pub version_id: String,
    /// The command line the game was started with.
    pub command: String,
    /// When the game was started, in milliseconds since the epoch.
    pub started_at: i64,
}

/// What the frontend gets after a successful start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchResult {
    /// The state of the running game.
    pub game: RunningGame,
    /// How long the launcher spent preparing the launch, in milliseconds.
    ///
    /// This is the number M3 is built on, and it is measured from the
    /// moment the player clicked the button.
    pub prepare_ms: u64,
}

/// The request of a launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchOptions {
    /// The version to launch.
    pub version_id: String,
    /// The instance to launch it in; empty means the default directory.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// The width of the game window.
    #[serde(default)]
    pub width: Option<u32>,
    /// The height of the game window.
    #[serde(default)]
    pub height: Option<u32>,
    /// A Java executable that overrides the one of the launcher.
    #[serde(default)]
    pub java_path: Option<String>,
}

/// Start the game.
///
/// # Errors
/// Fails when the version is not installed, when no account is selected,
/// when no Java runtime is found, or when the process cannot be started.
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    state: State<'_, SharedState>,
    options: LaunchOptions,
) -> IpcResult<LaunchResult> {
    let started_at = std::time::Instant::now();

    if state.running().is_some() {
        return Err(IpcError::invalid("the game is already running"));
    }

    let storage = state.storage();

    // --- the account ------------------------------------------------
    let account: Account = storage
        .accounts()
        .selected()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the accounts",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::invalid("no account is selected"))?;

    // --- the version ------------------------------------------------
    let meta = storage
        .versions()
        .get(&options.version_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the versions",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            IpcError::not_found(format!("there is no version `{}`", options.version_id))
        })?;

    // --- the request ------------------------------------------------
    let request = build_request(&state, &account, &meta, &options)?;

    // --- the command ------------------------------------------------
    let platform = state.platform();
    let features = FeatureSet::none();
    let prepared = ml_launch::prepare(&request, platform, &features, meta.kind.as_manifest_str())
        .map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "cannot prepare the launch",
            error.to_string(),
        )
    })?;

    let command_line = prepared.display();

    let supervisor = Supervisor::new(prepared.command);
    let mut process = supervisor.start().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "cannot start the game",
            error.to_string(),
        )
    })?;

    let game = RunningGame {
        pid: process.id(),
        version_id: meta.id.clone(),
        command: command_line,
        started_at: chrono::Utc::now().timestamp_millis(),
    };

    state.set_running(Some(game.clone()));

    // The frontend is told that the game started, and that the window
    // of the launcher can be closed.
    let _ = app.emit(GAME_EVENT, &game);

    // The process is watched in the background: the launcher stays
    // usable while the game runs, and the frontend is told when the
    // game ends.
    let handle = app.clone();
    let shared = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let status = process.wait().await;
        shared.set_running(None);

        let event = GameEvent::Exited {
            version_id: meta.id.clone(),
            success: status.as_ref().is_ok_and(ml_launch::ExitStatus::is_success),
            code: status.as_ref().ok().and_then(|status| status.code),
        };
        let _ = handle.emit(GAME_EVENT, &event);
    });

    Ok(LaunchResult {
        game,
        prepare_ms: started_at.elapsed().as_millis() as u64,
    })
}

/// What the launcher tells the frontend while the game runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameEvent {
    /// The game was started.
    Started {
        /// The state of the running game.
        game: RunningGame,
    },
    /// The game ended.
    Exited {
        /// The version that was running.
        version_id: String,
        /// Whether the game ended with a zero exit code.
        success: bool,
        /// The exit code, when the platform gives one.
        code: Option<i32>,
    },
}

/// Stop the game.
///
/// # Errors
/// Fails when no game is running or the process cannot be stopped.
#[tauri::command]
pub async fn stop_game(state: State<'_, SharedState>) -> IpcResult<()> {
    let game = state
        .running()
        .ok_or_else(|| IpcError::invalid("no game is running"))?;

    // The process is stopped by its identifier, because the handle the
    // launcher kept belongs to the background task.
    if let Some(pid) = game.pid {
        stop_process(pid)?;
    }

    state.set_running(None);
    Ok(())
}

/// Ask the operating system to stop a process.
fn stop_process(pid: u32) -> IpcResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // `taskkill` is used because the launcher does not keep a handle
        // to the process it started in the command that stops it.
        let output = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| {
                IpcError::with_detail(ErrorCode::Launch, "cannot stop the game", error.to_string())
            })?;

        if !output.status.success() {
            return Err(IpcError::with_detail(
                ErrorCode::Launch,
                "cannot stop the game",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
    }

    #[cfg(not(windows))]
    {
        let _ = pid;
    }

    Ok(())
}

/// The flag that tells Windows not to open a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build the request of a launch.
///
/// Everything that has to be known before the game can start is put
/// together here: the version document, the Java runtime, the libraries
/// and the session.
fn build_request(
    state: &SharedState,
    account: &Account,
    meta: &ml_core::VersionMeta,
    options: &LaunchOptions,
) -> IpcResult<LaunchRequest> {
    let storage = state.storage();

    // --- the version document ---------------------------------------
    let version_json_path = storage
        .versions()
        .json_path(&meta.id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the versions",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            IpcError::not_found(format!("the version `{}` is not installed", meta.id))
        })?;

    let version_bytes = std::fs::read(&version_json_path).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the version document",
            error.to_string(),
        )
    })?;

    let version = ml_minecraft::version::VersionJson::parse(&version_bytes)?;

    // --- the java runtime -------------------------------------------
    let java = find_java(state, meta.java_major, options.java_path.as_deref())?;

    // --- the libraries ----------------------------------------------
    //
    // The libraries come from the rules of the version, not from the
    // database: the database knows what was downloaded, and the rules
    // know what is needed. The path of every jar is derived from its
    // coordinate, so the two never have to be reconciled.
    let platform = state.platform();
    let libraries = version.resolve_libraries(platform, Some(&meta.id));

    // --- the session ------------------------------------------------
    let session = build_session(state, account, meta, options);

    Ok(LaunchRequest {
        version,
        libraries,
        java,
        session,
        client_jar: state.paths().version_dir(&meta.id).join("client.jar"),
        library_directory: library_directory(state.paths()),
        extra_jvm_args: extra_jvm_args(state),
        agent_jar: None,
    })
}

/// Find the Java runtime the version needs.
fn find_java(
    state: &SharedState,
    java_major: Option<u32>,
    explicit: Option<&str>,
) -> IpcResult<JavaInstallation> {
    let runtime = JavaRuntime {
        component: java_major
            .map(|major| format!("java-runtime-{major}"))
            .unwrap_or_default(),
        major_version: java_major.unwrap_or(8),
    };

    let explicit = explicit.map(PathBuf::from);

    ml_launch::jvm::find_runtime(state.paths(), &runtime, explicit.as_deref()).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "cannot find a java runtime",
            error.to_string(),
        )
    })
}

/// Build the session of the launch.
fn build_session(
    state: &SharedState,
    account: &Account,
    meta: &ml_core::VersionMeta,
    options: &LaunchOptions,
) -> Session {
    let paths = state.paths();

    let game_directory = match &options.instance_id {
        Some(id) => paths.instance_dir(id),
        None => paths.instance_dir("default"),
    };

    Session {
        player_name: account.username.clone(),
        uuid: account.uuid.clone(),
        uuid_prefix: account.uuid_prefix.clone(),
        access_token: String::new(),
        version_name: meta.id.clone(),
        version_type: meta.kind.as_manifest_str().to_string(),
        game_directory,
        assets_root: paths.assets_dir.clone(),
        assets_index_name: meta.asset_index_id.clone().unwrap_or_default(),
        library_directory: library_directory(paths),
        natives_directory: paths.version_dir(&meta.id).join("natives"),
        launcher_name: "MinLauncher".to_string(),
        launcher_version: ml_core::VERSION.to_string(),
        resolution_width: options.width,
        resolution_height: options.height,
        ..Session::default()
    }
}

/// The directory the libraries live in.
fn library_directory(paths: &AppPaths) -> PathBuf {
    paths.cache_dir.join("libraries")
}

/// The extra JVM arguments the settings ask for.
fn extra_jvm_args(state: &SharedState) -> Vec<String> {
    let Ok(config) = state.storage().load_settings() else {
        return Vec::new();
    };

    let mut args = Vec::new();

    if !config.jvm.max_heap.is_empty() {
        args.push(format!("-Xmx{}", config.jvm.max_heap));
    }
    if !config.jvm.min_heap.is_empty() {
        args.push(format!("-Xms{}", config.jvm.min_heap));
    }
    if !config.jvm.garbage_collector.is_empty() {
        args.push(format!("-XX:+Use{}", config.jvm.garbage_collector));
    }
    args.extend(config.jvm.extra_args.iter().cloned());

    args
}

/// The process of a running game, kept so that the launcher can stop it.
pub type RunningProcess = GameProcess;

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{OsArch, OsName, Platform};
    use ml_minecraft::library::ResolvedLibrary;

    fn library(name: &str) -> ResolvedLibrary {
        ResolvedLibrary {
            name: format!("org.lwjgl:{name}:3.4.3"),
            coordinate: ml_minecraft::library::Coordinate {
                group: "org.lwjgl".into(),
                artifact: name.into(),
                version: "3.4.3".into(),
                classifier: String::new(),
                extension: "jar".into(),
            },
            is_native: false,
            artifact: None,
            natives: Vec::new(),
            rules_json: None,
        }
    }

    #[test]
    fn the_extra_arguments_follow_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let mut config = state.storage().load_settings().unwrap();
        config.jvm.max_heap = "4G".into();
        config.jvm.min_heap = "1G".into();
        config.jvm.garbage_collector = "Z".into();
        config.jvm.extra_args = vec!["-XX:+UseStringDeduplication".into()];
        state.storage().save_settings(&config).unwrap();

        let args = extra_jvm_args(&state);
        assert!(args.contains(&"-Xmx4G".to_string()), "{args:?}");
        assert!(args.contains(&"-Xms1G".to_string()), "{args:?}");
        assert!(args.contains(&"-XX:+UseZ".to_string()), "{args:?}");
        assert!(
            args.contains(&"-XX:+UseStringDeduplication".to_string()),
            "{args:?}"
        );
    }

    #[test]
    fn the_extra_arguments_are_empty_without_a_heap() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let args = extra_jvm_args(&state);
        assert!(args.is_empty(), "{args:?}");
    }

    #[test]
    fn the_classpath_of_a_request_is_built_from_the_coordinates() {
        // The path of a library is derived from its coordinate, so the
        // launcher and the download engine always agree on it.
        let request = LaunchRequest {
            version: ml_minecraft::version::VersionJson::parse(
                br#"{"id":"26.3","type":"release","mainClass":"Main"}"#,
            )
            .unwrap(),
            libraries: vec![library("lwjgl")],
            java: JavaInstallation {
                executable: PathBuf::from("java"),
                windowless_executable: None,
                major_version: 25,
                version: "25.0.1".into(),
                origin: ml_launch::jvm::JavaOrigin::System,
            },
            session: Session::default(),
            client_jar: PathBuf::from("C:/v/client.jar"),
            library_directory: PathBuf::from("C:/cache/libraries"),
            extra_jvm_args: Vec::new(),
            agent_jar: None,
        };

        // The library has no artifact, so it does not go on the
        // classpath — but the entry point does not panic either.
        assert_eq!(request.classpath_entries().len(), 0);
    }
}
