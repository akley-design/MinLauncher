//! The installation of a version, against the real servers of Mojang.
//!
//! The other end-to-end test checks that the *loader* is put in front of
//! the game. This one checks the step before it: that the installation
//! puts the game on the disk the way the launcher expects to find it.
//!
//! It downloads a real version of the game — the client jar, the
//! libraries and every asset — so it needs the network and a few hundred
//! megabytes. It does not run by itself. It runs when `MINLAUNCHER_E2E`
//! is set:
//!
//! ```text
//! MINLAUNCHER_E2E=1 cargo test -p ml-desktop --test install_end_to_end
//! ```
//!
//! The directory of this test is its own, and not the one of the other
//! end-to-end test. A test that has to check an installation has to
//! start from nothing: a plan that only lists the files that are missing
//! says nothing about an installation that has already happened.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ml_core::AppPaths;
use ml_minecraft::{DownloadProgress, MinecraftService};
use ml_storage::Storage;
use tokio::sync::OnceCell;

/// The version of the game the test installs.
///
/// It is the same version the other end-to-end test uses, because the
/// mod of the launcher is built against it.
const MINECRAFT_VERSION: &str = "1.21.4";

/// Where the test keeps what it downloads.
///
/// The directory is its own, and not the one of the other end-to-end
/// test: this test has to start from nothing, because a plan that only
/// lists the files that are missing says nothing about an installation
/// that has already happened.
fn work_directory() -> PathBuf {
    let base = std::env::var("MINLAUNCHER_E2E_DIR")
        .unwrap_or_else(|_| "C:/Users/Public/ml-install-e2e".to_string());
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

/// What one installation produced.
#[derive(Clone)]
struct Installation {
    /// The files the installation had to fetch.
    plan: Arc<ml_minecraft::InstallPlan>,
    /// The reports the download engine sent, in the order they came.
    reports: Arc<Vec<DownloadProgress>>,
}

/// The installation of the game, done once for the whole test binary.
///
/// The installation is done *from nothing*: the version directory and
/// the store of the assets are emptied first, so that the plan holds
/// the whole game and the download path is really walked. The
/// installation is done once, and every test that needs it waits for
/// the same one — so the order the tests run in does not matter.
static INSTALLATION: OnceCell<Installation> = OnceCell::const_new();

/// The installation of the game, from nothing, once.
async fn installation() -> &'static Installation {
    INSTALLATION
        .get_or_init(|| async { install_from_nothing().await })
        .await
}

/// Install the version of the game into an empty directory.
async fn install_from_nothing() -> Installation {
    let paths = paths();
    let mut service = service(&paths);

    // The directory is emptied first, so that the plan of the version
    // holds the whole game and not only what is left of it. A plan
    // lists the files that are *missing*, so a directory that already
    // holds the libraries says nothing about an installation.
    let _ = std::fs::remove_dir_all(paths.version_dir(MINECRAFT_VERSION));
    let _ = std::fs::remove_dir_all(paths.assets_dir.join("objects"));
    let _ = std::fs::remove_dir_all(&paths.libraries_dir);

    // --- the version -------------------------------------------------
    let manifest = service.manifest().await.expect("the manifest must come");
    assert!(
        manifest.find(MINECRAFT_VERSION).is_some(),
        "the version {MINECRAFT_VERSION} must be in the manifest"
    );

    let version = service
        .version(&manifest, MINECRAFT_VERSION)
        .await
        .expect("the version document must come");
    assert_eq!(version.id, MINECRAFT_VERSION);

    // --- the asset index ----------------------------------------------
    let index = service
        .asset_index(&version)
        .await
        .expect("the asset index must come");
    assert!(
        !index.is_empty(),
        "the asset index of {MINECRAFT_VERSION} must hold objects"
    );

    // --- the plan ------------------------------------------------------
    let plan = service
        .plan(&version, &index)
        .expect("the plan must be built");
    assert!(
        plan.len() > 100,
        "the plan of {MINECRAFT_VERSION} must hold the whole game, not {} files",
        plan.len()
    );

    // The client jar is the one file every installation has.
    let client = plan
        .files
        .iter()
        .find(|file| file.kind == ml_minecraft::FileKind::Client)
        .expect("the plan must hold the client jar");
    assert!(client.target.ends_with("client.jar"));
    assert_eq!(
        client.target.parent().unwrap(),
        paths.version_dir(MINECRAFT_VERSION)
    );

    // --- the download ---------------------------------------------------
    //
    // The reports are counted, because a report that never comes is the
    // way a progress bar that never moves.
    let reports: Arc<Mutex<Vec<DownloadProgress>>> = Arc::new(Mutex::new(vec![]));
    let recorder = Arc::clone(&reports);
    let callback: ml_minecraft::ProgressCallback = Arc::new(move |progress| {
        recorder.lock().unwrap().push(progress);
    });

    let result = service
        .install_with_progress(&plan, callback)
        .await
        .expect("the installation must run");

    assert!(
        result.is_success(),
        "the files that failed: {:?}",
        result.failed
    );

    // The reports came, and the last one says that the batch is over.
    let reports = reports.lock().unwrap().clone();
    assert!(!reports.is_empty(), "the installation reported nothing");
    let last = reports.last().unwrap();
    assert!(
        last.is_finished(),
        "the last report is not finished: {last:?}"
    );
    assert_eq!(last.total, plan.len());
    assert_eq!(last.completed, plan.len());

    // --- what is on the disk --------------------------------------------
    for file in &plan.files {
        assert!(
            file.target.is_file(),
            "the file {} is not on the disk",
            file.target.display()
        );
    }

    // The client jar is the one file the game cannot start without.
    assert!(client.target.is_file());
    assert!(client.target.metadata().unwrap().len() > 20_000_000);

    // The libraries are in the store, under the path the document says.
    let library = plan
        .files
        .iter()
        .find(|file| file.kind == ml_minecraft::FileKind::Library)
        .expect("the plan must hold libraries");
    assert!(
        library.target.starts_with(paths.libraries_dir),
        "the library {} is not in the store",
        library.target.display()
    );

    // The assets are content-addressed: the name of the file is the hash
    // of its content.
    let asset = plan
        .files
        .iter()
        .find(|file| file.kind == ml_minecraft::FileKind::Asset)
        .expect("the plan must hold assets");
    assert!(asset.target.starts_with(paths.assets_dir.join("objects")));

    // The hash of the asset is the name of the file.
    let name = asset
        .target
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(name, asset.sha1.as_deref().unwrap());

    Installation {
        plan: Arc::new(plan),
        reports: Arc::new(reports),
    }
}

/// A version is installed the way the launcher expects to find it.
#[tokio::test]
async fn a_version_is_installed_the_way_the_launcher_expects_it() {
    if !enabled() {
        eprintln!("the end-to-end test is switched off");
        return;
    }

    let installation = installation().await;
    let plan = &installation.plan;

    assert!(
        plan.len() > 100,
        "the plan of {MINECRAFT_VERSION} must hold the whole game, not {} files",
        plan.len()
    );

    // Every file of the plan is on the disk.
    for file in &plan.files {
        assert!(
            file.target.is_file(),
            "the file {} is not on the disk",
            file.target.display()
        );
    }

    // The reports of the download came, and the last one says that the
    // batch is over.
    let reports = &installation.reports;
    assert!(!reports.is_empty(), "the installation reported nothing");
    let last = reports.last().unwrap();
    assert!(
        last.is_finished(),
        "the last report is not finished: {last:?}"
    );
    assert_eq!(last.total, plan.len());
    assert_eq!(last.completed, plan.len());
}

/// A second installation of the same version is nearly free.
///
/// The launcher keeps its library store content-addressed, so the
/// second installation of a version does not download what the first one
/// already downloaded. This is what makes the *second* version of the
/// game quick to install, and it is the reason the promise of the
/// launcher is kept.
#[tokio::test]
async fn the_second_installation_downloads_nothing() {
    if !enabled() {
        eprintln!("the end-to-end test is switched off");
        return;
    }

    // The installation has to have happened, or there is nothing to
    // compare the second one with.
    installation().await;

    let paths = paths();
    let mut service = service(&paths);

    let manifest = service.manifest().await.expect("the manifest must come");
    let version = service
        .version(&manifest, MINECRAFT_VERSION)
        .await
        .expect("the version document must come");
    let index = service
        .asset_index(&version)
        .await
        .expect("the asset index must come");

    // The plan only lists what is missing. After the first installation,
    // there is nothing missing.
    let plan = service
        .plan(&version, &index)
        .expect("the plan must be built");
    assert!(
        plan.is_empty(),
        "the plan of an installed version must be empty, not {} files",
        plan.len()
    );

    let result = service
        .install_with_progress(&plan, Arc::new(|_| {}))
        .await
        .expect("the installation must run");

    assert!(result.is_success());
    assert_eq!(result.bytes, 0, "nothing must be downloaded a second time");
}

/// The asset store is shared between the versions.
///
/// The store is content-addressed: the same object is fetched once for
/// all the versions. The test checks that the object of the version of
/// the test is where the store says it is.
#[tokio::test]
async fn the_assets_are_in_the_content_addressed_store() {
    if !enabled() {
        eprintln!("the end-to-end test is switched off");
        return;
    }

    // The store only holds something once the game has been installed.
    installation().await;

    let paths = paths();
    let objects = paths.assets_dir.join("objects");
    assert!(
        objects.is_dir(),
        "the store of the assets must be at {}",
        objects.display()
    );

    // Every object is under a two-letter prefix, and its name is the
    // whole hash.
    let mut counted = 0;
    for prefix in std::fs::read_dir(&objects).expect("the store must be readable") {
        let prefix = prefix.expect("the entry");
        assert!(prefix.path().is_dir());

        let name = prefix.file_name().to_string_lossy().into_owned();
        assert_eq!(name.len(), 2, "the prefix {name} must be two letters long");

        for object in std::fs::read_dir(prefix.path()).expect("the objects must be readable") {
            let object = object.expect("the object");
            let hash = object.file_name().to_string_lossy().into_owned();
            assert_eq!(
                hash.len(),
                40,
                "the hash {hash} must be forty characters long"
            );
            assert!(
                hash.starts_with(&name),
                "the object {hash} is not under the prefix {name}"
            );
            counted += 1;
        }
    }

    assert!(counted > 100, "the store holds {counted} objects");
}

/// The file that holds the objects is the one the plan points at.
#[test]
fn the_path_of_an_object_is_the_hash_of_its_content() {
    let paths = paths();
    let hash = "5ff04807c356f1beed0b86ccf659b44b9983e3fa";

    let path = paths.asset_object(hash);

    // The name of the file is the whole hash, and the directory it sits
    // in is the first two letters of it. This is the layout the official
    // launcher uses, and the one the server serves the objects at.
    assert!(path.ends_with("5f/5ff04807c356f1beed0b86ccf659b44b9983e3fa"));
    assert_eq!(
        path.parent().unwrap().parent().unwrap(),
        paths.assets_dir.join("objects")
    );
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

/// A file that is missing does not stop the test from being read.
#[test]
fn the_work_directory_can_be_created() {
    let root = work_directory();
    std::fs::create_dir_all(&root).expect("the work directory must be creatable");
    assert!(root.is_dir());
    assert!(Path::new(&root).is_dir());
}
