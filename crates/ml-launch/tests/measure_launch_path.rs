//! The launch path, measured against the installation of this machine.
//!
//! The launcher promises five seconds from the click to the process of
//! the game, and the share of the launcher in that time is what this
//! file measures. It runs against the real installation of the machine —
//! the real version document of 26.3, the real library store, the real
//! native jars — because the numbers of a hand-written fixture say
//! nothing about the numbers of the game.
//!
//! It does not run by itself:
//!
//! ```text
//! cargo test -p ml-launch --test measure_launch_path -- --ignored --nocapture
//! ```
//!
//! What it prints is what the launch path costs, and the second number
//! is the one that matters: the natives are unpacked once, so the second
//! preparation of the same version must not unpack them again.

use std::path::PathBuf;
use std::time::Instant;

use ml_core::{AppPaths, FeatureSet, Platform};
use ml_launch::args::Session;
use ml_launch::command::LaunchRequest;
use ml_launch::jvm::{JavaInstallation, JavaOrigin};
use ml_minecraft::version::VersionJson;

/// The version the measurement runs against.
const VERSION: &str = "26.3";

/// The Java the runtime of Mojang brings.
const RUNTIME: &str = "java-runtime-epsilon";

/// The request the launcher builds for the version.
fn the_request(paths: &AppPaths, natives: PathBuf, game: PathBuf) -> LaunchRequest {
    let bytes = std::fs::read(paths.version_dir(VERSION).join("version.json"))
        .expect("the version document of the machine");
    let version = VersionJson::parse(&bytes).expect("the version document must parse");
    let platform = Platform::current();
    let libraries = version.resolve_libraries(platform, Some(VERSION));

    LaunchRequest {
        version,
        libraries,
        java: JavaInstallation {
            executable: paths.jre_dir_for(RUNTIME).join("bin").join("java.exe"),
            windowless_executable: None,
            major_version: 25,
            version: "25".into(),
            origin: JavaOrigin::Mojang,
        },
        session: Session {
            player_name: "Steve".into(),
            uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
            uuid_prefix: "069a79f444e94726a5befca90e38aaf5".into(),
            version_name: VERSION.into(),
            version_type: "release".into(),
            game_directory: game,
            assets_root: paths.assets_dir.clone(),
            assets_index_name: "34".into(),
            library_directory: paths.libraries_dir.clone(),
            natives_directory: natives,
            launcher_name: "MinLauncher".into(),
            launcher_version: ml_core::VERSION.into(),
            ..Session::default()
        },
        client_jar: paths.version_dir(VERSION).join("client.jar"),
        library_directory: paths.libraries_dir.clone(),
        extra_jvm_args: Vec::new(),
        agent_jar: None,
        agent_report: None,
        loader: None,
        mod_jars: Vec::new(),
    }
}

#[test]
#[ignore = "it measures the installation of this machine"]
fn the_launch_path_is_measured() {
    let Ok(paths) = AppPaths::discover() else {
        eprintln!("the paths of this machine are not known; the measurement is skipped");
        return;
    };
    if !paths.version_dir(VERSION).join("version.json").is_file() {
        eprintln!("{VERSION} is not installed on this machine; the measurement is skipped");
        return;
    }

    // The natives go into a directory of their own: the measurement must
    // not touch the installation of the player.
    let directory = tempfile::tempdir().unwrap();
    let natives = directory.path().join("natives");
    let request = the_request(&paths, natives.clone(), directory.path().join("game"));
    let platform = Platform::current();

    // The first preparation unpacks the natives.
    let started = Instant::now();
    let first = ml_launch::prepare(&request, platform, &FeatureSet::none(), "release").unwrap();
    println!(
        "the first preparation took {} ms and unpacked {} files",
        started.elapsed().as_millis(),
        first.extraction.files.len()
    );

    // The second one finds them on the disk.
    let started = Instant::now();
    let second = ml_launch::prepare(&request, platform, &FeatureSet::none(), "release").unwrap();
    println!(
        "the second preparation took {} ms and unpacked {} files (skipped: {})",
        started.elapsed().as_millis(),
        second.extraction.files.len(),
        second.extraction.skipped
    );

    // The launcher builds the download engine once, not on every launch.
    let started = Instant::now();
    let _ = ml_minecraft::Downloader::new().unwrap();
    println!(
        "building the engine of the downloads took {} µs",
        started.elapsed().as_micros()
    );

    // Reading the version from the `release` file is the fast path.
    let home = paths.jre_dir_for(RUNTIME);
    let executable = home.join("bin").join("java.exe");
    if executable.is_file() {
        // The file first; the process only when the file says nothing.
        for round in 1..=5 {
            let started = Instant::now();
            let _ = ml_launch::jvm::version_of(&home, &executable).unwrap();
            println!(
                "finding the version of the java took {} µs (round {round})",
                started.elapsed().as_micros()
            );
        }

        // The fallback: start Java and ask.
        for round in 1..=5 {
            let started = Instant::now();
            let _ = ml_launch::jvm::probe_version(&executable).unwrap();
            println!(
                "asking java for its version took {} ms (round {round})",
                started.elapsed().as_millis()
            );
        }
    }
}
