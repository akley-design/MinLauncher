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
//! raw and once compressed with LZMA; the launcher takes the raw variant.
//!
//! The runtime is then unpacked into a directory that looks exactly like a
//! Java installation, so that `bin/java.exe` is where the game expects it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ml_core::{Error, Result, safe_relative_path};
use serde::{Deserialize, Serialize};

use crate::download::DownloadTask;

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

/// What has to be fetched to unpack a runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePlan {
    /// Where the runtime is unpacked.
    pub runtime_dir: PathBuf,
    /// The files to download, in the order the manifest lists them.
    pub files: Vec<PlannedRuntimeFile>,
    /// The directories to create, parents included.
    pub directories: Vec<PathBuf>,
    /// The total size of the downloads.
    pub total_size: i64,
}

/// One file of a runtime, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRuntimeFile {
    /// The path of the file inside the runtime, as the manifest writes it.
    pub relative: String,
    /// Where the file is written.
    pub target: PathBuf,
    /// The download of the plain file.
    pub download: RuntimeDownload,
    /// Whether the file has to be marked executable.
    pub executable: bool,
}

/// The plan of a runtime.
///
/// The runtime is not an archive: the manifest lists every file of it
/// separately, and each one is fetched on its own. The plan is that list,
/// with the directories the files need and the place each file goes.
///
/// # Errors
/// Fails when a path of the manifest is not a safe relative path, or
/// when a file of the manifest offers no download at all.
pub fn plan_runtime(list: &RuntimeFileList, runtime_dir: &Path) -> Result<RuntimePlan> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut total_size = 0;

    for (relative, file) in &list.files {
        // The path is remote data: it is checked before it is joined, so
        // that a manifest cannot write outside the runtime directory.
        let safe = safe_relative_path(relative)?;
        let target = runtime_dir.join(&safe);

        if file.kind == RuntimeFileType::Directory {
            directories.push(target);
            continue;
        }

        // Only the plain download is used. The LZMA variant is the same
        // content compressed, and writing it as the file itself would
        // leave a file the JVM cannot read.
        //
        // shortcut: the LZMA variant is not decoded, because every
        // published runtime offers the plain download for every one of
        // its files; add a decoder when a runtime ever does not.
        let Some(download) = file.downloads.raw.as_ref() else {
            return Err(Error::InvalidInput(format!(
                "the file `{relative}` of the runtime offers no plain download"
            )));
        };

        total_size += download.size;
        files.push(PlannedRuntimeFile {
            relative: relative.clone(),
            target,
            download: download.clone(),
            executable: file.executable.unwrap_or(false),
        });
    }

    // Every directory a file needs, parents included. The manifest
    // lists the directories itself, but a file may also live in a
    // directory it does not mention.
    let mut all: Vec<PathBuf> = directories.clone();
    for file in &files {
        let mut parent = file.target.parent().map(Path::to_path_buf);
        while let Some(directory) = parent {
            if directory == runtime_dir || !directory.starts_with(runtime_dir) {
                break;
            }
            all.push(directory.clone());
            parent = directory.parent().map(Path::to_path_buf);
        }
    }
    all.sort();
    all.dedup();

    Ok(RuntimePlan {
        runtime_dir: runtime_dir.to_path_buf(),
        files,
        directories: all,
        total_size,
    })
}

/// The tasks that fetch the files of a runtime.
#[must_use]
pub fn runtime_tasks(plan: &RuntimePlan) -> Vec<DownloadTask> {
    plan.files
        .iter()
        .map(|file| {
            DownloadTask::new(
                file.download.url.clone(),
                file.target.clone(),
                Some(file.download.sha1.clone()),
            )
            .with_size(Some(file.download.size))
        })
        .collect()
}

/// Unpack a runtime: create its directories and mark its executables.
///
/// The files themselves are downloaded by the download engine; this is
/// what has to happen around them. The directories are created first, so
/// that a file never has to compete with its own parent directory, and
/// the executables are marked last, because only a file that is really
/// there can be marked.
///
/// # Errors
/// Fails when a directory or a file cannot be created.
pub fn unpack_runtime(plan: &RuntimePlan) -> Result<()> {
    for directory in &plan.directories {
        std::fs::create_dir_all(directory).map_err(|e| Error::io(directory, e))?;
    }

    // The executables are marked on the systems that have permission
    // bits. On Windows the permission bits of a file do not decide
    // whether it can be started, so there is nothing to mark.
    #[cfg(unix)]
    for file in plan.files.iter().filter(|file| file.executable) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(&file.target)
            .map_err(|e| Error::io(&file.target, e))?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(&file.target, permissions)
            .map_err(|e| Error::io(&file.target, e))?;
    }

    Ok(())
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

    /// A manifest whose paths try to leave the runtime directory.
    const HOSTILE: &str = r#"{
        "files": {
            "bin": { "type": "directory" },
            "bin/java.exe": {
                "downloads": {
                    "raw": {
                        "sha1": "38dc560c6ff23b550a9df5b79a7de0d852e6f840",
                        "size": 39424,
                        "url": "https://example/java.exe"
                    }
                },
                "executable": true,
                "type": "file"
            },
            "../../evil.exe": {
                "downloads": {
                    "raw": {
                        "sha1": "38dc560c6ff23b550a9df5b79a7de0d852e6f840",
                        "size": 1,
                        "url": "https://example/evil.exe"
                    }
                },
                "type": "file"
            }
        }
    }"#;

    #[test]
    fn the_plan_of_a_runtime_holds_its_files() {
        let list = RuntimeFileList::parse(INNER.as_bytes()).unwrap();
        let plan = plan_runtime(&list, Path::new("/runtimes/epsilon")).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.total_size, 22056);

        let file = &plan.files[0];
        assert!(
            file.target
                .ends_with("bin/api-ms-win-core-console-l1-1-0.dll")
        );
        assert!(file.executable);
        assert_eq!(
            file.download.sha1,
            "fc7b39ea653d61f0e04faf229404b05c0c003753"
        );
    }

    #[test]
    fn the_plan_creates_the_directories_of_the_files() {
        let list = RuntimeFileList::parse(INNER.as_bytes()).unwrap();
        let plan = plan_runtime(&list, Path::new("/runtimes/epsilon")).unwrap();

        // The manifest names `bin` itself, and the file needs it too.
        assert!(plan.directories.iter().any(|d| d.ends_with("bin")));
    }

    #[test]
    fn the_plan_refuses_a_path_that_leaves_the_runtime() {
        // The manifest is remote data: a path that walks out of the
        // runtime directory must not become a file on the disk.
        let list = RuntimeFileList::parse(HOSTILE.as_bytes()).unwrap();
        let error = plan_runtime(&list, Path::new("/runtimes/epsilon")).unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)), "{error}");
    }

    #[test]
    fn a_file_without_a_plain_download_is_refused() {
        let list = RuntimeFileList::parse(
            r#"{"files": {"bin/java.exe": {"type": "file", "downloads": {"lzma": {"sha1": "a", "size": 1, "url": "https://example/x"}}}}}"#.as_bytes(),
        )
        .unwrap();
        let error = plan_runtime(&list, Path::new("/runtimes/epsilon")).unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)), "{error}");
    }

    #[test]
    fn the_tasks_carry_the_hash_and_the_size() {
        let list = RuntimeFileList::parse(INNER.as_bytes()).unwrap();
        let plan = plan_runtime(&list, Path::new("/runtimes/epsilon")).unwrap();
        let tasks = runtime_tasks(&plan);

        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].sha1.as_deref(),
            Some("fc7b39ea653d61f0e04faf229404b05c0c003753")
        );
        assert_eq!(tasks[0].size, Some(22056));
        assert!(
            tasks[0]
                .url
                .contains("fc7b39ea653d61f0e04faf229404b05c0c003753")
        );
    }

    #[test]
    fn unpacking_creates_the_directories() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("epsilon");

        let list = RuntimeFileList::parse(INNER.as_bytes()).unwrap();
        let plan = plan_runtime(&list, &runtime).unwrap();

        // The download engine puts the file in place and creates the
        // directory it needs; the unpack only has to fill in the rest.
        std::fs::create_dir_all(plan.files[0].target.parent().unwrap()).unwrap();
        std::fs::write(plan.files[0].target.clone(), b"payload").unwrap();

        unpack_runtime(&plan).unwrap();

        assert!(runtime.join("bin").is_dir());
        assert!(plan.files[0].target.is_file());
    }
}
