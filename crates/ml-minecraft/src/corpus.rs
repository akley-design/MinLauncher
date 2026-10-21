//! The corpus of the versions of the game.
//!
//! Mojang publishes every version of the game as a JSON document, and no
//! two documents are alike: the arguments changed their form in 1.13, the
//! assets changed their layout in 1.7, and the oldest releases are not
//! the same shape as the newest ones at all.
//!
//! A few examples do not catch that. The launcher has to work with
//! **every** version, and the only way to know that it does is to keep
//! the whole corpus and to walk it.
//!
//! # What the corpus is
//!
//! The corpus is a **stripped** copy of every document Mojang publishes:
//! only the fields the launcher reads are kept. The downloads of the
//! libraries are left out — the path of a library is derived from its
//! coordinates — and the corpus was checked against every version: the
//! derived path and the declared one are the same everywhere, in all the
//! libraries of all the versions.
//!
//! The corpus lives in `tests/corpus`: one document per version in
//! `versions/`, and `manifest.json` — the census of the corpus, one line
//! per version.
//!
//! # How it is refreshed
//!
//! ```text
//! cargo run -p ml-minecraft --bin fetch_corpus
//! ```
//!
//! The binary downloads the manifest and every document, strips them and
//! writes them out. The test that compares the corpus with the live
//! documents strips them the same way, so a change on the side of Mojang
//! shows up as a difference in the corpus and not as a failure of the
//! launcher.

use std::collections::BTreeSet;

use ml_core::{FeatureSet, OsArch, OsName, Platform};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::version::{Argument, VersionJson};

/// The name of the file the census of the corpus is written to.
pub const MANIFEST_FILE: &str = "manifest.json";

/// The name of the directory the documents of the corpus live in.
pub const VERSIONS_DIR: &str = "versions";

/// The fields of a version document the corpus keeps.
///
/// Everything the launcher reads is here. The `downloads` of the version
/// itself stay — the SHA-1 of the client jar is persisted with the
/// version — but the `downloads` of the libraries do not: the path of a
/// library is derived from its coordinates.
const KEPT_FIELDS: &[&str] = &[
    "id",
    "type",
    "mainClass",
    "assets",
    "assetIndex",
    "javaVersion",
    "libraries",
    "arguments",
    "minecraftArguments",
    "downloads",
    "releaseTime",
];

/// The fields of a library the corpus keeps.
///
/// The `natives` and the `extract` blocks are kept although the launcher
/// does not read them: they are part of the shape of the document, and
/// the corpus is the place where the shapes are pinned.
const KEPT_LIBRARY_FIELDS: &[&str] = &["name", "rules", "natives", "extract"];

/// How the document of a version spells its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentFormat {
    /// The `arguments` block, from 1.13 on.
    Arguments,
    /// The `minecraftArguments` string, before 1.13.
    MinecraftArguments,
}

/// Where the assets of a version are read from.
///
/// The rule is taken from the document itself: a version that names an
/// asset index is read from the content-addressed store, and a version
/// that names none — the ones before 1.7 — is read from a plain tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLayout {
    /// The content-addressed store, from 1.7 on.
    ContentAddressed,
    /// The plain tree, before 1.7.
    Plain,
}

/// What the corpus knows about one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    /// The identifier of the version.
    pub id: String,
    /// The kind of the version: release, snapshot, `old_beta`, `old_alpha`.
    pub kind: String,
    /// When the version was published, in ISO-8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,
    /// How the document spells its arguments.
    pub format: ArgumentFormat,
    /// Where the assets of the version are read from.
    pub assets: AssetLayout,
    /// How many game arguments the document prescribes on the reference
    /// platform.
    pub game_arguments: usize,
    /// How many JVM arguments the document prescribes on the reference
    /// platform.
    pub jvm_arguments: usize,
    /// How many libraries the document lists.
    pub libraries: usize,
    /// The placeholders the document uses, in alphabetical order.
    pub placeholders: Vec<String>,
}

/// One version of the corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The SHA-1 of the document, as the manifest of Mojang gives it.
    pub sha1: String,
    /// What the corpus knows about the version.
    #[serde(flatten)]
    pub census: Census,
}

/// The census of the whole corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpus {
    /// When the corpus was taken.
    pub taken_at: String,
    /// The SHA-1 of the manifest the corpus was taken from.
    pub manifest_sha1: String,
    /// The versions of the corpus, in the order of the manifest.
    pub versions: Vec<Entry>,
}

impl Corpus {
    /// How many versions the corpus holds.
    #[must_use]
    pub fn count(&self) -> usize {
        self.versions.len()
    }

    /// The entry of a version, if the corpus holds it.
    #[must_use]
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.versions.iter().find(|entry| entry.census.id == id)
    }
}

/// The platform the census is counted for.
///
/// The rules of the documents depend on the system the game runs on, so
/// the numbers of the census are only meaningful together with the
/// platform they were counted for. It is the system the launcher is
/// built for.
#[must_use]
pub fn reference_platform() -> Platform {
    Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
}

/// Strip a version document down to the fields the launcher reads.
///
/// The stripping is the one place where the corpus is defined, and it is
/// used by the binary that builds the corpus and by the test that
/// compares the corpus with the live documents — so both sides always
/// read the same shape.
///
/// A document that is not an object yields an empty object.
#[must_use]
pub fn strip(raw: &Value) -> Value {
    let mut out = Map::new();
    let Some(object) = raw.as_object() else {
        return Value::Object(out);
    };

    for field in KEPT_FIELDS {
        let Some(value) = object.get(*field) else {
            continue;
        };
        let value = match *field {
            "libraries" => strip_libraries(value),
            "downloads" => strip_downloads(value),
            _ => value.clone(),
        };
        out.insert((*field).to_string(), value);
    }

    Value::Object(out)
}

/// Strip the libraries of a document.
fn strip_libraries(value: &Value) -> Value {
    let Some(libraries) = value.as_array() else {
        return value.clone();
    };

    let stripped = libraries
        .iter()
        .map(|library| {
            let mut out = Map::new();
            if let Some(object) = library.as_object() {
                for field in KEPT_LIBRARY_FIELDS {
                    if let Some(value) = object.get(*field) {
                        out.insert((*field).to_string(), value.clone());
                    }
                }
            }
            Value::Object(out)
        })
        .collect();

    Value::Array(stripped)
}

/// Strip the downloads of a version, keeping the client jar only.
///
/// The server jar is of no use to a launcher that starts the game, and
/// the downloads of the libraries are dropped by
/// [`strip_libraries`].
fn strip_downloads(value: &Value) -> Value {
    let mut out = Map::new();
    if let Some(object) = value.as_object()
        && let Some(client) = object.get("client")
    {
        out.insert("client".to_string(), client.clone());
    }
    Value::Object(out)
}

/// What the corpus knows about a version.
#[must_use]
pub fn census(document: &VersionJson) -> Census {
    let templates = document.game_templates();

    Census {
        id: document.id.clone(),
        kind: document.kind.clone(),
        release_time: document.release_time.clone(),
        format: if document.arguments.game.is_empty() {
            ArgumentFormat::MinecraftArguments
        } else {
            ArgumentFormat::Arguments
        },
        assets: if templates.iter().any(|template| template == "--assetIndex") {
            AssetLayout::ContentAddressed
        } else {
            AssetLayout::Plain
        },
        game_arguments: document
            .prescribed_game_arguments(
                reference_platform(),
                Some(&document.id),
                &FeatureSet::none(),
            )
            .len(),
        jvm_arguments: document
            .arguments
            .resolve_jvm(
                reference_platform(),
                Some(&document.id),
                &FeatureSet::none(),
            )
            .len(),
        libraries: document.libraries.len(),
        placeholders: placeholders_of(document),
    }
}

/// The placeholders a document uses, in alphabetical order.
fn placeholders_of(document: &VersionJson) -> Vec<String> {
    let mut templates = document.game_templates();
    templates.extend(document.arguments.jvm.iter().flat_map(Argument::templates));
    templates.extend(
        document
            .arguments
            .default_user_jvm
            .iter()
            .flat_map(Argument::templates),
    );

    let mut names = BTreeSet::new();
    for template in templates {
        collect_placeholders(&template, &mut names);
    }
    names.into_iter().collect()
}

/// Collect the placeholders of one template.
fn collect_placeholders(template: &str, names: &mut BTreeSet<String>) {
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        names.insert(after[..end].to_string());
        rest = &after[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> VersionJson {
        VersionJson::parse(raw.as_bytes()).expect("the document is valid")
    }

    #[test]
    fn the_stripped_document_still_parses() {
        let raw = serde_json::json!({
            "id": "1.12.2",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "1.12",
            "assetIndex": {
                "id": "1.12", "sha1": "abc", "size": 1, "totalSize": 2,
                "url": "https://example.invalid/1.12.json"
            },
            "libraries": [
                { "name": "com.mojang:logging:1.0.11", "rules": [], "downloads": {} },
                { "name": "org.lwjgl:lwjgl:3.2.3", "extract": { "exclude": ["META-INF/"] } }
            ],
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name}",
            "downloads": { "client": { "sha1": "d", "size": 3, "url": "https://example.invalid/c.jar" } },
            "releaseTime": "2018-09-12T12:47:22+00:00",
            "logging": { "client": { "argument": "-Dlog4j", "file": {}, "type": "log4j2-xml" } }
        });

        let stripped = strip(&raw);
        let document = parse(&stripped.to_string());

        assert_eq!(document.id, "1.12.2");
        assert_eq!(document.libraries.len(), 2);
        assert_eq!(
            document.minecraft_arguments.as_deref(),
            Some("--username ${auth_player_name} --version ${version_name}")
        );
        // The downloads of the libraries are gone, the client jar stays.
        assert!(document.libraries[0].downloads.artifact.is_none());
        assert!(document.downloads.client.is_some());
    }

    #[test]
    fn the_census_names_the_form_and_the_layout() {
        let legacy = parse(
            r#"{"id":"1.6.4","type":"release","assets":"legacy",
                "minecraftArguments":"--username ${auth_player_name}"}"#,
        );
        let legacy_census = census(&legacy);
        assert_eq!(legacy_census.format, ArgumentFormat::MinecraftArguments);
        assert_eq!(legacy_census.assets, AssetLayout::Plain);
        assert_eq!(legacy_census.game_arguments, 2);
        assert_eq!(legacy_census.placeholders, vec!["auth_player_name"]);

        let modern = parse(
            r#"{"id":"1.21.4","type":"release","assets":"5",
                "arguments":{"game":["--username","${auth_player_name}","--assetIndex","${assets_index_name}"]}}"#,
        );
        let modern_census = census(&modern);
        assert_eq!(modern_census.format, ArgumentFormat::Arguments);
        assert_eq!(modern_census.assets, AssetLayout::ContentAddressed);
        assert_eq!(modern_census.game_arguments, 4);
    }

    #[test]
    fn the_placeholders_are_collected_from_every_section() {
        let document = parse(
            r#"{"id":"1.21.4","type":"release",
                "arguments":{
                    "game":["--username","${auth_player_name}","--uuid","${auth_uuid}"],
                    "jvm":["-Djava.library.path=${natives_directory}"],
                    "default-user-jvm":["-Xmx2G"]
                }}"#,
        );
        let census = census(&document);
        assert_eq!(
            census.placeholders,
            vec!["auth_player_name", "auth_uuid", "natives_directory"]
        );
    }
}
