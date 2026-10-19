//! The download of a Java runtime, against the real servers of Mojang.
//!
//! The other end-to-end tests check the game and the loader. This one
//! checks the piece that has to be there before either of them can run:
//! a Java installation the launcher fetched itself.
//!
//! It downloads a real runtime — a few hundred files — so it needs the
//! network and some bandwidth. It does not run by itself. It runs when
//! `MINLAUNCHER_E2E` is set:
//!
//! ```text
//! MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test runtime_end_to_end
//! ```
//!
//! The directory of this test is its own, and not the one of the other
//! end-to-end tests: two of them must never write to the same place.

use std::path::PathBuf;
use std::sync::Arc;

use ml_core::{AppPaths, JavaRuntime};
use ml_minecraft::MinecraftService;
use ml_storage::Storage;

/// The runtime the test downloads.
///
/// It is a delta runtime: the one the launcher needs for the versions
/// of the game that ask for Java 21.
const COMPONENT: &str = "java-runtime-delta";

/// Where the test keeps what it downloads.
fn work_directory() -> PathBuf {
    let base = std::env::var("MINLAUNCHER_E2E_DIR")
        .unwrap_or_else(|_| "C:/Users/Public/ml-runtime-e2e".to_string());
    PathBuf::from(base)
}

/// Whether the test should run at all.
fn enabled() -> bool {
    std::env::var("MINLAUNCHER_E2E").is_ok_and(|value| value != "0")
}

/// The paths of the test.
fn paths() -> AppPaths {
    let root = work_directory();
    AppPaths::new(root.join("data"), root.join("cache"))
}

/// The service of the test.
fn service(paths: &AppPaths) -> MinecraftService {
    let storage = Arc::new(Storage::open(paths).expect("the storage must open"));
    MinecraftService::new(paths.clone(), storage).expect("the service must build")
}

/// The runtime is downloaded and unpacked, so that the launcher finds a
/// Java it can start the game with.
#[tokio::test]
async fn a_runtime_is_downloaded_and_the_launcher_finds_it() {
    if !enabled() {
        eprintln!("the end-to-end test is switched off");
        return;
    }

    let paths = paths();

    // The runtime is fetched from nothing, so that the download path is
    // really walked. A runtime that is already on the disk says
    // nothing about the download.
    let _ = std::fs::remove_dir_all(paths.jre_dir_for(COMPONENT));

    let service = service(&paths);

    // --- the manifest -------------------------------------------------
    let manifest = service
        .runtime_manifest()
        .await
        .expect("the runtime manifest must come");

    let platform = ml_minecraft::jre::platform_key();
    assert!(
        manifest.for_platform(&platform).is_some(),
        "there must be runtimes for {platform}"
    );
    assert!(
        manifest.entry(&platform, COMPONENT).is_some(),
        "there must be a runtime {COMPONENT} for {platform}"
    );

    // --- the plan ------------------------------------------------------
    let plan = service
        .runtime_plan(COMPONENT)
        .await
        .expect("the plan must be built");

    assert!(
        plan.files.len() > 100,
        "the plan of {COMPONENT} must hold the whole runtime, not {} files",
        plan.files.len()
    );
    assert!(plan.total_size > 1_000_000, "the runtime is not that small");

    // The plan must not leave the runtime directory.
    for file in &plan.files {
        assert!(
            file.target.starts_with(paths.jre_dir_for(COMPONENT)),
            "the file {} is outside the runtime",
            file.target.display()
        );
    }

    // --- the download ---------------------------------------------------
    let result = service
        .install_runtime(COMPONENT)
        .await
        .expect("the runtime must download");

    assert!(
        result.is_success(),
        "the files that failed: {:?}",
        result.failed
    );
    assert_eq!(result.completed.len(), plan.files.len());

    // --- what is on the disk --------------------------------------------
    let home = paths.jre_dir_for(COMPONENT);
    let java = home
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });

    assert!(
        java.is_file(),
        "the java executable must be at {}",
        java.display()
    );

    // Every file of the plan is on the disk.
    for file in &plan.files {
        assert!(
            file.target.is_file(),
            "the file {} is not on the disk",
            file.target.display()
        );
    }

    // --- the launcher finds it ------------------------------------------
    //
    // This is the whole point of the runtime: the launcher has to be
    // able to start the game with the Java it downloaded itself.
    let runtime = JavaRuntime::new(COMPONENT, 21);
    let found = ml_launch::jvm::find_runtime(&paths, &runtime, None)
        .expect("the launcher must find the runtime it downloaded");

    assert_eq!(found.origin, ml_launch::jvm::JavaOrigin::Mojang);
    assert_eq!(found.executable, java);
    assert!(
        found.major_version >= 21,
        "the runtime is java {}, not 21",
        found.major_version
    );

    // The Java really runs: the version it reports is the one the
    // manifest promises.
    let (major, version) = ml_launch::jvm::probe_version(&found.executable).unwrap();
    assert_eq!(major, found.major_version);
    assert_ne!(version, "", "the java must name its version");
}

/// The directory of the test is not inside the project.
///
/// The test writes hundreds of megabytes, and the project lives on a
/// path with a Cyrillic character in it, which the linker of this
/// machine cannot read. The directory is therefore outside.
#[test]
fn the_work_directory_is_outside_the_project() {
    let root = work_directory();
    assert!(root.is_absolute());
    assert!(
        !root.to_string_lossy().contains("Desktop"),
        "the work directory must not be inside the project"
    );
}
