//! The contract over every version of the game.
//!
//! The corpus of the documents of Mojang lives in `ml-minecraft`; this
//! test walks all of it and builds the command of **every** version. A
//! launcher that starts 26.3 says nothing about the launcher that starts
//! 1.5.2: the documents are not alike, and every one of them has to end
//! up in a command that the game understands.
//!
//! The contract is one line: **no placeholder of the command is left
//! unresolved**. A `${...}` that reaches the game is a broken launch, and
//! the game complains about it in a way that says nothing about where it
//! came from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ml_core::{AppPaths, FeatureSet, Platform};
use ml_launch::args::Session;
use ml_launch::command::{LaunchRequest, build_command};
use ml_launch::jvm::{JavaInstallation, JavaOrigin};
use ml_minecraft::corpus::{self, Corpus};
use ml_minecraft::version::VersionJson;

/// The directory of the corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("ml-minecraft")
        .join("tests")
        .join("corpus")
}

/// Read the census of the corpus.
fn corpus() -> Corpus {
    let path = corpus_dir().join(corpus::MANIFEST_FILE);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).expect("the census of the corpus is not valid JSON")
}

/// Read the document of a version of the corpus.
fn document_of(id: &str) -> VersionJson {
    let path = corpus_dir()
        .join(corpus::VERSIONS_DIR)
        .join(format!("{id}.json"));
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    VersionJson::parse(&bytes)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", path.display()))
}

/// The paths of the contract.
fn paths() -> AppPaths {
    let mut paths = AppPaths::new("C:/data", "C:/cache");
    paths.home_dir = PathBuf::from("C:/home/player");
    paths
}

/// The session of the contract: every value the launcher can give.
fn session(document: &VersionJson) -> Session {
    let paths = paths();
    Session {
        player_name: "Steve".into(),
        uuid: "1234-5678-9abc-def0-1234-56789abc0000".into(),
        uuid_prefix: "123456789abcdef0123456789abc0000".into(),
        user_type: "msa".into(),
        access_token: "token".into(),
        xuid: "1234567890".into(),
        client_id: "client".into(),
        version_name: document.id.clone(),
        version_type: "release".into(),
        game_directory: PathBuf::from("C:/games/mc"),
        assets_root: paths.assets_dir.clone(),
        assets_tree: document
            .asset_index_id()
            .and_then(|id| ml_minecraft::plain_tree_root(&paths, id)),
        assets_index_name: document.asset_index_id().unwrap_or_default().to_string(),
        library_directory: PathBuf::from("C:/cache/libraries"),
        natives_directory: PathBuf::from("C:/games/mc/natives"),
        launcher_name: "MinLauncher".into(),
        launcher_version: "0.1.0".into(),
        resolution_width: Some(1920),
        resolution_height: Some(1080),
        ..Session::default()
    }
}

/// The request of the contract: the version, its libraries and a session.
fn request(document: &VersionJson, platform: Platform) -> LaunchRequest {
    LaunchRequest {
        version: document.clone(),
        libraries: document.resolve_libraries(platform, Some(&document.id)),
        java: JavaInstallation {
            executable: PathBuf::from("C:/jre/bin/java.exe"),
            windowless_executable: Some(PathBuf::from("C:/jre/bin/javaw.exe")),
            major_version: 25,
            version: "25.0.1".into(),
            origin: JavaOrigin::Mojang,
        },
        session: session(document),
        client_jar: PathBuf::from("C:/versions/client.jar"),
        library_directory: PathBuf::from("C:/cache/libraries"),
        extra_jvm_args: Vec::new(),
        agent_jar: None,
        agent_report: None,
        loader: None,
        mod_jars: Vec::new(),
    }
}

/// The placeholders an argument still carries.
fn placeholders_in(argument: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = argument;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        names.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    names
}

/// The features the launcher turns on for a session with a resolution.
///
/// This is what `ml-desktop` does, and the contract has to be measured
/// under the same conditions: a resolution the player asked for is
/// substituted, not left behind.
fn features() -> FeatureSet {
    FeatureSet::of(["has_custom_resolution"])
}

#[test]
fn the_command_of_every_version_has_no_unresolved_placeholder() {
    let platform = corpus::reference_platform();
    let corpus = corpus();

    // Every placeholder that is left unresolved, and the versions it is
    // left in.
    let mut unresolved: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for entry in &corpus.versions {
        let document = document_of(&entry.census.id);
        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        for argument in &command.arguments {
            for name in placeholders_in(argument) {
                let versions = unresolved.entry(name).or_default();
                if !versions.contains(&entry.census.id) {
                    versions.push(entry.census.id.clone());
                }
            }
        }
    }

    assert!(
        unresolved.is_empty(),
        "{} versions keep a placeholder that the launcher did not fill: {unresolved:?}",
        unresolved.len()
    );
}

#[test]
fn the_kind_of_the_account_reaches_the_command() {
    let platform = corpus::reference_platform();

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        // The document of the version says whether it asks for the kind
        // of the account at all.
        if !entry
            .census
            .placeholders
            .iter()
            .any(|name| name == "user_type")
        {
            continue;
        }

        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        let index = command
            .arguments
            .iter()
            .position(|argument| argument == "--userType")
            .unwrap_or_else(|| panic!("{} asks for no kind of account", entry.census.id));
        assert_eq!(
            command.arguments[index + 1],
            "msa",
            "{} has the wrong kind of account",
            entry.census.id
        );
    }
}

#[test]
fn every_version_gets_the_game_arguments_its_document_prescribes() {
    let platform = corpus::reference_platform();

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        // The arguments of the game are the ones after the class the
        // game is started with.
        let main = command
            .arguments
            .iter()
            .position(|argument| argument == document.main_class.as_deref().unwrap())
            .unwrap_or_else(|| panic!("{} starts no class", entry.census.id));
        let game = &command.arguments[main + 1..];

        // The command carries the number of arguments the document prescribes.
        let prescribed =
            document.prescribed_game_arguments(platform, Some(&document.id), &features());
        assert_eq!(
            game.len(),
            prescribed.len(),
            "{} has the wrong number of game arguments",
            entry.census.id
        );

        // The versions before 1.13 have no rules at all, so the number
        // the census recorded is the number the document prescribes.
        if entry.census.format == ml_minecraft::corpus::ArgumentFormat::MinecraftArguments {
            assert_eq!(
                prescribed.len(),
                entry.census.game_arguments,
                "{} contradicts the census",
                entry.census.id
            );
        }
    }
}

#[test]
fn every_version_puts_the_game_on_the_classpath() {
    let platform = corpus::reference_platform();

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        // The classpath is named exactly once, and the game jar is on it.
        let classpaths: Vec<&String> = command
            .arguments
            .iter()
            .filter(|argument| argument.as_str() == "-cp")
            .collect();
        assert_eq!(
            classpaths.len(),
            1,
            "{} has {classpaths:?}",
            entry.census.id
        );

        let index = command.arguments.iter().position(|a| a == "-cp").unwrap();
        assert!(
            command.arguments[index + 1].contains("client.jar"),
            "{} has no game jar on the classpath",
            entry.census.id
        );
    }
}

#[test]
fn every_version_is_pointed_at_the_assets_it_reads() {
    // The versions from 1.7 on read the content-addressed store, and
    // the versions before them read an ordinary tree of files. The
    // `--assetsDir` of the command has to say which one it is.
    let platform = corpus::reference_platform();
    let paths = paths();

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        // The 17 versions that name no assets at all — the oldest
        // snapshots and the ones of Classic — are given nothing.
        let Some(index) = command
            .arguments
            .iter()
            .position(|argument| argument == "--assetsDir")
        else {
            continue;
        };

        let expected = match entry.census.assets {
            ml_minecraft::corpus::AssetLayout::ContentAddressed => paths.assets_dir.clone(),
            ml_minecraft::corpus::AssetLayout::Plain => {
                let id = document
                    .asset_index_id()
                    .unwrap_or_else(|| panic!("{} names no assets", entry.census.id));
                ml_minecraft::plain_tree_root(&paths, id)
                    .unwrap_or_else(|| panic!("{} has no tree", entry.census.id))
            }
        };

        assert_eq!(
            command.arguments[index + 1],
            ml_launch::args::path_to_string(&expected),
            "{} is pointed at the wrong assets",
            entry.census.id
        );
    }
}

#[test]
fn every_version_builds_a_command_that_starts_its_own_class() {
    let platform = corpus::reference_platform();

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let command = build_command(
            &request(&document, platform),
            platform,
            &features(),
            &document.kind,
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.census.id));

        // The class the document names is the class that is started.
        let main_class = document
            .main_class
            .as_deref()
            .unwrap_or_else(|| panic!("{} names no class", entry.census.id));
        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument == main_class),
            "{} does not start {main_class}",
            entry.census.id
        );
    }
}
