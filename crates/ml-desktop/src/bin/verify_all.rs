//! Install and start every version of the game, once, and write a report.
//!
//! This is the harness the project uses to check its own promise: every
//! document Mojang publishes has to install and every version has to reach
//! its window. It is not a Tauri application; it opens the real launcher
//! storage, can move the game files to a chosen disk, and drives the same
//! install and launch code the commands use.
//!
//! ```text
//! cargo run -p ml-desktop --bin verify-all -- --root E:\MinLauncher --phase all
//! ```
//!
//! The report is written under `<root>\verify`: `results.jsonl` is
//! appended after every version, so a run that was interrupted resumes
//! instead of starting over, and `report.json`, `report.csv` and
//! `summary.md` are built at the end.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ml_core::{
    Account, AccountType, AppPaths, GamePerformanceProfile, JavaRuntime, LauncherConfig, Platform,
    VersionKind, offline_uuid, paths::sanitize_component,
};
use ml_launch::supervisor::Supervisor;
use ml_minecraft::MinecraftService;
use ml_storage::Storage;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use ml_desktop::commands::launch::build_request_from;
use ml_desktop::commands::versions::{
    InstallEvent, InstallOptions, InstallReporter, cache_manifest, install_version_files,
    store_the_manifest,
};
use ml_desktop::install_location::{self, InstallRootEvent, InstallRootProgress};
use ml_desktop::state::AppState;

/// One gibibyte, for the messages about free space.
const GIB: u64 = 1024 * 1024 * 1024;

/// How many launched versions between two refreshes of the report.
const REFRESH_REPORT_AFTER: usize = 10;

/// The runtimes the corpus of Mojang documents refers to.
const RUNTIMES: &[&str] = &[
    "jre-legacy",
    "java-runtime-alpha",
    "java-runtime-beta",
    "java-runtime-gamma",
    "java-runtime-gamma-snapshot",
    "java-runtime-delta",
    "java-runtime-epsilon",
];

/// How long the game has to keep its window alive to count as started.
const WINDOW_STABLE: Duration = Duration::from_secs(5);

/// How long a process may live without a window before its log is trusted.
/// The window is the promise, so the log only speaks once a real window had
/// every chance to appear: the newest versions need about half a minute of
/// booting before LWJGL opens one.
const LOG_FALLBACK_AFTER: Duration = Duration::from_secs(60);

/// The phase of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Install,
    Launch,
    All,
}

/// The command line of the harness.
#[derive(Debug, Clone)]
struct Args {
    root: Option<String>,
    phase: Phase,
    only: Vec<String>,
    limit: Option<usize>,
    launch_workers: usize,
    timeout: Duration,
    retry: u32,
    heap: String,
    require_window: bool,
    force: bool,
    retry_failed: bool,
    report: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            root: None,
            phase: Phase::All,
            only: Vec::new(),
            limit: None,
            launch_workers: 2,
            timeout: Duration::from_secs(120),
            retry: 1,
            heap: "2G".to_string(),
            require_window: false,
            force: false,
            retry_failed: false,
            report: None,
        }
    }
}

/// One version, as the report knows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionResult {
    id: String,
    kind: String,
    java_component: Option<String>,
    java_major: Option<u32>,
    install_ok: bool,
    install_files: usize,
    install_bytes: u64,
    install_ms: u64,
    #[serde(default)]
    install_error: Option<String>,
    launch_ok: Option<bool>,
    #[serde(default)]
    launch_ms: Option<u64>,
    #[serde(default)]
    window_title: Option<String>,
    #[serde(default)]
    java: Option<String>,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    launch_error: Option<String>,
    #[serde(default)]
    log_path: Option<String>,
    #[serde(default)]
    note: Option<String>,
    checked_at: String,
}

impl VersionResult {
    fn new(id: &str, kind: &str) -> Self {
        Self {
            id: id.to_string(),
            kind: kind.to_string(),
            java_component: None,
            java_major: None,
            install_ok: false,
            install_files: 0,
            install_bytes: 0,
            install_ms: 0,
            install_error: None,
            launch_ok: None,
            launch_ms: None,
            window_title: None,
            java: None,
            attempts: 0,
            exit_code: None,
            launch_error: None,
            log_path: None,
            note: None,
            checked_at: now(),
        }
    }

    fn failed(&self) -> bool {
        !self.install_ok || self.launch_ok == Some(false)
    }
}

/// The collector and writer of the results.
struct Reporter {
    directory: PathBuf,
    results: Mutex<BTreeMap<String, VersionResult>>,
}

impl Reporter {
    fn open(directory: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let mut results = BTreeMap::new();
        let path = directory.join("results.jsonl");
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                match serde_json::from_str::<VersionResult>(line) {
                    Ok(record) => {
                        results.insert(record.id.clone(), record);
                    }
                    Err(error) => eprintln!("verify-all: a result line is not readable: {error}"),
                }
            }
        }

        Ok(Self {
            directory,
            results: Mutex::new(results),
        })
    }

    fn get(&self, id: &str) -> Option<VersionResult> {
        self.lock().get(id).cloned()
    }

    fn record(&self, record: VersionResult) -> Result<()> {
        let path = self.directory.join("results.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        let line = serde_json::to_string(&record)?;
        writeln!(file, "{line}")?;
        file.flush()?;

        self.lock().insert(record.id.clone(), record);
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, VersionResult>> {
        self.results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write_reports(&self, root: &Path) -> Result<Summary> {
        // The report is cumulative: a retry of four versions must not
        // erase the results of the other versions of the same root.
        let versions: Vec<VersionResult> = self.lock().values().cloned().collect();

        let mut by_kind: BTreeMap<String, KindStats> = BTreeMap::new();
        for record in &versions {
            let stats = by_kind.entry(record.kind.clone()).or_default();
            stats.total += 1;
            stats.install_ok += usize::from(record.install_ok);
            stats.launch_ok += usize::from(record.launch_ok == Some(true));
            stats.launch_checked += usize::from(record.launch_ok.is_some());
            stats.failed += usize::from(record.failed());
        }
        let failures: Vec<&VersionResult> =
            versions.iter().filter(|record| record.failed()).collect();
        let install_ok = versions.iter().filter(|record| record.install_ok).count();
        let launch_checked = versions
            .iter()
            .filter(|record| record.launch_ok.is_some())
            .count();
        let launch_ok = versions
            .iter()
            .filter(|record| record.launch_ok == Some(true))
            .count();
        let summary = Summary {
            generated_at: now(),
            root: root.to_string_lossy().into_owned(),
            total: versions.len(),
            install_ok,
            launch_checked,
            launch_ok,
            failed: failures.len(),
            by_kind,
            failures: failures.iter().map(|record| Failure::of(record)).collect(),
        };

        let report = Report {
            summary: summary.clone(),
            versions: versions.clone(),
        };
        write_json(&self.directory.join("report.json"), &report)?;
        write_csv(&self.directory.join("report.csv"), &versions)?;
        write_summary(&self.directory.join("summary.md"), &summary)?;
        Ok(summary)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
struct KindStats {
    total: usize,
    install_ok: usize,
    launch_checked: usize,
    launch_ok: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Failure {
    id: String,
    kind: String,
    install_error: Option<String>,
    launch_error: Option<String>,
    note: Option<String>,
}

impl Failure {
    fn of(record: &VersionResult) -> Self {
        Self {
            id: record.id.clone(),
            kind: record.kind.clone(),
            install_error: record.install_error.clone(),
            launch_error: record.launch_error.clone(),
            note: record.note.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Summary {
    generated_at: String,
    root: String,
    total: usize,
    install_ok: usize,
    launch_checked: usize,
    launch_ok: usize,
    failed: usize,
    by_kind: BTreeMap<String, KindStats>,
    failures: Vec<Failure>,
}

#[derive(Debug, Serialize)]
struct Report {
    summary: Summary,
    versions: Vec<VersionResult>,
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .try_init();

    let args = match Args::parse() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("verify-all: {error}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(error) = run(args).await {
        eprintln!("verify-all: {error:#}");
        std::process::exit(2);
    }
}

#[allow(clippy::too_many_lines)]
async fn run(args: Args) -> Result<()> {
    check_launcher_is_not_running()?;

    let platform = Platform::current();
    let state = Arc::new(AppState::discover(platform)?);

    if let Some(root) = args.root.as_deref().filter(|root| !root.trim().is_empty()) {
        let current = state.paths();
        if !same_directory(root, current.install_dir.as_deref()) {
            switch_root(&state, root).await?;
        }
    }

    let paths = state.paths();
    let mut service = state.service_clone();
    check_free_space(&paths)?;

    println!("verify-all: fetching the manifest of Mojang");
    let manifest = service
        .manifest()
        .await
        .context("cannot fetch the version manifest")?;
    store_the_manifest(state.storage(), &manifest).context("cannot store the manifest")?;
    cache_manifest(&paths, &manifest).context("cannot cache the manifest")?;

    let _ = state
        .storage()
        .meta_set("latest_release", &manifest.latest_release);
    let _ = state
        .storage()
        .meta_set("latest_snapshot", &manifest.latest_snapshot);

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for version in &manifest.versions {
        *counts.entry(version.kind.as_manifest_str()).or_default() += 1;
    }
    println!(
        "verify-all: {} versions in the manifest: {:?}",
        manifest.versions.len(),
        counts
    );

    let mut selected: Vec<(String, VersionKind)> = manifest
        .versions
        .iter()
        .filter(|version| args.only.is_empty() || args.only.iter().any(|id| id == &version.id))
        .map(|version| (version.id.clone(), version.kind))
        .collect();
    if selected.is_empty() {
        bail!("no versions match --only");
    }
    if let Some(limit) = args.limit {
        selected.truncate(limit);
    }

    let root = paths
        .install_dir
        .clone()
        .unwrap_or_else(|| paths.data_dir.clone());
    let report_directory = args.report.clone().unwrap_or_else(|| root.join("verify"));
    let reporter = Arc::new(Reporter::open(report_directory.clone())?);

    // A run that has already checked a version does not check it again,
    // unless it is asked to. The report still knows about all the
    // versions that were selected, not only the ones this run touched.
    let process: Vec<(String, VersionKind)> = selected
        .iter()
        .filter(|(id, _)| {
            if args.force {
                return true;
            }
            let record = reporter.get(id);
            if args.retry_failed {
                return record.is_none_or(|record| record.failed() || record.launch_ok.is_none());
            }
            record.is_none_or(|record| record.launch_ok != Some(true) || !record.install_ok)
        })
        .cloned()
        .collect();

    if process.is_empty() {
        println!("verify-all: nothing to do: every selected version is already checked");
    } else {
        println!("verify-all: {} versions to process", process.len());
    }

    for (id, kind) in &process {
        if reporter.get(id).is_none() {
            reporter.record(VersionResult::new(id, kind.as_manifest_str()))?;
        }
    }

    install_runtimes(&service, state.storage(), &paths).await?;

    if args.phase != Phase::Launch {
        install_versions(
            &mut service,
            state.storage(),
            &paths,
            &process,
            &reporter,
            &args,
        )
        .await?;
    }

    if args.phase != Phase::Install {
        launch_versions(
            state.storage_handle(),
            &paths,
            Platform::current(),
            &process,
            &reporter,
            &args,
        )
        .await?;
    }

    let summary = reporter.write_reports(&root)?;
    print_summary(&summary, &report_directory);
    if summary.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

async fn switch_root(state: &AppState, root: &str) -> Result<()> {
    println!("verify-all: moving the game files to {root}");
    let old_paths = state.paths();
    let storage = state.storage_handle();
    let progress: InstallRootProgress = Arc::new(|event| {
        if let InstallRootEvent::Copying {
            copied_bytes,
            total_bytes,
        } = event
        {
            let percent = copied_bytes
                .saturating_mul(100)
                .checked_div(total_bytes)
                .unwrap_or(100);
            print!("\r  copying: {percent}%");
            let _ = std::io::stdout().flush();
        }
    });

    let root = root.to_string();
    let prepared = tokio::task::spawn_blocking(move || {
        install_location::prepare_transition(storage, &old_paths, &root, &progress)
    })
    .await
    .context("the copy task panicked")??;
    println!();

    let mut config = state.storage().load_settings()?;
    config.install_dir = prepared
        .paths
        .install_dir
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    state.storage().save_settings(&config)?;

    let report = state.apply_transition(prepared.paths, prepared.service)?;
    println!(
        "verify-all: the root is in use; {} installed versions were found",
        report.installed
    );
    Ok(())
}

async fn install_runtimes(
    service: &MinecraftService,
    storage: &Storage,
    paths: &AppPaths,
) -> Result<()> {
    for component in RUNTIMES {
        let java = paths
            .jre_dir_for(component)
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        if java.is_file() {
            println!("verify-all: runtime {component} is already installed");
            continue;
        }

        let mut attempt = 0;
        loop {
            match service
                .install_runtime_with_progress(component, Arc::new(|_| {}))
                .await
            {
                Ok(result) => {
                    println!(
                        "verify-all: runtime {component}: {} files, {} MiB",
                        result.completed.len(),
                        result.bytes / (1024 * 1024)
                    );
                    break;
                }
                Err(error) if attempt < 1 => {
                    attempt += 1;
                    eprintln!("verify-all: runtime {component} failed, retrying: {error}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(error) => {
                    eprintln!("verify-all: runtime {component} failed: {error}");
                    break;
                }
            }
        }
        let _ = storage.downloads().clear_completed();
        let _ = storage.downloads().clear_failed();
    }
    Ok(())
}

async fn install_versions(
    service: &mut MinecraftService,
    storage: &Storage,
    paths: &AppPaths,
    versions: &[(String, VersionKind)],
    reporter: &Arc<Reporter>,
    args: &Args,
) -> Result<()> {
    let total = versions.len();
    for (index, (id, kind)) in versions.iter().enumerate() {
        let mut record = reporter
            .get(id)
            .unwrap_or_else(|| VersionResult::new(id, kind.as_manifest_str()));
        let installed = storage.versions().is_installed(id).unwrap_or(false);
        if record.install_ok && installed && !args.force {
            println!("[{}/{total}] install {id}: already installed", index + 1);
            continue;
        }
        record.install_ok = false;
        record.kind = kind.as_manifest_str().to_string();

        println!("[{}/{total}] install {id} …", index + 1);
        let mut attempt = 0;
        loop {
            let progress = ProgressPrinter::new(id);
            let reporter_cb: InstallReporter = Arc::new(move |event| progress.event(event));
            let options = InstallOptions {
                version_id: id.clone(),
                allow_network: true,
            };
            match install_version_files(service, storage, paths, &options, &reporter_cb).await {
                Ok(result) => {
                    record.install_ok = true;
                    record.install_files = result.files;
                    record.install_bytes = result.bytes;
                    record.install_ms = result.elapsed_ms;
                    record.install_error = None;
                    break;
                }
                Err(error) if attempt < args.retry => {
                    attempt += 1;
                    record.install_error = Some(error.to_string());
                    eprintln!("  install {id} failed, retrying: {error}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(error) => {
                    record.install_error = Some(error.to_string());
                    break;
                }
            }
        }

        if let Ok(Some(meta)) = storage.versions().get(id) {
            record.java_component = meta.java_component;
            record.java_major = meta.java_major;
        }
        record.checked_at = now();
        reporter.record(record.clone())?;
        let _ = storage.downloads().clear_completed();
        let _ = storage.downloads().clear_failed();

        if record.install_ok {
            println!(
                "  install {id}: ok, {} files, {:.1} MiB, {} ms",
                record.install_files,
                record.install_bytes as f64 / (1024.0 * 1024.0),
                record.install_ms
            );
        } else {
            eprintln!(
                "  install {id}: FAILED: {}",
                record.install_error.as_deref().unwrap_or("unknown error")
            );
        }
    }
    Ok(())
}

/// A small printer for the progress of one download.
struct ProgressPrinter {
    version: String,
    last: Mutex<Instant>,
}

impl ProgressPrinter {
    fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            last: Mutex::new(Instant::now()),
        }
    }

    fn event(&self, event: InstallEvent) {
        if let InstallEvent::Progress {
            completed,
            total,
            bytes,
            ..
        } = event
        {
            let mut last = self
                .last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.elapsed() < Duration::from_secs(3) {
                return;
            }
            *last = Instant::now();
            print!(
                "\r  {}: {completed}/{total} files, {:.1} MiB",
                self.version,
                bytes as f64 / (1024.0 * 1024.0)
            );
            let _ = std::io::stdout().flush();
        }
    }
}

/// What everything one launch needs is shared through.
#[derive(Clone)]
struct LaunchContext {
    storage: Arc<Storage>,
    paths: AppPaths,
    platform: Platform,
    config: LauncherConfig,
    account: Account,
    root: PathBuf,
    timeout: Duration,
    retry: u32,
    require_window: bool,
}

async fn launch_versions(
    storage: Arc<Storage>,
    paths: &AppPaths,
    platform: Platform,
    versions: &[(String, VersionKind)],
    reporter: &Arc<Reporter>,
    args: &Args,
) -> Result<()> {
    let mut config = LauncherConfig::default();
    config.jvm.enable_agent = false;
    config.jvm.max_heap = args.heap.clone();
    config.optimization.profile = GamePerformanceProfile::Original;
    let account = Account::new(
        "verify-offline",
        offline_uuid("Verifier"),
        "Verifier",
        AccountType::Offline,
    );
    let root = paths
        .install_dir
        .clone()
        .unwrap_or_else(|| paths.data_dir.clone())
        .join("verify");
    let context = Arc::new(LaunchContext {
        storage: Arc::clone(&storage),
        paths: paths.clone(),
        platform,
        config,
        account,
        root,
        timeout: args.timeout,
        retry: args.retry,
        require_window: args.require_window,
    });

    let workers = args.launch_workers.clamp(1, 4);
    let semaphore = Arc::new(Semaphore::new(workers));
    let total = versions.len();
    let done = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for (index, (id, kind)) in versions.iter().enumerate() {
        let mut record = reporter
            .get(id)
            .unwrap_or_else(|| VersionResult::new(id, kind.as_manifest_str()));
        if record.launch_ok == Some(true) && !args.force {
            println!("[{}/{total}] launch {id}: already checked", index + 1);
            continue;
        }
        if !record.install_ok {
            if storage.versions().is_installed(id).unwrap_or(false) {
                record.install_ok = true;
            } else {
                record.launch_ok = Some(false);
                record.launch_error = Some("the version is not installed".to_string());
                record.checked_at = now();
                reporter.record(record)?;
                println!("[{}/{total}] launch {id}: NOT INSTALLED", index + 1);
                continue;
            }
        }

        let permit = Arc::clone(&semaphore).acquire_owned().await?;
        let context = Arc::clone(&context);
        let reporter = Arc::clone(reporter);
        let done = Arc::clone(&done);
        let id = id.clone();
        let kind = kind.as_manifest_str().to_string();
        handles.push((
            index,
            id.clone(),
            tokio::spawn(async move {
                let _permit = permit;
                let record = launch_one(&context, id.clone(), kind, record).await;
                // Every result is stored the moment its version finishes: the
                // run must be resumable even if it is interrupted halfway.
                if let Err(error) = reporter.record(record.clone()) {
                    eprintln!("verify-all: cannot save the result of {id}: {error}");
                }
                print_launch(index, total, &record);
                if done
                    .fetch_add(1, Ordering::SeqCst)
                    .is_multiple_of(REFRESH_REPORT_AFTER)
                    && let Err(error) = reporter.write_reports(&context.root)
                {
                    eprintln!("verify-all: cannot refresh the report: {error}");
                }
                record
            }),
        ));
    }

    for (index, id, handle) in handles {
        if let Err(error) = handle.await {
            let mut record = reporter.get(&id).unwrap_or_else(|| {
                VersionResult::new(&id, VersionKind::Snapshot.as_manifest_str())
            });
            record.launch_ok = Some(false);
            record.launch_error = Some(format!("the launch task panicked: {error}"));
            record.checked_at = now();
            reporter.record(record)?;
            eprintln!(
                "[{}/{total}] launch {id}: the task panicked: {error}",
                index + 1
            );
        }
    }
    Ok(())
}

fn print_launch(index: usize, total: usize, record: &VersionResult) {
    let ok = record.launch_ok == Some(true);
    println!(
        "[{}/{total}] launch {}: {} ({} ms{})",
        index + 1,
        record.id,
        if ok { "ok" } else { "FAILED" },
        record.launch_ms.unwrap_or_default(),
        record
            .window_title
            .as_ref()
            .map(|title| format!(", «{title}»"))
            .unwrap_or_default()
    );
}

#[allow(clippy::too_many_lines)]
async fn launch_one(
    context: &LaunchContext,
    id: String,
    kind: String,
    mut record: VersionResult,
) -> VersionResult {
    let started = Instant::now();
    let meta = match context.storage.versions().get(&id) {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            record.launch_ok = Some(false);
            record.launch_error = Some("the version has no database record".to_string());
            record.checked_at = now();
            return record;
        }
        Err(error) => {
            record.launch_ok = Some(false);
            record.launch_error = Some(error.to_string());
            record.checked_at = now();
            return record;
        }
    };

    let options = ml_desktop::commands::launch::LaunchOptions {
        version_id: id.clone(),
        instance_id: None,
        width: Some(854),
        height: Some(480),
        java_path: None,
    };
    let request = match build_request_from(
        &context.storage,
        &context.paths,
        context.platform,
        &context.config,
        &context.account,
        &meta,
        &options,
    ) {
        Ok(request) => request,
        Err(error) => {
            record.launch_ok = Some(false);
            record.launch_error = Some(error.to_string());
            record.checked_at = now();
            return record;
        }
    };

    record.java = Some(format!(
        "{} ({:?})",
        request.java.version, request.java.origin
    ));
    record.java_component = meta
        .java_component
        .clone()
        .or_else(|| Some(JavaRuntime::fallback_for(&id).component));
    record.java_major = meta
        .java_major
        .or_else(|| Some(JavaRuntime::fallback_for(&id).major_version));

    let game_directory = context.root.join("games").join(sanitize_component(&id));
    if let Err(error) = std::fs::create_dir_all(&game_directory) {
        record.launch_ok = Some(false);
        record.launch_error = Some(format!(
            "cannot create {}: {error}",
            game_directory.display()
        ));
        record.checked_at = now();
        return record;
    }
    let mut request = request;
    request.session.game_directory = game_directory.clone();
    let log_directory = context.root.join("logs");
    if let Err(error) = std::fs::create_dir_all(&log_directory) {
        record.launch_ok = Some(false);
        record.launch_error = Some(format!(
            "cannot create {}: {error}",
            log_directory.display()
        ));
        record.checked_at = now();
        return record;
    }
    let log_path = log_directory.join(format!("{}.log", sanitize_component(&id)));
    record.log_path = Some(log_path.to_string_lossy().into_owned());

    let features = ml_core::FeatureSet::of(["has_custom_resolution"]);
    let mut last_error = None;
    for attempt in 0..=context.retry {
        record.attempts = attempt + 1;
        match launch_once(
            &request,
            context.platform,
            meta.kind.as_manifest_str(),
            &features,
            &game_directory,
            &log_path,
            context.timeout,
            context.require_window,
        )
        .await
        {
            Ok(outcome) => {
                record.launch_ok = Some(outcome.ok);
                record.window_title = outcome.window_title;
                record.exit_code = outcome.exit_code;
                record.note = outcome.note;
                record.launch_error = outcome.error;
                if outcome.ok {
                    break;
                }
                last_error = record.launch_error.clone();
            }
            Err(error) => {
                record.launch_ok = Some(false);
                record.launch_error = Some(error.to_string());
                last_error = Some(error.to_string());
            }
        }
        if attempt < context.retry {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    if record.launch_ok != Some(true) && record.launch_error.is_none() {
        record.launch_error = last_error;
    }
    record.kind = kind;
    record.launch_ms = Some(started.elapsed().as_millis() as u64);
    record.checked_at = now();
    record
}

struct LaunchOutcome {
    ok: bool,
    window_title: Option<String>,
    exit_code: Option<i32>,
    note: Option<String>,
    error: Option<String>,
}

#[allow(clippy::too_many_arguments)] // Manual harness.
async fn launch_once(
    request: &ml_launch::command::LaunchRequest,
    platform: Platform,
    version_type: &str,
    features: &ml_core::FeatureSet,
    game_directory: &Path,
    log_path: &Path,
    timeout: Duration,
    require_window: bool,
) -> Result<LaunchOutcome> {
    let prepared = ml_launch::prepare(request, platform, features, version_type)?;
    let supervisor = Supervisor::new(prepared.command).with_log(log_path.to_path_buf());
    let mut process = supervisor.start()?;
    let pid = process.id().context("the game has no process id")?;
    let latest_log = game_directory.join("logs").join("latest.log");
    let started = Instant::now();
    let mut window_seen: Option<(String, Instant)> = None;

    loop {
        if !process.is_running() {
            let status = process.wait().await?;
            let error = format!(
                "the game exited before its window appeared (code {:?}){}",
                status.code,
                tail_of(&[log_path, &latest_log])
                    .map(|tail| format!("\n{tail}"))
                    .unwrap_or_default()
            );
            return Ok(LaunchOutcome {
                ok: false,
                window_title: None,
                exit_code: status.code,
                note: None,
                error: Some(error),
            });
        }

        if window_seen.is_none()
            && let Some(title) = window_title(pid)
        {
            window_seen = Some((title, Instant::now()));
        }
        if let Some((title, at)) = &window_seen
            && at.elapsed() >= WINDOW_STABLE
        {
            stop_process(pid).await;
            let _ = tokio::time::timeout(Duration::from_secs(10), process.wait()).await;
            return Ok(LaunchOutcome {
                ok: true,
                window_title: Some(title.clone()),
                exit_code: None,
                note: None,
                error: None,
            });
        }

        if window_seen.is_none()
            && !require_window
            && started.elapsed() >= LOG_FALLBACK_AFTER
            && let Some(marker) = started_marker(&[log_path, &latest_log])
        {
            stop_process(pid).await;
            let _ = tokio::time::timeout(Duration::from_secs(10), process.wait()).await;
            return Ok(LaunchOutcome {
                ok: true,
                window_title: None,
                exit_code: None,
                note: Some(format!(
                    "the window was not found; the log of the game says «{marker}»"
                )),
                error: None,
            });
        }

        if started.elapsed() >= timeout {
            stop_process(pid).await;
            let _ = tokio::time::timeout(Duration::from_secs(10), process.wait()).await;
            return Ok(LaunchOutcome {
                ok: false,
                window_title: window_seen.map(|(title, _)| title),
                exit_code: None,
                note: None,
                error: Some(format!(
                    "the window did not appear in {} s{}",
                    timeout.as_secs(),
                    tail_of(&[log_path, &latest_log])
                        .map(|tail| format!("\n{tail}"))
                        .unwrap_or_default()
                )),
            });
        }

        // The window appears within a second of the log going quiet, so the
        // title is read as often as it is cheap to read.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn window_title(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let output = std::process::Command::new("tasklist")
            .args(["/V", "/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let fields = csv_fields(line);
            let title = fields.last()?.trim();
            let lower = title.to_lowercase();
            // The classic versions of the game title their window "Game";
            // everything from the first releases titles it "Minecraft".
            if lower.contains("minecraft") || lower == "game" {
                return Some(title.to_string());
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

async fn stop_process(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// The line of the log that says the game booted, or nothing.
///
/// Only used when the window of the game cannot be read: a marker of the
/// render layer means a window was certainly created, the earlier markers
/// mean the game at least reached its own start-up.
fn started_marker(paths: &[&Path]) -> Option<&'static str> {
    const MARKERS: &[&str] = &[
        // The classic versions print their frame rate and never load
        // LWJGL through the modern launcher.
        " fps",
        "lwjgl",
        "created:",
        "started the game",
        "setting user",
        "loading minecraft",
    ];
    for path in paths {
        let Some(text) = read_last(path, 64 * 1024) else {
            continue;
        };
        let text = text.to_lowercase();
        if let Some(marker) = MARKERS.iter().find(|marker| text.contains(**marker)) {
            return Some(marker);
        }
    }
    None
}

fn tail_of(paths: &[&Path]) -> Option<String> {
    let mut parts = Vec::new();
    for path in paths {
        if let Some(tail) = read_last(path, 8192)
            && !tail.trim().is_empty()
        {
            parts.push(format!("{}:\n{}", path.display(), tail.trim()));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn read_last(path: &Path, bytes: usize) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let start = data.len().saturating_sub(bytes);
    Some(String::from_utf8_lossy(&data[start..]).into_owned())
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.trim().chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(ch),
        }
    }
    fields.push(field);
    fields
}

fn check_launcher_is_not_running() -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq minlaucher.exe", "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("cannot ask the system for the launcher process")?;
        let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if text.contains("minlaucher.exe") {
            bail!("the launcher is running; close it before the verification");
        }
    }
    Ok(())
}

fn check_free_space(paths: &AppPaths) -> Result<()> {
    let root = paths
        .install_dir
        .clone()
        .unwrap_or_else(|| paths.data_dir.clone());
    let available = fs2::available_space(&root)
        .with_context(|| format!("cannot read the free space of {}", root.display()))?;
    if available < 5 * GIB {
        bail!(
            "there is only {:.1} GiB free on {}; the verification needs more",
            available as f64 / GIB as f64,
            root.display()
        );
    }
    if available < 40 * GIB {
        println!(
            "verify-all: warning: only {:.1} GiB free on {}; the full installation may not fit",
            available as f64 / GIB as f64,
            root.display()
        );
    }
    Ok(())
}

fn same_directory(requested: &str, current: Option<&Path>) -> bool {
    let Some(current) = current else {
        return false;
    };
    normalize(requested) == normalize(&current.to_string_lossy())
}

fn normalize(value: &str) -> String {
    let value = value.trim().trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        value.replace('/', "\\").to_lowercase()
    } else {
        value.to_string()
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_csv(path: &Path, versions: &[VersionResult]) -> Result<()> {
    let mut text = String::from(
        "id,kind,install_ok,files,bytes,install_ms,install_error,launch_ok,launch_ms,window_title,java,attempts,exit_code,launch_error,log_path\n",
    );
    for record in versions {
        let fields = [
            record.id.clone(),
            record.kind.clone(),
            record.install_ok.to_string(),
            record.install_files.to_string(),
            record.install_bytes.to_string(),
            record.install_ms.to_string(),
            record.install_error.clone().unwrap_or_default(),
            record
                .launch_ok
                .map(|value| value.to_string())
                .unwrap_or_default(),
            record
                .launch_ms
                .map(|value| value.to_string())
                .unwrap_or_default(),
            record.window_title.clone().unwrap_or_default(),
            record.java.clone().unwrap_or_default(),
            record.attempts.to_string(),
            record
                .exit_code
                .map(|value| value.to_string())
                .unwrap_or_default(),
            record.launch_error.clone().unwrap_or_default(),
            record.log_path.clone().unwrap_or_default(),
        ];
        let fields: Vec<String> = fields.iter().map(|field| csv_escape(field)).collect();
        text.push_str(&fields.join(","));
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!(
            "\"{}\"",
            value.replace('"', "\"\"").replace(['\n', '\r'], " ")
        )
    } else {
        value.to_string()
    }
}

#[allow(clippy::format_push_string)]
fn write_summary(path: &Path, summary: &Summary) -> Result<()> {
    let mut text = String::new();
    text.push_str("# Проверка всех версий Minecraft\n\n");
    text.push_str(&format!("- Когда: {}\n", summary.generated_at));
    text.push_str(&format!("- Папка установки: `{}`\n", summary.root));
    text.push_str(&format!("- Версий проверено: {}\n", summary.total));
    text.push_str(&format!("- Установлено: {}\n", summary.install_ok));
    text.push_str(&format!(
        "- Запуск проверен: {} (успешно {})\n",
        summary.launch_checked, summary.launch_ok
    ));
    text.push_str(&format!("- Проблем: {}\n\n", summary.failed));

    text.push_str("## По типам\n\n");
    text.push_str("| Тип | Всего | Установлено | Запуск проверен | Запустилось | Проблем |\n");
    text.push_str("|---|---:|---:|---:|---:|---:|\n");
    for (kind, stats) in &summary.by_kind {
        text.push_str(&format!(
            "| {kind} | {} | {} | {} | {} | {} |\n",
            stats.total, stats.install_ok, stats.launch_checked, stats.launch_ok, stats.failed
        ));
    }

    if !summary.failures.is_empty() {
        text.push_str("\n## Проблемные версии\n\n");
        for failure in &summary.failures {
            text.push_str(&format!("### `{}` ({})\n\n", failure.id, failure.kind));
            if let Some(error) = &failure.install_error {
                text.push_str(&format!("- Установка: {error}\n"));
            }
            if let Some(error) = &failure.launch_error {
                text.push_str(&format!("- Запуск: {error}\n"));
            }
            if let Some(note) = &failure.note {
                text.push_str(&format!("- Примечание: {note}\n"));
            }
            text.push('\n');
        }
    }

    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn print_summary(summary: &Summary, directory: &Path) {
    println!();
    println!("verify-all: done");
    println!(
        "  versions: {}, installed: {}, launch checked: {}, launched: {}, problems: {}",
        summary.total,
        summary.install_ok,
        summary.launch_checked,
        summary.launch_ok,
        summary.failed
    );
    for (kind, stats) in &summary.by_kind {
        println!(
            "  {kind}: {}/{} installed, {}/{} launched, {} failed",
            stats.install_ok, stats.total, stats.launch_ok, stats.launch_checked, stats.failed
        );
    }
    if !summary.failures.is_empty() {
        println!("  problems:");
        for failure in summary.failures.iter().take(30) {
            let reason = failure
                .install_error
                .as_deref()
                .or(failure.launch_error.as_deref())
                .unwrap_or("unknown");
            println!("    {}: {}", failure.id, first_line(reason));
        }
        if summary.failures.len() > 30 {
            println!("    … and {} more", summary.failures.len() - 30);
        }
    }
    println!("  report: {}", directory.display());
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

const USAGE: &str = "\
usage: verify-all [options]

  --root <DIR>          use DIR as the installation folder (game files are copied there)
  --phase <PHASE>       install, launch, or all (default: all)
  --only <IDS>          comma-separated version ids
  --limit <N>           stop after N selected versions
  --launch-workers <N>  parallel game windows (1–4, default: 2)
  --timeout-secs <N>    how long to wait for the window (default: 120)
  --retry <N>           retries after a failure (default: 1)
  --heap <SIZE>         heap per verified game, e.g. 2G (default: 2G)
  --require-window      never trust the log: only a real window counts
  --force               check again even when a result already says ok
  --retry-failed        only the versions that are not fully ok
  --report <DIR>        where to write the report (default: <root>/verify)
  --help                this text";

impl Args {
    fn parse() -> Result<Self> {
        let mut args = Args::default();
        let mut arguments = std::env::args().skip(1);
        while let Some(arg) = arguments.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                "--root" => args.root = Some(value(&mut arguments, "--root")?),
                "--phase" => {
                    args.phase = match value(&mut arguments, "--phase")?.as_str() {
                        "install" => Phase::Install,
                        "launch" => Phase::Launch,
                        "all" => Phase::All,
                        other => bail!("unknown phase `{other}`"),
                    };
                }
                "--only" => {
                    args.only = value(&mut arguments, "--only")?
                        .split(',')
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                "--limit" => args.limit = Some(number(&mut arguments, "--limit")? as usize),
                "--launch-workers" => {
                    args.launch_workers = number(&mut arguments, "--launch-workers")? as usize;
                }
                "--timeout-secs" => {
                    args.timeout = Duration::from_secs(number(&mut arguments, "--timeout-secs")?);
                }
                "--retry" => args.retry = number(&mut arguments, "--retry")? as u32,
                "--heap" => {
                    let heap = value(&mut arguments, "--heap")?;
                    if heap.is_empty() || !heap.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                        bail!("--heap must be a size such as 2G or 512M");
                    }
                    args.heap = heap;
                }
                "--force" => args.force = true,
                "--retry-failed" => args.retry_failed = true,
                "--require-window" => args.require_window = true,
                "--report" => args.report = Some(PathBuf::from(value(&mut arguments, "--report")?)),
                other => bail!("unknown option `{other}`"),
            }
        }

        if args.launch_workers == 0 {
            bail!("--launch-workers must be at least 1");
        }
        Ok(args)
    }
}

fn value(argv: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    argv.next()
        .with_context(|| format!("{option} needs a value"))
}

fn number(argv: &mut impl Iterator<Item = String>, option: &str) -> Result<u64> {
    let raw = value(argv, option)?;
    raw.parse()
        .with_context(|| format!("{option} needs a number, not `{raw}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_csv_of_tasklist_is_read_to_its_last_field() {
        let line = r#""javaw.exe","1234","Console","1","1 234 КБ","Running","user","0:00:05","Minecraft 1.20.1""#;
        let fields = csv_fields(line);
        assert_eq!(fields.last().map(String::as_str), Some("Minecraft 1.20.1"));
    }

    #[test]
    fn a_comma_in_the_window_title_stays_in_the_title() {
        let line = r#""javaw.exe","1","Console","1","1","Running","u","0:00","Minecraft, 1.20.1""#;
        assert_eq!(
            csv_fields(line).last().map(String::as_str),
            Some("Minecraft, 1.20.1")
        );
    }

    #[test]
    fn the_same_root_is_recognised() {
        if cfg!(windows) {
            assert!(same_directory(
                "E:/MinLauncher",
                Some(Path::new("E:\\MinLauncher"))
            ));
            assert!(!same_directory(
                "E:/MinLauncher2",
                Some(Path::new("E:\\MinLauncher"))
            ));
        }
        assert!(same_directory("/tmp/game", Some(Path::new("/tmp/game"))));
    }

    #[test]
    fn the_report_keeps_the_kind_of_every_version() {
        let record = VersionResult::new("26.3", "release");
        assert_eq!(record.kind, "release");
        assert!(!record.install_ok);
        assert!(record.failed());
    }
}
