//! The Mojang version manifest.
//!
//! The manifest lists every published Minecraft version together with the
//! URL of its version JSON. It is the entry point of every update check.
//!
//! The document is re-fetched on every check but served with an `ETag`, so
//! an unchanged manifest costs one conditional request and a few hundred
//! bytes instead of a full 270 kB download. The parsed result is cached in
//! memory as well, because the manifest is read on every UI refresh.

use std::collections::HashMap;
use std::time::Duration;

use ml_core::{Error, Result, VersionKind};
use serde::{Deserialize, Serialize};

/// Where the version manifest lives.
pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// One entry of the version manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSummary {
    /// Version identifier, e.g. `26.3`.
    pub id: String,
    /// Release classification.
    pub kind: VersionKind,
    /// Absolute URL of the version JSON.
    pub url: String,
    /// ISO-8601 publication timestamp.
    pub release_time: String,
    /// SHA-1 of the version JSON.
    pub sha1: String,
}

impl VersionSummary {
    /// Whether this version is a stable release.
    #[must_use]
    pub fn is_release(&self) -> bool {
        self.kind == VersionKind::Release
    }
}

/// The parsed version manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    /// Identifier of the latest stable release.
    pub latest_release: String,
    /// Identifier of the latest snapshot.
    pub latest_snapshot: String,
    /// Every known version, newest first.
    pub versions: Vec<VersionSummary>,
}

impl VersionManifest {
    /// Look up a version by its identifier.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&VersionSummary> {
        self.versions.iter().find(|version| version.id == id)
    }

    /// The most recent stable release.
    #[must_use]
    pub fn latest_release(&self) -> Option<&VersionSummary> {
        self.find(&self.latest_release)
    }

    /// Every stable release, newest first.
    pub fn releases(&self) -> impl Iterator<Item = &VersionSummary> {
        self.versions.iter().filter(|version| version.is_release())
    }
}

// --- wire format -------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawManifest {
    latest: RawLatest,
    versions: Vec<RawVersion>,
}

#[derive(Debug, Deserialize)]
struct RawLatest {
    release: String,
    snapshot: String,
}

#[derive(Debug, Deserialize)]
struct RawVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    #[serde(rename = "releaseTime")]
    release_time: String,
    sha1: String,
}

impl From<RawManifest> for VersionManifest {
    fn from(raw: RawManifest) -> Self {
        let versions = raw
            .versions
            .into_iter()
            .map(|version| VersionSummary {
                id: version.id,
                kind: VersionKind::from_manifest(&version.kind),
                url: version.url,
                release_time: version.release_time,
                sha1: version.sha1,
            })
            .collect();

        Self {
            latest_release: raw.latest.release,
            latest_snapshot: raw.latest.snapshot,
            versions,
        }
    }
}

// --- client ------------------------------------------------------------

/// Fetches and caches the version manifest.
///
/// The client keeps the last `ETag` and the last parsed manifest, so a
/// repeated check is a conditional request that usually answers `304 Not
/// Modified` — the difference between a 270 kB download and a few hundred
/// bytes, on a path that the UI hits on every refresh.
#[derive(Debug, Clone)]
pub struct ManifestClient {
    http: reqwest::Client,
    url: String,
    etag: Option<String>,
    cached: Option<VersionManifest>,
}

impl ManifestClient {
    /// Create a client for the default Mojang manifest.
    ///
    /// # Errors
    /// Fails if the HTTP client cannot be built.
    pub fn new() -> Result<Self> {
        Self::with_url(MANIFEST_URL)
    }

    /// Create a client for a custom manifest URL.
    ///
    /// # Errors
    /// Fails if the HTTP client cannot be built.
    pub fn with_url(url: impl Into<String>) -> Result<Self> {
        let url = url.into();
        let http = reqwest::Client::builder()
            .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::network(&url, e))?;

        Ok(Self {
            http,
            url,
            etag: None,
            cached: None,
        })
    }

    /// The cached manifest, if one was fetched before.
    #[must_use]
    pub fn cached(&self) -> Option<&VersionManifest> {
        self.cached.as_ref()
    }

    /// Fetch the manifest, reusing the cached copy when the server
    /// confirms that nothing changed.
    ///
    /// # Errors
    /// Fails on a transport error, a non-success status, or a malformed
    /// document.
    pub async fn fetch(&mut self) -> Result<VersionManifest> {
        let mut request = self.http.get(&self.url);
        if let Some(etag) = &self.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::network(&self.url, e))?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(cached) = &self.cached {
                return Ok(cached.clone());
            }
            // The server said "not modified" but we have nothing to fall
            // back on. Drop the validator and ask again unconditionally.
            self.etag = None;
            return self.fetch_unconditional().await;
        }

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: self.url.clone(),
                status: response.status().as_u16(),
            });
        }

        self.etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let body = response
            .bytes()
            .await
            .map_err(|e| Error::network(&self.url, e))?;

        let manifest = parse_manifest(&body)?;
        self.cached = Some(manifest.clone());
        Ok(manifest)
    }

    /// Fetch the manifest without sending a validator.
    async fn fetch_unconditional(&mut self) -> Result<VersionManifest> {
        self.etag = None;
        let response = self
            .http
            .get(&self.url)
            .send()
            .await
            .map_err(|e| Error::network(&self.url, e))?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: self.url.clone(),
                status: response.status().as_u16(),
            });
        }

        self.etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let body = response
            .bytes()
            .await
            .map_err(|e| Error::network(&self.url, e))?;
        let manifest = parse_manifest(&body)?;
        self.cached = Some(manifest.clone());
        Ok(manifest)
    }
}

/// Parse a manifest document.
///
/// # Errors
/// Fails if the document is not valid JSON or does not have the expected
/// shape.
pub fn parse_manifest(bytes: &[u8]) -> Result<VersionManifest> {
    let raw: RawManifest =
        serde_json::from_slice(bytes).map_err(|e| Error::json("version_manifest", &e))?;
    Ok(raw.into())
}

/// Merge two manifests, keeping the newer information.
///
/// The manifest is append-only in practice, but a client that was offline
/// for a while can see a *shorter* list. Merging keeps the union so that
/// versions known from the older copy are not lost.
#[must_use]
pub fn merge_manifests(old: &VersionManifest, new: &VersionManifest) -> VersionManifest {
    let mut merged: HashMap<String, VersionSummary> = HashMap::with_capacity(new.versions.len());
    for version in &new.versions {
        merged.insert(version.id.clone(), version.clone());
    }
    for version in &old.versions {
        merged
            .entry(version.id.clone())
            .or_insert_with(|| version.clone());
    }

    let mut versions: Vec<VersionSummary> = merged.into_values().collect();
    versions.sort_by(|left, right| right.release_time.cmp(&left.release_time));

    VersionManifest {
        latest_release: new.latest_release.clone(),
        latest_snapshot: new.latest_snapshot.clone(),
        versions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "latest": {
            "release": "26.3",
            "snapshot": "26.4-snapshot-4"
        },
        "versions": [
            {
                "id": "26.4-snapshot-4",
                "type": "snapshot",
                "url": "https://piston-meta.mojang.com/v1/packages/aaa/26.4-snapshot-4.json",
                "time": "2026-10-13T12:12:37+00:00",
                "releaseTime": "2026-10-13T12:05:46+00:00",
                "sha1": "650b368231fa39ca6701be0f306ec5cc76beb8e0",
                "complianceLevel": 1
            },
            {
                "id": "26.3",
                "type": "release",
                "url": "https://piston-meta.mojang.com/v1/packages/bbb/26.3.json",
                "time": "2026-10-01T10:00:00+00:00",
                "releaseTime": "2026-10-01T09:00:00+00:00",
                "sha1": "11f1aefe473eefba0af62e3286c33b692807b2eb",
                "complianceLevel": 1
            }
        ]
    }"#;

    #[test]
    fn manifest_parses() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).unwrap();
        assert_eq!(manifest.latest_release, "26.3");
        assert_eq!(manifest.latest_snapshot, "26.4-snapshot-4");
        assert_eq!(manifest.versions.len(), 2);
    }

    #[test]
    fn releases_are_filtered_by_kind() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).unwrap();
        let releases: Vec<&str> = manifest.releases().map(|v| v.id.as_str()).collect();
        assert_eq!(releases, vec!["26.3"]);
    }

    #[test]
    fn latest_release_is_found() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).unwrap();
        let latest = manifest.latest_release().unwrap();
        assert_eq!(latest.id, "26.3");
        assert!(latest.is_release());
    }

    #[test]
    fn malformed_manifest_is_rejected() {
        let error = parse_manifest(b"{not json").unwrap_err();
        assert!(matches!(error, Error::Json { .. }));
    }

    #[test]
    fn merging_keeps_versions_from_both_sides() {
        let old = parse_manifest(SAMPLE.as_bytes()).unwrap();
        let mut new = old.clone();
        new.versions.truncate(1);
        new.latest_release = "26.4-snapshot-4".into();

        let merged = merge_manifests(&old, &new);
        assert_eq!(merged.versions.len(), 2);
        assert!(merged.find("26.3").is_some());
        assert!(merged.find("26.4-snapshot-4").is_some());
    }
}
