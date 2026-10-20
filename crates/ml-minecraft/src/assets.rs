//! Assets: the index, the object store and the virtual file system.
//!
//! The asset index maps a *logical* path — the path the game asks for, e.g.
//! `minecraft/sounds/ambient/cave.ogg` — onto the SHA-1 of the file's
//! content. The file itself is stored under its hash:
//!
//! ```text
//! assets/objects/<first two hex digits>/<the whole hash>
//! ```
//!
//! This is the layout the official launcher uses, and the one the server
//! serves the objects at. The two-letter directory is only a way to keep
//! any one directory from holding thousands of files; the name of the file
//! is the whole hash.
//!
//! Because the store is content-addressed, an object that several versions
//! share is downloaded once and kept forever. That is the single biggest
//! difference from the reference launcher, which keeps a separate copy per
//! version and re-downloads anything it cannot find in its own folder.

use std::collections::HashMap;
use std::path::PathBuf;

use ml_core::{AppPaths, Error, Result};
use serde::{Deserialize, Serialize};

/// One entry of the asset index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    /// SHA-1 of the object's content.
    pub hash: String,
    /// Size in bytes.
    pub size: i64,
}

/// The parsed asset index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    /// The objects, keyed by their logical path.
    pub objects: HashMap<String, AssetObject>,
}

impl AssetIndex {
    /// Parse an asset index document.
    ///
    /// # Errors
    /// Fails if the document is not valid JSON or does not have the
    /// expected shape.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Raw {
            objects: HashMap<String, AssetObject>,
        }

        let raw: Raw = serde_json::from_slice(bytes).map_err(|e| Error::json("asset_index", &e))?;
        Ok(Self {
            objects: raw.objects,
        })
    }

    /// The number of objects in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The total size of all objects.
    #[must_use]
    pub fn total_size(&self) -> i64 {
        self.objects.values().map(|object| object.size).sum()
    }

    /// The objects that are not present in the store yet.
    ///
    /// The check is by *presence*, not by size: a file that exists is
    /// assumed to be correct, because the download engine verifies the
    /// hash when it writes the file.
    #[must_use]
    pub fn missing(&self, paths: &AppPaths) -> Vec<(String, AssetObject)> {
        self.objects
            .iter()
            .filter(|(_, object)| !paths.asset_object(&object.hash).is_file())
            .map(|(name, object)| (name.clone(), object.clone()))
            .collect()
    }
}

/// Where a logical asset path is served from.
///
/// The game asks for `minecraft/textures/...`; the launcher has to hand it
/// the content-addressed file.
#[derive(Debug, Clone)]
pub struct AssetStore {
    paths: AppPaths,
}

impl AssetStore {
    /// Create a store rooted at the launcher's asset directory.
    #[must_use]
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    /// The physical path of an object.
    #[must_use]
    pub fn object_path(&self, hash: &str) -> PathBuf {
        self.paths.asset_object(hash)
    }

    /// The physical path of a logical asset path.
    ///
    /// # Errors
    /// Fails when the logical path is not in the index.
    pub fn resolve(&self, index: &AssetIndex, logical_path: &str) -> Result<PathBuf> {
        let object = index
            .objects
            .get(logical_path)
            .ok_or_else(|| Error::InvalidInput(format!("unknown asset `{logical_path}`")))?;
        Ok(self.object_path(&object.hash))
    }

    /// Whether an object is present on disk.
    #[must_use]
    pub fn has(&self, hash: &str) -> bool {
        self.object_path(hash).is_file()
    }

    /// The number of bytes the object occupies, if it exists.
    #[must_use]
    pub fn size_on_disk(&self, hash: &str) -> Option<u64> {
        std::fs::metadata(self.object_path(hash))
            .ok()
            .map(|metadata| metadata.len())
    }

    /// Remove every object that no index refers to any more.
    ///
    /// Only objects that are *not* referenced by any of the given indexes
    /// are removed, so a shared object is never collected while a version
    /// that needs it is still installed.
    ///
    /// # Errors
    /// Fails if a directory cannot be read or a file cannot be removed.
    pub fn collect(&self, keep: &[&AssetIndex]) -> Result<u64> {
        let mut referenced: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for index in keep {
            for object in index.objects.values() {
                referenced.insert(object.hash.as_str());
            }
        }

        let objects = self.paths.assets_dir.join("objects");
        if !objects.is_dir() {
            return Ok(0);
        }

        let mut removed = 0u64;
        for prefix in std::fs::read_dir(&objects).map_err(|e| Error::io(&objects, e))? {
            let prefix = prefix.map_err(|e| Error::io(&objects, e))?;
            let prefix_path = prefix.path();
            if !prefix_path.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&prefix_path).map_err(|e| Error::io(&prefix_path, e))? {
                let entry = entry.map_err(|e| Error::io(&prefix_path, e))?;
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(prefix_name) = prefix_path.file_name().and_then(|name| name.to_str())
                else {
                    continue;
                };
                // The name of the file is the whole hash, and the
                // directory it sits in is the first two letters of it.
                if !name.starts_with(prefix_name) {
                    continue;
                }
                if referenced.contains(name) {
                    continue;
                }
                let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
                std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
                removed += size;
            }
        }
        Ok(removed)
    }
}

/// The URL an asset object is downloaded from.
///
/// Mojang serves assets from a host that is not part of the content
/// address, so the URL has to be built from the hash.
///
/// The path is a two-letter directory and the *whole* hash as the name of
/// the file. The two-letter part is only a way to keep any one directory
/// from holding thousands of files; the name of the file is the whole
/// hash. Cutting the hash at the directory boundary — and serving the
/// rest of it — makes every asset answer 404, and there is no way to
/// notice that without asking the server for a real file.
#[must_use]
pub fn asset_url(base: &str, hash: &str) -> String {
    let hash = hash.to_ascii_lowercase();
    if hash.len() < 2 {
        return format!("{base}/{hash}");
    }
    let (prefix, _) = hash.split_at(2);
    format!("{base}/{prefix}/{hash}")
}

/// The default asset host.
pub const ASSET_HOST: &str = "https://resources.download.minecraft.net";

/// Where the assets of an installation are served from.
///
/// The game reads assets through a virtual file system, so the launcher
/// has to present a directory of logical paths. The launcher builds that
/// view on demand rather than materialising 5000 files per version.
#[derive(Debug, Clone)]
pub struct AssetRoots {
    /// The content-addressed store.
    pub store: AssetStore,
    /// The index the logical paths are resolved through.
    pub index: AssetIndex,
}

impl AssetRoots {
    /// The physical path of a logical asset path.
    ///
    /// # Errors
    /// Fails when the logical path is not in the index.
    pub fn resolve(&self, logical_path: &str) -> Result<PathBuf> {
        self.store.resolve(&self.index, logical_path)
    }
}

/// Whether the path looks like a logical asset path.
///
/// The game only ever asks for paths below `minecraft/`, which keeps a
/// malicious index from reaching outside the asset store.
#[must_use]
pub fn is_logical_asset_path(path: &str) -> bool {
    path.starts_with("minecraft/") && !path.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "objects": {
            "minecraft/sounds/ambient/cave.ogg": {
                "hash": "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356",
                "size": 9101
            },
            "minecraft/lang/en_us.json": {
                "hash": "5ff04807c356f1beed0b86ccf659b44b9983e3fa",
                "size": 781
            }
        }
    }"#;

    #[test]
    fn index_parses() {
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index.total_size(), 9101 + 781);
        assert!(!index.is_empty());
    }

    #[test]
    fn malformed_index_is_rejected() {
        assert!(AssetIndex::parse(b"[]").is_err());
    }

    #[test]
    fn object_url_is_content_addressed() {
        // The name of the file is the *whole* hash. Serving the rest of
        // it after the two-letter directory makes every asset answer 404.
        //
        // The hash below is a real one, taken from the asset index of
        // 1.21.4, and the address below is the one that answers 200.
        assert_eq!(
            asset_url(ASSET_HOST, "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"),
            "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
    }

    #[test]
    fn the_object_url_keeps_the_whole_hash() {
        // The two-letter directory is a way to spread the files over
        // directories; the name of the file is the whole hash. A URL
        // that loses the two letters of the prefix serves nothing.
        let hash = "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356";
        let url = asset_url(ASSET_HOST, hash);

        let name = url.rsplit('/').next().unwrap();
        assert_eq!(name, hash, "the name of the file must be the whole hash");
        assert_eq!(name.len(), 40);
    }

    #[test]
    fn object_url_lowercases_the_hash() {
        assert_eq!(
            asset_url(ASSET_HOST, "ABCDEF0123456789"),
            "https://resources.download.minecraft.net/ab/abcdef0123456789"
        );
    }

    #[test]
    fn store_resolves_logical_paths() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();
        let store = AssetStore::new(paths.clone());

        let resolved = store.resolve(&index, "minecraft/lang/en_us.json").unwrap();
        assert_eq!(
            resolved,
            paths.asset_object("5ff04807c356f1beed0b86ccf659b44b9983e3fa")
        );
    }

    #[test]
    fn unknown_logical_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();
        let store = AssetStore::new(paths);
        assert!(store.resolve(&index, "minecraft/nope.json").is_err());
    }

    #[test]
    fn missing_lists_what_is_not_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        paths.ensure_dirs().unwrap();
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();

        let missing = index.missing(&paths);
        assert_eq!(missing.len(), 2);

        // Once the file exists, it is no longer missing.
        let object = paths.asset_object("5ff04807c356f1beed0b86ccf659b44b9983e3fa");
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"x").unwrap();
        assert_eq!(index.missing(&paths).len(), 1);
    }

    #[test]
    fn collect_keeps_referenced_objects() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        paths.ensure_dirs().unwrap();
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();

        for object in index.objects.values() {
            let path = paths.asset_object(&object.hash);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        // An object no index refers to.
        let orphan = paths.asset_object("0011223344556677889900112233445566778899");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, b"xx").unwrap();

        let store = AssetStore::new(paths.clone());
        let removed = store.collect(&[&index]).unwrap();

        assert_eq!(removed, 2);
        assert!(!orphan.exists());
        for object in index.objects.values() {
            assert!(paths.asset_object(&object.hash).is_file());
        }
    }

    #[test]
    fn logical_paths_must_stay_inside_the_store() {
        assert!(is_logical_asset_path("minecraft/lang/en_us.json"));
        assert!(!is_logical_asset_path("../secret"));
        assert!(!is_logical_asset_path("other/lang/en_us.json"));
    }
}
