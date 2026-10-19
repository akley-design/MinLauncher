//! The commands about player accounts.
//!
//! The tokens of an account never leave the backend: the frontend gets
//! the public half of an account and nothing else. Adding a Microsoft
//! account is not implemented yet — the OAuth flow needs a browser
//! window and a local server, and that is a piece of work of its own.

use ml_core::{Account, AccountType};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::state::SharedState;

/// The accounts of the launcher, and which one is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountList {
    /// Every account the launcher knows.
    pub accounts: Vec<Account>,
    /// The identifier of the selected account, if any.
    pub selected_id: Option<String>,
}

/// The list of the accounts.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn list_accounts(state: State<'_, SharedState>) -> IpcResult<AccountList> {
    let storage = state.storage();
    let accounts = storage.accounts().list().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the accounts",
            error.to_string(),
        )
    })?;

    let selected_id = accounts
        .iter()
        .find(|account| account.selected)
        .map(|account| account.id.clone());

    Ok(AccountList {
        accounts,
        selected_id,
    })
}

/// The account that is selected.
///
/// # Errors
/// Fails when the database cannot be read.
#[tauri::command]
pub fn selected_account(state: State<'_, SharedState>) -> IpcResult<Option<Account>> {
    state.storage().accounts().selected().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read the accounts",
            error.to_string(),
        )
    })
}

/// Select the account the game is started with.
///
/// # Errors
/// Fails when the account does not exist or the database refuses the
/// change.
#[tauri::command]
pub fn select_account(account_id: String, state: State<'_, SharedState>) -> IpcResult<AccountList> {
    let storage = state.storage();

    let account = storage
        .accounts()
        .get(&account_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the accounts",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found(format!("there is no account `{account_id}`")))?;

    storage
        .accounts()
        .set_selected(&account.id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot select the account",
                error.to_string(),
            )
        })?;

    list_accounts(state)
}

/// Add an offline account.
///
/// An offline account has no token and no network round trip, which is
/// exactly what a player who wants to try the game without an
/// account needs.
///
/// # Errors
/// Fails when the name is empty or the database refuses the account.
#[tauri::command]
pub fn add_offline_account(
    username: String,
    state: State<'_, SharedState>,
) -> IpcResult<AccountList> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err(IpcError::invalid("the name of the player cannot be empty"));
    }
    if username.contains(char::is_whitespace) {
        return Err(IpcError::invalid(
            "the name of the player cannot contain spaces",
        ));
    }

    // The offline UUID is derived from the name by the rule of the game,
    // so that the world of an offline player stays the same across the
    // launchers.
    let uuid = ml_core::offline_uuid(&username);
    let id = format!("offline-{uuid}");

    let account = Account::new(id, uuid, username, AccountType::Offline);

    state
        .storage()
        .accounts()
        .upsert(&account)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot add the account",
                error.to_string(),
            )
        })?;

    list_accounts(state)
}

/// Remove an account.
///
/// The secrets of the account are removed with it, so that a token
/// cannot outlive the account it belongs to.
///
/// # Errors
/// Fails when the account does not exist or the database refuses the
/// change.
#[tauri::command]
pub fn remove_account(account_id: String, state: State<'_, SharedState>) -> IpcResult<AccountList> {
    let storage = state.storage();

    let removed = storage.accounts().delete(&account_id).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot remove the account",
            error.to_string(),
        )
    })?;

    if !removed {
        return Err(IpcError::not_found(format!(
            "there is no account `{account_id}`"
        )));
    }

    let _ = storage.secrets().remove(&account_id);

    list_accounts(state)
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
    fn the_offline_uuid_is_the_documented_one() {
        // The name "Notch" has this UUID in every launcher and in the
        // game itself.
        assert_eq!(
            ml_core::offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn the_offline_uuid_is_stable() {
        assert_eq!(
            ml_core::offline_uuid("Steve"),
            ml_core::offline_uuid("Steve")
        );
        assert_ne!(
            ml_core::offline_uuid("Steve"),
            ml_core::offline_uuid("Alex")
        );
    }

    #[test]
    fn an_account_can_be_added_and_removed() {
        let (_dir, state) = state();
        let storage = state.storage();

        let account = Account::new("offline-1", "uuid-1", "Steve", AccountType::Offline);
        storage.accounts().upsert(&account).unwrap();
        assert_eq!(storage.accounts().list().unwrap().len(), 1);

        assert!(storage.accounts().delete("offline-1").unwrap());
        assert_eq!(storage.accounts().list().unwrap().len(), 0);
    }
}
