//! Fetch the corpus of the versions of the game.
//!
//! The binary downloads the manifest of Mojang and every version document
//! it names, strips them and writes them into the corpus of the tests.
//! It is run by hand, when the corpus is refreshed:
//!
//! ```text
//! cargo run -p ml-minecraft --bin fetch_corpus
//! ```
//!
//! The result is checked into the repository, so that the test of the
//! contract runs without the network.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::TryStreamExt;
use futures::stream::StreamExt;
use ml_minecraft::corpus::{self, Corpus, Entry};
use ml_minecraft::manifest::{MANIFEST_URL, VersionSummary, parse_manifest};
use ml_minecraft::version::VersionJson;
use serde_json::Value;
use sha1::{Digest, Sha1};

/// How many documents are fetched at the same time.
const CONCURRENCY: usize = 16;

#[tokio::main]
async fn main() -> Result<(), String> {
    let root = corpus_root();
    let versions_dir = root.join(corpus::VERSIONS_DIR);
    std::fs::create_dir_all(&versions_dir).map_err(|e| e.to_string())?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    println!("fetching the manifest ...");
    let manifest_bytes = http
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let manifest = parse_manifest(&manifest_bytes).map_err(|e| e.to_string())?;
    println!("{} versions in the manifest", manifest.versions.len());

    let entries = fetch_all(&http, &manifest.versions).await?;
    remove_stale(&versions_dir, &entries)?;

    let corpus = Corpus {
        taken_at: chrono::Utc::now().to_rfc3339(),
        manifest_sha1: sha1_hex(&manifest_bytes),
        versions: entries,
    };

    let json = serde_json::to_string_pretty(&corpus).map_err(|e| e.to_string())?;
    std::fs::write(root.join(corpus::MANIFEST_FILE), format!("{json}\n"))
        .map_err(|e| e.to_string())?;

    println!("{} documents in the corpus", corpus.count());
    Ok(())
}

/// The root of the corpus, next to the tests that read it.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

/// Fetch every document of the manifest, in the order of the manifest.
async fn fetch_all(
    http: &reqwest::Client,
    versions: &[VersionSummary],
) -> Result<Vec<Entry>, String> {
    let mut collected: Vec<(usize, Entry)> =
        futures::stream::iter(versions.iter().enumerate())
            .map(|(index, version)| async move {
                Ok::<_, String>((index, fetch_one(http, version).await?))
            })
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;

    // The order of the manifest is the order of the corpus, whatever
    // order the documents arrived in.
    collected.sort_by_key(|(index, _)| *index);
    Ok(collected.into_iter().map(|(_, entry)| entry).collect())
}

/// Fetch one document, strip it and write it into the corpus.
async fn fetch_one(http: &reqwest::Client, version: &VersionSummary) -> Result<Entry, String> {
    let response = http
        .get(&version.url)
        .send()
        .await
        .map_err(|e| format!("{}: {e}", version.id))?;
    if !response.status().is_success() {
        return Err(format!("{}: HTTP {}", version.id, response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("{}: {e}", version.id))?;

    let raw: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", version.id))?;
    let stripped = corpus::strip(&raw);

    // The stripped document is the one the corpus holds, so it is the one
    // that has to parse.
    let document = VersionJson::parse(stripped.to_string().as_bytes())
        .map_err(|e| format!("{}: {e}", version.id))?;

    let census = corpus::census(&document);

    let path = corpus_root()
        .join(corpus::VERSIONS_DIR)
        .join(format!("{}.json", version.id));
    std::fs::write(&path, stripped.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;

    Ok(Entry {
        sha1: version.sha1.clone(),
        census,
    })
}

/// Remove the documents of versions Mojang has dropped.
fn remove_stale(dir: &Path, entries: &[Entry]) -> Result<(), String> {
    let keep: BTreeSet<&str> = entries
        .iter()
        .map(|entry| entry.census.id.as_str())
        .collect();

    let items = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for item in items {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if !keep.contains(id) {
            std::fs::remove_file(item.path()).map_err(|e| e.to_string())?;
            println!("removed the stale document {id}");
        }
    }
    Ok(())
}

/// The SHA-1 of a payload, in hexadecimal.
fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}
