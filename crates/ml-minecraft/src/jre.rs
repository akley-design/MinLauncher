//! Java runtimes: the runtime manifest and the files a runtime consists of.
//!
//! Mojang publishes runtimes in two steps. The *outer* manifest maps a
//! platform and a runtime family onto a small metadata document:
//!
//! ```text
//! windows-x64 -> java-runtime-epsilon -> [{ manifest: {url, sha1, size},
//!                                              version: {name, released} }]
//! ```
//!
//! The document the outer manifest points at is the *inner* manifest: a map
//! of the files the runtime consists of. Every file is offered twice, once
//! raw and once compressed with LZMA; the launcher picks the raw variant
//! and decompresses the LZMA one only when the raw download fails.
//!
//! The runtime is then unpacked into a directory that looks exactly like a
//! Java installation, so that `bin/java.exe` is where the game expects it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ml_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Where the runtime manifest lives.
pub const JRE_MANIFEST_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// A runtime as described by the outer manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEntry {
    /// The manifest of the runtime's file list.
    pub manifest: ManifestFile,
    /// The version of the runtime.
    pub version: RuntimeVersion,
}

/// The manifest reference of a runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    /// SHA-1 of the manifest document.
    pub sha1: String,
    /// Size of the manifest document in bytes.
    pub size: i64,
    /// Absolute URL of the manifest document.
    pub url: String,
}

/// The version of a runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersion {
    /// The version string, e.g. `25.0.1`.
    pub name: String,
    /// ISO-8601 publication timestamp.
    #[serde(default)]
    pub released: Option<String>,
}

/// The outer runtime manifest: platform to runtime family to entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeManifest {
    /// The runtimes, keyed by platform and then by family.
    pub platforms: BTreeMap<String, BTreeMap<String, Vec<RuntimeEntry>>>,
}

impl RuntimeManifest {
    /// Parse an outer runtime manifest.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the
    /// expected shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::json("jre_manifest", &e))
    }

    /// The runtimes of a platform.
    #[must_use]
    pub fn for_platform(&self, platform: &str) -> Option<&BTreeMap<String, Vec<RuntimeEntry>>> {
        self.platforms.get(platform)
    }

    /// The newest runtime of a family on a platform.
    ///
    /// The manifest lists the runtimes of a family newest first, so the
    /// first entry is the one to use.
    #[must_use]
    pub fn entry(&self, platform: &str, component: &str) -> Option<&RuntimeEntry> {
        self.for_platform(platform)?.get(component)?.first()
    }
}

/// The platform key Mojang uses for the current host.
#[must_use]
pub fn platform_key() -> String {
    use ml_core::{OsArch, OsName, Platform};

    let platform = Platform::current();
    let os = match platform.os {
        OsName::Windows => "windows",
        OsName::Osx => "mac-os",
        OsName::Linux => "linux",
        OsName::Unknown => "unknown",
    };
    let arch = match platform.arch {
        OsArch::X86_64 => "x64",
        OsArch::X86 => "x86",
        OsArch::Arm64 => "arm64",
        OsArch::Unknown => "unknown",
    };
    format!("{os}-{arch}")
}

/// The kind of a file inside a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeFileType {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// One download offered for a runtime file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDownload {
    /// SHA-1 of the payload.
    pub sha1: String,
    /// Size in bytes.
    pub size: i64,
    /// Absolute URL.
    pub url: String,
}

/// The downloads of a runtime file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDownloads {
    /// The plain file.
    #[serde(default)]
    pub raw: Option<RuntimeDownload>,
    /// The LZMA-compressed file.
    #[serde(default)]
    pub lzma: Option<RuntimeDownload>,
}

/// One file of a runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFile {
    /// The kind of the entry.
    #[serde(rename = "type")]
    pub kind: RuntimeFileType,
    /// The downloads, absent for directories.
    #[serde(default)]
    pub downloads: RuntimeDownloads,
    /// Whether the file has to be marked executable.
    #[serde(default)]
    pub executable: Option<bool>,
}

/// The inner manifest: the files a runtime consists of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFileList {
    /// The files, keyed by their path inside the runtime.
    pub files: BTreeMap<String, RuntimeFile>,
}

impl RuntimeFileList {
    /// Parse an inner runtime manifest.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the
    /// expected shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::json("jre_file_list", &e))
    }

    /// The files that have to be downloaded.
    pub fn downloadable(&self) -> impl Iterator<Item = (&String, &RuntimeFile)> {
        self.files
            .iter()
            .filter(|(_, file)| file.kind == RuntimeFileType::File)
    }

    /// The total size of all files.
    #[must_use]
    pub fn total_size(&self) -> i64 {
        self.files
            .values()
            .filter_map(|file| file.downloads.raw.as_ref())
            .map(|download| download.size)
            .sum()
    }
}

/// The path of the `java` executable inside an unpacked runtime.
#[must_use]
pub fn java_executable(runtime_dir: &Path) -> PathBuf {
    runtime_dir
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

/// The path of the `javaw` executable inside an unpacked runtime.
///
/// On Windows the game is started with `javaw.exe` so that no console
/// window appears.
#[must_use]
pub fn javaw_executable(runtime_dir: &Path) -> PathBuf {
    runtime_dir
        .join("bin")
        .join(if cfg!(windows) { "javaw.exe" } else { "java" })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTER: &str = r#"{
        "windows-x64": {
            "java-runtime-epsilon": [
                {
                    "availability": { "group": 6030, "progress": 100 },
                    "manifest": {
                        "sha1": "b374544c680d965fb5535977d7cb04c6befe1930",
                        "size": 152664,
                        "url": "https://piston-meta.mojang.com/v1/packages/b374544c680d965fb5535977d7cb04c6befe1930/manifest.json"
                    },
                    "version": { "name": "25.0.1", "released": "2025-12-10T14:14:17+00:00" }
                }
            ],
            "jre-legacy": [
                {
                    "manifest": {
                        "sha1": "03828bb8455f3cef94869286a0d941dd16933239",
                        "size": 80031,
                        "url": "https://piston-meta.mojang.com/v1/packages/03828bb8455f3cef94869286a0d941dd16933239/manifest.json"
                    },
                    "version": { "name": "8u51-cacert462b08" }
                }
            ]
        }
    }"#;

    const INNER: &str = r#"{
        "files": {
            "bin": { "type": "directory" },
            "bin/api-ms-win-core-console-l1-1-0.dll": {
                "downloads": {
                    "lzma": {
                        "sha1": "6966f0b36abed6f27cfc93ceb575986de4e56774",
                        "size": 7354,
                        "url": "https://piston-data.mojang.com/v1/objects/6966f0b36abed6f27cfc93ceb575986de4e56774/api-ms-win-core-console-l1-1-0.dll"
                    },
                    "raw": {
                        "sha1": "fc7b39ea653d61f0e04faf229404b05c0c003753",
                        "size": 22056,
                        "url": "https://piston-data.mojang.com/v1/objects/fc7b39ea653d61f0e04faf229404b05c0c003753/api-ms-win-core-console-l1-1-0.dll"
                    }
                },
                "executable": true,
                "type": "file"
            }
        }
    }"#;

    #[test]
    fn outer_manifest_parses() {
        let manifest = RuntimeManifest::parse(OUTER.as_bytes()).unwrap();
        let entry = manifest
            .entry("windows-x64", "java-runtime-epsilon")
            .unwrap();
        assert_eq!(entry.version.name, "25.0.1");
        assert_eq!(entry.manifest.size, 152_664);
    }

    #[test]
    fn missing_platform_or_family_is_none() {
        let manifest = RuntimeManifest::parse(OUTER.as_bytes()).unwrap();
        assert!(
            manifest
                .entry("linux-x64", "java-runtime-epsilon")
                .is_none()
        );
        assert!(
            manifest
                .entry("windows-x64", "java-runtime-delta")
                .is_none()
        );
    }

    #[test]
    fn inner_manifest_parses() {
        let list = RuntimeFileList::parse(INNER.as_bytes()).unwrap();
        assert_eq!(list.files.len(), 2);
        assert_eq!(list.total_size(), 22056);

        let downloadable = list.downloadable().count();
        assert_eq!(downloadable, 1);
    }

    #[test]
    fn platform_key_follows_the_host() {
        let key = platform_key();
        assert!(key.contains('-'));
        #[cfg(target_os = "windows")]
        assert!(key.starts_with("windows-"));
    }

    #[test]
    fn java_executable_uses_the_bin_directory() {
        let dir = Path::new("/tmp/runtime");
        assert!(
            java_executable(dir).ends_with("java.exe") || java_executable(dir).ends_with("java")
        );
        assert!(java_executable(dir).parent().unwrap().ends_with("bin"));
    }
}
