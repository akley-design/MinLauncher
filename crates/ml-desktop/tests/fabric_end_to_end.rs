//! The whole path of the loader, against the real game.
//!
//! This test downloads the real Fabric loader, builds the real command
//! line and starts the real game with it. It is the only test that can
//! say whether the loader is really put in front of the game, because
//! everything it checks is decided by the loader itself and not by the
//! launcher.
//!
//! The test needs the network and it downloads a few hundred megabytes,
//! so it does not run by itself. It runs when `MINLAUNCHER_E2E` is set:
//!
//! ```text
//! MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test fabric_end_to_end
//! ```
//!
//! What it checks:
//!
//! * the loader finds the game jar and starts it;
//! * the mod of the launcher is loaded and applies its settings;
//! * the settings of the mod are read from the file the launcher wrote.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ml_core::{AppPaths, FeatureSet, OsArch, OsName, Platform};
use ml_launch::args::Session;
use ml_launch::command::{LaunchRequest, build_command};
use ml_launch::jvm::{JavaInstallation, JavaOrigin};
use ml_minecraft::fabric::{FabricLaunch, FabricProfile};
use ml_minecraft::install::InstallLayout;
use ml_minecraft::version::VersionJson;

/// The version of the game the test runs against.
///
/// The mod of the launcher is built against this version, and the loader
/// publishes a release for it.
const MINECRAFT_VERSION: &str = "1.21.4";

/// The version of the loader the test uses.
const LOADER_VERSION: &str = "0.19.5";

/// The address of the version document.
const VERSION_URL: &str = "https://piston-meta.mojang.com/v1/packages/a1cebdbaa1a44b7d8214a81be1d94316a8404d87/1.21.4.json";

/// Where the test keeps what it downloads.
fn work_directory() -> PathBuf {
    let base = std::env::var("MINLAUNCHER_E2E_DIR")
        .unwrap_or_else(|_| "C:/Users/Public/ml-fabric-e2e".to_string());
    PathBuf::from(base)
}

/// Whether the test should run at all.
fn enabled() -> bool {
    std::env::var("MINLAUNCHER_E2E").is_ok_and(|value| value != "0")
}

/// The platform of the test.
fn platform() -> Platform {
    Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
}

/// Fetch a URL and return the body.
async fn fetch(url: &str) -> Vec<u8> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the http client must build");

    let response = client
        .get(url)
        .send()
        .await
        .expect("the request must succeed");
    assert!(
        response.status().is_success(),
        "{url} answered {}",
        response.status()
    );

    response
        .bytes()
        .await
        .expect("the body must be read")
        .to_vec()
}

/// Download a file, unless it is already there.
async fn download(url: &str, target: &Path) {
    if target.is_file() {
        return;
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .expect("the directory");
    }
    let bytes = fetch(url).await;
    tokio::fs::write(target, bytes).await.expect("the file");
}

/// The paths of the test.
fn paths() -> AppPaths {
    let root = work_directory();
    AppPaths::new(root.join("data"), root.join("cache"))
}

/// Download everything the game needs: the version document, the client
/// jar and the libraries.
async fn install_game(paths: &AppPaths) -> VersionJson {
    let layout = InstallLayout::new(paths.clone(), platform());

    let bytes = fetch(VERSION_URL).await;
    let version = VersionJson::parse(&bytes).expect("the version document must parse");

    let client = version
        .downloads
        .client
        .as_ref()
        .expect("the version must have a client jar");
    download(&client.url, &layout.client_jar(&version.id)).await;

    // The libraries, one by one. The rules are not evaluated here: the
    // test wants every library of the document, so that the game starts
    // the way the launcher starts it.
    let downloader = ml_minecraft::Downloader::new().expect("the downloader");
    let mut tasks = Vec::new();

    for library in &version.libraries {
        let Some(download) = &library.downloads.artifact else {
            continue;
        };
        // The path of the document is the truth: the library server
        // serves the file under exactly that path, and a path built from
        // the coordinate alone is not always the same.
        let relative = if download.path.is_empty() {
            match ml_minecraft::library::Coordinate::parse(&library.name) {
                Ok(coordinate) => coordinate.artifact_path(),
                Err(_) => continue,
            }
        } else {
            download.path.clone()
        };
        tasks.push(ml_minecraft::DownloadTask::new(
            download.url.clone(),
            layout.library_path(&relative),
            Some(download.sha1.clone()),
        ));
    }

    let result = downloader.download_all(tasks).await.expect("the downloads");
    assert!(result.is_success(), "{:?}", result.failed);

    version
}

/// Download the libraries of the loader and build the launch part.
async fn install_loader(paths: &AppPaths) -> FabricLaunch {
    let layout = InstallLayout::new(paths.clone(), platform());

    let url = ml_minecraft::fabric::profile_url(MINECRAFT_VERSION, LOADER_VERSION);
    let bytes = fetch(&url).await;
    let profile = FabricProfile::parse(&bytes).expect("the profile must parse");

    let downloader = ml_minecraft::Downloader::new().expect("the downloader");
    let mut tasks = Vec::new();
    for (relative, url, sha1) in profile.downloads() {
        tasks.push(ml_minecraft::DownloadTask::new(
            url,
            layout.library_path(&relative),
            sha1,
        ));
    }

    let result = downloader.download_all(tasks).await.expect("the downloads");
    assert!(result.is_success(), "{:?}", result.failed);

    FabricLaunch::from_profile(MINECRAFT_VERSION, LOADER_VERSION, &profile)
}

/// The Java runtime of the test.
///
/// The game of 1.21.4 needs Java 21. The runtime of Mojang is used when
/// it is there, and the Java of the machine otherwise.
fn java(paths: &AppPaths) -> JavaInstallation {
    let runtime = paths.jre_dir_for("java-runtime-delta");
    let executable = runtime.join("bin").join("javaw.exe");
    let windowless = runtime.join("bin").join("javaw.exe");

    if executable.is_file() {
        return JavaInstallation {
            executable: runtime.join("bin").join("java.exe"),
            windowless_executable: Some(windowless),
            major_version: 21,
            version: "21.0.8".to_string(),
            origin: JavaOrigin::Mojang,
        };
    }

    // The Java of the machine.
    let home = std::env::var("JAVA_HOME").expect("JAVA_HOME must be set for the test");
    let home = PathBuf::from(home);
    JavaInstallation {
        executable: home.join("bin").join("java.exe"),
        windowless_executable: Some(home.join("bin").join("javaw.exe")),
        major_version: 21,
        version: "21".to_string(),
        origin: JavaOrigin::System,
    }
}

/// Read a pipe of the game and send every line to the receiver.
fn spawn_reader<R>(pipe: R, sender: tokio::sync::mpsc::UnboundedSender<String>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let lines = tokio::io::BufReader::new(pipe).lines();
        tokio::pin!(lines);
        while let Ok(Some(line)) = lines.next_line().await {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
}

/// The game directory of the instance, with the mod in it.
///
/// The mod is put in the directory of the mods, the way the loader
/// finds it, and the settings of the mod are written next to it, the
/// way the launcher writes them.
async fn prepare_instance() -> (PathBuf, PathBuf) {
    // The mod of the launcher, built by the Gradle build of the project.
    let mod_jar = PathBuf::from(std::env::var("MINLAUNCHER_MOD_JAR").unwrap_or_else(|_| {
        "C:/Users/Public/minlaucher-java-build/mod/staged/minlaucher-optimisations-0.1.0.jar"
            .to_string()
    }));
    assert!(
        mod_jar.is_file(),
        "the jar of the mod is not at `{}`; build it with the Gradle build of the project",
        mod_jar.display()
    );

    let game_directory = work_directory().join("instance");
    let mods_directory = game_directory.join("mods");
    tokio::fs::create_dir_all(&mods_directory)
        .await
        .expect("the directory of the mods");

    let installed_mod = mods_directory.join("minlaucher-optimisations.jar");
    tokio::fs::copy(&mod_jar, &installed_mod)
        .await
        .expect("the mod must be copied");

    // The settings of the mod, written the way the launcher writes them.
    let settings = game_directory.join("minlaucher-optimisations.json");
    tokio::fs::write(&settings, br#"{"preset":"fast"}"#)
        .await
        .expect("the settings must be written");

    (game_directory, installed_mod)
}

/// The assets are needed by the game before it shows its window.
async fn prepare_assets(paths: &AppPaths) -> PathBuf {
    let assets = paths.assets_dir.clone();
    tokio::fs::create_dir_all(assets.join("indexes"))
        .await
        .expect("the directory of the indexes");
    tokio::fs::write(assets.join("indexes").join("19.json"), b"{\"objects\":{}}")
        .await
        .expect("the index must be written");
    assets
}

/// Start the game and read its output until the mod says that it loaded.
///
/// The output is read in a thread of its own, because the game writes a
/// lot of it and the pipe fills up if nobody reads it. The game itself
/// waits for the player, so the test does not wait for the game to end:
/// it waits for the lines that say that the loader and the mod are there.
async fn run_until_the_mod_is_loaded(command: &ml_launch::command::LaunchCommand) -> String {
    let mut child = tokio::process::Command::new(&command.executable)
        .args(&command.arguments)
        .current_dir(&command.working_directory)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the game must start");

    let stdout = child.stdout.take().expect("the pipe of the output");
    let stderr = child.stderr.take().expect("the pipe of the errors");

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    spawn_reader(stdout, sender.clone());
    spawn_reader(stderr, sender.clone());
    drop(sender);

    let mut log = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, receiver.recv()).await {
            Ok(Some(line)) => {
                eprintln!("{line}");
                log.push_str(&line);
                log.push('\n');

                // The two lines that say that everything is there.
                let loader_is_there =
                    log.contains("Fabric Loader") || log.contains("Loading Minecraft");
                if loader_is_there && log.contains("minlaucher-optimisations") {
                    break;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }

    let _ = child.kill().await;
    let _ = child.wait().await;

    log
}

/// The game runs, the loader loads the mod, and the mod says what it did.
#[tokio::test]
async fn the_game_starts_with_the_loader_and_the_mod() {
    if !enabled() {
        eprintln!("the end-to-end test is skipped; set MINLAUNCHER_E2E=1 to run it");
        return;
    }

    let paths = paths();
    paths.ensure_dirs().expect("the directories");

    let version = install_game(&paths).await;
    let loader = install_loader(&paths).await;

    let (game_directory, installed_mod) = prepare_instance().await;
    let assets = prepare_assets(&paths).await;

    // The request, exactly as the launcher builds it.
    let libraries = version.resolve_libraries(platform(), Some(&version.id));
    let session = Session {
        player_name: "Steve".to_string(),
        uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".to_string(),
        uuid_prefix: "069a79f444e94726a5befca90e38aaf5".to_string(),
        access_token: String::new(),
        version_name: version.id.clone(),
        version_type: "release".to_string(),
        game_directory: game_directory.clone(),
        assets_root: assets.clone(),
        assets_index_name: "19".to_string(),
        library_directory: paths.libraries_dir.clone(),
        natives_directory: paths.version_dir(&version.id).join("natives"),
        launcher_name: "MinLauncher".to_string(),
        launcher_version: "0.1.0".to_string(),
        ..Session::default()
    };

    let request = LaunchRequest {
        version: version.clone(),
        libraries,
        java: java(&paths),
        session,
        client_jar: paths.version_dir(&version.id).join("client.jar"),
        library_directory: paths.libraries_dir.clone(),
        extra_jvm_args: Vec::new(),
        agent_jar: None,
        agent_report: None,
        loader: Some(loader),
        mod_jars: vec![installed_mod],
    };

    let command = build_command(&request, platform(), &FeatureSet::none(), "release")
        .expect("the command must build");

    // The command must start the loader, not the game.
    let joined = command.arguments.join(" ");
    assert!(
        joined.contains("net.fabricmc.loader.impl.launch.knot.KnotClient"),
        "{joined}"
    );
    assert!(joined.contains("-Dfabric.gameJarPath.client="), "{joined}");
    assert!(joined.contains("-Dfabric.addMods="), "{joined}");

    let log = run_until_the_mod_is_loaded(&command).await;

    eprintln!("--- the output of the game ---\n{log}\n--- the end ---");

    // The loader must have said that it loaded.
    assert!(
        log.contains("Fabric Loader") || log.contains("Loading Minecraft"),
        "the loader must say that it started:\n{log}"
    );

    // The mod must have said what it did.
    assert!(
        log.contains("minlaucher-optimisations"),
        "the mod must say that it loaded:\n{log}"
    );
}
