//! The commands about game instances.
//!
//! An instance is a directory with its own `saves`, `resourcepacks` and
//! `mods`, bound to one version. The commands here create them, list them
//! and remove them.

use ml_storage::Instance;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// The instances of the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceList {
    /// Every instance the launcher knows.
    pub instances: Vec<Instance>,
}

/// The list of the instances.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn list_instances(state: State<'_, SharedState>) -> IpcResult<InstanceList> {
    let instances = state.storage().instances().list().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the instances",
            error.to_string(),
        )
    })?;

    Ok(InstanceList { instances })
}

/// Create an instance.
///
/// # Errors
/// Fails when the name is empty, when the version is not known, or when
/// the directory cannot be created.
#[tauri::command]
pub fn create_instance(
    name: String,
    version_id: String,
    state: State<'_, SharedState>,
) -> IpcResult<InstanceList> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(IpcError::invalid(
            "the name of the instance cannot be empty",
        ));
    }

    let storage = state.storage();

    let version = storage
        .versions()
        .get(&version_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the versions",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found(format!("there is no version `{version_id}`")))?;

    // The identifier is derived from the name, so that the instance can
    // be found again after a restart and so that two instances of the
    // same name do not collide.
    let id = instance_id(&name);

    let game_dir = state.paths().instance_dir(&id);
    std::fs::create_dir_all(&game_dir).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot create the directory of the instance",
            error.to_string(),
        )
    })?;

    let instance = Instance {
        id,
        version_id: version.id,
        name,
        game_dir: game_dir.to_string_lossy().into_owned(),
        java_path: None,
        jvm_args: None,
        created_at: ml_storage::now_iso8601(),
    };

    storage.instances().upsert(&instance).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot save the instance",
            error.to_string(),
        )
    })?;

    list_instances(state)
}

/// Import the local Forge 1.20.1 profile into an isolated MinLauncher instance.
///
/// The source installation is read only. The selected version, libraries,
/// mods, configuration, resource packs, shaders, options and saves are copied
/// into MinLauncher-owned directories.
#[tauri::command]
pub async fn import_forge_instance(
    source_profile_id: String,
    name: String,
    source_game_dir: Option<String>,
    state: State<'_, SharedState>,
) -> IpcResult<InstanceList> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(IpcError::invalid("the instance name cannot be empty"));
    }
    if source_profile_id.is_empty()
        || source_profile_id.contains(['/', '\\', ':'])
        || source_profile_id == "."
        || source_profile_id == ".."
    {
        return Err(IpcError::invalid("the Forge profile identifier is invalid"));
    }

    let instance_id = instance_id(&name);
    if instance_id.is_empty() {
        return Err(IpcError::invalid(
            "the instance name has no usable characters",
        ));
    }

    let paths = state.paths().clone();
    let platform = state.platform();
    let source_game_dir = source_game_dir.map_or_else(
        || {
            paths
                .data_dir
                .parent()
                .unwrap_or(&paths.data_dir)
                .join(".minecraft")
        },
        PathBuf::from,
    );
    let source_profile_id = source_profile_id.clone();
    let copied = tokio::task::spawn_blocking(move || {
        import_forge_profile_files(
            &paths,
            platform,
            &source_game_dir,
            &source_profile_id,
            &name,
            &instance_id,
        )
    })
    .await
    .map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "the Forge profile import task failed",
            error.to_string(),
        )
    })?
    .map_err(|error| {
        IpcError::with_detail(ErrorCode::Storage, "cannot import the Forge profile", error)
    })?;

    let storage = state.storage();
    storage
        .versions()
        .upsert(&copied.version_meta)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot save the imported Forge version",
                error.to_string(),
            )
        })?;
    storage
        .versions()
        .set_json_path(&copied.version_id, &copied.version_json.to_string_lossy())
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot register the imported Forge version document",
                error.to_string(),
            )
        })?;

    let instance = Instance {
        id: copied.instance_id,
        version_id: copied.version_id,
        name: copied.name,
        game_dir: copied.game_dir.to_string_lossy().into_owned(),
        java_path: None,
        jvm_args: None,
        created_at: ml_storage::now_iso8601(),
    };
    storage.instances().upsert(&instance).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot register the imported Forge instance",
            error.to_string(),
        )
    })?;

    list_instances(state)
}

/// Mods installed from official Modrinth files for the supported Forge 1.20.1 profile.
///
/// Versions and SHA-512 hashes are pinned so an import is reproducible and
/// a server response can never silently replace a trusted local jar.
const FORGE_1201_PERFORMANCE_MODS: &[PerformanceArtifact] = &[
    PerformanceArtifact {
        id: "embeddium",
        filename: "embeddium-0.3.31+mc1.20.1.jar",
        version: "0.3.31+mc1.20.1",
        url: "https://cdn.modrinth.com/data/sk9rgfiA/versions/UTbfe5d1/embeddium-0.3.31%2Bmc1.20.1.jar",
        sha512: "ffbf2da4685260a4d5c14c621708bd20722563f084f042d3dfb0a7b87f048e39299648c854a93939129da0d23a15a91ec628560d601e76074b08e275f6e132e9",
    },
    PerformanceArtifact {
        id: "entityculling",
        filename: "entityculling-forge-1.10.5-mc1.20.1.jar",
        version: "1.10.5-1.20.1-forge",
        url: "https://cdn.modrinth.com/data/NNAgCjsB/versions/MloBcsQQ/entityculling-forge-1.10.5-mc1.20.1.jar",
        sha512: "00f4d6987a7902597e6e3f178f77308d795b2e1c46a2bfdad79f718e8a7417373971b83004cbbe491582d91d1658718187383a368a813af0266ba99804746814",
    },
    PerformanceArtifact {
        id: "ferritecore",
        filename: "ferritecore-6.0.1-forge.jar",
        version: "6.0.1",
        url: "https://cdn.modrinth.com/data/uXXizFIs/versions/DG5Fn9Sz/ferritecore-6.0.1-forge.jar",
        sha512: "a1960a7c03dc32d4ccaccaf28afdd9b078758bbd62d15a91d4039a83fa9397a098e89b69591f6bd5190254d9ee97e502504154b9aec764adb8c65f000b75ba2c",
    },
    PerformanceArtifact {
        id: "modernfix",
        filename: "modernfix-forge-5.27.89+mc1.20.1.jar",
        version: "5.27.89+mc1.20.1",
        url: "https://cdn.modrinth.com/data/nmDcB62a/versions/sgbtnCX5/modernfix-forge-5.27.89%2Bmc1.20.1.jar",
        sha512: "4e7d42fa18a0b15a7c38a7ce39291f2c0abc47f06746c10d2f3a8a2360d3df0dd44b2024e83867d3de8f597200b79ac3e899fc5b4a577f2df1cf7e01788b2234",
    },
    PerformanceArtifact {
        id: "immediatelyfast",
        filename: "ImmediatelyFast-1.2.4+1.20.1.jar",
        version: "1.2.4+1.20.1",
        url: "https://cdn.modrinth.com/data/5ZwdcRci/versions/NJ17fqEK/ImmediatelyFast-1.2.4%2B1.20.1.jar",
        sha512: "07c8b0bfe2c032985a664109abe5d55e6eae2faed8b72b38e19af824a38cf08086c31e198fa1890e9871baacdc2421a8bb0fa0c67a08057235b64f3d946319b5",
    },
    PerformanceArtifact {
        id: "midnightlib",
        filename: "midnightlib-forge-1.9.2+1.20.1.jar",
        version: "1.9.2+1.20.1-forge",
        url: "https://cdn.modrinth.com/data/codAaoxh/versions/EVVb5nP3/midnightlib-forge-1.9.2%2B1.20.1.jar",
        sha512: "a4f50d46ad30de0f5622fd6aa8415a949889d114a1dc051b2d8d15ff505273aff80f8b40d61193ca71112567a3be396899e96d0ff1c26af31e77a9c68f70e896",
    },
    PerformanceArtifact {
        id: "cullleaves",
        filename: "cullleaves-forge-4.1.0+1.20.1.jar",
        version: "4.1.0+1.20.1-forge",
        url: "https://cdn.modrinth.com/data/GNxdLCoP/versions/GMVPW4xw/cullleaves-forge-4.1.0%2B1.20.1.jar",
        sha512: "16face192d27bf37dfb7aaca8405c4ebd3a536d43eede9d76aad8714990c91f5bfe8874e4b18f619060e058f5294b6e883ec55d09ce350327618d2323f3d2a44",
    },
];

#[derive(Debug, Clone, Copy)]
struct PerformanceArtifact {
    id: &'static str,
    filename: &'static str,
    version: &'static str,
    url: &'static str,
    sha512: &'static str,
}

/// The result of installing the pinned Forge 1.20.1 performance bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceModInstall {
    /// The instance that received the mods.
    pub instance_id: String,
    /// Mods downloaded and installed during this request.
    pub installed: Vec<String>,
    /// Mods that were already present in a supported version.
    pub already_present: Vec<String>,
}

/// Install client-side performance mods into one isolated Forge 1.20.1 instance.
///
/// The source `.minecraft` tree is never touched. Every downloaded jar is
/// checked against the pinned SHA-512 published for its Modrinth file.
#[tauri::command]
pub async fn install_performance_mods(
    instance_id: String,
    state: State<'_, SharedState>,
) -> IpcResult<PerformanceModInstall> {
    if state.running().is_some() {
        return Err(IpcError::invalid("close the game before changing its mods"));
    }
    let instance = state
        .storage()
        .instances()
        .get(&instance_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the instance",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found(format!("there is no instance `{instance_id}`")))?;
    if !instance.version_id.starts_with("1.20.1-forge-") {
        return Err(IpcError::invalid(
            "the FPS mod bundle currently supports Forge 1.20.1 only",
        ));
    }

    let game_dir = state.paths().instance_dir(&instance.id);
    if instance.game_dir != game_dir.to_string_lossy() {
        return Err(IpcError::invalid(
            "the instance folder does not match MinLauncher's private instance path",
        ));
    }
    let mods_dir = game_dir.join("mods");
    std::fs::create_dir_all(&mods_dir).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot prepare the private mods folder",
            error.to_string(),
        )
    })?;

    for (mod_id, name) in [
        ("rubidium", "Rubidium"),
        ("textrues_embeddium_options", "TexTrue's Embeddium Options"),
    ] {
        if ml_minecraft::mod_meta::directory_contains_forge_mod(&mods_dir, mod_id) {
            return Err(IpcError::invalid(format!(
                "remove {name} from this private instance before installing Embeddium"
            )));
        }
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "cannot prepare the Modrinth downloader",
                error.to_string(),
            )
        })?;
    let stage = mods_dir.join(format!(".minlauncher-performance-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&stage).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot create a temporary mod folder",
            error.to_string(),
        )
    })?;

    let mut installed = Vec::new();
    let mut already_present = Vec::new();
    let mut staged_files = Vec::new();
    let result = async {
        for artifact in FORGE_1201_PERFORMANCE_MODS {
            let destination = mods_dir.join(artifact.filename);
            if ml_minecraft::mod_meta::directory_contains_forge_mod(&mods_dir, artifact.id) {
                let is_expected_artifact = destination.is_file()
                    && hash_sha512(&std::fs::read(&destination).map_err(|error| {
                        IpcError::with_detail(
                            ErrorCode::Storage,
                            "cannot inspect an installed performance mod",
                            error.to_string(),
                        )
                    })?) == artifact.sha512;
                if is_expected_artifact {
                    already_present.push(format!("{} {}", artifact.id, artifact.version));
                    continue;
                }
                return Err(IpcError::invalid(format!(
                    "a different {} mod is already installed; remove it from this private instance before installing {}",
                    artifact.id, artifact.version
                )));
            }

            if destination.exists() {
                let actual = hash_sha512(&std::fs::read(&destination).map_err(|error| {
                    IpcError::with_detail(
                        ErrorCode::Storage,
                        "cannot inspect a mod filename collision",
                        error.to_string(),
                    )
                })?);
                if actual == artifact.sha512 {
                    already_present.push(format!("{} {}", artifact.id, artifact.version));
                    continue;
                }
                return Err(IpcError::invalid(format!(
                    "a different file already uses {}",
                    artifact.filename
                )));
            }

            let response = client
                .get(artifact.url)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|error| {
                    IpcError::with_detail(
                        ErrorCode::Network,
                        "cannot download the Forge performance mod",
                        error.to_string(),
                    )
                })?;
            let bytes = response.bytes().await.map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Network,
                    "cannot read the downloaded Forge performance mod",
                    error.to_string(),
                )
            })?;
            if hash_sha512(&bytes) != artifact.sha512 {
                return Err(IpcError::invalid(format!(
                    "SHA-512 verification failed for {}",
                    artifact.filename
                )));
            }
            let staged = stage.join(artifact.filename);
            std::fs::write(&staged, &bytes).map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot stage a verified performance mod",
                    error.to_string(),
                )
            })?;
            staged_files.push((staged, destination));
            installed.push(format!("{} {}", artifact.id, artifact.version));
        }

        for (staged, destination) in &staged_files {
            if destination.exists() {
                return Err(IpcError::invalid(format!(
                    "a mod appeared at {} while the bundle was downloading",
                    destination.display()
                )));
            }
            std::fs::rename(staged, destination).map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot install a verified performance mod",
                    error.to_string(),
                )
            })?;
        }
        Ok(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&stage);
    result?;

    Ok(PerformanceModInstall {
        instance_id: instance.id,
        installed,
        already_present,
    })
}

fn hash_sha512(bytes: &[u8]) -> String {
    hex::encode(Sha512::digest(bytes))
}

struct ImportedForgeFiles {
    version_id: String,
    version_meta: ml_core::VersionMeta,
    version_json: PathBuf,
    instance_id: String,
    name: String,
    game_dir: PathBuf,
}

fn import_forge_profile_files(
    paths: &ml_core::AppPaths,
    platform: ml_core::Platform,
    source_game_dir: &Path,
    source_profile_id: &str,
    name: &str,
    instance_id: &str,
) -> Result<ImportedForgeFiles, String> {
    if !source_game_dir.is_dir() {
        return Err(format!(
            "the Minecraft directory does not exist: {}",
            source_game_dir.display()
        ));
    }
    let source_versions = source_game_dir.join("versions");
    let source_version_dir = source_versions.join(source_profile_id);
    let source_json_path = source_version_dir.join(format!("{source_profile_id}.json"));
    let source_jar_path = source_version_dir.join(format!("{source_profile_id}.jar"));
    let raw_json = fs::read(&source_json_path)
        .map_err(|error| format!("cannot read {}: {error}", source_json_path.display()))?;
    let mut document: Value = serde_json::from_slice(&raw_json)
        .map_err(|error| format!("the Forge version document is invalid JSON: {error}"))?;
    let (minecraft_version, forge_version) = forge_identity(&document)?;
    if minecraft_version != "1.20.1" {
        return Err(format!(
            "this import supports Forge 1.20.1; the selected profile targets Minecraft {minecraft_version}"
        ));
    }

    let version_id = format!("{minecraft_version}-forge-{forge_version}");
    let version_dir = paths.version_dir(&version_id);
    let game_dir = paths.instance_dir(instance_id);
    let staging_version = paths.versions_dir.join(format!("{version_id}.importing"));
    let staging_game = paths.instances_dir.join(format!("{instance_id}.importing"));
    if version_dir.exists()
        || game_dir.exists()
        || staging_version.exists()
        || staging_game.exists()
    {
        return Err("an instance or Forge version with this identity already exists".into());
    }

    let installed_base = paths.version_dir(&minecraft_version);
    let base_json = installed_base.join("version.json");
    if !base_json.is_file() {
        return Err("install Minecraft 1.20.1 in MinLauncher before importing Forge".into());
    }

    document["id"] = Value::String(version_id.clone());
    document["type"] = Value::String("release".into());
    let normalized_json = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("cannot normalize the Forge profile: {error}"))?;
    let version = ml_minecraft::version::VersionJson::parse(&normalized_json)
        .map_err(|error| error.to_string())?;
    let version_meta = version.to_meta(None);

    fs::create_dir_all(&staging_version)
        .map_err(|error| format!("cannot create the Forge version directory: {error}"))?;
    fs::create_dir_all(&staging_game)
        .map_err(|error| format!("cannot create the instance directory: {error}"))?;

    let result = (|| {
        let target_json = staging_version.join("version.json");
        fs::write(&target_json, &normalized_json)
            .map_err(|error| format!("cannot write the Forge version document: {error}"))?;

        let target_jar = staging_version.join("client.jar");
        let source_jar = if source_jar_path.is_file() {
            source_jar_path.clone()
        } else {
            installed_base.join("client.jar")
        };
        let client = version
            .downloads
            .client
            .as_ref()
            .ok_or_else(|| "the Forge profile has no Minecraft client artifact".to_string())?;
        copy_checked(&source_jar, &target_jar, &client.sha1)?;

        let source_libraries = source_game_dir.join("libraries");
        for library in version.resolve_libraries(platform, Some(&version_id)) {
            if let (Some(relative), Some(download)) =
                (library.artifact_path(), library.artifact.as_ref())
            {
                let relative =
                    ml_core::safe_relative_path(&relative).map_err(|error| error.to_string())?;
                copy_checked(
                    &source_libraries.join(&relative),
                    &paths.libraries_dir.join(&relative),
                    &download.sha1,
                )?;
            }
            for (classifier, download) in &library.natives {
                let Some(relative) = library.native_path(classifier) else {
                    continue;
                };
                let relative =
                    ml_core::safe_relative_path(&relative).map_err(|error| error.to_string())?;
                copy_checked(
                    &source_libraries.join(&relative),
                    &paths.libraries_dir.join(&relative),
                    &download.sha1,
                )?;
            }
        }

        if let Some(asset_id) = version.asset_index_id() {
            let source_index = paths.asset_index(asset_id);
            if !source_index.is_file() {
                return Err(format!(
                    "Minecraft asset index `{asset_id}` is not installed"
                ));
            }
        }

        for directory in [
            "mods",
            "config",
            "defaultconfigs",
            "resourcepacks",
            "shaderpacks",
            "saves",
        ] {
            let source = source_game_dir.join(directory);
            if source.exists() {
                copy_directory(&source, &staging_game.join(directory))
                    .map_err(|error| format!("cannot copy `{directory}`: {error}"))?;
            }
        }
        let source_options = source_game_dir.join("options.txt");
        if source_options.is_file() {
            fs::copy(&source_options, staging_game.join("options.txt"))
                .map_err(|error| format!("cannot copy the game graphics settings: {error}"))?;
        }
        Ok::<(), String>(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging_version);
        let _ = fs::remove_dir_all(&staging_game);
        return Err(error);
    }

    fs::rename(&staging_version, &version_dir)
        .map_err(|error| format!("cannot finalize the Forge version files: {error}"))?;
    fs::rename(&staging_game, &game_dir)
        .map_err(|error| format!("cannot finalize the Forge instance files: {error}"))?;

    Ok(ImportedForgeFiles {
        version_id,
        version_meta,
        version_json: version_dir.join("version.json"),
        instance_id: instance_id.to_string(),
        name: name.to_string(),
        game_dir,
    })
}

fn forge_identity(document: &Value) -> Result<(String, String), String> {
    let mut args = Vec::new();
    for section in ["game", "jvm"] {
        if let Some(entries) = document["arguments"][section].as_array() {
            for entry in entries {
                let values = entry.get("values").or_else(|| entry.get("value"));
                match values {
                    Some(Value::String(value)) => args.push(value.clone()),
                    Some(Value::Array(values)) => {
                        args.extend(values.iter().filter_map(Value::as_str).map(str::to_string))
                    }
                    _ => {}
                }
            }
        }
    }
    let value_after = |flag: &str| {
        args.iter()
            .position(|argument| argument == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let minecraft = value_after("--fml.mcVersion")
        .ok_or_else(|| "the Forge profile does not name its Minecraft version".to_string())?;
    let forge = value_after("--fml.forgeVersion")
        .ok_or_else(|| "the Forge profile does not name its loader version".to_string())?;
    Ok((minecraft, forge))
}

fn copy_checked(source: &Path, target: &Path, expected_sha1: &str) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    if target.is_file()
        && (expected_sha1.is_empty()
            || ml_minecraft::download::hash_file(target)
                .is_ok_and(|hash| hash.eq_ignore_ascii_case(expected_sha1)))
    {
        return Ok(());
    }
    if !source.is_file() {
        return Err(format!(
            "required Forge file is missing: {}",
            source.display()
        ));
    }
    fs::copy(source, target)
        .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
    if !expected_sha1.is_empty() {
        let actual =
            ml_minecraft::download::hash_file(target).map_err(|error| error.to_string())?;
        if !actual.eq_ignore_ascii_case(expected_sha1) {
            let _ = fs::remove_file(target);
            return Err(format!(
                "SHA-1 verification failed for {}",
                source.display()
            ));
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

/// Remove an instance.
///
/// The directory of the instance is **not** removed: the world of a
/// player is worth more than the tidiness of a launcher, and a launcher
/// that deletes a world on a misclick is a launcher nobody trusts.
///
/// # Errors
/// Fails when the instance does not exist or the database refuses the
/// change.
#[tauri::command]
pub fn remove_instance(
    instance_id: String,
    state: State<'_, SharedState>,
) -> IpcResult<InstanceList> {
    let removed = state
        .storage()
        .instances()
        .delete(&instance_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot remove the instance",
                error.to_string(),
            )
        })?;

    if !removed {
        return Err(IpcError::not_found(format!(
            "there is no instance `{instance_id}`"
        )));
    }

    list_instances(state)
}

/// The identifier an instance gets from its name.
///
/// The name is lowered and everything that is not a letter or a digit
/// becomes a dash, so that the identifier is safe to use as a directory
/// name on every operating system.
///
/// Letters and digits are counted in the whole of Unicode, not only in
/// ASCII: an instance called "Мой мир" is a perfectly ordinary name, and
/// an identifier built out of dashes alone would send every such instance
/// into the same directory.
#[must_use]
pub fn instance_id(name: &str) -> String {
    let mut id = String::with_capacity(name.len());
    let mut last_was_dash = false;

    for character in name.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            id.push(character);
            last_was_dash = false;
        } else if !last_was_dash {
            id.push('-');
            last_was_dash = true;
        }
    }

    id.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{AppPaths, OsArch, OsName, Platform};

    fn state() -> (tempfile::TempDir, SharedState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let platform = Platform::new(OsName::Windows, OsArch::X86_64);
        let state = crate::state::AppState::new(paths, platform).unwrap();
        (dir, std::sync::Arc::new(state))
    }

    #[test]
    fn the_identifier_is_safe_as_a_directory_name() {
        assert_eq!(instance_id("My World"), "my-world");
        assert_eq!(instance_id("  Spaced  Out  "), "spaced-out");
        assert_eq!(instance_id("Weird/Name:Here"), "weird-name-here");
        assert_eq!(instance_id("Ünïcödé"), "ünïcödé");
    }

    #[test]
    fn the_identifier_keeps_the_letters_of_the_name() {
        // A name in any alphabet has to keep its letters, or every
        // instance of that name would end up in the same directory.
        assert_eq!(instance_id("Мой мир"), "мой-мир");
        assert_eq!(instance_id("Мир"), "мир");
        assert_ne!(instance_id("Мой мир"), "");
    }

    #[test]
    fn the_identifier_does_not_have_empty_parts() {
        assert_eq!(instance_id("---"), "");
        assert_eq!(instance_id("a---b"), "a-b");
    }

    #[test]
    fn the_same_name_gives_the_same_identifier() {
        assert_eq!(instance_id("Modded"), instance_id("modded"));
    }

    #[test]
    fn an_instance_is_saved_and_removed() {
        let (_dir, state) = state();
        let storage = state.storage();

        let instance = Instance {
            id: instance_id("My World"),
            version_id: "26.3".into(),
            name: "My World".into(),
            game_dir: state
                .paths()
                .instance_dir("my-world")
                .to_string_lossy()
                .into_owned(),
            java_path: None,
            jvm_args: None,
            created_at: ml_storage::now_iso8601(),
        };

        storage.instances().upsert(&instance).unwrap();
        let list = storage.instances().list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "my-world");
        assert_eq!(list[0].name, "My World");

        assert!(storage.instances().delete("my-world").unwrap());
        assert_eq!(storage.instances().list().unwrap().len(), 0);
    }
}
