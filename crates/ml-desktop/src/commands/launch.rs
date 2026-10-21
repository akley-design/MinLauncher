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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ml_core::{Account, AppPaths, FeatureSet, JavaRuntime};
use ml_launch::args::Session;
use ml_launch::command::LaunchRequest;
use ml_launch::jvm::JavaInstallation;
use ml_launch::supervisor::{GameProcess, Supervisor};
use ml_minecraft::mod_meta::{ModFit, fit_of_mod};
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

    // --- the account and the version --------------------------------
    let (account, meta) = read_context(&state, &options)?;

    // --- the request ------------------------------------------------
    let mut request = build_request(&state, &account, &meta, &options)?;

    // --- the loader -------------------------------------------------
    //
    // The loader makes the game itself faster. It is optional: a
    // version of the game the loader does not support is started without
    // it, and the game is a little slower but works.
    prepare_loader(&state, &mut request, &meta.id).await;

    // The scenario of the run is known now: the preset of the mod that
    // was in the game. The runs with the mod and without it are stored
    // apart, because they are compared against each other.
    let scenario = scenario_of_run(&state, !request.mod_jars.is_empty());

    // --- the command ------------------------------------------------
    let platform = state.platform();
    let features = features_for(&request);
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
    let process = supervisor.start().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "cannot start the game",
            error.to_string(),
        )
    })?;

    let game = RunningGame {
        pid: process.id(),
        version_id: meta.id.clone(),
        command: command_line.clone(),
        started_at: chrono::Utc::now().timestamp_millis(),
    };

    state.set_running(Some(game.clone()));
    // Every run gets a flag of its own. The watcher of this run reads
    // this flag and not the one of the run before it.
    let run_flag = state.begin_run();

    // The frontend is told that the game started, and that the window
    // of the launcher can be closed.
    let _ = app.emit(GAME_EVENT, &GameEvent::Started { game: game.clone() });

    // The process is watched in the background: the launcher stays
    // usable while the game runs, and the frontend is told when the
    // game ends.
    tauri::async_runtime::spawn(watch_game(Watch {
        app: app.clone(),
        state: Arc::clone(&state),
        supervisor,
        process,
        game: game.clone(),
        report_path: request.agent_report.clone(),
        scenario,
        run_flag,
    }));

    Ok(LaunchResult {
        game,
        prepare_ms: started_at.elapsed().as_millis() as u64,
    })
}

/// The account and the version a launch needs.
fn read_context(
    state: &SharedState,
    options: &LaunchOptions,
) -> IpcResult<(Account, ml_core::VersionMeta)> {
    let storage = state.storage();

    let account = storage
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

    Ok((account, meta))
}

/// What the watcher of a game needs to know.
struct Watch {
    /// The handle of the application, for the events.
    app: AppHandle,
    /// The state of the launcher.
    state: SharedState,
    /// The supervisor that starts the game.
    supervisor: Supervisor,
    /// The game that is running now.
    process: GameProcess,
    /// The game as the frontend knows it.
    game: RunningGame,
    /// Where the agent writes its report.
    report_path: Option<PathBuf>,
    /// The scenario the measurements of the run are stored under.
    scenario: String,
    /// The flag that says the player stopped the game.
    run_flag: Arc<AtomicBool>,
}

/// Watch a game that was started, and start it again when it falls over.
///
/// The task lives as long as the game does. It tells the frontend about
/// the start, about the restart and about the end.
async fn watch_game(watch: Watch) {
    let Watch {
        app,
        state,
        supervisor,
        process,
        game,
        report_path,
        scenario,
        run_flag,
    } = watch;

    let policy = restart_policy(&state);

    // The game is started again when it falls over. The frontend is
    // told about it, so that the player is not left in front of a
    // screen that says the game ended and does not explain why it is
    // still running.
    let mut on_started = |attempt: u32, pid: Option<u32>| {
        let started = RunningGame {
            pid,
            version_id: game.version_id.clone(),
            command: game.command.clone(),
            started_at: chrono::Utc::now().timestamp_millis(),
        };
        state.set_running(Some(started.clone()));
        let _ = app.emit(GAME_EVENT, &GameEvent::Started { game: started });
        tracing::info!("the game was started again, attempt {attempt}");
    };

    // A game the player closed is not a game that fell over, and a game
    // that ended quietly is not one either — the player closed the
    // window. Only a game that ended with a failure is started again.
    let mut on_ended = |attempt: u32, status: &ml_launch::ExitStatus| -> bool {
        if run_flag.load(Ordering::Relaxed) || status.is_success() {
            return false;
        }
        let _ = app.emit(
            GAME_EVENT,
            &GameEvent::Restarting {
                version_id: game.version_id.clone(),
                attempt: attempt + 1,
            },
        );
        true
    };

    let status = supervisor
        .watch_with_restart(process, &policy, &mut on_started, &mut on_ended)
        .await;
    state.set_running(None);

    // The agent writes its report when the game ends, and it is read
    // now that the game is gone. What it says becomes the measurements
    // of the metrics M5 and M6.
    if let Some(path) = report_path {
        store_agent_report(&state, &path, &scenario);
    }

    let stopped = run_flag.load(Ordering::Relaxed);
    let event = GameEvent::Exited {
        version_id: game.version_id.clone(),
        success: status.as_ref().is_ok_and(ml_launch::ExitStatus::is_success),
        code: status.as_ref().ok().and_then(|status| status.code),
        stopped,
    };
    let _ = app.emit(GAME_EVENT, &event);
}

/// The rules of the restart of a game that fell over.
///
/// The settings are read once, when the game is started: a rule that
/// changes while the game runs would change the meaning of the run that
/// is already under way.
fn restart_policy(state: &SharedState) -> ml_launch::RestartPolicy {
    let config = state.storage().load_settings().unwrap_or_default();

    ml_launch::RestartPolicy {
        enabled: config.restart.enabled,
        max_restarts: config.restart.max_restarts,
        backoff: std::time::Duration::from_millis(config.restart.backoff_ms),
        window: std::time::Duration::from_millis(config.restart.window_ms),
    }
}

/// Read the report of the agent and store what it says.
///
/// A report that is missing, or that holds nothing usable, is not an
/// error: the game may have been started without the agent, and a game
/// that was not measured is not a broken game.
fn store_agent_report(state: &SharedState, path: &std::path::Path, scenario: &str) {
    let Some(report) = ml_launch::read_report(path) else {
        tracing::info!("the agent wrote no report at {}", path.display());
        return;
    };

    for failure in &report.failures {
        tracing::warn!("the agent: {failure}");
    }

    let Some(measurements) = ml_bench::measurements_of(&report) else {
        tracing::info!("the report holds no measurement");
        return;
    };

    let mut meta = ml_bench::Meta::of_this_machine();
    meta.set("frames", report.frames.to_string());
    meta.set("agent", "minlaucher-agent");

    let stored = ml_bench::store_measurements(state.storage(), &measurements, &mut meta, scenario);
    tracing::info!("the agent report was stored as {stored} measurements under {scenario}");

    // The measurements are also written into the file of the scenario,
    // so that a measurer outside can pick them up.
    for (metric, sample) in [
        (ml_bench::Metric::InGameFps, measurements.fps),
        (ml_bench::Metric::InGameFrametime, measurements.frametime_ms),
    ] {
        if let Err(error) =
            ml_bench::samples::append(state.paths(), scenario, metric, sample, &meta)
        {
            tracing::warn!("the measurement of {} was not written: {error}", metric);
        }
    }
}

/// The scenario of a run: the preset of the mod that was in the game.
///
/// The preset is `off` when the mod of the launcher was not in the
/// game: either the player switched it off, or the loader was not
/// there, or its jar could not be put in place, or the loader would
/// have refused it in this version of the game. The scenario has to
/// say what really happened — a run without the mod that calls itself a
/// run with it would poison the comparison.
fn scenario_of_run(state: &SharedState, mod_is_in_the_game: bool) -> String {
    if !mod_is_in_the_game {
        return ml_bench::scenario_without_mod();
    }

    let preset = match state.storage().load_settings() {
        Ok(config) => config.mods.preset,
        Err(_) => ml_bench::PRESET_OFF.to_string(),
    };

    ml_bench::scenario_of(&preset)
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
    /// The game ended and is being started again.
    Restarting {
        /// The version that was running.
        version_id: String,
        /// The number of the attempt that is coming, counting from one.
        attempt: u32,
    },
    /// The game ended.
    Exited {
        /// The version that was running.
        version_id: String,
        /// Whether the game ended with a zero exit code.
        success: bool,
        /// The exit code, when the platform gives one.
        code: Option<i32>,
        /// Whether the player stopped the game on purpose.
        ///
        /// A game the player closed is not a failure, and the interface
        /// must not say that it was.
        stopped: bool,
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

    // The flag is set before the process is killed: the watcher of the
    // game must know that the player closed it, and it must know it
    // before the process is gone.
    state.stop_current_run();

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
    let java = find_java(state, meta, options.java_path.as_deref())?;

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

    // --- the agent --------------------------------------------------
    //
    // The agent measures the game from the inside. It is optional: a
    // game without it runs perfectly well, it is only not measured.
    let agent = agent_for(state, &meta.id);

    Ok(LaunchRequest {
        version,
        libraries,
        java,
        session,
        client_jar: state.paths().version_dir(&meta.id).join("client.jar"),
        library_directory: library_directory(state.paths()),
        extra_jvm_args: extra_jvm_args(state),
        agent_jar: agent.as_ref().map(|agent| agent.jar.clone()),
        agent_report: agent.as_ref().map(|agent| agent.report.clone()),
        loader: None,
        mod_jars: Vec::new(),
    })
}

/// Put the loader in front of the game, when everything is ready for it.
///
/// Everything that can go wrong here is a normal thing: the loader does
/// not publish a loader for every version of the game, the network may
/// be down, and the jar of the mod may not be built yet. In every one of
/// those cases the game is started without the loader, and the launcher
/// says why in its log.
async fn prepare_loader(state: &SharedState, request: &mut LaunchRequest, version_id: &str) {
    let Ok(config) = state.storage().load_settings() else {
        return;
    };

    if !config.mods.enabled {
        tracing::info!("the mod of the launcher is switched off");
        return;
    }

    // The loader needs a version of the game it supports. The answers of
    // the project are kept on the disk, so that the second launch of a
    // version does not wait for a server that may be slow or down.
    let cache = crate::commands::fabric::LoaderCache::new(state.paths());

    let Some(loader_version) = crate::commands::fabric::loader_version_for(
        &crate::commands::fabric::NetworkFetcher::default(),
        &cache,
        version_id,
        &config.mods.loader_version,
    )
    .await
    .ok()
    .flatten() else {
        tracing::info!("the loader has no release for the version {version_id}");
        return;
    };

    let profile = match crate::commands::fabric::profile(
        &crate::commands::fabric::NetworkFetcher::default(),
        &cache,
        version_id,
        &loader_version,
    )
    .await
    {
        Ok(profile) => profile,
        Err(error) => {
            tracing::warn!("cannot fetch the profile of the loader: {error}");
            return;
        }
    };

    // The libraries of the loader go into the same store the libraries
    // of the game go into.
    if let Err(error) =
        crate::commands::fabric::download_libraries(state.paths(), state.platform(), &profile).await
    {
        tracing::warn!("cannot download the libraries of the loader: {error}");
        return;
    }

    // The settings of the mod are read by the mod itself, at the start
    // of the game.
    let game_directory = request.session.game_directory.clone();
    if let Err(error) =
        crate::commands::fabric::write_mod_settings(&game_directory, &config.mods.preset, true)
    {
        tracing::warn!("cannot write the settings of the mod: {error}");
    }

    // The jar of the mod goes where the loader looks for the mods.
    let source = if config.mods.mod_jar.is_empty() {
        state.paths().mod_jar()
    } else {
        PathBuf::from(&config.mods.mod_jar)
    };

    // The loader refuses a mod whose requirement the version of the game
    // does not meet, and the launcher has to know that before the game
    // starts: a mod that is put in place and then refused would still be
    // counted as a run of the game with the mod, and the measurement would
    // say that the optimisation was in the game when it was not.
    if let Some(mod_to_install) = mod_jar_to_install(&source, version_id) {
        let mods_directory = game_directory.join("mods");
        match crate::commands::fabric::install_mod_jar(mod_to_install, &mods_directory) {
            Ok(jar) => request.mod_jars.push(jar),
            Err(error) => {
                tracing::warn!("cannot install the jar of the mod: {error}");
                // The loader without the mod of the launcher is still worth
                // having: the mods of the player are still loaded.
            }
        }
    }

    request.loader = Some(ml_minecraft::fabric::FabricLaunch::from_profile(
        version_id,
        &loader_version,
        &profile,
    ));

    tracing::info!(
        "the game is started with the loader {loader_version} and {} mods",
        request.mod_jars.len()
    );
}

/// The jar of the mod that goes into the game, when the loader will take it.
///
/// The loader refuses a mod whose requirement the version of the game does
/// not meet, and the launcher has to know that before the game starts: a
/// mod that is put in place and then refused would still be counted as a
/// run of "the game with the mod", and the measurement would say that the
/// optimisation was in the game when it was not.
///
/// `None` when there is nothing to install: the jar is not on the disk,
/// or the loader would not take it in this version of the game.
fn mod_jar_to_install<'a>(source: &'a Path, version_id: &str) -> Option<&'a Path> {
    match fit_of_mod(source, version_id) {
        ModFit::Fits => Some(source),
        ModFit::Refused => {
            tracing::warn!(
                "the loader will not take the mod of the launcher in the version {version_id}: it is left out of the game"
            );
            None
        }
        ModFit::Missing => {
            tracing::warn!(
                "cannot install the jar of the mod: it is not at `{}`",
                source.display()
            );
            None
        }
    }
}

/// The agent to attach to the game, when the settings ask for it.
fn agent_for(state: &SharedState, version_id: &str) -> Option<crate::commands::agent::Agent> {
    let enabled = state
        .storage()
        .load_settings()
        .map_or(true, |config| config.jvm.enable_agent);

    if !enabled {
        return None;
    }

    crate::commands::agent::prepare(state.paths(), version_id)
}

/// The runtime a version needs.
///
/// The component is the name the runtime is stored under, and the major
/// version alone does not give it: `java-runtime-21` is not a directory
/// anybody writes, `java-runtime-delta` is. The document of the version
/// carries the right name, so it is used whenever it is known.
fn runtime_for(meta: &ml_core::VersionMeta) -> JavaRuntime {
    let fallback = JavaRuntime::fallback_for(&meta.id);
    JavaRuntime {
        component: meta
            .java_component
            .clone()
            .filter(|component| !component.is_empty())
            .unwrap_or(fallback.component),
        major_version: meta.java_major.unwrap_or(fallback.major_version),
    }
}

/// Find the Java runtime the version needs.
fn find_java(
    state: &SharedState,
    meta: &ml_core::VersionMeta,
    explicit: Option<&str>,
) -> IpcResult<JavaInstallation> {
    let runtime = runtime_for(meta);

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
        user_type: account.kind.user_type().to_string(),
        access_token: String::new(),
        version_name: meta.id.clone(),
        version_type: meta.kind.as_manifest_str().to_string(),
        game_directory,
        assets_root: paths.assets_dir.clone(),
        // The versions before 1.7 read a plain tree of assets instead of
        // the store. The tree of the versions from 1.6.4 to 1.7.2 is the
        // directory the game is given as `--assetsDir`, and the tree of
        // the older ones is `~/.minecraft/resources`.
        assets_tree: meta
            .asset_index_id
            .as_deref()
            .and_then(|id| ml_minecraft::plain_tree_root(paths, id)),
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

/// The features the launcher can turn on for the rules of the version.
///
/// Mojang puts `--width` and `--height` behind the `has_custom_resolution`
/// feature, so a resolution the player asked for is dropped from the
/// command line unless the feature is on.
fn features_for(request: &LaunchRequest) -> FeatureSet {
    let sized =
        request.session.resolution_width.is_some() && request.session.resolution_height.is_some();
    if sized {
        FeatureSet::of(["has_custom_resolution"])
    } else {
        FeatureSet::none()
    }
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
    // The player may name a collector in the free-text field, and the
    // dropdown may name another one. The virtual machine refuses to
    // start when two different collectors are named at once ("Multiple
    // garbage collectors selected"), so the choice written out by hand
    // replaces the one of the dropdown — the same rule that lets the
    // choice of the player replace the one of the version of the game
    // (see `build_command`).
    let player_names_a_collector = config
        .jvm
        .extra_args
        .iter()
        .any(|argument| ml_core::config::is_collector_flag(argument));
    if !player_names_a_collector {
        args.extend(config.jvm.collector_flag());
    }

    // The profile of the flags comes after the arguments of the version
    // and before the arguments of the player: the last of a pair
    // `+Flag` / `-Flag` is the one the virtual machine obeys, so a flag
    // of the version can be switched off without deleting it, and a
    // player who writes the flag out by hand still gets what they
    // asked for.
    args.extend(ml_core::jvmflags::adjustment(config.jvm.flags_profile));
    args.extend(config.jvm.extra_args.iter().cloned());

    // The log says what was asked of the virtual machine, so that a
    // measurement of the flags can be checked against what the launcher
    // really did. The arguments of the virtual machine hold no secret:
    // the token of the player is in the arguments of the game.
    tracing::info!(
        "the profile of the flags of the virtual machine is {}; the extra arguments are {:?}",
        config.jvm.flags_profile.as_str(),
        args
    );

    args
}

/// The process of a running game, kept so that the launcher can stop it.
pub type RunningProcess = GameProcess;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
        assert!(args.contains(&"-XX:+UseZGC".to_string()), "{args:?}");
        assert!(
            args.contains(&"-XX:+UseStringDeduplication".to_string()),
            "{args:?}"
        );
    }

    #[test]
    fn a_fresh_installation_leaves_the_collector_to_the_version() {
        // The version of the game knows the platform it runs on, and the
        // launcher does not. A collector of its own here would either
        // fight the version — the JVM refuses to start when two
        // collectors are named at once — or be a flag the JVM does not
        // know: `UseG1` is gone since Java 25.
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let args = extra_jvm_args(&state);
        assert!(args.is_empty(), "{args:?}");
        assert!(!args.iter().any(|arg| arg.starts_with("-Xmx")), "{args:?}");
        assert!(!args.iter().any(|arg| arg.starts_with("-Xms")), "{args:?}");
    }

    #[test]
    fn an_empty_collector_is_left_to_the_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let mut config = state.storage().load_settings().unwrap();
        config.jvm.garbage_collector = String::new();
        state.storage().save_settings(&config).unwrap();

        let args = extra_jvm_args(&state);
        assert!(args.is_empty(), "{args:?}");
    }

    #[test]
    fn the_profile_of_the_flags_follows_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        // The profile of the version adds nothing: the version's own
        // flags are already in the command.
        let mut config = state.storage().load_settings().unwrap();
        config.jvm.flags_profile = ml_core::FlagsProfile::Default;
        state.storage().save_settings(&config).unwrap();

        let args = extra_jvm_args(&state);
        assert!(
            !args.iter().any(|arg| arg.contains("AlwaysPreTouch")),
            "{args:?}"
        );

        // The measured profile switches the flag off — by appending, so
        // that the argument of the version stays where it is.
        config.jvm.flags_profile = ml_core::FlagsProfile::Measured;
        state.storage().save_settings(&config).unwrap();

        let args = extra_jvm_args(&state);
        assert_eq!(args, vec!["-XX:-AlwaysPreTouch".to_string()], "{args:?}");
    }

    #[test]
    fn the_arguments_of_the_player_come_after_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let mut config = state.storage().load_settings().unwrap();
        config.jvm.flags_profile = ml_core::FlagsProfile::Measured;
        // The player writes the flag out by hand, against the profile.
        config.jvm.extra_args = vec!["-XX:+AlwaysPreTouch".to_string()];
        state.storage().save_settings(&config).unwrap();

        // The profile switches the flag off, and the player switches it
        // back on: the last of a pair `+Flag` / `-Flag` is the one the
        // virtual machine obeys, and the player is the one who has the
        // last word.
        let args = extra_jvm_args(&state);
        assert_eq!(
            args,
            vec![
                "-XX:-AlwaysPreTouch".to_string(),
                "-XX:+AlwaysPreTouch".to_string()
            ],
            "{args:?}"
        );
    }

    #[test]
    fn two_collectors_are_never_sent_to_the_virtual_machine() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let mut config = state.storage().load_settings().unwrap();
        // The dropdown names one collector and the free-text field
        // another. The virtual machine refuses to start when two
        // different collectors are named at once.
        config.jvm.garbage_collector = "G1".to_string();
        config.jvm.extra_args = vec!["-XX:+UseZGC".to_string()];
        state.storage().save_settings(&config).unwrap();

        let args = extra_jvm_args(&state);
        let collectors: Vec<&String> = args
            .iter()
            .filter(|arg| ml_core::config::is_collector_flag(arg))
            .collect();
        assert_eq!(collectors, vec!["-XX:+UseZGC"], "{args:?}");
    }

    #[test]
    fn the_rules_of_the_restart_follow_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        let mut config = state.storage().load_settings().unwrap();
        config.restart.enabled = false;
        config.restart.max_restarts = 7;
        config.restart.backoff_ms = 1500;
        config.restart.window_ms = 60_000;
        state.storage().save_settings(&config).unwrap();

        let policy = restart_policy(&state);
        assert!(!policy.enabled, "the restart is switched off");
        assert_eq!(policy.max_restarts, 7);
        assert_eq!(policy.backoff, std::time::Duration::from_millis(1500));
        assert_eq!(policy.window, std::time::Duration::from_secs(60));
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
            agent_report: None,
            loader: None,
            mod_jars: Vec::new(),
        };

        // The library has no artifact, so it does not go on the
        // classpath — but the entry point does not panic either.
        assert_eq!(request.classpath_entries().len(), 0);
    }

    #[test]
    fn the_runtime_keeps_the_name_of_its_component() {
        // `java-runtime-25` is not a directory anybody writes: the
        // runtime of Java 25 is `java-runtime-epsilon`. A component made
        // up from the major version never matches what was downloaded.
        let mut meta = ml_core::VersionMeta {
            id: "26.3".into(),
            kind: ml_core::VersionKind::Release,
            release_time: None,
            java_component: Some("java-runtime-epsilon".into()),
            java_major: Some(25),
            main_class: None,
            asset_index_id: None,
            asset_index_sha1: None,
            asset_index_size: None,
            client_sha1: None,
            client_size: None,
            json_sha1: None,
        };

        assert_eq!(runtime_for(&meta).component, "java-runtime-epsilon");
        assert_eq!(runtime_for(&meta).major_version, 25);

        // A version whose document was not read falls back to the table.
        meta.java_component = None;
        meta.java_major = None;
        assert_eq!(runtime_for(&meta).component, "java-runtime-epsilon");
        assert_eq!(runtime_for(&meta).major_version, 25);
    }

    #[test]
    fn a_resolution_is_only_announced_when_it_is_complete() {
        let mut request = LaunchRequest {
            version: ml_minecraft::version::VersionJson::parse(
                br#"{"id":"26.3","type":"release","mainClass":"Main"}"#,
            )
            .unwrap(),
            libraries: Vec::new(),
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
            agent_report: None,
            loader: None,
            mod_jars: Vec::new(),
        };

        assert!(!features_for(&request).is_enabled("has_custom_resolution"));

        request.session.resolution_width = Some(1920);
        assert!(!features_for(&request).is_enabled("has_custom_resolution"));

        request.session.resolution_height = Some(1080);
        assert!(features_for(&request).is_enabled("has_custom_resolution"));
    }

    #[test]
    fn the_scenario_of_a_run_says_what_was_in_the_game() {
        // The whole point of the scenario: a run without the mod and a
        // run with it are two different measurements, and they have to
        // stand side by side instead of overwriting each other.
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = std::sync::Arc::new(crate::state::AppState::new(paths, platform).unwrap());

        // Without the mod in the game.
        assert_eq!(scenario_of_run(&state, false), "in_game/off");

        // With the mod, under the preset of the settings.
        let mut config = state.storage().load_settings().unwrap();
        config.mods.preset = "fast".into();
        state.storage().save_settings(&config).unwrap();

        assert_eq!(scenario_of_run(&state, true), "in_game/fast");
    }

    #[test]
    fn a_mod_the_loader_would_refuse_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("mod.jar");
        jar_with_manifest(&jar, r#"{"depends":{"minecraft":"~1.21.4"}}"#);

        // The mod is built for another version of the game: the loader
        // would refuse it, so it must not go into the game — otherwise the
        // run would be counted as a run of the game with the mod.
        assert!(mod_jar_to_install(&jar, "26.3").is_none());

        // The version the mod is built for takes it.
        assert!(mod_jar_to_install(&jar, "1.21.4").is_some());

        // A jar that is not a mod at all is not a mod either.
        let not_a_mod = dir.path().join("not-a-mod.jar");
        std::fs::write(&not_a_mod, b"not a zip file").unwrap();
        assert!(mod_jar_to_install(&not_a_mod, "1.21.4").is_none());

        // A mod that is not on the disk is not installed either.
        assert!(mod_jar_to_install(&dir.path().join("missing.jar"), "1.21.4").is_none());
    }

    /// A jar with the manifest of a mod in it.
    fn jar_with_manifest(path: &Path, manifest: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, '_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("fabric.mod.json", options).unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
}
