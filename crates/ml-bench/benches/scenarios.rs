//! The scenarios of the benchmark harness.
//!
//! These measure the parts of a launch that happen on every single
//! start, because that is where the twenty percent have to come from.
//! The network and the disk are measured separately, by the protocol of
//! `docs/PLAN.md` § 1, because a benchmark that hits the network
//! measures the network and nothing else.
//!
//! The version document is the real 26.3 one, kept as a fixture, so
//! that the numbers describe the work the launcher actually does.

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use ml_core::{FeatureSet, OsArch, OsName, Platform};
use ml_launch::args::Session;
use ml_launch::command::{LaunchRequest, build_command};
use ml_launch::jvm::JavaInstallation;
use ml_minecraft::version::VersionJson;

/// The real 26.3 document.
const VERSION: &[u8] = include_bytes!("../tests/fixtures/version_26.3.json");

fn platform() -> Platform {
    Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
}

fn version() -> VersionJson {
    VersionJson::parse(VERSION).expect("the fixture is a valid version document")
}

fn session() -> Session {
    Session {
        player_name: "Steve".into(),
        uuid: "1234-5678-9abc-def0-1234-56789abc0000".into(),
        uuid_prefix: "123456789abcdef0123456789abc0000".into(),
        access_token: "token".into(),
        version_name: "26.3".into(),
        game_directory: PathBuf::from("C:/games/mc"),
        assets_root: PathBuf::from("C:/cache/assets"),
        assets_index_name: "34".into(),
        library_directory: PathBuf::from("C:/cache/libraries"),
        natives_directory: PathBuf::from("C:/games/mc/natives"),
        launcher_name: "MinLauncher".into(),
        launcher_version: "0.1.0".into(),
        ..Session::default()
    }
}

fn request() -> LaunchRequest {
    let version = version();
    let libraries = version.resolve_libraries(platform(), Some("26.3"));

    LaunchRequest {
        version,
        libraries,
        java: JavaInstallation {
            executable: PathBuf::from("C:/jre/bin/java.exe"),
            windowless_executable: Some(PathBuf::from("C:/jre/bin/javaw.exe")),
            major_version: 25,
            version: "25.0.1".into(),
            origin: ml_launch::jvm::JavaOrigin::Mojang,
        },
        session: session(),
        client_jar: PathBuf::from("C:/versions/26.3/client.jar"),
        library_directory: PathBuf::from("C:/cache/libraries"),
        extra_jvm_args: Vec::new(),
        agent_jar: None,
        agent_report: None,
    }
}

/// Parsing the version document.
fn parsing(criterion: &mut Criterion) {
    criterion.bench_function("parse_version_json", |bencher| {
        bencher.iter(|| VersionJson::parse(VERSION).expect("the fixture parses"));
    });
}

/// Resolving the libraries of the version for the platform.
fn resolving_libraries(criterion: &mut Criterion) {
    let version = version();

    criterion.bench_function("resolve_libraries", |bencher| {
        bencher.iter(|| version.resolve_libraries(platform(), Some("26.3")));
    });
}

/// Resolving the arguments of the version.
fn resolving_arguments(criterion: &mut Criterion) {
    let version = version();
    let features = FeatureSet::none();

    criterion.bench_function("resolve_arguments", |bencher| {
        bencher.iter(|| {
            let _ = version
                .arguments
                .resolve_default_user_jvm(platform(), Some("26.3"), &features);
            let _ = version
                .arguments
                .resolve_jvm(platform(), Some("26.3"), &features);
            let _ = version
                .arguments
                .resolve_game(platform(), Some("26.3"), &features);
        });
    });
}

/// The whole command line, from the document to the argument vector.
fn building_the_command(criterion: &mut Criterion) {
    let request = request();
    let features = FeatureSet::none();

    criterion.bench_function("build_command", |bencher| {
        bencher.iter(|| {
            build_command(&request, platform(), &features, "release").expect("the command builds")
        });
    });
}

/// The classpath, which is the biggest part of the command line.
fn building_the_classpath(criterion: &mut Criterion) {
    let request = request();

    criterion.bench_function("build_classpath", |bencher| {
        bencher.iter(|| request.classpath_entries());
    });
}

criterion_group!(
    scenarios,
    parsing,
    resolving_libraries,
    resolving_arguments,
    building_the_command,
    building_the_classpath
);
criterion_main!(scenarios);
