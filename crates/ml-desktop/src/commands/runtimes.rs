//! The Java runtimes the launcher can download.
//!
//! A version of the game needs a Java of a certain feature release, and
//! the launcher can fetch the runtime of Mojang itself, so that a user
//! without a Java installation can still play.
//!
//! The download is reported the same way the installation of a version is
//! reported: the frontend is told what is happening, and never has to
//! guess.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use ml_minecraft::{DownloadProgress, MinecraftService};

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

use super::throttle::Throttle;

/// The event the frontend listens to while a runtime is downloaded.
pub const RUNTIME_EVENT: &str = "minlauncher://runtime";

/// What the launcher tells the frontend while it downloads a runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    /// The launcher started working on a runtime.
    Started {
        /// The runtime being downloaded.
        component: String,
        /// How many files have to be fetched.
        total: usize,
        /// How many bytes have to be fetched.
        bytes: i64,
    },
    /// Some of the files are done.
    Progress {
        /// The runtime being downloaded.
        component: String,
        /// How many files are done.
        completed: usize,
        /// How many files there are.
        total: usize,
        /// How many bytes have been transferred.
        bytes: u64,
    },
    /// The runtime is ready to use.
    Finished {
        /// The runtime that was downloaded.
        component: String,
        /// How many files were fetched.
        files: usize,
        /// How many bytes were transferred.
        bytes: u64,
        /// How long the download took, in milliseconds.
        elapsed_ms: u64,
    },
    /// The download failed.
    Failed {
        /// The runtime that was being downloaded.
        component: String,
        /// What went wrong.
        message: String,
    },
}

/// What the frontend gets after a successful download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeResult {
    /// The runtime that was downloaded.
    pub component: String,
    /// How many files were fetched.
    pub files: usize,
    /// How many bytes were transferred.
    pub bytes: u64,
    /// How long the download took, in milliseconds.
    pub elapsed_ms: u64,
}

/// A runtime the launcher knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeView {
    /// The component of the runtime, e.g. `java-runtime-epsilon`.
    pub component: String,
    /// The version of the runtime, e.g. `21.0.7`.
    pub version: String,
    /// Whether the runtime is on the disk already.
    pub installed: bool,
    /// Where the runtime lives.
    pub path: String,
}

/// List the runtimes Mojang publishes for this platform.
#[tauri::command]
pub async fn list_runtimes(state: State<'_, SharedState>) -> IpcResult<Vec<RuntimeView>> {
    let service = state.service_clone();

    let manifest = service.runtime_manifest().await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot read the runtime manifest",
            error.to_string(),
        )
    })?;

    let platform = ml_minecraft::jre::platform_key();
    let Some(families) = manifest.for_platform(&platform) else {
        return Ok(Vec::new());
    };

    Ok(families
        .iter()
        .filter_map(|(component, entries)| {
            let entry = entries.first()?;
            let home = state.paths().jre_dir_for(component);
            Some(RuntimeView {
                component: (*component).clone(),
                version: entry.version.name.clone(),
                installed: home.join("bin").join(java_executable_name()).is_file(),
                path: home.to_string_lossy().into_owned(),
            })
        })
        .collect())
}

/// The name of the `java` executable on this platform.
fn java_executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Download a Java runtime and unpack it.
///
/// # Errors
/// Fails when the runtime does not exist for this platform, when the
/// network is unreachable, or when the files cannot be written.
#[tauri::command]
pub async fn install_runtime(
    app: AppHandle,
    state: State<'_, SharedState>,
    component: String,
) -> IpcResult<RuntimeResult> {
    let _activity = state.begin_activity()?;
    let service = state.service_clone();

    let result = install(&service, &component, &app).await;

    match result {
        Ok(result) => {
            let event = RuntimeEvent::Finished {
                component: result.component.clone(),
                files: result.files,
                bytes: result.bytes,
                elapsed_ms: result.elapsed_ms,
            };
            let _ = app.emit(RUNTIME_EVENT, &event);
            Ok(result)
        }
        Err(error) => {
            let event = RuntimeEvent::Failed {
                component,
                message: error.message.clone(),
            };
            let _ = app.emit(RUNTIME_EVENT, &event);
            Err(error)
        }
    }
}

/// Do the work of a download.
async fn install(
    service: &MinecraftService,
    component: &str,
    app: &AppHandle,
) -> IpcResult<RuntimeResult> {
    let started_at = Instant::now();

    // The plan is built first, so that the frontend is told how much
    // there is to fetch before the first byte moves.
    let plan = service.runtime_plan(component).await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot read the files of the runtime",
            error.to_string(),
        )
    })?;

    let _ = app.emit(
        RUNTIME_EVENT,
        &RuntimeEvent::Started {
            component: component.to_string(),
            total: plan.files.len(),
            bytes: plan.total_size,
        },
    );

    // The report is thinned out: a runtime holds hundreds of files, and
    // an event per file would cost more than the download.
    let throttle = Arc::new(Throttle::new(RUNTIME_EVENT, Duration::from_millis(100)));
    let emitter = Arc::clone(&throttle);
    let progress_component = component.to_string();
    let handle = app.clone();

    let callback: ml_minecraft::ProgressCallback = Arc::new(move |progress: DownloadProgress| {
        emitter.report(
            &handle,
            RuntimeEvent::Progress {
                component: progress_component.clone(),
                completed: progress.completed,
                total: progress.total,
                bytes: progress.bytes,
            },
        );
    });

    let result = service
        .install_runtime_plan(&plan, callback)
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "cannot download the runtime",
                error.to_string(),
            )
        })?;

    Ok(RuntimeResult {
        component: component.to_string(),
        files: result.completed.len(),
        bytes: result.bytes,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
    })
}
