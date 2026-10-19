//! The commands about game instances.
//!
//! An instance is a directory with its own `saves`, `resourcepacks` and
//! `mods`, bound to one version. The commands here create them, list them
//! and remove them.

use ml_storage::Instance;
use serde::{Deserialize, Serialize};
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
