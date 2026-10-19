//! Libraries: Maven coordinates, artifact paths and native extraction.
//!
//! A version JSON lists its libraries by Maven coordinates, e.g.
//! `com.mojang:logging:1.1.1`. The coordinates are turned into a path
//! inside the library store, mirroring Maven's own layout:
//!
//! ```text
//! com.mojang:logging:1.1.1
//!   -> com/mojang/logging/1.1.1/logging-1.1.1.jar
//! ```
//!
//! Libraries that carry native code (OpenAL, LWJGL) are shipped as jars
//! with a `natives-<platform>` classifier. Those jars have to be unpacked
//! before the game can load the shared libraries from them.

use ml_core::{Error, Platform, Result};
use serde::{Deserialize, Serialize};

use crate::version::{FileDownload, Library};

/// The parsed form of a Maven coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coordinate {
    /// Group id, e.g. `com.mojang`.
    pub group: String,
    /// Artifact id, e.g. `logging`.
    pub artifact: String,
    /// Version, e.g. `1.1.1`.
    pub version: String,
    /// Classifier, e.g. `natives-windows`; empty for plain libraries.
    pub classifier: String,
    /// File extension of the artifact, `jar` for everything Mojang and
    /// Fabric publish.
    ///
    /// The extension is never taken from the version. A version of two
    /// pieces is a version — `org.ow2.asm:asm:9.6` is the version 9.6 —
    /// and reading its last piece as an extension puts the jar under the
    /// wrong path, where the library server never put it.
    pub extension: String,
}

impl Coordinate {
    /// Parse a Maven coordinate.
    ///
    /// # Errors
    /// Fails when the coordinate does not have the expected
    /// `group:artifact:version` shape.
    pub fn parse(raw: &str) -> Result<Self> {
        let mut parts = raw.split(':');
        let group = parts.next().unwrap_or_default();
        let artifact = parts.next().unwrap_or_default();
        let version = parts.next().unwrap_or_default();
        let classifier = parts.next().unwrap_or_default();

        if group.is_empty() || artifact.is_empty() || version.is_empty() {
            return Err(Error::InvalidInput(format!(
                "`{raw}` is not a maven coordinate"
            )));
        }

        Ok(Self {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
            classifier: classifier.to_string(),
            extension: "jar".to_string(),
        })
    }

    /// The path of the artifact inside the library store.
    ///
    /// The layout matches Maven's, which is also what Mojang's library
    /// server serves: `group/with/slashes/artifact/version/file`.
    #[must_use]
    pub fn artifact_path(&self) -> String {
        let group = self.group.replace('.', "/");
        let version = &self.version;
        let file = if self.classifier.is_empty() {
            format!("{}-{}.{}", self.artifact, version, self.extension)
        } else {
            format!(
                "{}-{}-{}.{}",
                self.artifact, version, self.classifier, self.extension
            )
        };
        format!("{group}/{}/{version}/{file}", self.artifact)
    }

    /// Whether the coordinate describes a native library.
    #[must_use]
    pub fn is_native(&self) -> bool {
        Classifier::parse(&self.classifier).natives
    }

    /// The identity of the artifact, without its version.
    ///
    /// Two coordinates with the same identity are two versions of the
    /// same artifact, and only one of them may be on the classpath: the
    /// classes of the newer one have to win, or the game breaks.
    ///
    /// The identity is the group, the artifact and the classifier. The
    /// extension is not part of it, because the same artifact may be
    /// published both as a jar and as something else.
    #[must_use]
    pub fn identity(&self) -> String {
        if self.classifier.is_empty() {
            format!("{}:{}", self.group, self.artifact)
        } else {
            format!("{}:{}:{}", self.group, self.artifact, self.classifier)
        }
    }
}

/// The operating system a classifier names.
///
/// Mojang has used two spellings for macOS over the years, so both are
/// recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassifierOs {
    /// Windows.
    Windows,
    /// Linux.
    Linux,
    /// macOS, spelled `macos` or `osx`.
    Macos,
}

impl ClassifierOs {
    /// The name Mojang uses for the system.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
        }
    }

    /// Whether the name spells this system.
    #[must_use]
    pub fn is(self, name: &str) -> bool {
        match self {
            Self::Windows => name == "windows",
            Self::Linux => name == "linux",
            // Older versions spell macOS `osx`.
            Self::Macos => name == "macos" || name == "osx",
        }
    }
}

/// A parsed library classifier.
///
/// Mojang uses classifiers in two shapes, both of which name a system:
///
/// * `natives-<os>` and `natives-<os>-<arch>` name a jar of native
///   code, e.g. `natives-windows` and `natives-windows-arm64`;
/// * `<os>-<arch>` name a plain jar of one system, e.g.
///   `linux-x86_64`.
///
/// The architecture is optional, and the rules of a version usually only
/// look at the system: a version that ships `natives-windows` and
/// `natives-windows-arm64` lets both through on Windows, because the two
/// jars hold the binaries of different architectures side by side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classifier {
    /// The system the classifier names, if any.
    pub os: Option<ClassifierOs>,
    /// The architecture the classifier names, if any.
    pub arch: Option<String>,
    /// Whether the classifier names a jar of native code.
    pub natives: bool,
}

impl Classifier {
    /// The classifier of a plain library, without a system.
    #[must_use]
    pub fn plain() -> Self {
        Self {
            os: None,
            arch: None,
            natives: false,
        }
    }

    /// Parse a classifier.
    #[must_use]
    pub fn parse(classifier: &str) -> Self {
        if let Some(rest) = classifier.strip_prefix("natives-") {
            let (os, arch) = split_system(rest);
            return Self {
                os,
                arch,
                natives: true,
            };
        }

        let (os, arch) = split_system(classifier);
        Self {
            os,
            arch,
            natives: false,
        }
    }

    /// Whether the classifier names the given platform.
    ///
    /// A classifier that names no system at all belongs to every platform.
    #[must_use]
    pub fn matches(&self, platform: Platform) -> bool {
        match self.os {
            Some(os) => os.is(platform.os.as_str()),
            None => true,
        }
    }
}

/// Split a system part into the system and the architecture.
///
/// The system is the longest of the known names the text starts with, so
/// that `macos-arm64` is not read as `mac` plus `os-arm64`.
fn split_system(text: &str) -> (Option<ClassifierOs>, Option<String>) {
    const SYSTEMS: &[(&str, ClassifierOs)] = &[
        ("windows", ClassifierOs::Windows),
        ("linux", ClassifierOs::Linux),
        ("macos", ClassifierOs::Macos),
        ("osx", ClassifierOs::Macos),
    ];

    for (name, os) in SYSTEMS {
        if text == *name {
            return (Some(*os), None);
        }
        if let Some(rest) = text.strip_prefix(name)
            && let Some(arch) = rest.strip_prefix('-')
            && !arch.is_empty()
        {
            return (Some(*os), Some(arch.to_string()));
        }
    }

    (None, None)
}

/// A library that applies on the current platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLibrary {
    /// The original Maven coordinate.
    pub name: String,
    /// The parsed coordinate.
    pub coordinate: Coordinate,
    /// Whether the library carries native code.
    pub is_native: bool,
    /// The main jar, if the library has one.
    pub artifact: Option<FileDownload>,
    /// The native jars, keyed by classifier.
    pub natives: Vec<(String, FileDownload)>,
    /// The rules the library was selected by, kept for diagnostics.
    pub rules_json: Option<String>,
}

impl ResolvedLibrary {
    /// Build a resolved library from a version JSON entry.
    ///
    /// Mojang has shipped natives in two different shapes, and both are
    /// still in the wild:
    ///
    /// * the **coordinate** form, where the classifier is part of the
    ///   Maven name — `org.lwjgl:lwjgl:3.4.3:natives-windows`. This is
    ///   what every version since 1.19 uses, and what 26.3 uses
    ///   exclusively;
    /// * the **downloads** form, where the classifier is a key of
    ///   `downloads.classifiers` — the shape older versions used.
    ///
    /// The natives of a library are recognised in both cases, so the
    /// launcher can extract them regardless of the version.
    #[must_use]
    pub fn from_library(library: &Library, platform: Platform, _version: Option<&str>) -> Self {
        let coordinate = Coordinate::parse(&library.name).unwrap_or_else(|_| Coordinate {
            group: String::new(),
            artifact: library.name.clone(),
            version: String::new(),
            classifier: String::new(),
            extension: "jar".to_string(),
        });

        let mut natives = Vec::new();
        for (classifier, file) in &library.downloads.classifiers {
            let parsed = Classifier::parse(classifier);
            if parsed.natives && parsed.matches(platform) {
                natives.push((classifier.clone(), file.clone()));
            }
        }

        // The coordinate form: the library *is* the natives jar. The
        // classifier has to name the system of the platform, otherwise
        // the natives of another system would be unpacked as well.
        let coordinate_classifier = Classifier::parse(&coordinate.classifier);
        let coordinate_is_native =
            coordinate_classifier.natives && coordinate_classifier.matches(platform);

        let is_native = coordinate_is_native || !natives.is_empty();

        Self {
            name: library.name.clone(),
            coordinate,
            is_native,
            artifact: library.downloads.artifact.clone(),
            natives,
            rules_json: library
                .rules
                .iter()
                .map(|rule| serde_json::to_string(rule).ok())
                .collect::<Option<Vec<_>>>()
                .map(|rules| rules.join("\n")),
        }
    }

    /// The path of the main jar relative to the library store.
    ///
    /// The path of the document wins over the one built from the
    /// coordinate: Mojang's own server puts the file under that path,
    /// and the coordinate of some libraries cannot be read back into
    /// the same path — `org.ow2.asm:asm:9.6` is the one that shows it.
    ///
    /// Returns `None` for a library that has no main artifact.
    #[must_use]
    pub fn artifact_path(&self) -> Option<String> {
        let artifact = self.artifact.as_ref()?;
        if artifact.path.is_empty() {
            Some(self.coordinate.artifact_path())
        } else {
            Some(artifact.path.clone())
        }
    }

    /// The path of a native jar relative to the library store.
    ///
    /// The path of the document wins here as well, for the same reason.
    #[must_use]
    pub fn native_path(&self, classifier: &str) -> Option<String> {
        let (_, download) = self.natives.iter().find(|(name, _)| name == classifier)?;
        if download.path.is_empty() {
            let mut coordinate = self.coordinate.clone();
            coordinate.classifier = classifier.to_string();
            Some(coordinate.artifact_path())
        } else {
            Some(download.path.clone())
        }
    }

    /// The jar that carries the native code of this library.
    ///
    /// In the coordinate form the library's own artifact *is* the natives
    /// jar; in the downloads form it is the entry of `downloads.classifiers`
    /// that matches the platform.
    ///
    /// Returns `None` when the library carries no native code for the
    /// platform: the natives of another system are never unpacked.
    #[must_use]
    pub fn native_jar(&self, platform: Platform) -> Option<&FileDownload> {
        let classifier = Classifier::parse(&self.coordinate.classifier);
        if classifier.natives {
            if !classifier.matches(platform) {
                return None;
            }
            return self.artifact.as_ref();
        }
        self.natives
            .iter()
            .find(|(name, _)| Classifier::parse(name).matches(platform))
            .map(|(_, file)| file)
    }
}

/// The family a native library belongs to.
///
/// Modern versions give every native family its own directory inside the
/// natives folder, because each library unpacks itself there: LWJGL into
/// `lwjgl`, JNA into `jna`, Netty into `netty`, and everything that is
/// loaded through `java.library.path` into `java`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeFamily {
    /// Unpacked by LWJGL itself, into `SharedLibraryExtractPath`.
    Lwjgl,
    /// Unpacked by JNA itself, into `jna.tmpdir`.
    Jna,
    /// Unpacked by Netty itself, into `io.netty.native.workdir`.
    Netty,
    /// Loaded through `java.library.path`.
    System,
}

impl NativeFamily {
    /// The subdirectory of the natives folder this family belongs in.
    #[must_use]
    pub fn directory(self) -> &'static str {
        match self {
            Self::Lwjgl => "lwjgl",
            Self::Jna => "jna",
            Self::Netty => "netty",
            Self::System => "java",
        }
    }

    /// The family a library belongs to, judged by its Maven group.
    ///
    /// The groups are stable across every version that ships the library.
    #[must_use]
    pub fn of_group(group: &str) -> Self {
        match group {
            "org.lwjgl" => Self::Lwjgl,
            "net.java.dev.jna" => Self::Jna,
            "io.netty" => Self::Netty,
            _ => Self::System,
        }
    }
}

/// The canonical native classifier of a platform.
///
/// Mojang names the natives after the operating system, not the
/// architecture: `natives-windows`, `natives-linux`, `natives-macos`.
///
/// The classifier is only a convenience: a version may also ship an
/// architecture specific variant (`natives-windows-arm64`), and older
/// versions spell macOS `natives-osx`. The launcher matches classifiers
/// by the system they name, so all of these are recognised — see
/// [`Classifier`].
#[must_use]
pub fn native_classifier(platform: Platform) -> &'static str {
    use ml_core::OsName;
    match platform.os {
        OsName::Windows => "natives-windows",
        OsName::Osx => "natives-macos",
        OsName::Linux => "natives-linux",
        OsName::Unknown => "natives-unknown",
    }
}

/// The directory name a native library is extracted into.
///
/// Mojang's own launcher groups the extracted files by a short name
/// (`windows-x64`, `linux`) and a hash of the library name, so that two
/// libraries with the same file name do not overwrite each other.
#[must_use]
pub fn natives_directory_name(platform: Platform) -> String {
    use ml_core::OsName;
    let os = match platform.os {
        OsName::Windows => "windows",
        OsName::Osx => "osx",
        OsName::Linux => "linux",
        OsName::Unknown => "unknown",
    };
    let arch = platform.arch.as_str();
    format!("{os}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{Library, LibraryDownloads};
    use ml_core::{OsArch, OsName};
    use std::collections::BTreeMap;

    fn platform() -> Platform {
        Platform::new(OsName::Windows, OsArch::X86_64)
    }

    #[test]
    fn plain_coordinate_parses() {
        let coordinate = Coordinate::parse("com.mojang:logging:1.1.1").unwrap();
        assert_eq!(coordinate.group, "com.mojang");
        assert_eq!(coordinate.artifact, "logging");
        assert_eq!(coordinate.version, "1.1.1");
        assert_eq!(coordinate.classifier, "");
        assert_eq!(coordinate.extension, "jar");
    }

    #[test]
    fn classifier_is_parsed() {
        let coordinate = Coordinate::parse("com.mojang:jtracy:1.14.38:natives-linux").unwrap();
        assert_eq!(coordinate.classifier, "natives-linux");
        assert!(coordinate.is_native());
    }

    #[test]
    fn artifact_path_matches_the_maven_layout() {
        let coordinate = Coordinate::parse("com.mojang:logging:1.1.1").unwrap();
        assert_eq!(
            coordinate.artifact_path(),
            "com/mojang/logging/1.1.1/logging-1.1.1.jar"
        );
    }

    #[test]
    fn artifact_path_includes_the_classifier() {
        let coordinate = Coordinate::parse("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap();
        assert_eq!(
            coordinate.artifact_path(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
        );
    }

    #[test]
    fn dotted_group_becomes_slashes() {
        let coordinate = Coordinate::parse("at.yawk.lz4:lz4-java:1.10.1").unwrap();
        assert_eq!(
            coordinate.artifact_path(),
            "at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
        );
    }

    #[test]
    fn a_version_of_two_pieces_is_a_version() {
        // The game of 1.21.4 carries eight libraries whose version has
        // two pieces, `org.ow2.asm:asm:9.6` among them. Reading the
        // last piece as an extension puts the jar under a path the
        // library server never uses.
        let coordinate = Coordinate::parse("org.ow2.asm:asm:9.6").unwrap();
        assert_eq!(coordinate.version, "9.6");
        assert_eq!(coordinate.extension, "jar");
        assert_eq!(
            coordinate.artifact_path(),
            "org/ow2/asm/asm/9.6/asm-9.6.jar"
        );
    }

    #[test]
    fn the_identity_of_a_coordinate_has_no_version() {
        let old = Coordinate::parse("org.ow2.asm:asm:9.6").unwrap();
        let new = Coordinate::parse("org.ow2.asm:asm:9.10.1").unwrap();
        assert_eq!(old.identity(), new.identity());

        // The classifier is part of the identity: the natives of a
        // library are a different artifact from the library itself.
        let native = Coordinate::parse("org.lwjgl:lwjgl:3.4.3:natives-windows").unwrap();
        let plain = Coordinate::parse("org.lwjgl:lwjgl:3.4.3").unwrap();
        assert_ne!(native.identity(), plain.identity());
    }

    #[test]
    fn broken_coordinate_is_rejected() {
        assert!(Coordinate::parse("nonsense").is_err());
        assert!(Coordinate::parse("a:b").is_err());
    }

    #[test]
    fn native_classifier_follows_the_platform() {
        assert_eq!(native_classifier(platform()), "natives-windows");

        let linux = Platform::new(OsName::Linux, OsArch::X86_64);
        assert_eq!(native_classifier(linux), "natives-linux");
    }

    #[test]
    fn natives_directory_is_named_after_os_and_arch() {
        assert_eq!(natives_directory_name(platform()), "windows-x86_64");
    }

    #[test]
    fn coordinate_form_natives_are_recognised() {
        // This is the shape every version since 1.19 uses: the classifier
        // is part of the Maven name and the natives jar is the artifact.
        let library = Library {
            name: "org.lwjgl:lwjgl:3.4.3:natives-windows".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 178_848,
                    url: "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-windows.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert!(resolved.is_native);
        assert_eq!(resolved.coordinate.classifier, "natives-windows");
        assert_eq!(
            resolved.artifact_path().as_deref(),
            Some("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-windows.jar")
        );
        assert!(resolved.native_jar(platform()).is_some());
    }

    #[test]
    fn downloads_form_natives_are_recognised() {
        // The older shape: the classifier is a key of `downloads.classifiers`.
        let mut classifiers = BTreeMap::new();
        classifiers.insert(
            "natives-windows".to_string(),
            FileDownload {
                path: String::new(),
                sha1: "abc".into(),
                size: 1,
                url: "https://example.invalid/native.jar".into(),
            },
        );
        let library = Library {
            name: "org.lwjgl:lwjgl:3.3.1".into(),
            downloads: LibraryDownloads {
                artifact: None,
                classifiers,
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("1.16.5"));

        assert!(resolved.is_native);
        assert_eq!(
            resolved.native_path("natives-windows").as_deref(),
            Some("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar")
        );
        assert!(resolved.native_jar(platform()).is_some());
    }

    #[test]
    fn the_path_of_the_document_wins_over_the_coordinate() {
        // Mojang says where the file is, and the library server serves
        // it under that path. The coordinate alone is not always enough
        // to find it again.
        let library = Library {
            name: "org.ow2.asm:asm:9.6".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: "org/ow2/asm/asm/9.6/asm-9.6.jar".into(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://libraries.minecraft.net/org/ow2/asm/asm/9.6/asm-9.6.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert_eq!(
            resolved.artifact_path().as_deref(),
            Some("org/ow2/asm/asm/9.6/asm-9.6.jar")
        );
    }

    #[test]
    fn the_coordinate_is_the_fallback_when_the_document_has_no_path() {
        let library = Library {
            name: "com.google.code.gson:gson:2.14.0".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://example.invalid/gson.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert_eq!(
            resolved.artifact_path().as_deref(),
            Some("com/google/code/gson/gson/2.14.0/gson-2.14.0.jar")
        );
    }

    #[test]
    fn a_plain_library_has_no_natives() {
        let library = Library {
            name: "com.google.code.gson:gson:2.14.0".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://example.invalid/gson.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert!(!resolved.is_native);
        assert!(resolved.native_jar(platform()).is_none());
    }

    #[test]
    fn the_natives_of_another_system_are_ignored() {
        // A version may list the natives of every system; only the ones
        // of the platform the launcher runs on may be unpacked.
        let library = Library {
            name: "org.lwjgl:lwjgl:3.4.3:natives-linux".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://example.invalid/lwjgl.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert!(!resolved.is_native);
        assert!(resolved.native_jar(platform()).is_none());
    }

    #[test]
    fn the_classifier_names_a_system() {
        let classifier = Classifier::parse("natives-windows");
        assert_eq!(classifier.os, Some(ClassifierOs::Windows));
        assert!(classifier.natives);
        assert_eq!(classifier.arch, None);

        let arm = Classifier::parse("natives-windows-arm64");
        assert_eq!(arm.os, Some(ClassifierOs::Windows));
        assert!(arm.natives);
        assert_eq!(arm.arch.as_deref(), Some("arm64"));

        // The old spelling of macOS is understood as well.
        let osx = Classifier::parse("natives-osx");
        assert_eq!(osx.os, Some(ClassifierOs::Macos));
        assert!(osx.natives);
    }

    #[test]
    fn a_platform_classifier_names_a_system_but_is_not_native() {
        // Netty ships one plain jar per system, e.g. `linux-x86_64`.
        let classifier = Classifier::parse("linux-x86_64");
        assert_eq!(classifier.os, Some(ClassifierOs::Linux));
        assert_eq!(classifier.arch.as_deref(), Some("x86_64"));
        assert!(!classifier.natives);
    }

    #[test]
    fn a_classifier_of_another_system_does_not_match() {
        let linux = Platform::new(OsName::Linux, OsArch::X86_64);

        assert!(Classifier::parse("natives-windows").matches(platform()));
        assert!(!Classifier::parse("natives-windows").matches(linux));
        assert!(Classifier::parse("natives-linux").matches(linux));
        assert!(!Classifier::parse("natives-linux").matches(platform()));
        // The architecture of a variant does not change the system.
        assert!(Classifier::parse("natives-windows-arm64").matches(platform()));
    }

    #[test]
    fn an_arm_variant_is_recognised_as_a_native() {
        // Mojang ships the binaries of the two architectures in two jars
        // of their own, both of which the rules of the version allow.
        let library = Library {
            name: "org.lwjgl:lwjgl:3.4.3:natives-windows-arm64".into(),
            downloads: LibraryDownloads {
                artifact: Some(FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 152_235,
                    url: "https://example.invalid/lwjgl-arm64.jar".into(),
                }),
                classifiers: BTreeMap::new(),
            },
            rules: Vec::new(),
        };

        let resolved = ResolvedLibrary::from_library(&library, platform(), Some("26.3"));

        assert!(resolved.is_native);
        assert!(resolved.native_jar(platform()).is_some());
        assert_eq!(
            resolved.coordinate.artifact_path(),
            "org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-windows-arm64.jar"
        );
    }

    #[test]
    fn native_families_map_to_directories() {
        assert_eq!(NativeFamily::of_group("org.lwjgl"), NativeFamily::Lwjgl);
        assert_eq!(
            NativeFamily::of_group("net.java.dev.jna"),
            NativeFamily::Jna
        );
        assert_eq!(NativeFamily::of_group("io.netty"), NativeFamily::Netty);
        assert_eq!(NativeFamily::of_group("com.mojang"), NativeFamily::System);

        assert_eq!(NativeFamily::Lwjgl.directory(), "lwjgl");
        assert_eq!(NativeFamily::Jna.directory(), "jna");
        assert_eq!(NativeFamily::Netty.directory(), "netty");
        assert_eq!(NativeFamily::System.directory(), "java");
    }
}
