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

use std::path::{Path, PathBuf};

use ml_core::{AppPaths, Error, Result};
use ml_minecraft::fabric::{FabricLaunch, FabricProfile};
use ml_minecraft::install::InstallLayout;

use crate::error::{ErrorCode, IpcError};

/// The name of the file the settings of the mod are written to.
///
/// The mod reads the file from the directory it runs in, so the file
/// travels with the instance and not with the launcher.
pub const MOD_SETTINGS_FILE: &str = "minlaucher-optimisations.json";

/// The name of the jar of the mod of the launcher.
pub const MOD_JAR_NAME: &str = "minlaucher-optimisations.jar";

/// What the launcher prepared for the loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricPreparation {
    /// The launch part of the loader.
    pub launch: FabricLaunch,
    /// The jars of the mods, in the order they are loaded.
    pub mod_jars: Vec<PathBuf>,
}

/// Whether a version of the game can be run with the loader.
///
/// The loader publishes a profile only for the versions it supports. The
/// launcher asks the project itself, and the answer is what the project
/// says.
pub async fn loader_version_for(
    minecraft_version: &str,
    requested: &str,
) -> Result<Option<String>> {
    let url = ml_minecraft::fabric::loader_list_url(minecraft_version);
    let bytes = fetch(&url).await?;
    let list = ml_minecraft::fabric::FabricLoaderList::parse(&bytes)?;

    Ok(if requested.is_empty() {
        list.newest_stable()
            .or_else(|| list.newest())
            .map(|entry| entry.loader.version.clone())
    } else {
        list.find(requested)
            .map(|entry| entry.loader.version.clone())
    })
}

/// Fetch the profile of the loader.
pub async fn profile(minecraft_version: &str, loader_version: &str) -> Result<FabricProfile> {
    let url = ml_minecraft::fabric::profile_url(minecraft_version, loader_version);
    let bytes = fetch(&url).await?;
    FabricProfile::parse(&bytes)
}

/// Download the libraries of the loader into the library store.
///
/// The libraries go into the same store the libraries of Mojang go into,
/// so that the two never fight over the same path and nothing is
/// downloaded twice.
pub async fn download_libraries(
    paths: &AppPaths,
    platform: ml_core::Platform,
    profile: &FabricProfile,
) -> Result<()> {
    let layout = InstallLayout::new(paths.clone(), platform);
    let downloader = ml_minecraft::Downloader::new()?;

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

/// Fetch a URL and return the body.
async fn fetch(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
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

/// Turn an error of the core into the error of the commands.
pub fn to_ipc(error: Error) -> IpcError {
    IpcError::with_detail(
        ErrorCode::Network,
        "the loader of the mods",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
