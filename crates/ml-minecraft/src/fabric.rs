//! The Fabric loader: the profile, the libraries and the way the game is
//! started with them.
//!
//! Fabric is the loader that lets a mod change the game itself. The
//! launcher does not have to run the installer of Fabric: the loader
//! remaps the game jar by itself the first time it starts, so the launcher
//! only has to hand it the right libraries and the right entry point.
//!
//! The profile is a *child* version document — the same shape Mojang uses
//! for its own versions that build on another one:
//!
//! ```json
//! {
//!   "id": "fabric-loader-0.16.9-1.21.4",
//!   "inheritsFrom": "1.21.4",
//!   "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
//!   "arguments": { "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
//!   "libraries": [ { "name": "net.fabricmc:fabric-loader:0.16.9", "url": "https://maven.fabricmc.net/" } ]
//! }
//! ```
//!
//! The libraries of the profile are Maven coordinates served from a Maven
//! repository, and each of them carries the checksum of the file. The
//! launcher puts them in the same library store it uses for the libraries of
//! Mojang, so that nothing is downloaded twice.

use serde::{Deserialize, Serialize};

use ml_core::{Error, Result};

use crate::library::{Coordinate, ResolvedLibrary};
use crate::version::FileDownload;

/// The address of the metadata of the Fabric project.
pub const META_URL: &str = "https://meta.fabricmc.net/v2";

/// The address the libraries of the loader are served from.
pub const MAVEN_URL: &str = "https://maven.fabricmc.net/";

/// The entry point of the client of the loader.
pub const KNOT_CLIENT: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";

/// The loader tells the game which class to start through this property.
///
/// The value is the entry point of the *vanilla* game, and the loader
/// starts it itself once the mods are loaded.
pub const EMULATION_PROPERTY: &str = "FabricMcEmu";

/// The name of the version the profile of the loader has.
#[must_use]
pub fn profile_id(minecraft_version: &str, loader_version: &str) -> String {
    format!("fabric-loader-{loader_version}-{minecraft_version}")
}

/// Parse a Maven coordinate of the profile of the loader.
///
/// The coordinate of a library of the loader is always
/// `group:artifact:version`, and the version is taken as it is written.
///
/// It cannot be read with [`Coordinate::parse`], which tries to guess the
/// extension of a version from the last piece of it: the version `1.21.4`
/// survives that guess, but a version like `2.5` would be read as the
/// version `2` with the extension `5`, and the file would be looked for
/// under a name the repository does not serve.
fn fabric_coordinate(name: &str) -> Option<Coordinate> {
    let mut parts = name.split(':');
    let group = parts.next().unwrap_or_default();
    let artifact = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    let classifier = parts.next().unwrap_or_default();

    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }

    Some(Coordinate {
        group: group.to_string(),
        artifact: artifact.to_string(),
        version: version.to_string(),
        classifier: classifier.to_string(),
        extension: "jar".to_string(),
    })
}

/// The address of the profile of the loader.
///
/// The *v1* endpoint is the one that carries the whole profile, the
/// libraries of the loader included. The v2 endpoint carries only the
/// dependencies of the loader.
#[must_use]
pub fn profile_url(minecraft_version: &str, loader_version: &str) -> String {
    format!("{META_URL}/versions/loader/{minecraft_version}/{loader_version}/profile/json")
}

/// The address of the list of the loaders that exist for a version of the
/// game.
#[must_use]
pub fn loader_list_url(minecraft_version: &str) -> String {
    format!("{META_URL}/versions/loader/{minecraft_version}")
}

/// A library of the profile of the loader.
///
/// The shape is the Maven one: a coordinate, the address of the repository
/// that serves it, and the checksums of the file. It is not the shape Mojang
/// uses, which is why this is a type of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricLibrary {
    /// The Maven coordinate, e.g. `net.fabricmc:fabric-loader:0.16.9`.
    pub name: String,
    /// The address of the repository that serves the file.
    #[serde(default)]
    pub url: Option<String>,
    /// The SHA-1 of the file.
    #[serde(default)]
    pub sha1: Option<String>,
    /// The SHA-256 of the file.
    #[serde(default)]
    pub sha256: Option<String>,
    /// The MD5 of the file.
    #[serde(default)]
    pub md5: Option<String>,
    /// The size of the file, in bytes.
    #[serde(default)]
    pub size: Option<i64>,
}

/// The arguments of the profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricArguments {
    /// The arguments of the game.
    #[serde(default)]
    pub game: Vec<String>,
    /// The arguments of the JVM.
    #[serde(default)]
    pub jvm: Vec<String>,
}

/// The profile of the loader for one version of the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricProfile {
    /// The name of the profile.
    pub id: String,
    /// The version of the game the profile is built on.
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    /// The class the game is started with.
    #[serde(rename = "mainClass", default)]
    pub main_class: Option<String>,
    /// The arguments of the profile.
    #[serde(default)]
    pub arguments: FabricArguments,
    /// The libraries the loader needs.
    #[serde(default)]
    pub libraries: Vec<FabricLibrary>,
}

impl FabricProfile {
    /// Parse a profile document.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the expected
    /// shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::json("fabric_profile", &e))
    }

    /// The entry point of the client, or the one of the loader.
    ///
    /// The loader is the one that starts the game, and it is the entry
    /// point the launcher has to use.
    #[must_use]
    pub fn client_main_class(&self) -> &str {
        self.main_class.as_deref().unwrap_or(KNOT_CLIENT)
    }

    /// The libraries of the profile, in the form the launcher stores them.
    ///
    /// The files of the libraries are served from a Maven repository, and
    /// their path in the library store is the path they have in the
    /// repository — which is the same layout Mojang's library server uses.
    #[must_use]
    pub fn resolved_libraries(&self) -> Vec<ResolvedLibrary> {
        self.libraries
            .iter()
            .filter_map(|library| {
                let coordinate = fabric_coordinate(&library.name)?;
                let base = library.url.as_deref().unwrap_or(MAVEN_URL);
                let relative = coordinate.artifact_path();
                let url = format!("{base}{relative}");

                Some(ResolvedLibrary {
                    name: library.name.clone(),
                    coordinate,
                    is_native: false,
                    artifact: Some(FileDownload {
                        url,
                        sha1: library.sha1.clone().unwrap_or_default(),
                        size: library.size.unwrap_or(0),
                    }),
                    natives: Vec::new(),
                    rules_json: None,
                })
            })
            .collect()
    }

    /// The arguments of the JVM of the profile, with the property that
    /// tells the loader which class of the game to start.
    ///
    /// The property the profile carries is kept as it is, and the one that
    /// names the entry point of the game is added when it is not there.
    #[must_use]
    pub fn jvm_arguments(&self) -> Vec<String> {
        let mut arguments = self.arguments.jvm.clone();

        let tells_the_game = arguments
            .iter()
            .any(|argument| argument.starts_with(&format!("-D{EMULATION_PROPERTY}=")));

        if !tells_the_game {
            arguments.push(format!(
                "-D{EMULATION_PROPERTY}= net.minecraft.client.main.Main "
            ));
        }

        arguments
    }

    /// The files the profile needs, in the form the download engine takes.
    ///
    /// The target is the path of the file inside the library store.
    ///
    /// The checksum is `None` when the profile does not carry one. The
    /// profile of the loader leaves the checksum of two of its libraries —
    /// the loader itself and the intermediary — empty, and an empty
    /// checksum must not be taken for a checksum that fails.
    #[must_use]
    pub fn downloads(&self) -> Vec<(String, String, Option<String>)> {
        self.libraries
            .iter()
            .filter_map(|library| {
                let coordinate = fabric_coordinate(&library.name)?;
                let base = library.url.as_deref().unwrap_or(MAVEN_URL);
                let relative = coordinate.artifact_path();
                let url = format!("{base}{relative}");
                let sha1 = library
                    .sha1
                    .as_deref()
                    .filter(|hash| !hash.is_empty())
                    .map(ToString::to_string);
                Some((relative, url, sha1))
            })
            .collect()
    }
}

/// What the loader adds to a launch.
///
/// The loader does not replace the game, it wraps it: the game is still
/// the game of Mojang, with its own libraries and its own arguments, and
/// the loader is put in front of it. This is what that front part consists
/// of — the libraries to put on the class path, the class to start, and the
/// arguments that tell the loader which class of the game to start itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricLaunch {
    /// The version of the game the loader is for.
    pub minecraft_version: String,
    /// The version of the loader.
    pub loader_version: String,
    /// The class the game is started with.
    pub main_class: String,
    /// The libraries of the loader, in the form the launcher stores them.
    pub libraries: Vec<ResolvedLibrary>,
    /// The arguments the loader adds to the JVM.
    pub jvm_arguments: Vec<String>,
}

impl FabricLaunch {
    /// Build the launch part of a loader from its profile.
    #[must_use]
    pub fn from_profile(
        minecraft_version: &str,
        loader_version: &str,
        profile: &FabricProfile,
    ) -> Self {
        Self {
            minecraft_version: minecraft_version.to_string(),
            loader_version: loader_version.to_string(),
            main_class: profile.client_main_class().to_string(),
            libraries: profile.resolved_libraries(),
            jvm_arguments: profile.jvm_arguments(),
        }
    }

    /// The files the loader needs, in the form the download engine takes.
    ///
    /// The target is the path of the file inside the library store.
    #[must_use]
    pub fn downloads(profile: &FabricProfile) -> Vec<(String, String, Option<String>)> {
        profile.downloads()
    }

    /// The arguments of the game the profile carries.
    ///
    /// The profile of the loader carries no arguments of the game of its
    /// own — the arguments of the game come from the version of Mojang —
    /// but they are kept here so that a profile that does carry them is
    /// not silently dropped.
    #[must_use]
    pub fn game_arguments(profile: &FabricProfile) -> Vec<String> {
        profile.arguments.game.clone()
    }
}

/// The list of the loaders that exist for a version of the game.
///
/// The address of the list serves an **array**, not an object: every entry
/// of it carries the loader itself, the intermediary that goes with it and
/// the metadata the launcher needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FabricLoaderList {
    /// The loaders, the newest first.
    pub loaders: Vec<FabricLoaderEntry>,
}

/// One loader of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricLoaderEntry {
    /// The loader itself.
    pub loader: FabricLoader,
    /// The intermediary that goes with the loader.
    #[serde(rename = "intermediary", default)]
    pub intermediary: Option<FabricLoader>,
    /// The metadata the launcher needs.
    #[serde(rename = "launcherMeta", default)]
    pub launcher_meta: Option<FabricLauncherMeta>,
}

/// The loader itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricLoader {
    /// The version of the loader.
    pub version: String,
    /// Whether the loader is marked as stable.
    #[serde(default)]
    pub stable: bool,
    /// The maven coordinate of the loader.
    #[serde(default)]
    pub maven: Option<String>,
}

/// The metadata the launcher needs to start the game with the loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricLauncherMeta {
    /// The version of the metadata.
    #[serde(default)]
    pub version: i64,
    /// The least version of Java the loader needs.
    #[serde(rename = "min_java_version", default)]
    pub min_java_version: Option<i64>,
    /// The libraries of the loader, by the side of the game they are for.
    #[serde(default)]
    pub libraries: FabricMetaLibraries,
    /// The classes the game is started with.
    #[serde(rename = "mainClass", default)]
    pub main_class: Option<FabricMetaMainClass>,
}

/// The libraries of the metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricMetaLibraries {
    /// The libraries that are only needed by the client.
    #[serde(default)]
    pub client: Vec<FabricLibrary>,
    /// The libraries that both sides of the game need.
    #[serde(default)]
    pub common: Vec<FabricLibrary>,
}

/// The classes the game is started with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricMetaMainClass {
    /// The class of the client.
    #[serde(default)]
    pub client: Option<String>,
    /// The class of the server.
    #[serde(default)]
    pub server: Option<String>,
}

impl FabricLoaderList {
    /// Parse the list of the loaders.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the expected
    /// shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::json("fabric_loaders", &e))
    }

    /// The newest loader of the list, whether it is stable or not.
    #[must_use]
    pub fn newest(&self) -> Option<&FabricLoaderEntry> {
        self.loaders.first()
    }

    /// The newest stable loader of the list.
    #[must_use]
    pub fn newest_stable(&self) -> Option<&FabricLoaderEntry> {
        self.loaders.iter().find(|entry| entry.loader.stable)
    }

    /// The loader of the given version.
    #[must_use]
    pub fn find(&self, version: &str) -> Option<&FabricLoaderEntry> {
        self.loaders
            .iter()
            .find(|entry| entry.loader.version == version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"{
      "id": "fabric-loader-0.16.9-1.21.4",
      "inheritsFrom": "1.21.4",
      "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
      "arguments": { "game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
      "libraries": [
        { "name": "org.ow2.asm:asm:9.7.1", "url": "https://maven.fabricmc.net/",
          "sha1": "f0ed132a49244b042cd0e15702ab9f2ce3cc8436", "size": 126093 },
        { "name": "net.fabricmc:fabric-loader:0.16.9", "url": "https://maven.fabricmc.net/",
          "sha1": "aaaa", "size": 100 }
      ]
    }"#;

    #[test]
    fn the_profile_is_read() {
        let profile = FabricProfile::parse(PROFILE.as_bytes()).unwrap();

        assert_eq!(profile.id, "fabric-loader-0.16.9-1.21.4");
        assert_eq!(profile.inherits_from.as_deref(), Some("1.21.4"));
        assert_eq!(profile.client_main_class(), KNOT_CLIENT);
        assert_eq!(profile.libraries.len(), 2);
    }

    #[test]
    fn the_libraries_are_put_in_the_library_store() {
        let profile = FabricProfile::parse(PROFILE.as_bytes()).unwrap();
        let libraries = profile.resolved_libraries();

        assert_eq!(libraries.len(), 2);
        let loader = &libraries[1];
        assert_eq!(
            loader.coordinate.artifact_path(),
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(
            loader.artifact.as_ref().unwrap().url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
    }

    #[test]
    fn the_files_of_the_profile_are_listed_for_the_download() {
        let profile = FabricProfile::parse(PROFILE.as_bytes()).unwrap();
        let downloads = profile.downloads();

        assert_eq!(downloads.len(), 2);
        assert_eq!(downloads[0].0, "org/ow2/asm/asm/9.7.1/asm-9.7.1.jar");
        assert_eq!(
            downloads[0].2.as_deref(),
            Some("f0ed132a49244b042cd0e15702ab9f2ce3cc8436")
        );
    }

    #[test]
    fn the_property_that_names_the_game_is_kept() {
        let profile = FabricProfile::parse(PROFILE.as_bytes()).unwrap();
        let arguments = profile.jvm_arguments();

        assert_eq!(arguments.len(), 1);
        assert!(arguments[0].starts_with("-DFabricMcEmu="));
    }

    #[test]
    fn the_property_that_names_the_game_is_added_when_it_is_missing() {
        let profile =
            FabricProfile::parse(br#"{"id": "x", "arguments": { "jvm": ["-Xmx1G"] }}"#).unwrap();
        let arguments = profile.jvm_arguments();

        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0], "-Xmx1G");
        assert!(arguments[1].starts_with("-DFabricMcEmu="));
    }

    #[test]
    fn the_profile_of_a_loader_is_named_after_both_versions() {
        assert_eq!(
            profile_id("1.21.4", "0.16.9"),
            "fabric-loader-0.16.9-1.21.4"
        );
    }

    #[test]
    fn the_address_of_the_profile_is_the_one_of_the_project() {
        assert_eq!(
            profile_url("1.21.4", "0.16.9"),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.4/0.16.9/profile/json"
        );
    }

    #[test]
    fn the_list_of_the_loaders_is_read() {
        let list = FabricLoaderList::parse(
            br#"[
              { "loader": { "separator": ".", "build": 0, "maven": "net.fabricmc:fabric-loader:0.20.0",
                           "version": "0.20.0", "stable": false },
                "intermediary": { "maven": "net.fabricmc:intermediary:1.21.4", "version": "1.21.4", "stable": true },
                "launcherMeta": { "version": 2, "min_java_version": 8,
                                  "libraries": { "client": [],
                                                 "common": [ { "name": "net.fabricmc:sponge-mixin:0.17.5+mixin.0.8.7",
                                                               "url": "https://maven.fabricmc.net/" } ] },
                                  "mainClass": { "client": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                                                 "server": "net.fabricmc.loader.impl.launch.knot.KnotServer" } } },
              { "loader": { "version": "0.19.5", "stable": true, "maven": "net.fabricmc:fabric-loader:0.19.5" },
                "intermediary": { "version": "1.21.4", "stable": true } }
            ]"#,
        )
        .unwrap();

        assert_eq!(list.loaders.len(), 2);
        assert_eq!(list.newest().unwrap().loader.version, "0.20.0");
        assert_eq!(list.newest_stable().unwrap().loader.version, "0.19.5");
        assert_eq!(list.find("0.20.0").unwrap().loader.version, "0.20.0");
        assert!(list.find("0.1.0").is_none());
    }

    #[test]
    fn the_metadata_of_a_loader_carries_the_class_of_the_client() {
        let list = FabricLoaderList::parse(
            br#"[{ "loader": { "version": "0.20.0", "stable": false },
                   "launcherMeta": { "version": 2, "min_java_version": 8,
                                     "libraries": { "client": [], "common": [] },
                                     "mainClass": { "client": "net.fabricmc.loader.impl.launch.knot.KnotClient" } } }]"#,
        )
        .unwrap();

        let meta = list.newest().unwrap().launcher_meta.as_ref().unwrap();
        assert_eq!(
            meta.main_class.as_ref().unwrap().client.as_deref(),
            Some(KNOT_CLIENT)
        );
        assert_eq!(meta.min_java_version, Some(8));
    }

    #[test]
    fn a_profile_that_is_broken_is_an_error() {
        assert!(FabricProfile::parse(b"{ not json").is_err());
    }

    #[test]
    fn a_version_with_two_pieces_is_not_mistaken_for_an_extension() {
        // The guess of `Coordinate::parse` would read this as the version
        // `2` with the extension `5`, and the repository does not serve
        // such a file.
        let coordinate = fabric_coordinate("org.example:thing:2.5").unwrap();

        assert_eq!(coordinate.version, "2.5");
        assert_eq!(coordinate.extension, "jar");
        assert_eq!(
            coordinate.artifact_path(),
            "org/example/thing/2.5/thing-2.5.jar"
        );
    }

    #[test]
    fn a_coordinate_that_is_not_one_is_refused() {
        assert!(fabric_coordinate("org.example:thing").is_none());
        assert!(fabric_coordinate(":thing:1.0").is_none());
    }

    #[test]
    fn the_libraries_of_the_loader_keep_their_whole_version() {
        let profile = FabricProfile::parse(
            br#"{"id": "x", "libraries": [
                 { "name": "net.fabricmc:intermediary:1.21.4", "url": "https://maven.fabricmc.net/" },
                 { "name": "net.fabricmc:sponge-mixin:0.15.5+mixin.0.8.7", "url": "https://maven.fabricmc.net/" }
               ]}"#,
        )
        .unwrap();
        let libraries = profile.resolved_libraries();

        assert_eq!(
            libraries[0].coordinate.artifact_path(),
            "net/fabricmc/intermediary/1.21.4/intermediary-1.21.4.jar"
        );
        assert_eq!(
            libraries[1].coordinate.artifact_path(),
            "net/fabricmc/sponge-mixin/0.15.5+mixin.0.8.7/sponge-mixin-0.15.5+mixin.0.8.7.jar"
        );
    }
}
