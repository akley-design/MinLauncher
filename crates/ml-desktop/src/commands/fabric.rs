//! Putting the Fabric loader in front of the game.
//!
//! The launcher does not run the installer of Fabric. The loader remaps
//! the jar of the game by itself the first time it starts, so all the
//! launcher has to do is:
//!
//! * fetch the profile of the loader, which says which libraries it needs
//!   and which class the game is started with;
//! * download those libraries into the same library store the libraries
//!   of Mojang go into;
//! * write the settings of the mod where the mod looks for them;
//! * put the jar of the mod where the loader looks for the mods.
//!
//! Everything here is best effort. A version of the game that the loader
//! does not support is a normal thing, not an error: the game is started
//! without the loader, and the game is a little slower.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ml_core::{AppPaths, Error, Result};
use ml_minecraft::fabric::{FabricLoaderList, FabricProfile};
use ml_minecraft::install::InstallLayout;

/// The name of the file the settings of the mod are written to.
///
/// The mod reads the file from the directory it runs in, so the file
/// travels with the instance and not with the launcher.
pub const MOD_SETTINGS_FILE: &str = "minlaucher-optimisations.json";

/// The name of the jar of the mod of the launcher.
pub const MOD_JAR_NAME: &str = "minlaucher-optimisations.jar";

/// How long an answer of the network is kept on the disk.
///
/// A day is long enough that the launcher asks the network once a day and
/// not on every launch, and short enough that a loader that came out
/// yesterday is noticed today.
pub const LOADER_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The documents of the loader, kept on the disk.
///
/// The launcher asks `meta.fabricmc.net` for the list of the loaders and
/// for the profile of the loader — on every launch. That makes the launch
/// depend on the network of somebody else, and a launch that waits for a
/// server is a launch that cannot promise five seconds. The answers are
/// therefore kept on the disk and read from there.
///
/// The documents are stored as they came, byte for byte, and their age is
/// the age of the file: there is no envelope around them and no clock in
/// them that could disagree with the clock of the filesystem.
#[derive(Debug, Clone)]
pub struct LoaderCache {
    /// The directory the documents are kept in.
    root: PathBuf,
    /// How long a document stays fresh.
    ttl: Duration,
}

impl LoaderCache {
    /// The cache of the launcher.
    #[must_use]
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            root: paths.cache_dir.join("loader"),
            ttl: LOADER_CACHE_TTL,
        }
    }

    /// The cache of the launcher, with a life of its own.
    ///
    /// The life is a parameter so that the test can make the documents of
    /// the cache stale without waiting for a day.
    #[must_use]
    pub fn with_ttl(paths: &AppPaths, ttl: Duration) -> Self {
        Self {
            root: paths.cache_dir.join("loader"),
            ttl,
        }
    }

    /// The name of the file the list of the loaders is kept in.
    ///
    //// shortcut: two versions of the game whose names differ only in the
    //// characters that are replaced by `_` would share one file of the
    //// cache, and the launcher would answer about one of them with what
    //// it knows about the other. The ids of Mojang never look like that,
    //// and the launcher only knows the ids of Mojang. Put the hash of the
    //// id into the name when that ever stops being true.
    fn list_name(minecraft_version: &str) -> String {
        format!(
            "{}.list.json",
            ml_core::paths::sanitize_component(minecraft_version)
        )
    }

    /// The name of the file the profile of the loader is kept in.
    fn profile_name(minecraft_version: &str, loader_version: &str) -> String {
        format!(
            "{}-{}.profile.json",
            ml_core::paths::sanitize_component(minecraft_version),
            ml_core::paths::sanitize_component(loader_version)
        )
    }

    /// The name of the file that says that there is no loader at all.
    fn none_name(minecraft_version: &str) -> String {
        format!(
            "{}.none",
            ml_core::paths::sanitize_component(minecraft_version)
        )
    }

    /// A document of the cache, if it is there and still fresh.
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let path = self.root.join(name);
        if !self.is_fresh(&path) {
            return None;
        }
        std::fs::read(&path).ok()
    }

    /// Keep a document in the cache.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.root).map_err(|e| Error::io(&self.root, e))?;
        let path = self.root.join(name);

        // The document is written next to its place and moved over it, so
        // that a launcher that starts at that moment never reads half of
        // a document.
        let temporary = path.with_extension("part");
        std::fs::write(&temporary, bytes).map_err(|e| Error::io(&temporary, e))?;
        std::fs::rename(&temporary, &path).map_err(|e| Error::io(&path, e))?;

        Ok(())
    }

    /// Throw a document of the cache away.
    fn forget(&self, name: &str) {
        let _ = std::fs::remove_file(self.root.join(name));
    }

    /// Whether the document is still fresh.
    fn is_fresh(&self, path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        let Ok(modified) = metadata.modified() else {
            return false;
        };
        // A document from the future means that the clock moved. It is
        // treated as old, because an answer that cannot be dated cannot
        // be trusted.
        SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age < self.ttl)
    }

    /// Whether the cache says that there is no loader for a version.
    fn says_no_loader(&self, minecraft_version: &str) -> bool {
        self.read(&Self::none_name(minecraft_version)).is_some()
    }

    /// Remember that there is no loader for a version of the game.
    ///
    /// The status of the answer is written into the file, so that a person
    /// who looks at the cache can see what the project said.
    fn remember_no_loader(&self, minecraft_version: &str, status: u16) -> Result<()> {
        self.write(
            &Self::none_name(minecraft_version),
            status.to_string().as_bytes(),
        )
    }
}

/// Where the launcher gets the documents of the loader.
///
/// The trait exists so that the test can answer without the network: the
/// whole point of the cache is that a warm launch does not ask anybody.
///
/// The future of the fetch says that it is `Send`, because the launch of the
/// game is `Send` and a fetcher that could hold the thread would not fit
/// into it.
pub trait Fetcher: Send + Sync {
    /// Fetch the body of an address.
    ///
    /// # Errors
    /// Fails when the address cannot be reached, or when the server
    /// answers with an error.
    // The signature is not `async fn` on purpose: in a public trait that
    // form does not say that the future is `Send`, and the whole point of
    // the signature above is that it says it.
    #[allow(clippy::manual_async_fn)]
    fn fetch(&self, url: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

/// How long the answer of the network is waited for.
///
/// The documents of the loader are small, and the project answers them in
/// a fraction of a second: the answer to the list of the loaders of the
/// version 26.3 came in 0.23 s on this machine. The launch must not wait
/// for a network that is broken — a socket that hangs is worse than a
/// socket that refuses, because a refusal is answered at once and a hang
/// is not answered at all. Without this limit the launch could stand still
/// for minutes on a broken network, and the game would not start at all.
///
/// Three seconds are long enough for a slow connection and short enough
/// that a broken one is not felt as a hang. The game is started without
/// the loader when the answer does not come in time.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// The fetcher that goes to the network.
#[derive(Debug, Clone, Copy)]
pub struct NetworkFetcher {
    /// How long the answer of the server is waited for.
    timeout: Duration,
}

impl Default for NetworkFetcher {
    fn default() -> Self {
        Self {
            timeout: FETCH_TIMEOUT,
        }
    }
}

impl NetworkFetcher {
    /// The fetcher that waits for the answer of the server for a while.
    ///
    /// The wait is a parameter so that the test can prove that a server
    /// that never answers does not hold the launch, without waiting three
    /// seconds for it.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Fetcher for NetworkFetcher {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout(self.timeout)
            .build()
            .map_err(|e| Error::network("http client", e))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::network(url, e))?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: response.status().as_u16(),
            });
        }

        let bytes = response.bytes().await.map_err(|e| Error::network(url, e))?;
        Ok(bytes.to_vec())
    }
}

/// Whether a version of the game can be run with the loader.
///
/// The loader publishes a profile only for the versions it supports. The
/// launcher asks the project itself, and the answer is what the project
/// says.
///
/// The disk is asked first: a warm launch does not touch the network.
///
/// # Errors
/// Fails when the list of the loaders cannot be read and cannot be
/// fetched.
pub async fn loader_version_for<F: Fetcher + ?Sized>(
    fetcher: &F,
    cache: &LoaderCache,
    minecraft_version: &str,
    requested: &str,
) -> Result<Option<String>> {
    let name = LoaderCache::list_name(minecraft_version);

    // The list of the loaders, if the disk has it and it is still fresh.
    if let Some(bytes) = cache.read(&name) {
        match FabricLoaderList::parse(&bytes) {
            Ok(list) => return Ok(resolve_loader_version(&list, requested)),
            // A document that cannot be read is not an answer: it is
            // thrown away and asked for again.
            Err(error) => {
                tracing::warn!("the cached list of the loaders is broken: {error}");
                cache.forget(&name);
            }
        }
    }

    // The disk says that the project does not know this version of the
    // game: there is no loader for it, and asking again would not change
    // the answer.
    if cache.says_no_loader(minecraft_version) {
        return Ok(None);
    }

    let url = ml_minecraft::fabric::loader_list_url(minecraft_version);
    let bytes = match fetcher.fetch(&url).await {
        Ok(bytes) => bytes,
        Err(error) => {
            // The project answered that it does not know this version of
            // the game. That is an answer, and it is remembered, so that
            // the next launch of an old version of the game does not ask
            // again.
            //
            // A network that is down is not an answer, and is not
            // remembered: the next launch tries again.
            if let Error::HttpStatus { status, .. } = &error
                && matches!(*status, 400 | 404)
            {
                if let Err(write_error) = cache.remember_no_loader(minecraft_version, *status) {
                    tracing::warn!("cannot remember that there is no loader: {write_error}");
                }
                return Ok(None);
            }
            return Err(error);
        }
    };

    let list = FabricLoaderList::parse(&bytes)?;
    if let Err(error) = cache.write(&name, &bytes) {
        tracing::warn!("cannot keep the list of the loaders: {error}");
    }

    Ok(resolve_loader_version(&list, requested))
}

/// The version of the loader out of the list of the loaders.
///
/// An empty request means the newest loader the project marks as stable,
/// and the newest one at that if there is no stable one.
fn resolve_loader_version(list: &FabricLoaderList, requested: &str) -> Option<String> {
    if requested.is_empty() {
        list.newest_stable()
            .or_else(|| list.newest())
            .map(|entry| entry.loader.version.clone())
    } else {
        list.find(requested)
            .map(|entry| entry.loader.version.clone())
    }
}

/// Fetch the profile of the loader.
///
/// The disk is asked first: a warm launch does not touch the network.
///
/// # Errors
/// Fails when the profile cannot be read and cannot be fetched.
pub async fn profile<F: Fetcher + ?Sized>(
    fetcher: &F,
    cache: &LoaderCache,
    minecraft_version: &str,
    loader_version: &str,
) -> Result<FabricProfile> {
    let name = LoaderCache::profile_name(minecraft_version, loader_version);

    // The profile, if the disk has it and it is still fresh.
    if let Some(bytes) = cache.read(&name) {
        match FabricProfile::parse(&bytes) {
            Ok(profile) => return Ok(profile),
            // A document that cannot be read is not an answer: it is
            // thrown away and asked for again.
            Err(error) => {
                tracing::warn!("the cached profile of the loader is broken: {error}");
                cache.forget(&name);
            }
        }
    }

    let url = ml_minecraft::fabric::profile_url(minecraft_version, loader_version);
    let bytes = fetcher.fetch(&url).await?;
    if let Err(error) = cache.write(&name, &bytes) {
        tracing::warn!("cannot keep the profile of the loader: {error}");
    }

    FabricProfile::parse(&bytes)
}

/// Download the libraries of the loader into the library store.
///
/// The libraries go into the same store the libraries of Mojang go into,
/// so that the two never fight over the same path and nothing is
/// downloaded twice.
///
/// The libraries of the loader are fetched once. When they are all on the
/// disk there is nothing to fetch, and the engine — with its HTTP client
/// — is not even built: a launch that waits for a server cannot promise
/// five seconds.
pub async fn download_libraries(
    paths: &AppPaths,
    platform: ml_core::Platform,
    profile: &FabricProfile,
) -> Result<()> {
    let layout = InstallLayout::new(paths.clone(), platform);

    let tasks: Vec<ml_minecraft::DownloadTask> = profile
        .downloads()
        .into_iter()
        .map(|(relative, url, sha1)| {
            ml_minecraft::DownloadTask::new(url, layout.library_path(&relative), sha1)
        })
        .collect();

    if tasks.is_empty() {
        return Ok(());
    }

    // The engine reuses a file that is already there, so this is the
    // same answer it would give — only without building a client and
    // without a request that is never sent.
    if tasks.iter().all(|task| task.target.is_file()) {
        tracing::info!("the libraries of the loader are already on the disk");
        return Ok(());
    }

    let downloader = ml_minecraft::Downloader::new()?;

    let result = downloader.download_all(tasks).await?;
    if !result.is_success() {
        let failed = result.failed.len();
        return Err(Error::network(
            "the libraries of the loader",
            std::io::Error::other(format!("{failed} of the libraries failed")),
        ));
    }

    Ok(())
}

/// Write the settings of the mod into the directory the game runs in.
///
/// The mod reads the file at the start of the game, before the game
/// itself starts. The file is written in the directory of the instance,
/// because the mod looks for it next to the game.
pub fn write_mod_settings(game_directory: &Path, preset: &str, enabled: bool) -> Result<PathBuf> {
    let path = game_directory.join(MOD_SETTINGS_FILE);

    let document = serde_json::json!({
        "preset": if enabled { preset } else { "off" },
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let text =
        serde_json::to_string_pretty(&document).map_err(|e| Error::json("mod_settings", &e))?;
    std::fs::write(&path, text).map_err(|e| Error::io(&path, e))?;

    Ok(path)
}

/// Put the jar of the mod where the loader looks for the mods.
///
/// The loader reads the mods of the directory of the game, so the jar is
/// copied there. The copy is made every time: the jar of the launcher
/// changes with every build, and a stale copy would be an old mod.
pub fn install_mod_jar(source: &Path, mods_directory: &Path) -> Result<PathBuf> {
    if !source.is_file() {
        return Err(Error::InvalidInput(format!(
            "the jar of the mod is not at `{}`",
            source.display()
        )));
    }

    std::fs::create_dir_all(mods_directory).map_err(|e| Error::io(mods_directory, e))?;
    let target = mods_directory.join(MOD_JAR_NAME);

    // The file is written next to the target and moved over it, so that
    // the loader never sees a half-written jar.
    let temporary = target.with_extension("jar.part");
    std::fs::copy(source, &temporary).map_err(|e| Error::io(&temporary, e))?;
    std::fs::rename(&temporary, &target).map_err(|e| Error::io(&target, e))?;

    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// The list of the loaders, as the project serves it.
    const LIST: &str = r#"[
      { "loader": { "version": "0.20.0", "stable": false } },
      { "loader": { "version": "0.19.5", "stable": true } }
    ]"#;

    /// The profile of the loader, as the project serves it.
    const PROFILE: &str = r#"{
      "id": "fabric-loader-0.19.5-1.21.4",
      "inheritsFrom": "1.21.4",
      "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
      "arguments": { "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
      "libraries": []
    }"#;

    /// A fetcher that answers from a table and remembers what it was asked.
    struct Stub {
        bodies: HashMap<String, Vec<u8>>,
        asked: Mutex<Vec<String>>,
    }

    impl Stub {
        /// A fetcher that answers with the given bodies.
        fn new(bodies: &[(&str, &[u8])]) -> Self {
            Self {
                bodies: bodies
                    .iter()
                    .map(|(url, body)| ((*url).to_string(), (*body).to_vec()))
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        /// The addresses that were asked for.
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl Fetcher for Stub {
        fn fetch(&self, url: &str) -> impl Future<Output = Result<Vec<u8>>> + Send {
            self.asked.lock().unwrap().push(url.to_string());
            std::future::ready(
                self.bodies
                    .get(url)
                    .cloned()
                    .ok_or_else(|| Error::HttpStatus {
                        url: url.to_string(),
                        status: 404,
                    }),
            )
        }
    }

    /// A fetcher whose network is down.
    struct Offline;

    impl Fetcher for Offline {
        fn fetch(&self, url: &str) -> impl Future<Output = Result<Vec<u8>>> + Send {
            std::future::ready(Err(Error::network(
                url,
                std::io::Error::other("the network is down"),
            )))
        }
    }

    /// The cache of a launcher in a temporary directory.
    fn cache_in_a_temporary_directory() -> (tempfile::TempDir, LoaderCache) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        (dir, LoaderCache::new(&paths))
    }

    #[tokio::test]
    async fn the_second_launch_of_a_version_does_not_ask_the_network() {
        let (_dir, cache) = cache_in_a_temporary_directory();

        let list_url = ml_minecraft::fabric::loader_list_url("1.21.4");
        let profile_url = ml_minecraft::fabric::profile_url("1.21.4", "0.19.5");

        // The first launch asks the network and keeps the answers.
        let online = Stub::new(&[
            (list_url.as_str(), LIST.as_bytes()),
            (profile_url.as_str(), PROFILE.as_bytes()),
        ]);
        let version = loader_version_for(&online, &cache, "1.21.4", "")
            .await
            .unwrap();
        assert_eq!(version.as_deref(), Some("0.19.5"));
        let loaded = profile(&online, &cache, "1.21.4", "0.19.5").await.unwrap();
        assert_eq!(loaded.id, "fabric-loader-0.19.5-1.21.4");
        assert_eq!(online.asked().len(), 2);

        // The second launch finds everything on the disk, and the network
        // is not touched at all.
        let offline = Offline;
        let version = loader_version_for(&offline, &cache, "1.21.4", "")
            .await
            .unwrap();
        assert_eq!(version.as_deref(), Some("0.19.5"));
        let loaded = profile(&offline, &cache, "1.21.4", "0.19.5").await.unwrap();
        assert_eq!(loaded.id, "fabric-loader-0.19.5-1.21.4");
    }

    #[tokio::test]
    async fn a_version_the_project_does_not_know_is_asked_only_once() {
        let (_dir, cache) = cache_in_a_temporary_directory();

        // The project answers that it does not know this version of the
        // game: there is no loader for it.
        let online = Stub::new(&[]);
        assert!(
            loader_version_for(&online, &cache, "1.0", "")
                .await
                .unwrap()
                .is_none()
        );

        // The next launch does not ask again: the answer is on the disk,
        // and the network may be down for good.
        let offline = Offline;
        assert!(
            loader_version_for(&offline, &cache, "1.0", "")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_network_that_is_down_is_not_remembered() {
        let (_dir, cache) = cache_in_a_temporary_directory();
        let list_url = ml_minecraft::fabric::loader_list_url("1.21.4");

        // The network is down. That is not an answer, and it is not
        // remembered as one.
        let offline = Offline;
        assert!(
            loader_version_for(&offline, &cache, "1.21.4", "")
                .await
                .is_err()
        );

        // The next launch tries again.
        let online = Stub::new(&[(list_url.as_str(), LIST.as_bytes())]);
        let version = loader_version_for(&online, &cache, "1.21.4", "")
            .await
            .unwrap();
        assert_eq!(version.as_deref(), Some("0.19.5"));
        assert_eq!(online.asked().len(), 1);
    }

    #[tokio::test]
    async fn an_old_answer_is_asked_again() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        // A cache whose documents are never fresh: everything is old.
        let cache = LoaderCache::with_ttl(&paths, Duration::ZERO);
        let list_url = ml_minecraft::fabric::loader_list_url("1.21.4");

        let first = Stub::new(&[(list_url.as_str(), LIST.as_bytes())]);
        assert!(
            loader_version_for(&first, &cache, "1.21.4", "")
                .await
                .unwrap()
                .is_some()
        );

        // The answer on the disk is old, so the network is asked again.
        let second = Stub::new(&[(list_url.as_str(), LIST.as_bytes())]);
        assert!(
            loader_version_for(&second, &cache, "1.21.4", "")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(second.asked().len(), 1);
    }

    /// The address that answers nothing.
    ///
    /// The port 1 of the loopback refuses the connection at once, so a
    /// test that must not fetch anything fails fast and does not touch the
    /// network of the machine.
    const NOWHERE: &str = "http://127.0.0.1:1/never";

    /// A profile of a loader that needs one library.
    ///
    /// The library is served by an address that answers nothing: a test
    /// that must not fetch anything fails at once, and a test that must
    /// fetch something fails too — with an error that says what happened.
    fn a_profile_with_one_library() -> FabricProfile {
        let document = format!(
            r#"{{
              "id": "fabric-loader-0.19.5-1.21.4",
              "inheritsFrom": "1.21.4",
              "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
              "libraries": [
                {{
                  "name": "net.fabricmc:fabric-loader:0.19.5",
                  "url": "{NOWHERE}"
                }}
              ]
            }}"#
        );
        FabricProfile::parse(document.as_bytes()).unwrap()
    }

    #[tokio::test]
    async fn the_libraries_that_are_on_the_disk_are_not_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let layout = InstallLayout::new(paths.clone(), ml_core::Platform::current());
        let profile = a_profile_with_one_library();

        // The library is on the disk: nothing is to be fetched.
        let (relative, _, _) = profile.downloads()[0].clone();
        let target = layout.library_path(&relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"the library is already here").unwrap();

        // The address of the library answers nothing, so a fetch would
        // fail: the answer of the call says that no fetch was made.
        download_libraries(&paths, ml_core::Platform::current(), &profile)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"the library is already here".to_vec()
        );
    }

    #[tokio::test]
    async fn a_library_that_is_missing_is_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let profile = a_profile_with_one_library();

        // The library is not on the disk, so it is fetched — and the
        // address that answers nothing makes the fetch fail.
        assert!(
            download_libraries(&paths, ml_core::Platform::current(), &profile)
                .await
                .is_err()
        );
    }

    #[test]
    fn the_settings_of_the_mod_are_written_next_to_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_mod_settings(dir.path(), "fast", true).unwrap();

        assert!(path.is_file());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"preset\""), "{text}");
        assert!(text.contains("fast"), "{text}");
    }

    #[test]
    fn a_mod_that_is_off_gets_the_off_preset() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_mod_settings(dir.path(), "fast", false).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("off"), "{text}");
    }

    #[test]
    fn the_settings_are_written_into_a_directory_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");

        let path = write_mod_settings(&nested, "balanced", true).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn the_jar_of_the_mod_is_copied_into_the_directory_of_the_mods() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jar");
        std::fs::write(&source, b"jar").unwrap();

        let mods = dir.path().join("mods");
        let target = install_mod_jar(&source, &mods).unwrap();

        assert!(target.is_file());
        assert_eq!(std::fs::read(&target).unwrap(), b"jar");
        assert_eq!(target.file_name().unwrap(), MOD_JAR_NAME);
    }

    #[test]
    fn the_copy_of_the_mod_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jar");
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join(MOD_JAR_NAME), b"old").unwrap();

        std::fs::write(&source, b"new").unwrap();
        let target = install_mod_jar(&source, &mods).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        // The half-written file must not be left behind.
        assert!(!target.with_extension("jar.part").exists());
    }

    #[test]
    fn a_mod_that_is_not_there_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.jar");

        assert!(install_mod_jar(&missing, &dir.path().join("mods")).is_err());
    }

    #[tokio::test]
    async fn a_server_that_does_not_answer_cannot_hold_the_launch() {
        // A socket that hangs is worse than a socket that refuses: the
        // refusal is answered at once, and the hang is not answered at
        // all. The launcher has to give up on it and start the game
        // without the loader, and it has to do it in a bounded time.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            // The connection is accepted, and nothing is ever said on
            // it: the socket is held open and silent.
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                std::thread::sleep(Duration::from_secs(5));
                drop(stream);
            }
        });

        let fetcher = NetworkFetcher::with_timeout(Duration::from_millis(300));
        let started = std::time::Instant::now();
        let answer = fetcher.fetch(&format!("http://{address}/")).await;

        assert!(
            answer.is_err(),
            "a server that says nothing cannot answer the launcher"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the launcher waited {} ms on a server that says nothing",
            started.elapsed().as_millis()
        );
    }

    #[test]
    fn the_wait_of_the_network_is_bounded() {
        // The limit is a promise: the launch does not stand still on a
        // broken network.
        assert!(FETCH_TIMEOUT < Duration::from_secs(10));
        assert_eq!(
            NetworkFetcher::default().timeout,
            FETCH_TIMEOUT,
            "the fetcher of the launcher waits as long as it promises"
        );
    }
}
