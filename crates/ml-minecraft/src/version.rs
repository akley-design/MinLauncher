//! The version JSON: the document that describes one Minecraft version.
//!
//! It carries the entry point, the libraries, the asset index, the
//! download links and the command line arguments. Everything the launcher
//! needs to start the game comes from here.
//!
//! # The array-or-string quirk
//!
//! In `arguments.jvm` the `value` field is **sometimes an array and
//! sometimes a bare string**. The same document mixes both forms:
//!
//! ```json
//! { "rules": [ { "action": "allow", "os": { "name": "osx" } } ],
//!   "value": [ "-XstartOnFirstThread" ] }
//! ```
//!
//! ```json
//! { "rules": [ { "action": "allow", "os": { "name": "windows" } } ],
//!   "value": "-XX:HeapDumpPath=..." }
//! ```
//!
//! A parser that expects one shape silently drops half of the arguments, and
//! the game then starts with the wrong JVM flags. [`ArgumentValue`] accepts
//! both and normalises them to a vector.

use std::collections::BTreeMap;

use ml_core::{
    Error, FeatureSet, JavaRuntime, Platform, Result, Rule, RuleOutcome, VersionMeta,
    evaluate_with_features,
};
use serde::{Deserialize, Serialize};

use crate::library::ResolvedLibrary;

// --- wire format -------------------------------------------------------

/// The `value` of an argument, which may be a string or a list of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    /// A single argument.
    Single(String),
    /// Several arguments.
    Many(Vec<String>),
}

impl ArgumentValue {
    /// The arguments as a slice, whichever form was used.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::Single(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }

    /// The arguments as an owned vector.
    #[must_use]
    pub fn to_vec(&self) -> Vec<String> {
        self.as_slice().to_vec()
    }

    /// Whether the value carries no arguments at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

/// One entry of an `arguments` list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    /// A plain argument that always applies.
    Plain(String),
    /// An argument guarded by rules.
    Guarded(GuardedArgument),
}

impl Argument {
    /// The arguments this entry contributes on the given platform.
    ///
    /// An entry with rules contributes its value only when the rules allow
    /// it; a plain entry always contributes.
    #[must_use]
    pub fn resolve(
        &self,
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        match self {
            Self::Plain(value) => vec![value.clone()],
            Self::Guarded(guarded) => {
                if evaluate_with_features(&guarded.rules, platform, version, features).is_allowed()
                {
                    guarded.value.to_vec()
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// The templates of this entry, with the values unresolved.
    ///
    /// The values still carry their placeholders and the rules are not
    /// applied: this is what the document says, not what the machine
    /// gets.
    #[must_use]
    pub fn templates(&self) -> Vec<String> {
        match self {
            Self::Plain(value) => vec![value.clone()],
            Self::Guarded(guarded) => guarded.value.to_vec(),
        }
    }

    /// The rules attached to this entry, if any.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        match self {
            Self::Plain(_) => &[],
            Self::Guarded(guarded) => &guarded.rules,
        }
    }
}

/// An argument guarded by rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedArgument {
    /// Conditions under which the argument applies.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// The argument itself.
    #[serde(alias = "values")]
    pub value: ArgumentValue,
}

/// The `arguments` block of a version JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arguments {
    /// Arguments handed to the game process.
    #[serde(default)]
    pub game: Vec<Argument>,
    /// Arguments handed to the JVM.
    #[serde(default)]
    pub jvm: Vec<Argument>,
    /// Arguments the user configured; applied before the other JVM flags.
    #[serde(default, rename = "default-user-jvm")]
    pub default_user_jvm: Vec<Argument>,
}

impl Arguments {
    /// Resolve the JVM arguments for a platform.
    #[must_use]
    pub fn resolve_jvm(
        &self,
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        Self::resolve(&self.jvm, platform, version, features)
    }

    /// Resolve the game arguments for a platform.
    #[must_use]
    pub fn resolve_game(
        &self,
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        Self::resolve(&self.game, platform, version, features)
    }

    /// Resolve the default user JVM arguments for a platform.
    ///
    /// These are the memory and garbage collector settings. Mojang ships
    /// them per platform, and the two garbage collector variants are
    /// mutually exclusive — a modern Windows gets ZGC, an old one gets the
    /// legacy G1GC flags.
    #[must_use]
    pub fn resolve_default_user_jvm(
        &self,
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        Self::resolve(&self.default_user_jvm, platform, version, features)
    }

    fn resolve(
        arguments: &[Argument],
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        arguments
            .iter()
            .flat_map(|argument| argument.resolve(platform, version, features))
            .collect()
    }
}

/// A file offered for download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDownload {
    /// SHA-1 of the payload.
    pub sha1: String,
    /// Size in bytes.
    pub size: i64,
    /// Absolute URL.
    pub url: String,
    /// The path of the file inside the library store.
    ///
    /// Mojang's documents carry it, and it is the truth: the library
    /// server serves the file under exactly that path, and a path built
    /// from the coordinate alone can differ from it — the coordinate of
    /// `org.ow2.asm:asm:9.6` has two pieces that look like a version
    /// and an extension, and the naive reading puts the jar under
    /// `asm/9/asm-9.6` instead of `asm/9.6/asm-9.6.jar`.
    ///
    /// It is empty when the document does not carry it, and then the
    /// path is built from the coordinate.
    #[serde(default)]
    pub path: String,
}

/// The `downloads` block of a version JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Downloads {
    /// The client jar.
    #[serde(default)]
    pub client: Option<FileDownload>,
    /// The server jar.
    #[serde(default)]
    pub server: Option<FileDownload>,
}

/// The asset index reference of a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndexRef {
    /// Identifier of the index, e.g. `34`.
    pub id: String,
    /// SHA-1 of the index document.
    pub sha1: String,
    /// Size of the index document in bytes.
    pub size: i64,
    /// Total size of all objects the index references.
    #[serde(rename = "totalSize", default)]
    pub total_size: i64,
    /// Absolute URL of the index document.
    pub url: String,
}

/// The Java runtime a version requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersion {
    /// Runtime family, e.g. `java-runtime-epsilon`.
    pub component: String,
    /// Java feature release the runtime implements.
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

impl From<&JavaVersion> for JavaRuntime {
    fn from(java: &JavaVersion) -> Self {
        Self::new(java.component.clone(), java.major_version)
    }
}

/// The logging configuration of a version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Logging {
    /// The client logging configuration.
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

/// The client logging configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingClient {
    /// The JVM argument that points at the configuration.
    pub argument: String,
    /// The configuration file itself.
    pub file: FileDownload,
    /// The type of the configuration, e.g. `log4j2-xml`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// A library as declared in a version JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    /// Maven coordinates, e.g. `com.mojang:logging:1.1.1`.
    pub name: String,
    /// The files of the library.
    #[serde(default)]
    pub downloads: LibraryDownloads,
    /// The direct artifact shape used by Forge version documents.
    #[serde(default)]
    pub artifact: Option<FileDownload>,
    /// Rules that decide whether the library is used.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// The download entries of a library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    /// The main jar.
    #[serde(default)]
    pub artifact: Option<FileDownload>,
    /// Native libraries, keyed by classifier.
    #[serde(default)]
    pub classifiers: BTreeMap<String, FileDownload>,
}

/// A complete version JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionJson {
    /// Version identifier.
    pub id: String,
    /// Release classification.
    #[serde(rename = "type")]
    pub kind: String,
    /// Entry point class.
    #[serde(default)]
    #[serde(rename = "mainClass")]
    pub main_class: Option<String>,
    /// Identifier of the assets directory.
    #[serde(default)]
    pub assets: Option<String>,
    /// The asset index.
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    /// The required Java runtime.
    #[serde(default, rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    /// The libraries.
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// The command line arguments.
    #[serde(default)]
    pub arguments: Arguments,
    /// The game arguments of the versions before 1.13.
    ///
    /// The versions before 1.13 spell their arguments as one string with
    /// the placeholders inside it, and the versions from 1.13 on use the
    /// `arguments` block. No document carries both, so the two forms
    /// never have to be reconciled.
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    /// The downloadable files of the version itself.
    #[serde(default)]
    pub downloads: Downloads,
    /// The logging configuration.
    #[serde(default)]
    pub logging: Option<Logging>,
    /// ISO-8601 publication timestamp.
    #[serde(default, rename = "releaseTime")]
    pub release_time: Option<String>,
    /// Minimum launcher version required.
    #[serde(default, rename = "minimumLauncherVersion")]
    pub minimum_launcher_version: Option<u32>,
}

impl VersionJson {
    /// Parse a version JSON document.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the expected
    /// shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::json("version_json", &e))
    }

    /// The id of the asset index of the version.
    ///
    /// The modern documents carry it in `assetIndex`, and the old ones in
    /// `assets`; both name the same thing, and the id is what decides
    /// where the game reads its assets from.
    #[must_use]
    pub fn asset_index_id(&self) -> Option<&str> {
        self.asset_index
            .as_ref()
            .map(|index| index.id.as_str())
            .or(self.assets.as_deref())
            .filter(|id| !id.is_empty())
    }

    /// The game arguments the document names, with the values unresolved.
    ///
    /// The values still carry their placeholders. The versions before
    /// 1.13 spell their arguments as one string, and the versions from
    /// 1.13 on use the `arguments` block; no document carries both, so
    /// the two forms never have to be reconciled.
    #[must_use]
    pub fn game_templates(&self) -> Vec<String> {
        if !self.arguments.game.is_empty() {
            return self
                .arguments
                .game
                .iter()
                .flat_map(Argument::templates)
                .collect();
        }
        self.legacy_game_templates()
    }

    /// The game arguments the document prescribes on a platform.
    ///
    /// The same two forms as [`VersionJson::game_templates`], but the
    /// rules are applied and the entries they exclude are dropped.
    #[must_use]
    pub fn prescribed_game_arguments(
        &self,
        platform: Platform,
        version: Option<&str>,
        features: &FeatureSet,
    ) -> Vec<String> {
        if !self.arguments.game.is_empty() {
            return self.arguments.resolve_game(platform, version, features);
        }
        self.legacy_game_templates()
    }

    /// The arguments of the versions before 1.13, split on the spaces.
    ///
    /// The string of those versions holds no quotes and no doubled
    /// spaces — checked over every version Mojang publishes — so the
    /// split on the whitespace is the exact reading of the document.
    fn legacy_game_templates(&self) -> Vec<String> {
        match &self.minecraft_arguments {
            Some(line) => line.split_whitespace().map(str::to_string).collect(),
            None => Vec::new(),
        }
    }

    /// The libraries that apply on the given platform.
    ///
    /// A library without rules always applies; a library with rules applies
    /// when the rules allow it. The last matching rule wins, which is what
    /// the vanilla launcher does.
    #[must_use]
    pub fn resolve_libraries(
        &self,
        platform: Platform,
        version: Option<&str>,
    ) -> Vec<ResolvedLibrary> {
        let mut libraries: Vec<ResolvedLibrary> = self
            .libraries
            .iter()
            .filter_map(|library| {
                let outcome =
                    evaluate_with_features(&library.rules, platform, version, &FeatureSet::none());
                match outcome {
                    RuleOutcome::Allowed => {
                        Some(ResolvedLibrary::from_library(library, platform, version))
                    }
                    RuleOutcome::Disallowed | RuleOutcome::Unmatched => None,
                }
            })
            .collect();

        // Some documents of the old versions name the same library twice,
        // sometimes once with natives and once without. Two identical
        // entries would download the same jar twice, fight over its
        // partial file, and violate the key of the library table. The
        // entries are merged by their full coordinate; the natives of
        // both survive.
        let mut positions: BTreeMap<(String, String), usize> = BTreeMap::new();
        let mut merged: Vec<ResolvedLibrary> = Vec::with_capacity(libraries.len());
        for library in libraries.drain(..) {
            let key = (library.name.clone(), library.coordinate.classifier.clone());
            if let Some(&index) = positions.get(&key) {
                merge_library(&mut merged[index], library);
            } else {
                positions.insert(key, merged.len());
                merged.push(library);
            }
        }
        merged
    }

    /// The Java runtime the version requires.
    ///
    /// Falls back to the table in [`ml_core::version`] when the document
    /// does not carry a `javaVersion` block, which is the case for the
    /// oldest versions.
    #[must_use]
    pub fn java_runtime(&self) -> JavaRuntime {
        self.java_version
            .as_ref()
            .map_or_else(|| JavaRuntime::fallback_for(&self.id), JavaRuntime::from)
    }

    /// The metadata that is persisted for this version.
    #[must_use]
    pub fn to_meta(&self, json_sha1: Option<String>) -> VersionMeta {
        let java = self.java_runtime();
        let asset_index = self.asset_index.as_ref();
        // The id of the index is named by `assetIndex` of the modern
        // documents and by `assets` of the old ones. The metadata stores
        // the same id the launcher reads back with, so an old version
        // keeps the name of the tree its game actually reads.
        let asset_index_id = self.asset_index_id().map(str::to_owned);

        VersionMeta {
            id: self.id.clone(),
            kind: ml_core::VersionKind::from_manifest(&self.kind),
            release_time: self.release_time.clone(),
            java_component: Some(java.component),
            java_major: Some(java.major_version),
            main_class: self.main_class.clone(),
            asset_index_id,
            asset_index_sha1: asset_index.map(|index| index.sha1.clone()),
            asset_index_size: asset_index.map(|index| index.size),
            client_sha1: self.downloads.client.as_ref().map(|file| file.sha1.clone()),
            client_size: self.downloads.client.as_ref().map(|file| file.size),
            json_sha1,
        }
    }
}

/// Add the parts of a duplicate library that the first entry lacks.
fn merge_library(base: &mut ResolvedLibrary, other: ResolvedLibrary) {
    if base.artifact.is_none() {
        base.artifact = other.artifact;
    }
    for (classifier, download) in other.natives {
        if !base.natives.iter().any(|(name, _)| name == &classifier) {
            base.natives.push((classifier, download));
        }
    }
    base.is_native |= other.is_native;
    if base.rules_json.is_none() {
        base.rules_json = other.rules_json;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{OsArch, OsName, OsVersionRange};

    fn platform() -> Platform {
        Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
    }

    fn features() -> FeatureSet {
        FeatureSet::none()
    }

    #[test]
    fn argument_value_accepts_both_shapes() {
        let single = ArgumentValue::Single("-Xss1M".into());
        assert_eq!(single.as_slice(), &["-Xss1M".to_string()]);
        assert!(!single.is_empty());

        let many = ArgumentValue::Many(vec!["-Xss1M".into(), "-Xmx4G".into()]);
        assert_eq!(many.as_slice().len(), 2);
        assert!(!many.is_empty());

        let empty = ArgumentValue::Many(Vec::new());
        assert!(empty.is_empty());
    }

    #[test]
    fn guarded_argument_respects_os_rules() {
        let argument = Argument::Guarded(GuardedArgument {
            rules: vec![Rule {
                action: ml_core::RuleAction::Allow,
                os: Some(ml_core::OsCondition {
                    name: Some("windows".into()),
                    ..ml_core::OsCondition::default()
                }),
                arch: None,
                version: None,
                features: None,
            }],
            value: ArgumentValue::Single("-XstartOnFirstThread".into()),
        });

        assert_eq!(
            argument.resolve(platform(), None, &features()),
            vec!["-XstartOnFirstThread".to_string()]
        );
    }

    #[test]
    fn guarded_argument_is_dropped_on_other_platforms() {
        let argument = Argument::Guarded(GuardedArgument {
            rules: vec![Rule {
                action: ml_core::RuleAction::Allow,
                os: Some(ml_core::OsCondition {
                    name: Some("osx".into()),
                    ..ml_core::OsCondition::default()
                }),
                arch: None,
                version: None,
                features: None,
            }],
            value: ArgumentValue::Many(vec!["-XstartOnFirstThread".into()]),
        });

        assert_eq!(
            argument.resolve(platform(), None, &features()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn plain_argument_always_applies() {
        let argument = Argument::Plain("--username".into());
        assert_eq!(
            argument.resolve(platform(), None, &features()),
            vec!["--username"]
        );
    }

    #[test]
    fn a_guarded_argument_without_rules_applies() {
        // The `default-user-jvm` block uses this shape: a value with no
        // conditions at all, which therefore always applies.
        let argument = Argument::Guarded(GuardedArgument {
            rules: Vec::new(),
            value: ArgumentValue::Many(vec!["-Xms2G".into(), "-Xmx4G".into()]),
        });

        assert_eq!(
            argument.resolve(platform(), None, &features()),
            vec!["-Xms2G".to_string(), "-Xmx4G".to_string()]
        );
    }

    /// The exact shape found in Minecraft 26.3, reproduced so that a
    /// regression in the array-or-string handling is caught here and not
    /// in production.
    ///
    /// The document was copied from the live manifest. In particular the
    /// `default-user-jvm` block carries the two mutually exclusive garbage
    /// collector variants, and the game arguments are gated behind
    /// *features* rather than operating systems.
    const REAL_26_3: &str = r#"{
        "id": "26.3",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "34",
        "assetIndex": {
            "id": "34",
            "sha1": "e787e8afd149a4cefa89d341b91f5aed8f5d93a0",
            "size": 597035,
            "totalSize": 484829952,
            "url": "https://piston-meta.mojang.com/v1/packages/e787e8afd149a4cefa89d341b91f5aed8f5d93a0/34.json"
        },
        "javaVersion": { "component": "java-runtime-epsilon", "majorVersion": 25 },
        "libraries": [
            {
                "name": "at.yawk.lz4:lz4-java:1.10.1",
                "downloads": {
                    "artifact": {
                        "path": "at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar",
                        "sha1": "f541d7f910fe3d76f38f799c507c48cc81b12ecb",
                        "size": 910232,
                        "url": "https://libraries.minecraft.net/at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
                    }
                }
            },
            {
                "name": "com.mojang:jtracy:1.14.38:natives-linux",
                "downloads": {
                    "artifact": {
                        "path": "com/mojang/jtracy/1.14.38/jtracy-1.14.38-natives-linux.jar",
                        "sha1": "d2433300f8a4e99595d513546d19ecddc3eaa15e",
                        "size": 256636,
                        "url": "https://libraries.minecraft.net/com/mojang/jtracy/1.14.38/jtracy-1.14.38-natives-linux.jar"
                    }
                },
                "rules": [ { "action": "allow", "os": { "name": "linux" } } ]
            }
        ],
        "arguments": {
            "jvm": [
                { "rules": [ { "action": "allow", "os": { "name": "osx" } } ], "value": [ "-XstartOnFirstThread" ] },
                { "rules": [ { "action": "allow", "os": { "name": "windows" } } ], "value": "-XX:HeapDumpPath=heapdump" },
                { "rules": [ { "action": "allow", "os": { "arch": "x86" } } ], "value": [ "-Xss1M" ] },
                "-XX:StackShadowPages=32",
                "--enable-native-access=ALL-UNNAMED",
                "-Djava.library.path=${natives_directory}/java"
            ],
            "game": [
                "--username", "${auth_player_name}",
                { "rules": [ { "action": "allow", "features": { "is_demo_user": true } } ], "value": "--demo" },
                { "rules": [ { "action": "allow", "features": { "has_custom_resolution": true } } ], "value": [ "--width", "${resolution_width}", "--height", "${resolution_height}" ] },
                { "rules": [ { "action": "allow", "features": { "has_quick_plays_support": true } } ], "value": [ "--quickPlayPath", "${quickPlayPath}" ] }
            ],
            "default-user-jvm": [
                { "value": [ "-Xms2G", "-Xmx4G", "-XX:+UseCompactObjectHeaders", "-XX:+AlwaysPreTouch", "-XX:+UseStringDeduplication" ] },
                { "rules": [ { "action": "allow", "os": { "name": "osx" } }, { "action": "allow", "os": { "name": "linux" } }, { "action": "allow", "os": { "name": "windows", "versionRange": { "min": "10.0.17134" } } } ], "value": [ "-XX:+UseZGC" ] },
                { "rules": [ { "action": "allow", "os": { "name": "windows", "versionRange": { "max": "10.0.17134" } } } ], "value": [ "-XX:+UnlockExperimentalVMOptions", "-XX:+UseG1GC", "-XX:G1NewSizePercent=20", "-XX:G1ReservePercent=20", "-XX:MaxGCPauseMillis=50", "-XX:G1HeapRegionSize=32M" ] }
            ]
        },
        "downloads": {
            "client": {
                "sha1": "e877b6a07acd633fb3bb475002175cec036e7b87",
                "size": 41483720,
                "url": "https://piston-data.mojang.com/v1/objects/e877b6a07acd633fb3bb475002175cec036e7b87/client.jar"
            }
        },
        "releaseTime": "2026-10-01T09:00:00+00:00"
    }"#;

    #[test]
    fn version_parses() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        assert_eq!(version.id, "26.3");
        assert_eq!(
            version.main_class.as_deref(),
            Some("net.minecraft.client.main.Main")
        );
        assert_eq!(version.java_runtime().major_version, 25);
        assert_eq!(version.libraries.len(), 2);
    }

    #[test]
    fn mixed_argument_shapes_resolve_together() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let jvm = version
            .arguments
            .resolve_jvm(platform(), Some("26.3"), &features());

        // The bare-string argument and the array arguments must both be
        // there, and the platform-specific ones must be filtered out.
        assert!(jvm.contains(&"-XX:StackShadowPages=32".to_string()));
        assert!(jvm.contains(&"-Djava.library.path=${natives_directory}/java".to_string()));
        assert!(!jvm.contains(&"-XstartOnFirstThread".to_string()));
        assert!(!jvm.contains(&"-Xss1M".to_string()));
        assert!(jvm.contains(&"-XX:HeapDumpPath=heapdump".to_string()));
    }

    #[test]
    fn default_user_jvm_is_resolved() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let user =
            version
                .arguments
                .resolve_default_user_jvm(platform(), Some("26.3"), &features());

        assert!(user.contains(&"-Xms2G".to_string()));
        assert!(user.contains(&"-Xmx4G".to_string()));
        assert!(user.contains(&"-XX:+UseCompactObjectHeaders".to_string()));
    }

    #[test]
    fn a_modern_windows_gets_zgc_and_not_the_legacy_collector() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let user =
            version
                .arguments
                .resolve_default_user_jvm(platform(), Some("26.3"), &features());

        // Windows 11 is build 26300, which is newer than 17134.
        assert!(user.contains(&"-XX:+UseZGC".to_string()));
        assert!(!user.contains(&"-XX:+UseG1GC".to_string()));
        assert!(!user.contains(&"-XX:+UnlockExperimentalVMOptions".to_string()));
    }

    #[test]
    fn an_old_windows_gets_the_legacy_collector() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let old =
            Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.16299"));
        let user = version
            .arguments
            .resolve_default_user_jvm(old, Some("26.3"), &features());

        assert!(!user.contains(&"-XX:+UseZGC".to_string()));
        assert!(user.contains(&"-XX:+UseG1GC".to_string()));
    }

    #[test]
    fn feature_gated_arguments_stay_out_of_a_normal_launch() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let game = version
            .arguments
            .resolve_game(platform(), Some("26.3"), &features());

        assert_eq!(game[0], "--username");
        assert_eq!(game[1], "${auth_player_name}");
        assert!(!game.contains(&"--demo".to_string()));
        assert!(!game.contains(&"--quickPlayPath".to_string()));
        assert!(!game.contains(&"--width".to_string()));
    }

    #[test]
    fn feature_gated_arguments_appear_when_the_feature_is_on() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let enabled = FeatureSet::of(["has_quick_plays_support"]);
        let game = version
            .arguments
            .resolve_game(platform(), Some("26.3"), &enabled);

        assert!(game.contains(&"--quickPlayPath".to_string()));
        assert!(!game.contains(&"--demo".to_string()));
    }

    #[test]
    fn library_rules_filter_by_platform() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let libraries = version.resolve_libraries(platform(), Some("26.3"));

        // The Linux-native library must be dropped on Windows, and the
        // library without rules must be kept.
        assert_eq!(libraries.len(), 1);
        assert_eq!(libraries[0].name, "at.yawk.lz4:lz4-java:1.10.1");
    }

    #[test]
    fn a_library_without_rules_is_kept() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let libraries = version.resolve_libraries(platform(), Some("26.3"));
        assert!(
            libraries
                .iter()
                .any(|library| library.name == "at.yawk.lz4:lz4-java:1.10.1")
        );
    }

    #[test]
    fn meta_carries_the_asset_index_and_client_hash() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let meta = version.to_meta(Some("abc".into()));

        assert_eq!(meta.id, "26.3");
        assert_eq!(meta.asset_index_id.as_deref(), Some("34"));
        assert_eq!(meta.asset_index_size, Some(597_035));
        assert_eq!(
            meta.client_sha1.as_deref(),
            Some("e877b6a07acd633fb3bb475002175cec036e7b87")
        );
        assert_eq!(meta.java_component.as_deref(), Some("java-runtime-epsilon"));
    }

    #[test]
    fn the_version_range_is_read_from_the_document() {
        let version = VersionJson::parse(REAL_26_3.as_bytes()).unwrap();
        let zgc = version
            .arguments
            .default_user_jvm
            .iter()
            .find(|argument| {
                argument.rules().iter().any(|rule| {
                    rule.os.as_ref().and_then(|os| os.version_range.as_ref())
                        == Some(&OsVersionRange {
                            min: Some("10.0.17134".into()),
                            max: None,
                        })
                })
            })
            .expect("the ZGC rule is present");

        assert!(zmc_flag(zgc));
    }

    /// Whether the argument carries the ZGC flag.
    fn zmc_flag(argument: &Argument) -> bool {
        argument
            .resolve(platform(), Some("26.3"), &features())
            .contains(&"-XX:+UseZGC".to_string())
    }

    #[test]
    fn forge_profile_values_and_direct_artifacts_parse() {
        let raw = r#"{
            "id":"1.20.1-forge-47.4.20",
            "type":"release",
            "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "arguments":{
                "game":[{"values":["--launchTarget","forgeclient"],"rules":[]}],
                "jvm":[{"values":["-p","${library_directory}${classpath_separator}${classpath}"],"rules":[]}]
            },
            "libraries":[{
                "name":"net.minecraftforge:forge:1.20.1-47.4.20",
                "artifact":{"path":"net/minecraftforge/forge/1.20.1-47.4.20/forge-1.20.1-47.4.20.jar","url":"https://maven.minecraftforge.net/forge.jar","sha1":"abc","size":3}
            }]
        }"#;

        let version = VersionJson::parse(raw.as_bytes()).unwrap();
        let game = version
            .arguments
            .resolve_game(platform(), Some(&version.id), &features());
        let jvm = version
            .arguments
            .resolve_jvm(platform(), Some(&version.id), &features());
        let resolved = version.resolve_libraries(platform(), Some(&version.id));

        assert_eq!(game, vec!["--launchTarget", "forgeclient"]);
        assert!(
            jvm.iter()
                .any(|argument| argument.contains("${classpath_separator}"))
        );
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].artifact_path().as_deref(),
            Some("net/minecraftforge/forge/1.20.1-47.4.20/forge-1.20.1-47.4.20.jar")
        );
    }

    #[test]
    fn duplicate_library_entries_are_merged() {
        let version = VersionJson::parse(
            br#"{
                "id": "1.12.2",
                "type": "release",
                "libraries": [
                    {"name": "com.mojang:text2speech:1.10.3"},
                    {"name": "com.mojang:text2speech:1.10.3",
                     "downloads": {"classifiers": {
                        "natives-windows": {
                            "path": "com/mojang/text2speech/1.10.3/text2speech-1.10.3-natives-windows.jar",
                            "sha1": "abc", "size": 1, "url": "https://example/text2speech.jar"
                        }
                     }}}
                ]
            }"#,
        )
        .unwrap();

        let libraries = version.resolve_libraries(platform(), Some("1.12.2"));

        assert_eq!(libraries.len(), 1, "the duplicate was not merged");
        assert_eq!(libraries[0].natives.len(), 1);
        assert!(libraries[0].native_jar(platform()).is_some());
    }
}
