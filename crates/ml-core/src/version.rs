//! Domain model for Minecraft versions and the Java runtimes they require.

use serde::{Deserialize, Serialize};

/// How Mojang classifies a version in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionKind {
    /// A stable release.
    Release,
    /// A pre-release / snapshot build.
    Snapshot,
    /// A beta-era build.
    OldBeta,
    /// An alpha-era build.
    OldAlpha,
}

impl VersionKind {
    /// Parse the manifest's `type` string.
    #[must_use]
    pub fn from_manifest(raw: &str) -> Self {
        match raw {
            "release" => Self::Release,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            // "snapshot" and anything the launcher does not know.
            _ => Self::Snapshot,
        }
    }

    /// The manifest's `type` string for this classification.
    ///
    /// Round-trips with [`VersionKind::from_manifest`], so the value can be
    /// stored in the database and read back without loss.
    #[must_use]
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Snapshot => "snapshot",
            Self::OldBeta => "old_beta",
            Self::OldAlpha => "old_alpha",
        }
    }
}

/// A Mojang-published Java runtime.
///
/// The `component` string identifies the runtime family (for example
/// `java-runtime-epsilon`); `major_version` is the Java feature release it
/// implements (25 for epsilon).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JavaRuntime {
    /// Runtime family, e.g. `java-runtime-epsilon`.
    pub component: String,
    /// Java major version implemented by this runtime.
    pub major_version: u32,
}

/// Mojang runtime component names, as published in the runtime manifest.
pub mod components {
    /// Java 8 — Minecraft 1.16.5 and older.
    pub const LEGACY: &str = "jre-legacy";
    /// Java 16 — Minecraft 1.17.
    pub const ALPHA: &str = "java-runtime-alpha";
    /// Java 17 — Minecraft 1.18 … 1.20.4.
    pub const BETA: &str = "java-runtime-beta";
    /// Java 17 — newer Java 17 runtime family.
    pub const GAMMA: &str = "java-runtime-gamma";
    /// Java 21 — Minecraft 1.20.5 … 1.21.x.
    pub const DELTA: &str = "java-runtime-delta";
    /// Java 25 — Minecraft 26.x and newer.
    pub const EPSILON: &str = "java-runtime-epsilon";
}

impl JavaRuntime {
    /// Build a runtime descriptor from the values found in a version manifest.
    #[must_use]
    pub fn new(component: impl Into<String>, major_version: u32) -> Self {
        Self {
            component: component.into(),
            major_version,
        }
    }

    /// Best-effort runtime for a version id, used when the version manifest
    /// entry has not been fetched yet.
    ///
    /// The authoritative source is always the `javaVersion` block of the
    /// version JSON; this table only avoids a network round-trip.
    #[must_use]
    pub fn fallback_for(version_id: &str) -> Self {
        const BOUNDARIES: &[((u64, u64, u64), &str, u32)] = &[
            ((1, 17, 0), components::ALPHA, 16),
            ((1, 18, 0), components::GAMMA, 17),
            ((1, 20, 5), components::DELTA, 21),
            ((26, 1, 0), components::EPSILON, 25),
        ];

        let key = comparable(version_id);

        let mut chosen = (components::LEGACY, 8u32);
        for (boundary, component, major) in BOUNDARIES {
            if key >= *boundary {
                chosen = (component, *major);
            }
        }
        Self::new(chosen.0, chosen.1)
    }

    /// Whether this runtime satisfies a required Java major version.
    #[must_use]
    pub fn satisfies(&self, required_major: u32) -> bool {
        self.major_version >= required_major
    }
}

/// Turn a version id into a comparable numeric triple.
///
/// Handles both the historic `1.20.5` scheme and the year-based `26.3`
/// scheme introduced for the 2026 releases.
#[must_use]
pub fn comparable(version_id: &str) -> (u64, u64, u64) {
    let core = version_id
        .split(['-', '_', '+'])
        .next()
        .unwrap_or(version_id);
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major, minor, patch)
}

/// Metadata about a Minecraft version that the launcher persists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionMeta {
    /// Version identifier, e.g. `26.3` or `1.20.5`.
    pub id: String,
    /// Release classification.
    pub kind: VersionKind,
    /// ISO-8601 release timestamp.
    pub release_time: Option<String>,
    /// Required Java runtime family.
    pub java_component: Option<String>,
    /// Required Java major version.
    pub java_major: Option<u32>,
    /// Entry point class.
    pub main_class: Option<String>,
    /// Asset index id.
    pub asset_index_id: Option<String>,
    /// Asset index SHA-1.
    pub asset_index_sha1: Option<String>,
    /// Asset index size in bytes.
    pub asset_index_size: Option<i64>,
    /// Client jar SHA-1.
    pub client_sha1: Option<String>,
    /// Client jar size in bytes.
    pub client_size: Option<i64>,
    /// SHA-1 of the version JSON itself.
    pub json_sha1: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_matches_observed_manifests() {
        // These expectations were verified against Mojang's live version
        // manifest in October 2026.
        assert_eq!(
            JavaRuntime::fallback_for("1.8.9").component,
            components::LEGACY
        );
        assert_eq!(JavaRuntime::fallback_for("1.16.5").major_version, 8);
        assert_eq!(JavaRuntime::fallback_for("1.17").major_version, 16);
        assert_eq!(JavaRuntime::fallback_for("1.18").major_version, 17);
        assert_eq!(JavaRuntime::fallback_for("1.20.4").major_version, 17);
        assert_eq!(JavaRuntime::fallback_for("1.20.5").major_version, 21);
        assert_eq!(JavaRuntime::fallback_for("1.21.11").major_version, 21);
        assert_eq!(JavaRuntime::fallback_for("26.3").major_version, 25);
        assert_eq!(
            JavaRuntime::fallback_for("26.4-snapshot-4").major_version,
            25
        );
    }

    #[test]
    fn comparable_handles_both_version_schemes() {
        assert_eq!(comparable("1.20.5"), (1, 20, 5));
        assert_eq!(comparable("26.3"), (26, 3, 0));
        assert_eq!(comparable("26.4-snapshot-4"), (26, 4, 0));
        assert_eq!(comparable("rd-132211"), (0, 0, 0));
    }

    #[test]
    fn satisfies_compares_majors() {
        let r = JavaRuntime::new(components::EPSILON, 25);
        assert!(r.satisfies(21));
        assert!(!r.satisfies(26));
    }

    #[test]
    fn kind_parses_manifest_values() {
        assert_eq!(VersionKind::from_manifest("release"), VersionKind::Release);
        assert_eq!(
            VersionKind::from_manifest("snapshot"),
            VersionKind::Snapshot
        );
        assert_eq!(VersionKind::from_manifest("old_beta"), VersionKind::OldBeta);
    }
}
