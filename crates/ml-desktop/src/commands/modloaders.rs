//! The stable Forge and Fabric releases available for each Minecraft release.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ml_core::AppPaths;
use ml_minecraft::download::{DownloadTask, Downloader, hash_file};
use ml_minecraft::install::InstallLayout;
use ml_minecraft::version::VersionJson;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN_METADATA: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FABRIC_GAMES: &str = "https://meta.fabricmc.net/v2/versions/game";
const MODRINTH_FABRIC_API: &str = "https://api.modrinth.com/v2/project/P7dR8mSH/version";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Stable Forge and Fabric choices for supported Minecraft releases.
pub struct ModloaderCatalog {
    /// The entries are ordered newest Minecraft release first.
    pub versions: Vec<ModloaderVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// The recommended loader builds for one Minecraft release.
pub struct ModloaderVersion {
    /// The stable Minecraft release identifier.
    pub minecraft_version: String,
    /// Forge's recommended build, or its latest promoted build.
    pub forge_version: Option<String>,
    /// Exact Forge Maven version; older builds can append the game version.
    #[serde(default)]
    pub forge_maven_version: Option<String>,
    /// The newest Fabric Loader build marked stable.
    pub fabric_loader: Option<String>,
    /// The newest stable Fabric API version compatible with this release.
    pub fabric_api_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CatalogCache {
    fetched_at: u64,
    catalog: ModloaderCatalog,
}

#[derive(Debug, Deserialize)]
struct ForgePromotions {
    promos: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct FabricGame {
    version: String,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    version_number: String,
    version_type: String,
    date_published: String,
    game_versions: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    filename: String,
    url: String,
    primary: bool,
    hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FabricApiArtifact {
    url: String,
    sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
/// The user selection sent to the loader installation command.
pub struct CreateModloaderInstanceOptions {
    /// The display name of the isolated instance.
    pub name: String,
    /// A stable Minecraft release from the loader catalog.
    pub minecraft_version: String,
    /// Either `forge` or `fabric`.
    pub loader: String,
}

/// Resolve stable loaders from their publishers and intersect them with Mojang releases.
#[tauri::command]
pub async fn list_modloader_versions(state: State<'_, SharedState>) -> IpcResult<ModloaderCatalog> {
    let paths = state.paths();
    let cache_path = paths.cache_dir.join("modloader-catalog-v2.json");
    if let Some(cache) = read_catalog_cache(&cache_path)
        && now_seconds()
            .checked_sub(cache.fetched_at)
            .is_some_and(|age| age < 6 * 60 * 60)
    {
        return Ok(cache.catalog);
    }
    let http = reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| network_error("cannot create the modloader catalog client", error))?;

    let loaded = fetch_catalog(&http).await;

    match loaded {
        Ok(catalog) => {
            if let Err(error) = write_catalog_cache(&cache_path, &catalog) {
                tracing::warn!("cannot cache the modloader catalog: {error}");
            }
            Ok(catalog)
        }
        Err(error) => read_catalog_cache(&cache_path).map_or(Err(error), |cache| Ok(cache.catalog)),
    }
}

/// Install the selected stable loader and create one isolated game instance.
#[tauri::command]
#[allow(clippy::too_many_lines)] // The loader install is one transaction.
pub async fn create_modloader_instance(
    options: CreateModloaderInstanceOptions,
    state: State<'_, SharedState>,
) -> IpcResult<crate::commands::instances::InstanceList> {
    let name = options.name.trim().to_string();
    if name.is_empty() {
        return Err(IpcError::invalid("the instance name cannot be empty"));
    }
    let loader = options.loader.as_str();
    if !matches!(loader, "forge" | "fabric") {
        return Err(IpcError::invalid("unknown modloader"));
    }
    let instance_id = crate::commands::instances::instance_id(&name);
    if instance_id.is_empty() {
        return Err(IpcError::invalid(
            "the instance name has no usable characters",
        ));
    }
    if state.storage().instances().get(&instance_id)?.is_some() {
        return Err(IpcError::invalid(
            "an instance with this name already exists",
        ));
    }

    let _activity = state.begin_activity()?;
    let paths = state.paths();
    let http = http_client()?;
    let catalog = catalog_for_install(&paths, &http).await?;
    let selected = catalog
        .versions
        .iter()
        .find(|entry| entry.minecraft_version == options.minecraft_version)
        .ok_or_else(|| {
            IpcError::invalid("this Minecraft release is not in the current modloader catalog")
        })?;
    let loader_version = match loader {
        "forge" => selected.forge_maven_version.as_deref(),
        _ => selected.fabric_loader.as_deref(),
    }
    .ok_or_else(|| {
        IpcError::invalid(format!(
            "{loader} is not available for Minecraft {}",
            selected.minecraft_version
        ))
    })?;
    let forge_version = selected.forge_version.as_deref().unwrap_or_default();
    if !is_supported_minecraft_release(&selected.minecraft_version)
        || !is_safe_version_token(loader_version)
        || (loader == "forge" && !is_safe_version_token(forge_version))
    {
        return Err(IpcError::invalid(
            "the selected version metadata is invalid",
        ));
    }

    let version_id = if loader == "fabric" {
        ml_minecraft::fabric::profile_id(&selected.minecraft_version, loader_version)
    } else {
        format!("{}-forge-{forge_version}", selected.minecraft_version)
    };
    let game_directory = paths.instance_dir(&instance_id);
    if game_directory.exists() {
        return Err(IpcError::invalid("the instance directory already exists"));
    }

    let mut service = state.service_clone();
    let base_options = crate::commands::versions::InstallOptions {
        version_id: selected.minecraft_version.clone(),
        allow_network: true,
    };
    let base_installed = state
        .storage()
        .versions()
        .is_installed(&selected.minecraft_version)?;
    let base_json_path = state
        .storage()
        .versions()
        .json_path(&selected.minecraft_version)?;
    let base_is_usable = base_installed
        && base_json_path.as_deref().is_some_and(|json_path| {
            fs::read(json_path)
                .ok()
                .and_then(|bytes| VersionJson::parse(&bytes).ok())
                .is_some_and(|version| {
                    version.downloads.client.as_ref().is_some_and(|client| {
                        file_matches(
                            &paths
                                .version_dir(&selected.minecraft_version)
                                .join("client.jar"),
                            &client.sha1,
                        )
                    })
                })
        });
    if !base_is_usable {
        let reporter: crate::commands::versions::InstallReporter = Arc::new(|_| {});
        crate::commands::versions::install_version_files(
            &mut service,
            state.storage(),
            &paths,
            &base_options,
            &reporter,
        )
        .await?;
    }
    let base_json_path = state
        .storage()
        .versions()
        .json_path(&selected.minecraft_version)?
        .ok_or_else(|| IpcError::not_found("the base Minecraft profile is missing"))?;
    let base_json = fs::read(&base_json_path).map_err(IpcError::from)?;
    let base_version = VersionJson::parse(&base_json)?;

    let api_artifact = if loader == "fabric" {
        let api_version = selected.fabric_api_version.as_deref().ok_or_else(|| {
            IpcError::invalid("Fabric API is not available for this Minecraft release")
        })?;
        if !is_safe_version_token(api_version) {
            return Err(IpcError::invalid(
                "the selected Fabric API version metadata is invalid",
            ));
        }
        Some(fetch_fabric_api_artifact(&http, &selected.minecraft_version, api_version).await?)
    } else {
        None
    };
    let existing_version_path = paths.version_dir(&version_id).join("version.json");
    let profile_value = if existing_version_path.is_file() {
        let existing = fs::read(&existing_version_path).map_err(IpcError::from)?;
        serde_json::from_slice(&existing).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "the installed loader profile is invalid",
                error.to_string(),
            )
        })?
    } else if loader == "fabric" {
        let loader_version = loader_version.to_string();
        let game_version = selected.minecraft_version.clone();
        let profile = fetch_fabric_profile(&http, &game_version, &loader_version).await?;
        compose_fabric_profile(&base_json, &game_version, &loader_version, profile)?
    } else {
        let profile = install_forge_profile(
            &http,
            &mut service,
            &paths,
            &selected.minecraft_version,
            loader_version,
            &base_version,
            &version_id,
        )
        .await?;
        compose_forge_profile(&base_json, profile, &version_id)?
    };
    let normalized = serde_json::to_vec_pretty(&profile_value).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot write the modloader profile",
            error.to_string(),
        )
    })?;
    let version = VersionJson::parse(&normalized)?;
    if version.id != version_id {
        return Err(IpcError::invalid(
            "the loader profile has an unexpected version identifier",
        ));
    }
    let layout = InstallLayout::new(paths.clone(), state.platform());
    ensure_profile_libraries(&version, &layout, &paths, state.platform()).await?;

    let version_dir = paths.version_dir(&version_id);
    let version_dir_existed = version_dir.exists();
    if version_dir_existed
        && !version
            .downloads
            .client
            .as_ref()
            .is_some_and(|client| file_matches(&version_dir.join("client.jar"), &client.sha1))
    {
        return Err(IpcError::invalid(
            "the installed loader profile is incomplete; remove its version folder and retry",
        ));
    }
    let staging_version = paths
        .versions_dir
        .join(format!("{version_id}.installing-{}", uuid::Uuid::new_v4()));
    let staging_game = paths
        .instances_dir
        .join(format!("{instance_id}.installing-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&staging_version).map_err(IpcError::from)?;
    fs::create_dir_all(&staging_game).map_err(IpcError::from)?;
    let setup_result = (|| {
        fs::write(staging_version.join("version.json"), &normalized).map_err(IpcError::from)?;
        copy_client_jar(
            &paths
                .version_dir(&selected.minecraft_version)
                .join("client.jar"),
            &staging_version.join("client.jar"),
            &version,
        )?;
        Ok::<(), IpcError>(())
    })();
    if let Err(error) = setup_result {
        let _ = fs::remove_dir_all(&staging_version);
        let _ = fs::remove_dir_all(&staging_game);
        return Err(error);
    }
    if let Some(api) = &api_artifact {
        let target = staging_game.join("mods").join("fabric-api.jar");
        if let Err(error) = download_file_verified(&api.url, &api.sha1, &target).await {
            let _ = fs::remove_dir_all(&staging_version);
            let _ = fs::remove_dir_all(&staging_game);
            return Err(error);
        }
    }
    if version_dir.exists() {
        let _ = fs::remove_dir_all(&staging_version);
    } else if let Err(error) = fs::rename(&staging_version, &version_dir) {
        let _ = fs::remove_dir_all(&staging_version);
        let _ = fs::remove_dir_all(&staging_game);
        return Err(IpcError::from(error));
    }
    if let Err(error) = fs::rename(&staging_game, &game_directory) {
        let _ = fs::remove_dir_all(&staging_game);
        if !version_dir_existed {
            let _ = fs::remove_dir_all(&version_dir);
        }
        return Err(IpcError::from(error));
    }

    let version_path = version_dir.join("version.json");
    let mut meta = version.to_meta(None);
    meta.id.clone_from(&version_id);
    let registration = (|| {
        state.storage().versions().upsert(&meta)?;
        state
            .storage()
            .versions()
            .set_json_path(&version_id, &version_path.to_string_lossy())?;
        state
            .storage()
            .versions()
            .set_installed(&version_id, true)?;
        ml_minecraft::install::store_libraries(
            state.storage(),
            &version_id,
            &version.resolve_libraries(state.platform(), Some(&version_id)),
        )?;
        let instance = ml_storage::Instance {
            id: instance_id,
            version_id: version_id.clone(),
            name,
            game_dir: game_directory.to_string_lossy().into_owned(),
            java_path: None,
            jvm_args: None,
            created_at: ml_storage::now_iso8601(),
        };
        state.storage().instances().upsert(&instance)?;
        Ok::<_, IpcError>(())
    })();
    if let Err(error) = registration {
        let _ = state.storage().versions().set_installed(&version_id, false);
        let _ = fs::remove_dir_all(&game_directory);
        if !version_dir_existed {
            let _ = fs::remove_dir_all(&version_dir);
        }
        return Err(error);
    }
    Ok(crate::commands::instances::InstanceList {
        instances: state.storage().instances().list()?,
    })
}

fn http_client() -> IpcResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| network_error("cannot create the modloader download client", error))
}

async fn catalog_for_install(
    paths: &AppPaths,
    http: &reqwest::Client,
) -> IpcResult<ModloaderCatalog> {
    let cache_path = paths.cache_dir.join("modloader-catalog-v2.json");
    if let Some(cache) = read_catalog_cache(&cache_path)
        && now_seconds()
            .checked_sub(cache.fetched_at)
            .is_some_and(|age| age < 6 * 60 * 60)
    {
        return Ok(cache.catalog);
    }
    match fetch_catalog(http).await {
        Ok(catalog) => {
            if let Err(error) = write_catalog_cache(&cache_path, &catalog) {
                tracing::warn!("cannot cache the modloader catalog: {error}");
            }
            Ok(catalog)
        }
        Err(error) => read_catalog_cache(&cache_path).map_or(Err(error), |cache| Ok(cache.catalog)),
    }
}

async fn fetch_fabric_profile(
    http: &reqwest::Client,
    game_version: &str,
    loader_version: &str,
) -> IpcResult<Value> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{loader_version}/profile/json"
    );
    get_json(http, &url).await
}

async fn fetch_fabric_api_artifact(
    http: &reqwest::Client,
    game_version: &str,
    api_version: &str,
) -> IpcResult<FabricApiArtifact> {
    let response = http
        .get(MODRINTH_FABRIC_API)
        .query(&[
            ("loaders", "[\"fabric\"]".to_string()),
            ("game_versions", format!("[\"{game_version}\"]")),
            ("include_changelog", "false".to_string()),
        ])
        .send()
        .await
        .map_err(|error| network_error("cannot fetch the selected Fabric API", error))?;
    if !response.status().is_success() {
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch the selected Fabric API",
            format!("Modrinth returned HTTP {}", response.status()),
        ));
    }
    let versions: Vec<ModrinthVersion> = response
        .json()
        .await
        .map_err(|error| network_error("Fabric API metadata is invalid", error))?;
    let version = versions
        .into_iter()
        .find(|version| {
            version.version_type == "release"
                && version.version_number == api_version
                && version
                    .game_versions
                    .iter()
                    .any(|version| version == game_version)
        })
        .ok_or_else(|| {
            IpcError::invalid("the selected Fabric API version is no longer available")
        })?;
    let file = version
        .files
        .iter()
        .find(|file| file.primary && is_jar_name(&file.filename))
        .or_else(|| {
            version
                .files
                .iter()
                .find(|file| is_jar_name(&file.filename))
        })
        .ok_or_else(|| IpcError::invalid("the selected Fabric API version has no jar file"))?;
    let sha1 = file
        .hashes
        .get("sha1")
        .filter(|hash| hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .cloned()
        .ok_or_else(|| IpcError::invalid("the selected Fabric API file has no valid SHA-1"))?;
    Ok(FabricApiArtifact {
        url: file.url.clone(),
        sha1,
    })
}

fn compose_fabric_profile(
    base_json: &[u8],
    game_version: &str,
    loader_version: &str,
    mut profile: Value,
) -> IpcResult<Value> {
    let expected_id = ml_minecraft::fabric::profile_id(game_version, loader_version);
    if profile.get("id").and_then(Value::as_str) != Some(expected_id.as_str())
        || profile.get("inheritsFrom").and_then(Value::as_str) != Some(game_version)
    {
        return Err(IpcError::invalid(
            "Fabric returned a profile for a different game or loader version",
        ));
    }
    if profile.get("mainClass").and_then(Value::as_str).is_none() {
        return Err(IpcError::invalid("Fabric's profile has no launch class"));
    }
    let mut libraries = Vec::new();
    for library in profile
        .get_mut("libraries")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        normalize_maven_library(library)?;
        libraries.push(library.clone());
    }
    profile["libraries"] = Value::Array(libraries);
    let mut base: Value = serde_json::from_slice(base_json).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "the base Minecraft profile is invalid",
            error.to_string(),
        )
    })?;
    merge_version_profile(&mut base, &profile, &expected_id);
    Ok(base)
}

fn compose_forge_profile(
    base_json: &[u8],
    mut profile: Value,
    version_id: &str,
) -> IpcResult<Value> {
    if profile.get("mainClass").and_then(Value::as_str).is_none() {
        return Err(IpcError::invalid(
            "Forge's installer created a profile with no launch class",
        ));
    }
    for library in profile
        .get_mut("libraries")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        normalize_maven_library(library)?;
    }
    let mut base: Value = serde_json::from_slice(base_json).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "the base Minecraft profile is invalid",
            error.to_string(),
        )
    })?;
    merge_version_profile(&mut base, &profile, version_id);
    Ok(base)
}

fn merge_version_profile(base: &mut Value, child: &Value, version_id: &str) {
    base["id"] = Value::String(version_id.to_string());
    base["type"] = Value::String("release".to_string());
    for field in ["mainClass", "minecraftArguments"] {
        if let Some(value) = child.get(field) {
            base[field] = value.clone();
        }
    }
    for field in [
        "downloads",
        "assetIndex",
        "assets",
        "javaVersion",
        "logging",
    ] {
        if base.get(field).is_none_or(Value::is_null)
            && let Some(value) = child.get(field)
        {
            base[field] = value.clone();
        }
    }
    if let Some(child_libraries) = child.get("libraries").and_then(Value::as_array) {
        let libraries = base["libraries"].as_array_mut();
        if let Some(libraries) = libraries {
            for child_library in child_libraries {
                let name = child_library.get("name").and_then(Value::as_str);
                if let Some(index) = name.and_then(|name| {
                    libraries.iter().position(|library| {
                        library.get("name").and_then(Value::as_str) == Some(name)
                    })
                }) {
                    libraries[index] = child_library.clone();
                } else {
                    libraries.push(child_library.clone());
                }
            }
        } else {
            base["libraries"] = Value::Array(child_libraries.clone());
        }
    }
    if let Some(child_arguments) = child.get("arguments").and_then(Value::as_object) {
        if !base.get("arguments").is_some_and(Value::is_object) {
            base["arguments"] = serde_json::json!({"jvm": [], "game": []});
        }
        for section in ["jvm", "game"] {
            if let Some(extra) = child_arguments.get(section).and_then(Value::as_array) {
                let target = base["arguments"][section]
                    .as_array_mut()
                    .expect("argument sections were initialized as arrays");
                target.extend(extra.iter().cloned());
            }
        }
    }
    base.as_object_mut()
        .map(|object| object.remove("inheritsFrom"));
}

fn normalize_maven_library(library: &mut Value) -> IpcResult<()> {
    if library.pointer("/downloads/artifact").is_some() || library.get("artifact").is_some() {
        return Ok(());
    }
    let Some(name) = library.get("name").and_then(Value::as_str) else {
        return Ok(());
    };
    let coord_name = name.split('@').next().unwrap_or(name);
    let coordinate = ml_minecraft::library::Coordinate::parse(coord_name).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "a loader library has an invalid Maven coordinate",
            error.to_string(),
        )
    })?;
    let path = coordinate.artifact_path();
    let safe = ml_core::safe_relative_path(&path).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "a loader library has an unsafe path",
            error.to_string(),
        )
    })?;
    let repository = library
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("https://libraries.minecraft.net/");
    let repository = format!("{}/", repository.trim_end_matches('/'));
    let sha1 = library
        .get("sha1")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let size = library.get("size").and_then(Value::as_i64).unwrap_or(0);
    library["downloads"]["artifact"] = serde_json::json!({
        "path": safe.to_string_lossy().replace('\\', "/"),
        "sha1": sha1,
        "size": size,
        "url": format!("{repository}{path}"),
    });
    Ok(())
}

#[allow(clippy::too_many_lines)] // The profile install is one transaction.
async fn install_forge_profile(
    http: &reqwest::Client,
    service: &mut ml_minecraft::MinecraftService,
    paths: &AppPaths,
    game_version: &str,
    forge_maven_version: &str,
    base_version: &VersionJson,
    profile_id: &str,
) -> IpcResult<Value> {
    let runtime = base_version.java_runtime();
    let runtime_home = paths.jre_dir_for(&runtime.component);
    if !ml_launch::jvm::java_in_home(&runtime_home).is_file() {
        let plan = service
            .runtime_plan(&runtime.component)
            .await
            .map_err(|error| network_error("cannot prepare Java for the Forge installer", error))?;
        let installed = service
            .install_runtime_plan(&plan, Arc::new(|_| {}))
            .await
            .map_err(|error| {
                network_error("cannot download Java for the Forge installer", error)
            })?;
        if !installed.is_success() {
            return Err(IpcError::with_detail(
                ErrorCode::Network,
                "cannot install Java for the Forge installer",
                "the runtime download was incomplete",
            ));
        }
    }
    let java = ml_launch::jvm::find_runtime(paths, &runtime, None).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "cannot find Java for the Forge installer",
            error.to_string(),
        )
    })?;
    let artifact = forge_maven_version.to_string();
    let base_url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{artifact}/forge-{artifact}-installer.jar"
    );
    let installer_url = base_url;
    let hash_url = format!("{installer_url}.sha1");
    let hash_response = http
        .get(&hash_url)
        .send()
        .await
        .map_err(|error| network_error("cannot fetch the Forge installer checksum", error))?;
    if !hash_response.status().is_success() {
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot verify the Forge installer",
            format!(
                "Maven returned HTTP {} for the checksum",
                hash_response.status()
            ),
        ));
    }
    let hash_text = hash_response
        .text()
        .await
        .map_err(|error| network_error("cannot read the Forge installer checksum", error))?;
    let expected = hash_text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 40 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(IpcError::invalid(
            "Forge returned an invalid installer checksum",
        ));
    }
    let installer = paths
        .cache_dir
        .join("forge-installers")
        .join(format!("forge-{artifact}-installer.jar"));
    download_file_verified(&installer_url, &expected, &installer).await?;
    let staging_root = paths
        .cache_dir
        .join(format!("forge-stage-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&staging_root).map_err(IpcError::from)?;
    let java_path = java.executable;
    let installer_path = installer.clone();
    let root_path = staging_root.clone();
    let mut command = tokio::process::Command::new(java_path);
    command
        .arg("-jar")
        .arg(installer_path)
        .arg("--installClient")
        .arg(root_path)
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.as_std_mut().creation_flags(0x0800_0000);
    }
    let output = match tokio::time::timeout(Duration::from_secs(15 * 60), command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(IpcError::with_detail(
                ErrorCode::Launch,
                "cannot start the Forge installer",
                error.to_string(),
            ));
        }
        Err(_) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(IpcError::new(
                ErrorCode::Launch,
                "Forge installation timed out after 15 minutes",
            ));
        }
    };
    if !output.status.success() {
        let _ = fs::remove_dir_all(&staging_root);
        let details = String::from_utf8_lossy(&output.stderr);
        return Err(IpcError::with_detail(
            ErrorCode::Launch,
            "Forge installer failed",
            details.chars().take(1600).collect::<String>(),
        ));
    }
    let profile = match find_forge_profile(&staging_root, game_version, profile_id) {
        Ok(profile) => profile,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
    };
    let staged_libraries = staging_root.join("libraries");
    if staged_libraries.is_dir()
        && let Err(error) = copy_tree(&staged_libraries, &paths.libraries_dir)
    {
        let _ = fs::remove_dir_all(&staging_root);
        return Err(IpcError::with_detail(
            ErrorCode::Storage,
            "cannot copy Forge libraries",
            error,
        ));
    }
    let _ = fs::remove_dir_all(staging_root);
    Ok(profile)
}

fn find_forge_profile(root: &Path, game_version: &str, id: &str) -> IpcResult<Value> {
    let versions_dir = root.join("versions");
    let entries = fs::read_dir(&versions_dir).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Launch,
            "Forge installer created no client profile",
            error.to_string(),
        )
    })?;
    for entry in entries.flatten() {
        let candidate = entry
            .path()
            .join(format!("{}.json", entry.file_name().to_string_lossy()));
        let Ok(bytes) = fs::read(candidate) else {
            continue;
        };
        let Ok(profile) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let profile_id = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let matches_identity = profile_id == id
            || (profile_id.starts_with(game_version)
                && profile_id.to_ascii_lowercase().contains("forge"));
        let has_launch_class = profile.get("mainClass").and_then(Value::as_str).is_some();
        if matches_identity && has_launch_class {
            return Ok(profile);
        }
    }
    Err(IpcError::invalid(
        "Forge installer finished but did not create a recognizable profile",
    ))
}

async fn ensure_profile_libraries(
    version: &VersionJson,
    layout: &InstallLayout,
    paths: &AppPaths,
    platform: ml_core::Platform,
) -> IpcResult<()> {
    let mut tasks = Vec::new();
    for library in version.resolve_libraries(platform, Some(&version.id)) {
        if let (Some(relative), Some(download)) =
            (library.artifact_path(), library.artifact.as_ref())
        {
            let relative_path = ml_core::safe_relative_path(&relative).map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::InvalidInput,
                    "a loader library path is unsafe",
                    error.to_string(),
                )
            })?;
            let target = layout.library_path(&relative_path.to_string_lossy());
            if !file_matches(&target, &download.sha1) {
                tasks.push(
                    DownloadTask::new(download.url.clone(), target, nonempty_hash(&download.sha1))
                        .with_size((download.size > 0).then_some(download.size)),
                );
            }
        }
        for (classifier, download) in &library.natives {
            let Some(relative) = library.native_path(classifier) else {
                continue;
            };
            let relative_path = ml_core::safe_relative_path(&relative).map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::InvalidInput,
                    "a loader native path is unsafe",
                    error.to_string(),
                )
            })?;
            let target = paths.libraries_dir.join(relative_path);
            if !file_matches(&target, &download.sha1) {
                tasks.push(
                    DownloadTask::new(download.url.clone(), target, nonempty_hash(&download.sha1))
                        .with_size((download.size > 0).then_some(download.size)),
                );
            }
        }
    }
    if tasks.is_empty() {
        return Ok(());
    }
    let downloader = Downloader::new()
        .map_err(|error| network_error("cannot create the library downloader", error))?;
    let result = downloader
        .download_all(tasks)
        .await
        .map_err(|error| network_error("cannot download modloader libraries", error))?;
    if !result.is_success() {
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot install modloader libraries",
            result
                .failed
                .iter()
                .map(|(url, cause)| format!("{url}: {cause}"))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(())
}

async fn download_file_verified(url: &str, sha1: &str, target: &Path) -> IpcResult<()> {
    if file_matches(target, sha1) {
        return Ok(());
    }
    let parent = target
        .parent()
        .ok_or_else(|| IpcError::invalid("download path has no parent directory"))?;
    fs::create_dir_all(parent).map_err(IpcError::from)?;
    let task = DownloadTask::new(url, target, Some(sha1.to_string()));
    let downloader = Downloader::new()
        .map_err(|error| network_error("cannot create the verified downloader", error))?;
    let result = downloader
        .download_all(vec![task])
        .await
        .map_err(|error| network_error("cannot download a loader file", error))?;
    if result.is_success() && file_matches(target, sha1) {
        Ok(())
    } else {
        Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot verify a downloaded loader file",
            result
                .failed
                .iter()
                .map(|(_, cause)| cause.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}

fn copy_client_jar(source: &Path, target: &Path, version: &VersionJson) -> IpcResult<()> {
    let client =
        version.downloads.client.as_ref().ok_or_else(|| {
            IpcError::invalid("the base Minecraft release has no client download")
        })?;
    if !source.is_file()
        || !hash_file(source).is_ok_and(|hash| hash.eq_ignore_ascii_case(&client.sha1))
    {
        return Err(IpcError::invalid(
            "the installed Minecraft client jar is missing or corrupt",
        ));
    }
    fs::copy(source, target).map_err(IpcError::from)?;
    Ok(())
}

fn file_matches(path: &Path, sha1: &str) -> bool {
    path.is_file()
        && (sha1.is_empty()
            || hash_file(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(sha1)))
}

fn nonempty_hash(hash: &str) -> Option<String> {
    (!hash.is_empty()).then(|| hash.to_string())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let safe = ml_core::safe_relative_path(&relative.to_string_lossy())
            .map_err(|error| error.to_string())?;
        let target = destination.join(safe);
        let parent = target
            .parent()
            .ok_or_else(|| "library path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::copy(entry.path(), &target).map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn fetch_catalog(http: &reqwest::Client) -> IpcResult<ModloaderCatalog> {
    let mut manifest = ml_minecraft::manifest::ManifestClient::new()
        .map_err(|error| network_error("cannot create the Minecraft manifest client", error))?;
    let manifest = manifest
        .fetch()
        .await
        .map_err(|error| network_error("cannot fetch the Minecraft release manifest", error))?;
    let (forge, forge_maven_xml, fabric_games, api_versions) = tokio::try_join!(
        get_json::<ForgePromotions>(http, FORGE_PROMOTIONS),
        get_text(http, FORGE_MAVEN_METADATA),
        get_json::<Vec<FabricGame>>(http, FABRIC_GAMES),
        fetch_fabric_api_versions(http, &manifest),
    )?;
    let forge_maven_versions = parse_forge_maven_versions(&forge_maven_xml);

    let fabric_stable: std::collections::HashSet<String> = fabric_games
        .iter()
        .filter(|game| game.stable)
        .map(|game| game.version.clone())
        .collect();
    let supported_fabric_versions: Vec<String> = manifest
        .releases()
        .filter(|game| {
            fabric_stable.contains(&game.id)
                && api_versions.contains_key(&game.id)
                && is_supported_minecraft_release(&game.id)
        })
        .map(|game| game.id.clone())
        .collect();

    let mut fabric_loaders = HashMap::new();
    for batch in supported_fabric_versions.chunks(6) {
        let mut loader_jobs = tokio::task::JoinSet::new();
        for version in batch {
            let client = http.clone();
            let version = version.clone();
            loader_jobs.spawn(async move {
                let result = newest_stable_fabric_loader(&client, &version).await;
                (version, result)
            });
        }
        while let Some(job) = loader_jobs.join_next().await {
            let (version, loader) = job.map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Network,
                    "Fabric version lookup failed",
                    error.to_string(),
                )
            })?;
            if let Some(loader) = loader? {
                fabric_loaders.insert(version, loader);
            }
        }
    }

    let versions = manifest
        .releases()
        .filter(|game| is_supported_minecraft_release(&game.id))
        .filter_map(|game| {
            let (forge_version, forge_maven_version) =
                forge_version_for(&forge.promos, &forge_maven_versions, &game.id);
            let fabric_loader = fabric_loaders.get(&game.id).cloned();
            let fabric_api_version = api_versions.get(&game.id).cloned();
            let fabric_loader = fabric_api_version.as_ref().and(fabric_loader);
            (forge_version.is_some() || fabric_loader.is_some()).then(|| ModloaderVersion {
                minecraft_version: game.id.clone(),
                forge_version,
                forge_maven_version,
                fabric_loader,
                fabric_api_version,
            })
        })
        .collect();
    Ok(ModloaderCatalog { versions })
}

async fn get_json<T: for<'de> Deserialize<'de>>(http: &reqwest::Client, url: &str) -> IpcResult<T> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| network_error("cannot fetch modloader metadata", error))?;
    if !response.status().is_success() {
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch modloader metadata",
            format!("{url} returned HTTP {}", response.status()),
        ));
    }
    response
        .json()
        .await
        .map_err(|error| network_error("modloader metadata is invalid", error))
}

async fn get_text(http: &reqwest::Client, url: &str) -> IpcResult<String> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| network_error("cannot fetch Forge Maven metadata", error))?;
    if !response.status().is_success() {
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "cannot fetch Forge Maven metadata",
            format!("{url} returned HTTP {}", response.status()),
        ));
    }
    response
        .text()
        .await
        .map_err(|error| network_error("Forge Maven metadata is invalid", error))
}

async fn fetch_fabric_api_versions(
    http: &reqwest::Client,
    manifest: &ml_minecraft::manifest::VersionManifest,
) -> IpcResult<HashMap<String, String>> {
    let game_versions: Vec<_> = manifest
        .releases()
        .filter(|version| is_supported_minecraft_release(&version.id))
        .map(|version| version.id.clone())
        .collect();
    let mut versions = Vec::new();
    // Keep the URL short enough for proxies and the Modrinth edge even as
    // the Mojang release manifest grows. Each game version appears once.
    for game_version_batch in game_versions.chunks(30) {
        let game_versions = serde_json::to_string(game_version_batch).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Internal,
                "cannot build the Fabric API query",
                error.to_string(),
            )
        })?;
        let mut offset = 0usize;
        loop {
            let response = http
                .get(MODRINTH_FABRIC_API)
                .query(&[
                    ("loaders", "[\"fabric\"]"),
                    ("game_versions", game_versions.as_str()),
                    ("include_changelog", "false"),
                    ("limit", "100"),
                ])
                .query(&[("offset", offset)])
                .send()
                .await
                .map_err(|error| network_error("cannot fetch Fabric API versions", error))?;
            if !response.status().is_success() {
                return Err(IpcError::with_detail(
                    ErrorCode::Network,
                    "cannot fetch Fabric API versions",
                    format!("Modrinth returned HTTP {}", response.status()),
                ));
            }
            let mut page: Vec<ModrinthVersion> = response
                .json()
                .await
                .map_err(|error| network_error("Fabric API metadata is invalid", error))?;
            let page_size = page.len();
            versions.append(&mut page);
            if page_size < 100 {
                break;
            }
            offset += page_size;
        }
    }
    versions.retain(|version| version.version_type == "release");
    versions.sort_by(|left, right| right.date_published.cmp(&left.date_published));

    let mut result = HashMap::new();
    for version in versions {
        let Some(file) = version
            .files
            .iter()
            .find(|file| file.primary && is_jar_name(&file.filename))
            .or_else(|| {
                version
                    .files
                    .iter()
                    .find(|file| is_jar_name(&file.filename))
            })
        else {
            continue;
        };
        if !file.hashes.get("sha1").is_some_and(|hash| valid_sha1(hash))
            || !file.url.starts_with("https://")
        {
            continue;
        }
        for game in version.game_versions {
            result
                .entry(game)
                .or_insert_with(|| version.version_number.clone());
        }
    }
    Ok(result)
}

async fn newest_stable_fabric_loader(
    http: &reqwest::Client,
    game_version: &str,
) -> IpcResult<Option<String>> {
    #[derive(Deserialize)]
    struct LoaderEntry {
        loader: Loader,
    }
    #[derive(Deserialize)]
    struct Loader {
        version: String,
        stable: bool,
    }
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}");
    let loaders: Vec<LoaderEntry> = match get_json(http, &url).await {
        Ok(loaders) => loaders,
        Err(error)
            if error.detail.as_deref().is_some_and(|detail| {
                detail.contains("HTTP 400") || detail.contains("HTTP 404")
            }) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    Ok(loaders
        .into_iter()
        .find(|entry| entry.loader.stable && is_safe_version_token(&entry.loader.version))
        .map(|entry| entry.loader.version))
}

fn forge_version_for(
    promos: &HashMap<String, String>,
    maven_versions: &std::collections::HashSet<String>,
    game: &str,
) -> (Option<String>, Option<String>) {
    for key in [format!("{game}-recommended"), format!("{game}-latest")] {
        let Some(forge_version) = promos
            .get(&key)
            .filter(|version| is_safe_version_token(version))
        else {
            continue;
        };
        if let Some(maven_version) =
            forge_maven_artifact_version(maven_versions, game, forge_version)
        {
            return (Some(forge_version.clone()), Some(maven_version));
        }
    }
    (None, None)
}

fn forge_maven_artifact_version(
    maven_versions: &std::collections::HashSet<String>,
    game: &str,
    forge: &str,
) -> Option<String> {
    let base = format!("{game}-{forge}");
    if maven_versions.contains(&base) {
        return Some(base);
    }
    let parts: Vec<_> = game.split('.').collect();
    let mut suffixes = vec![game.to_string()];
    if parts.len() == 2 {
        suffixes.push(format!("{game}.0"));
    }
    suffixes.push(format!("mc{}", game.replace('.', "")));
    suffixes
        .into_iter()
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| maven_versions.contains(candidate))
}

fn parse_forge_maven_versions(xml: &str) -> std::collections::HashSet<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|tail| tail.split_once("</version>").map(|(version, _)| version))
        .filter(|version| is_safe_version_token(version))
        .map(str::to_owned)
        .collect()
}

fn is_supported_minecraft_release(version: &str) -> bool {
    let parts: Vec<u32> = version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    let valid = (2..=4).contains(&parts.len())
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        && version.split('.').all(|part| !part.is_empty());
    valid && (parts[0], parts[1], parts.get(2).copied().unwrap_or(0)) >= (1, 7, 5)
}

fn is_safe_version_token(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 80
        && version != "."
        && version != ".."
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

fn valid_sha1(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_jar_name(filename: &str) -> bool {
    std::path::Path::new(filename)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
}

fn write_catalog_cache(path: &Path, catalog: &ModloaderCatalog) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(&CatalogCache {
        fetched_at: now_seconds(),
        catalog: catalog.clone(),
    })
    .map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

fn read_catalog_cache(path: &Path) -> Option<CatalogCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn network_error(context: &str, error: impl std::fmt::Display) -> IpcError {
    IpcError::with_detail(ErrorCode::Network, context, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_prefers_recommended_and_uses_latest_as_fallback() {
        let promos = HashMap::from([
            ("1.20.1-latest".to_string(), "47.4.20".to_string()),
            ("1.20.1-recommended".to_string(), "47.4.18".to_string()),
            ("1.21.4-latest".to_string(), "54.1.0".to_string()),
        ]);
        let maven = std::collections::HashSet::from([
            "1.20.1-47.4.18".to_string(),
            "1.21.4-54.1.0".to_string(),
        ]);
        assert_eq!(
            forge_version_for(&promos, &maven, "1.20.1"),
            (
                Some("47.4.18".to_string()),
                Some("1.20.1-47.4.18".to_string())
            )
        );
        assert_eq!(
            forge_version_for(&promos, &maven, "1.21.4"),
            (
                Some("54.1.0".to_string()),
                Some("1.21.4-54.1.0".to_string())
            )
        );
        assert_eq!(forge_version_for(&promos, &maven, "1.21.5"), (None, None));
    }

    #[test]
    fn legacy_forge_builds_use_their_published_maven_suffix() {
        let versions = parse_forge_maven_versions(
            "<metadata><versioning><versions><version>1.7.10-10.13.4.1614-1.7.10</version><version>1.7.2-10.12.2.1161-mc172</version></versions></versioning></metadata>",
        );
        assert_eq!(
            forge_maven_artifact_version(&versions, "1.7.10", "10.13.4.1614").as_deref(),
            Some("1.7.10-10.13.4.1614-1.7.10")
        );
        assert_eq!(
            forge_maven_artifact_version(&versions, "1.7.2", "10.12.2.1161").as_deref(),
            Some("1.7.2-10.12.2.1161-mc172")
        );
    }

    #[test]
    fn release_floor_is_inclusive_and_compares_numeric_components() {
        assert!(!is_supported_minecraft_release("1.7.4"));
        assert!(is_supported_minecraft_release("1.7.5"));
        assert!(is_supported_minecraft_release("1.20.1"));
        assert!(is_supported_minecraft_release("26.3"));
        assert!(!is_supported_minecraft_release("1.7-snapshot"));
    }

    #[test]
    fn fabric_profile_keeps_vanilla_files_and_adds_loader_launch_data() {
        let base = serde_json::json!({
            "id": "1.21.4",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "1.21",
            "assetIndex": {"id": "1.21", "sha1": "abc", "size": 10, "url": "https://example.invalid/index.json"},
            "javaVersion": {"component": "java-runtime-gamma", "majorVersion": 21},
            "downloads": {"client": {"sha1": "1234", "size": 20, "url": "https://example.invalid/client.jar"}},
            "libraries": [{"name": "com.mojang:base:1.0", "downloads": {"artifact": {"path": "com/mojang/base/1.0/base-1.0.jar", "sha1": "abcd", "size": 5, "url": "https://example.invalid/base.jar"}}}],
            "arguments": {"jvm": ["-Xmx2G"], "game": ["--username", "${auth_player_name}"]}
        });
        let child = serde_json::json!({
            "id": "fabric-loader-0.19.5-1.21.4",
            "inheritsFrom": "1.21.4",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "], "game": []},
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.19.5", "url": "https://maven.fabricmc.net/", "sha1": "0123456789012345678901234567890123456789", "size": 100}]
        });

        let result = compose_fabric_profile(
            &serde_json::to_vec(&base).unwrap(),
            "1.21.4",
            "0.19.5",
            child,
        )
        .unwrap();
        let parsed = VersionJson::parse(&serde_json::to_vec(&result).unwrap()).unwrap();

        assert_eq!(parsed.id, "fabric-loader-0.19.5-1.21.4");
        assert_eq!(
            parsed.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        assert_eq!(parsed.downloads.client.as_ref().unwrap().sha1, "1234");
        assert_eq!(parsed.asset_index_id(), Some("1.21"));
        assert_eq!(parsed.arguments.jvm.len(), 2);
        assert_eq!(parsed.libraries.len(), 2);
        assert_eq!(
            parsed.libraries[1].downloads.artifact.as_ref().unwrap().url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
        );
    }

    #[test]
    fn forge_child_profile_is_flattened_and_legacy_arguments_are_kept() {
        let base = serde_json::json!({
            "id": "1.12.2",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "1.12",
            "downloads": {"client": {"sha1": "1234", "size": 20, "url": "https://example.invalid/client.jar"}},
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name}"
        });
        let child = serde_json::json!({
            "id": "1.12.2-forge-14.23.5.2860",
            "inheritsFrom": "1.12.2",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
            "libraries": [{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860", "url": "https://maven.minecraftforge.net/"}]
        });

        let result = compose_forge_profile(
            &serde_json::to_vec(&base).unwrap(),
            child,
            "1.12.2-forge-14.23.5.2860",
        )
        .unwrap();
        let parsed = VersionJson::parse(&serde_json::to_vec(&result).unwrap()).unwrap();
        assert_eq!(parsed.id, "1.12.2-forge-14.23.5.2860");
        assert_eq!(parsed.downloads.client.unwrap().sha1, "1234");
        assert!(parsed.minecraft_arguments.unwrap().contains("FMLTweaker"));
        assert!(parsed.libraries[0].downloads.artifact.is_some());
    }

    #[test]
    fn version_tokens_reject_path_and_url_separators() {
        assert!(is_safe_version_token("14.23.5.2860"));
        assert!(is_safe_version_token("0.16.9+build.1"));
        assert!(!is_safe_version_token("../latest"));
        assert!(!is_safe_version_token("1.2\\x"));
    }
}
