//! The contract over every version of the game.
//!
//! Mojang publishes a new version every now and then, and no two of the
//! documents are alike: the arguments changed their form in 1.13, the
//! assets changed their layout in 1.7, and the oldest releases are not
//! the shape of the newest ones at all. The launcher has to start
//! **every** one of them, and the only way to know that it does is to
//! keep the whole corpus of the documents and to walk it.
//!
//! The corpus lives in `tests/corpus`: one document per version, stripped
//! of the fields the launcher does not read, and `manifest.json` — the
//! census of the corpus, one line per version. It is checked into the
//! repository, so that this test runs without the network. The test
//! `the_corpus_is_the_one_of_mojang` compares it with the live documents
//! and is run by hand.
//!
//! The fixtures in `tests/fixtures` are eighteen documents that pin the
//! shapes: one per form the document of a version can take. They are
//! copies of the corpus documents, but they are **pinned** — they are not
//! rewritten when the corpus is refreshed, so a version that Mojang
//! changes or drops leaves its shape behind.

use std::path::{Path, PathBuf};

use futures::TryStreamExt;
use futures::stream::StreamExt;
use ml_core::AppPaths;
use ml_minecraft::corpus::{self, ArgumentFormat, AssetLayout, Corpus};
use ml_minecraft::manifest::{MANIFEST_URL, parse_manifest};
use ml_minecraft::version::VersionJson;
use ml_minecraft::{PRE_1_6_ASSET_INDEX, layout_of, plain_tree_root};
use serde_json::Value;

/// How many versions the corpus holds.
///
/// The number is written out, so that a new version of the game shows up
/// as a failure of this test and not as a silent growth of the corpus.
/// The corpus is refreshed with
/// `cargo run -p ml-minecraft --bin fetch_corpus`, and the number is
/// changed here together with the corpus.
const EXPECTED_VERSIONS: usize = 920;

/// One document per shape of the document of a version.
///
/// The expectations are written out here and are not taken from the
/// corpus, so that a change of the corpus cannot change the expectation
/// with it.
struct Fixture {
    /// The name of the file of the fixture.
    file: &'static str,
    /// How the document spells its arguments.
    format: ArgumentFormat,
    /// Where the assets of the version are read from.
    assets: AssetLayout,
    /// The identifier of the assets, as the document names it.
    assets_id: &'static str,
    /// The Java the document asks for, if it names one.
    java: Option<u32>,
    /// The arguments the document must name.
    arguments: &'static [&'static str],
    /// The arguments the document must not name.
    absent: &'static [&'static str],
    /// The placeholders the document must use.
    placeholders: &'static [&'static str],
}

/// The shapes of the document of a version, one fixture each.
const FIXTURES: &[Fixture] = &[
    Fixture {
        file: "26.3.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "34",
        java: Some(25),
        arguments: &[
            "--username",
            "--assetIndex",
            "--versionType",
            "--demo",
            "--quickPlayMultiplayer",
        ],
        absent: &["--userType"],
        placeholders: &[
            "auth_player_name",
            "assets_index_name",
            "auth_access_token",
            "version_type",
            "resolution_width",
            "quickPlayMultiplayer",
        ],
    },
    Fixture {
        file: "26.4-snapshot-5.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "36",
        java: Some(25),
        arguments: &["--username", "--assetIndex", "--quickPlayRealms"],
        absent: &["--userType"],
        placeholders: &["auth_player_name", "assets_index_name", "quickPlayRealms"],
    },
    Fixture {
        file: "1.21.4.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "19",
        java: Some(21),
        arguments: &["--username", "--userType", "--assetIndex"],
        absent: &[],
        placeholders: &[
            "auth_player_name",
            "user_type",
            "assets_index_name",
            "auth_uuid",
        ],
    },
    Fixture {
        file: "1.21.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "17",
        java: Some(21),
        arguments: &["--username", "--userType"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type"],
    },
    Fixture {
        file: "1.18.2.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.18",
        java: Some(17),
        arguments: &["--username", "--userType"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type"],
    },
    Fixture {
        file: "1.17.1.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.17",
        java: Some(16),
        arguments: &["--username", "--userType"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type"],
    },
    Fixture {
        file: "1.16.5.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.16",
        java: Some(8),
        arguments: &["--username", "--userType"],
        absent: &["--quickPlayMultiplayer"],
        placeholders: &["auth_player_name", "user_type"],
    },
    Fixture {
        file: "1.13.json",
        format: ArgumentFormat::Arguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.13",
        java: Some(8),
        arguments: &["--username", "--userType"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type"],
    },
    Fixture {
        file: "1.12.2.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.12",
        java: Some(8),
        arguments: &["--username", "--userType", "--versionType", "--assetIndex"],
        absent: &[],
        placeholders: &[
            "auth_player_name",
            "user_type",
            "version_type",
            "assets_index_name",
        ],
    },
    Fixture {
        file: "1.8.9.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.8",
        java: Some(8),
        arguments: &["--username", "--userType", "--userProperties"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type", "user_properties"],
    },
    Fixture {
        file: "1.7.10.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::ContentAddressed,
        assets_id: "1.7.10",
        java: Some(8),
        arguments: &["--username", "--userType", "--assetIndex"],
        absent: &[],
        placeholders: &["auth_player_name", "user_type", "assets_index_name"],
    },
    Fixture {
        file: "1.7.2.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "legacy",
        java: Some(8),
        arguments: &["--username", "--accessToken"],
        absent: &["--assetIndex", "--session"],
        placeholders: &["auth_player_name", "auth_access_token", "game_assets"],
    },
    Fixture {
        file: "1.6.4.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "legacy",
        java: None,
        arguments: &["--username", "--session"],
        absent: &["--assetIndex"],
        placeholders: &["auth_player_name", "auth_session", "game_assets"],
    },
    Fixture {
        file: "1.5.2.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "pre-1.6",
        java: Some(8),
        arguments: &["--gameDir", "--assetsDir"],
        absent: &["--username"],
        placeholders: &["auth_player_name", "auth_session", "game_assets"],
    },
    Fixture {
        file: "1.0.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "pre-1.6",
        java: Some(8),
        arguments: &["--gameDir", "--assetsDir"],
        absent: &["--username"],
        placeholders: &["auth_player_name", "auth_session", "game_assets"],
    },
    Fixture {
        file: "b1.7.3.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "pre-1.6",
        java: Some(8),
        arguments: &["--gameDir", "--assetsDir"],
        absent: &["--username"],
        placeholders: &["auth_player_name", "auth_session", "game_assets"],
    },
    Fixture {
        file: "c0.30_01c.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "pre-1.6",
        java: Some(8),
        arguments: &["--gameDir", "--tweakClass"],
        absent: &["--username"],
        placeholders: &["auth_player_name", "auth_session", "game_assets"],
    },
    Fixture {
        file: "13w48b.json",
        format: ArgumentFormat::MinecraftArguments,
        assets: AssetLayout::Plain,
        assets_id: "legacy",
        java: Some(8),
        arguments: &["--username", "--accessToken", "--userProperties"],
        absent: &["--session"],
        placeholders: &[
            "auth_player_name",
            "auth_access_token",
            "user_properties",
            "game_assets",
        ],
    },
];

/// The directory of the corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

/// The directory of the fixtures.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
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

/// Read a fixture.
fn fixture_of(file: &str) -> VersionJson {
    let path = fixtures_dir().join(file);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    VersionJson::parse(&bytes)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", path.display()))
}

#[test]
fn every_document_of_the_corpus_parses() {
    let corpus = corpus();

    assert_eq!(
        corpus.count(),
        EXPECTED_VERSIONS,
        "the corpus holds {} versions, the expectation is {EXPECTED_VERSIONS}: \
         refresh it with `cargo run -p ml-minecraft --bin fetch_corpus` \
         and change the number here",
        corpus.count()
    );

    for entry in &corpus.versions {
        let document = document_of(&entry.census.id);
        assert_eq!(
            document.id, entry.census.id,
            "{} is not itself",
            entry.census.id
        );
    }

    println!("{} documents, {} fixtures", corpus.count(), FIXTURES.len());
}

#[test]
fn the_census_of_every_version_matches_the_manifest() {
    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let census = corpus::census(&document);
        assert_eq!(
            census, entry.census,
            "the census of {} does not match the manifest",
            entry.census.id
        );
    }
}

#[test]
fn the_layout_of_every_version_is_decided_by_its_index() {
    // The launcher decides the layout of the assets by the id of the
    // index, and the census says what the layout of a version is. They
    // have to agree on every one of the 920 versions: a version that is
    // read from the store but gets a tree — or the other way round —
    // starts without its sounds.
    let paths = AppPaths::new("/data", "/cache");
    let mut plain = 0;
    let mut addressed = 0;

    for entry in corpus().versions {
        let document = document_of(&entry.census.id);
        let id = document
            .asset_index_id()
            .unwrap_or_else(|| panic!("{} names no assets", entry.census.id));

        assert_eq!(
            layout_of(id),
            entry.census.assets,
            "{} reads its assets differently",
            entry.census.id
        );

        // The tree of a version that reads the store is not built, and
        // the tree of a version that reads a plain tree is.
        assert_eq!(
            plain_tree_root(&paths, id).is_some(),
            entry.census.assets == AssetLayout::Plain,
            "{} gets the wrong tree",
            entry.census.id
        );

        match entry.census.assets {
            AssetLayout::Plain => plain += 1,
            AssetLayout::ContentAddressed => addressed += 1,
        }
    }

    assert_eq!(plain, 129, "the versions that read a plain tree");
    assert_eq!(addressed, 791, "the versions that read the store");
}

#[test]
fn the_two_plain_trees_live_where_the_games_read_them() {
    // The tree of the games from 1.6.4 to 1.7.2 is the directory the
    // game is given as `--assetsDir`, and the tree of the older ones is
    // `~/.minecraft/resources`: the game of 1.5.2 reads them from there
    // and from nowhere else.
    let mut paths = AppPaths::new("/data", "/cache");
    paths.home_dir = PathBuf::from("/home/player");

    for entry in corpus().versions {
        if entry.census.assets != AssetLayout::Plain {
            continue;
        }

        let document = document_of(&entry.census.id);
        let id = document
            .asset_index_id()
            .unwrap_or_else(|| panic!("{} names no assets", entry.census.id));
        let root = plain_tree_root(&paths, id).unwrap();

        let expected = if id == PRE_1_6_ASSET_INDEX {
            PathBuf::from("/home/player/.minecraft/resources")
        } else {
            PathBuf::from("/cache/assets/virtual/legacy")
        };
        assert_eq!(
            root, expected,
            "{} gets its tree elsewhere",
            entry.census.id
        );
    }
}

#[test]
fn the_corpus_covers_every_shape_of_the_document() {
    let corpus = corpus();

    let arguments = corpus
        .versions
        .iter()
        .filter(|entry| entry.census.format == ArgumentFormat::Arguments)
        .count();
    let legacy = corpus
        .versions
        .iter()
        .filter(|entry| entry.census.format == ArgumentFormat::MinecraftArguments)
        .count();
    let plain = corpus
        .versions
        .iter()
        .filter(|entry| entry.census.assets == AssetLayout::Plain)
        .count();

    // The two forms of the arguments and the two layouts of the assets
    // are both in the corpus: a corpus that held one form only would
    // not be a corpus.
    assert!(
        arguments > 0 && legacy > 0,
        "{arguments} arguments, {legacy} legacy"
    );
    assert!(
        plain > 0 && plain < corpus.count(),
        "{plain} plain of {}",
        corpus.count()
    );
}

#[test]
fn every_fixture_is_recognised() {
    for fixture in FIXTURES {
        let document = fixture_of(fixture.file);
        let census = corpus::census(&document);

        assert_eq!(
            census.format, fixture.format,
            "{} spells its arguments differently",
            fixture.file
        );
        assert_eq!(
            census.assets, fixture.assets,
            "{} reads its assets differently",
            fixture.file
        );
        assert_eq!(
            document.assets.as_deref(),
            Some(fixture.assets_id),
            "{} names another assets",
            fixture.file
        );
        assert_eq!(
            document
                .java_version
                .as_ref()
                .map(|java| java.major_version),
            fixture.java,
            "{} asks for another Java",
            fixture.file
        );

        let templates = document.game_templates();
        for argument in fixture.arguments {
            assert!(
                templates.iter().any(|template| template == argument),
                "{} does not name {argument}",
                fixture.file
            );
        }
        for argument in fixture.absent {
            assert!(
                !templates.iter().any(|template| template == argument),
                "{} names {argument}, which it must not",
                fixture.file
            );
        }
        for placeholder in fixture.placeholders {
            assert!(
                census.placeholders.iter().any(|name| name == placeholder),
                "{} does not use {placeholder}",
                fixture.file
            );
        }
    }
}

#[test]
fn the_fixtures_are_copies_of_the_corpus() {
    for fixture in FIXTURES {
        let path = corpus_dir()
            .join(corpus::VERSIONS_DIR)
            .join(format!("{}.json", fixture.file));
        if !path.exists() {
            // The version is gone from the manifest of Mojang. The
            // fixture keeps its shape anyway — that is what it is for.
            continue;
        }
        let stored = std::fs::read_to_string(&path).expect("the document is readable");
        let pinned = std::fs::read_to_string(fixtures_dir().join(fixture.file))
            .expect("the fixture is readable");
        assert_eq!(
            pinned, stored,
            "{} differs from the corpus: either the document of Mojang has \
             changed, or the fixture has been edited",
            fixture.file
        );
    }
}

#[tokio::test]
#[ignore = "it needs the network"]
async fn the_corpus_is_the_one_of_mojang() {
    let http = reqwest::Client::new();

    let bytes = http
        .get(MANIFEST_URL)
        .send()
        .await
        .expect("the manifest is reachable")
        .error_for_status()
        .expect("the manifest answers")
        .bytes()
        .await
        .expect("the manifest is read");
    let manifest = parse_manifest(&bytes).expect("the manifest is valid");

    let corpus = corpus();
    assert_eq!(
        manifest.versions.len(),
        corpus.count(),
        "the manifest of Mojang holds {} versions, the corpus holds {}",
        manifest.versions.len(),
        corpus.count()
    );

    // Every document of the manifest is downloaded, stripped the same way
    // and compared with the corpus, byte for byte.
    let versions = manifest.versions.clone();
    let failures = futures::stream::iter(versions)
        .map(|version| {
            // The client is a handle on one pool of connections, so
            // copying it is cheap and it can be shared.
            let http = http.clone();
            async move {
                let document = http
                    .get(&version.url)
                    .send()
                    .await
                    .map_err(|error| format!("{}: {error}", version.id))?
                    .error_for_status()
                    .map_err(|error| format!("{}: {error}", version.id))?
                    .bytes()
                    .await
                    .map_err(|error| format!("{}: {error}", version.id))?;

                let raw: Value = serde_json::from_slice(&document)
                    .map_err(|e| format!("{}: {e}", version.id))?;
                let stripped = corpus::strip(&raw).to_string();

                let path = corpus_dir()
                    .join(corpus::VERSIONS_DIR)
                    .join(format!("{}.json", version.id));
                let stored = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", version.id))?;

                if stripped == stored {
                    Ok(())
                } else {
                    Err(format!(
                        "{} has changed on the side of Mojang: refresh the corpus",
                        version.id
                    ))
                }
            }
        })
        .buffer_unordered(16)
        .try_collect::<Vec<_>>()
        .await;

    if let Err(error) = failures {
        panic!("{error}");
    }
}
