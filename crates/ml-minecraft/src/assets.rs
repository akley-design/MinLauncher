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
//!
//! # The two layouts
//!
//! The games before 1.7 do not read that store. They read an ordinary
//! tree of files, and they read it from a place that depends on the
//! version:
//!
//! * the games from 1.6.4 to 1.7.2 read the directory they are given as
//!   `--assetsDir` — the bytecode of 1.6.4 scans that directory and
//!   takes the path of every file relative to it;
//! * the games before 1.6 read `~/.minecraft/resources` and nothing else:
//!   `LaunchWrapper` drops `--assetsDir` before the game is started, and
//!   the game of 1.5.2 looks for its resources where it used to
//!   download them to.
//!
//! So the tree of the old games is a *view* of the store: it is
//! materialised from it, file by file, and the store stays the single
//! copy of every byte.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ml_core::{AppPaths, Error, Result, plain_object};
use serde::{Deserialize, Serialize};

/// The id of the asset index of the games before 1.6.
pub const PRE_1_6_ASSET_INDEX: &str = "pre-1.6";

/// The id of the asset index of the games from 1.6 to 1.7.2.
pub const LEGACY_ASSET_INDEX: &str = "legacy";

/// Where the assets of a version are read from.
///
/// The rule is taken from the document itself: a version that names an
/// asset index in its arguments is read from the content-addressed
/// store, and a version that names none — the ones before 1.7 — is read
/// from a plain tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLayout {
    /// The content-addressed store, from 1.7 on.
    ContentAddressed,
    /// The plain tree, before 1.7.
    Plain,
}

/// The layout of the assets of a version, decided by the id of its index.
///
/// The rule was read off the whole corpus of the 920 documents of
/// Mojang: a version reads the content-addressed store when its
/// arguments name an `--assetIndex`, and it reads a plain tree when they
/// do not. The 791 versions that name one are the ones from 1.7 on, and
/// the 129 that do not name one are the ones before it — and they are
/// exactly the ones whose index is `pre-1.6` or `legacy`.
///
/// The id of the index is therefore the same rule in a form the launcher
/// can use without the document of the version at hand.
#[must_use]
pub fn layout_of(index_id: &str) -> AssetLayout {
    match index_id {
        PRE_1_6_ASSET_INDEX | LEGACY_ASSET_INDEX => AssetLayout::Plain,
        _ => AssetLayout::ContentAddressed,
    }
}

/// The directory the plain tree of a version lives in.
///
/// The two kinds of plain tree live in different places, and the reason
/// is in the bytecode of the games themselves:
///
/// * `legacy` — the game scans the directory it was given as
///   `--assetsDir` and takes the path of every file relative to it, so
///   the tree is that directory;
/// * `pre-1.6` — the game does not read `--assetsDir` at all:
///   `LaunchWrapper` passes the game only the arguments that are not
///   options, and the game of 1.5.2 reads
///   `~/.minecraft/resources`.
///
/// Returns `None` for a content-addressed index: there is no tree.
///
/// # Note
///
/// shortcut: the plain layout is decided by the id of the asset index; a
/// third kind of plain index would need a rule of its own here — when
/// Mojang publishes one, add it to this match.
#[must_use]
pub fn plain_tree_root(paths: &AppPaths, index_id: &str) -> Option<PathBuf> {
    match layout_of(index_id) {
        AssetLayout::ContentAddressed => None,
        AssetLayout::Plain => Some(match index_id {
            PRE_1_6_ASSET_INDEX => paths.vanilla_resources_dir(),
            _ => paths.asset_virtual_dir(index_id),
        }),
    }
}

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

    /// Materialise the plain tree of a version.
    ///
    /// The store is content-addressed and the games before 1.7 read an
    /// ordinary tree of files, so the tree is a view of the store: every
    /// file of the index is linked into it. A hard link is used when the
    /// disk allows it, and a copy when it does not — the content is the
    /// same either way, and the store stays the single copy of it.
    ///
    /// The function is idempotent: a file that is already there with the
    /// size the index gives it is left alone, so the tree can be built
    /// again at any time and costs nothing the second time.
    ///
    /// The number returned is how many files were written; the files
    /// that were already there are not counted.
    ///
    /// # Errors
    /// Fails if a directory cannot be created or a file cannot be
    /// written.
    pub fn expand_plain_tree(&self, index: &AssetIndex, root: &Path) -> Result<usize> {
        let mut written = 0usize;

        for (key, object) in &index.objects {
            let target = match plain_object(root, key) {
                Ok(target) => target,
                Err(error) => {
                    // A key that leaves the tree is a key that is not
                    // ours to write. The rest of the tree is still
                    // worth building.
                    tracing::warn!("the asset key `{key}` is not a safe path: {error}");
                    continue;
                }
            };

            let source = self.object_path(&object.hash);
            if !source.is_file() {
                // The object was never downloaded. The caller decides
                // whether that is a failure.
                continue;
            }

            // The file is already there when it weighs what the index
            // says the object weighs: the store verifies the hash of
            // everything it writes, so the size of an object on the
            // disk is the size the index gives it.
            if let Ok(metadata) = std::fs::metadata(&target) {
                if u64::try_from(object.size) == Ok(metadata.len()) {
                    continue;
                }
                // A file of the wrong size is not the object. It has to
                // go before the new one can take its place: Windows
                // will not link a file onto a file that is there.
                std::fs::remove_file(&target).map_err(|e| Error::io(&target, e))?;
            }

            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }

            if std::fs::hard_link(&source, &target).is_ok() {
                written += 1;
                continue;
            }

            std::fs::copy(&source, &target).map_err(|e| Error::io(&target, e))?;
            written += 1;
        }

        Ok(written)
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

    #[test]
    fn the_layout_is_decided_by_the_index_id() {
        // The two ids of the old games are the only ones that name a
        // plain tree.
        assert_eq!(layout_of(PRE_1_6_ASSET_INDEX), AssetLayout::Plain);
        assert_eq!(layout_of(LEGACY_ASSET_INDEX), AssetLayout::Plain);
        assert_eq!(layout_of("1.7.10"), AssetLayout::ContentAddressed);
        assert_eq!(layout_of("5"), AssetLayout::ContentAddressed);
    }

    #[test]
    fn the_tree_of_the_legacy_games_is_next_to_the_store() {
        // The game of 1.6.4 scans the directory it was given as
        // `--assetsDir`, so that is where the tree has to be.
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));

        let root = plain_tree_root(&paths, LEGACY_ASSET_INDEX).unwrap();
        assert_eq!(root, paths.asset_virtual_dir(LEGACY_ASSET_INDEX));
        assert!(root.starts_with(&paths.assets_dir));
    }

    #[test]
    fn the_tree_of_the_old_games_is_in_the_home_directory() {
        // The game of 1.5.2 reads `~/.minecraft/resources` and nothing
        // else: LaunchWrapper drops `--assetsDir` before the game is
        // started.
        let dir = tempfile::tempdir().unwrap();
        let mut paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        paths.home_dir = dir.path().join("home");

        let root = plain_tree_root(&paths, PRE_1_6_ASSET_INDEX).unwrap();
        assert_eq!(root, dir.path().join("home/.minecraft/resources"));
    }

    #[test]
    fn a_modern_version_has_no_tree() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        assert!(plain_tree_root(&paths, "1.7.10").is_none());
        assert!(plain_tree_root(&paths, "5").is_none());
    }

    /// A store with the two objects of the sample index on the disk.
    ///
    /// The objects are as long as the index says they are: the store
    /// verifies the hash of everything it writes, so the size of a file
    /// on the disk is the size the index gives.
    fn store_with_sample(dir: &tempfile::TempDir) -> (AppPaths, AssetStore, AssetIndex) {
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        paths.ensure_dirs().unwrap();
        let index = AssetIndex::parse(SAMPLE.as_bytes()).unwrap();

        let store = AssetStore::new(paths.clone());
        for object in index.objects.values() {
            let path = store.object_path(&object.hash);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let size = usize::try_from(object.size).expect("the size of the object fits");
            std::fs::write(&path, vec![b'x'; size]).unwrap();
        }
        (paths, store, index)
    }

    #[test]
    fn the_tree_is_a_view_of_the_store() {
        // The game of the old versions reads the files by their name, so
        // the tree has to carry the name of every key of the index.
        let dir = tempfile::tempdir().unwrap();
        let (_paths, store, index) = store_with_sample(&dir);

        let root = dir.path().join("tree");
        let written = store.expand_plain_tree(&index, &root).unwrap();

        assert_eq!(written, 2);
        assert!(root.join("minecraft/sounds/ambient/cave.ogg").is_file());
        assert!(root.join("minecraft/lang/en_us.json").is_file());
        assert_eq!(
            std::fs::metadata(root.join("minecraft/lang/en_us.json"))
                .unwrap()
                .len(),
            781
        );
    }

    #[test]
    fn a_file_of_the_wrong_size_is_replaced() {
        // A file of a broken download has to give way to the right one.
        let dir = tempfile::tempdir().unwrap();
        let (_paths, store, index) = store_with_sample(&dir);

        let root = dir.path().join("tree");
        store.expand_plain_tree(&index, &root).unwrap();

        // The file of the tree is a hard link to the object of the
        // store, so writing over it would break the object as well.
        // The broken file is put there in place: a file of its own.
        let broken = root.join("minecraft/lang/en_us.json");
        std::fs::remove_file(&broken).unwrap();
        std::fs::write(&broken, b"broken").unwrap();

        assert_eq!(store.expand_plain_tree(&index, &root).unwrap(), 1);
        assert_eq!(std::fs::metadata(&broken).unwrap().len(), 781);
    }

    #[test]
    fn the_tree_is_built_again_only_when_it_has_to() {
        // The tree is rebuilt on every installation, so it has to be
        // cheap when nothing changed.
        let dir = tempfile::tempdir().unwrap();
        let (_paths, store, index) = store_with_sample(&dir);

        let root = dir.path().join("tree");
        assert_eq!(store.expand_plain_tree(&index, &root).unwrap(), 2);
        assert_eq!(store.expand_plain_tree(&index, &root).unwrap(), 0);
    }

    #[test]
    fn a_key_that_leaves_the_tree_is_not_written() {
        // The key comes from the index, which is remote data.
        let dir = tempfile::tempdir().unwrap();
        let (_paths, store, mut index) = store_with_sample(&dir);

        index.objects.insert(
            "../../escape".to_string(),
            AssetObject {
                hash: "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356".into(),
                size: 7,
            },
        );

        let root = dir.path().join("tree");
        let written = store.expand_plain_tree(&index, &root).unwrap();

        assert_eq!(written, 2);
        assert!(!dir.path().join("escape").exists());
    }
}
