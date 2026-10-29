//! The commands about player accounts.
//!
//! Tokens stay in the encrypted credentials store; frontend account lists
//! contain only public account information.

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
    /// Local status of globally reserved offline nicknames.
    pub nickname_claims: Vec<ml_storage::NicknameClaim>,
}

/// Identity details calculated with Minecraft's offline UUID rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineIdentity {
    /// Stable local account id.
    pub id: String,
    /// UUID used by the game.
    pub uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountSyncPayload {
    account: Account,
    #[serde(default)]
    secrets: Option<ml_core::AccountSecrets>,
}

#[derive(Debug, Deserialize)]
struct MinecraftHandoffResponse {
    payload: MinecraftHandoffPayload,
}

#[derive(Debug, Deserialize)]
struct MinecraftHandoffPayload {
    account: Account,
    secrets: ml_core::AccountSecrets,
}

/// Redeem a short-lived website link and store its Minecraft identity locally.
#[tauri::command]
#[allow(clippy::too_many_lines)] // One linear validation flow.
pub async fn redeem_minecraft_handoff(
    endpoint: String,
    anon_key: String,
    code: String,
    state: State<'_, SharedState>,
) -> IpcResult<AccountList> {
    let parsed = url::Url::parse(&endpoint)
        .map_err(|_| IpcError::invalid("Адрес передачи Minecraft-аккаунта неверен"))?;
    if parsed.scheme() != "https"
        || parsed
            .host_str()
            .is_none_or(|host| !host.ends_with(".supabase.co"))
        || !parsed
            .path()
            .ends_with("/functions/v1/redeem-minecraft-handoff")
        || parsed.query().is_some()
        || parsed.username() != ""
        || parsed.password().is_some()
        || code.len() != 64
        || !code.bytes().all(|byte| byte.is_ascii_hexdigit())
        || anon_key.is_empty()
        || anon_key.len() > 4096
    {
        return Err(IpcError::invalid(
            "Ссылка для Minecraft-входа повреждена или истекла",
        ));
    }
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "не удалось начать передачу аккаунта",
                error.to_string(),
            )
        })?
        .post(endpoint)
        .header("apikey", anon_key)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "не удалось получить Minecraft-аккаунт",
                error.to_string(),
            )
        })?;
    if !response.status().is_success() {
        let message = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Передача аккаунта истекла или не завершилась".to_string());
        return Err(IpcError::with_detail(
            ErrorCode::Network,
            "не удалось подключить Minecraft-аккаунт",
            message,
        ));
    }
    let handoff: MinecraftHandoffResponse = response.json().await.map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "ответ сайта повреждён",
            error.to_string(),
        )
    })?;
    let mut account = handoff.payload.account;
    let secrets = handoff.payload.secrets;
    if account.kind != AccountType::Microsoft
        || account.id.len() > 128
        || !valid_nickname(&account.username)
        || uuid::Uuid::parse_str(&account.uuid).is_err()
        || account.uuid_prefix != account.uuid.replace('-', "")
        || secrets.account_id != account.id
        || secrets.access_token.as_deref().is_none_or(str::is_empty)
        || secrets.xuid.as_deref().is_none_or(str::is_empty)
        || secrets.client_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(IpcError::invalid(
            "Полученные данные Minecraft-аккаунта некорректны",
        ));
    }
    account.selected = false;
    let storage = state.storage();
    storage.secrets().put(secrets).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "не удалось сохранить защищённые данные Minecraft",
            error.to_string(),
        )
    })?;
    storage.accounts().upsert(&account).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "не удалось сохранить Minecraft-аккаунт",
            error.to_string(),
        )
    })?;
    if storage
        .accounts()
        .selected()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "не удалось прочитать выбранный аккаунт",
                error.to_string(),
            )
        })?
        .is_none()
    {
        storage
            .accounts()
            .set_selected(&account.id)
            .map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "не удалось выбрать Minecraft-аккаунт",
                    error.to_string(),
                )
            })?;
    }
    list_accounts(state)
}

/// Validate a proposed offline nickname and return its stable identity.
#[tauri::command]
pub fn offline_account_identity(username: String) -> IpcResult<OfflineIdentity> {
    if !valid_nickname(&username) {
        return Err(IpcError::invalid(
            "Ник должен содержать 3–16 латинских букв, цифр или _",
        ));
    }
    let uuid = ml_core::offline_uuid(&username);
    Ok(OfflineIdentity {
        id: format!("offline-{uuid}"),
        uuid,
    })
}

/// Add an offline account after the server has granted its nickname claim.
#[tauri::command]
pub fn add_offline_account(
    username: String,
    claim_token: String,
    state: State<'_, SharedState>,
) -> IpcResult<AccountList> {
    if !valid_nickname(&username)
        || claim_token.len() != 64
        || !claim_token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(IpcError::invalid(
            "Нужно подтвердить резервирование ника MinLauncher",
        ));
    }
    let identity = offline_account_identity(username.clone())?;
    let account = Account::new(identity.id, identity.uuid, username, AccountType::Offline);
    state
        .storage()
        .accounts()
        .upsert_reserved_offline(&account)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot add the reserved account",
                error.to_string(),
            )
        })?;
    list_accounts(state)
}

/// Add or refresh a registered `MinLauncher` profile in the local account list.
#[tauri::command]
pub fn upsert_minlauncher_account(
    nickname: String,
    player_uuid: String,
    state: State<'_, SharedState>,
) -> IpcResult<AccountList> {
    if !valid_nickname(&nickname) || uuid::Uuid::parse_str(&player_uuid).is_err() {
        return Err(IpcError::invalid(
            "Профиль MinLauncher содержит неверный ник или UUID",
        ));
    }
    let player_uuid = uuid::Uuid::parse_str(&player_uuid)
        .expect("validated UUID")
        .to_string();
    let account = Account::new(
        format!("minlauncher-{player_uuid}"),
        player_uuid,
        nickname,
        AccountType::MinLauncher,
    );
    let storage = state.storage();
    storage.accounts().upsert(&account).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot sync the MinLauncher profile",
            error.to_string(),
        )
    })?;
    if storage
        .accounts()
        .selected()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the selected account",
                error.to_string(),
            )
        })?
        .is_none()
    {
        storage
            .accounts()
            .set_selected(&account.id)
            .map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot select the MinLauncher profile",
                    error.to_string(),
                )
            })?;
    }
    list_accounts(state)
}

/// Restore an account received from the authenticated cloud sync layer.
#[tauri::command]
pub fn upsert_synced_account(
    account: Account,
    state: State<'_, SharedState>,
) -> IpcResult<AccountList> {
    if account.id.is_empty()
        || account.id.len() > 128
        || !valid_nickname(&account.username)
        || uuid::Uuid::parse_str(&account.uuid).is_err()
        || account.uuid_prefix != account.uuid.replace('-', "")
    {
        return Err(IpcError::invalid(
            "Облачная запись игрового аккаунта повреждена",
        ));
    }
    state
        .storage()
        .accounts()
        .upsert(&account)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot restore synced account",
                error.to_string(),
            )
        })?;
    list_accounts(state)
}

/// Encrypt one account and its credentials locally before they are uploaded.
#[tauri::command]
pub fn encrypt_account_sync_payload(
    account_id: String,
    password: String,
    state: State<'_, SharedState>,
) -> IpcResult<Vec<u8>> {
    if password.chars().count() < 12 {
        return Err(IpcError::invalid(
            "Пароль синхронизации должен содержать не менее 12 символов",
        ));
    }
    let storage = state.storage();
    let mut account = storage
        .accounts()
        .get(&account_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read account for sync",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found("Игровой аккаунт не найден"))?;
    account.selected = false;
    let secrets = storage.secrets().get(&account_id).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot read account credentials for sync",
            error.to_string(),
        )
    })?;
    let payload =
        serde_json::to_vec(&AccountSyncPayload { account, secrets }).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot serialize account sync data",
                error.to_string(),
            )
        })?;
    ml_storage::secrets::encrypt(&payload, password.as_bytes()).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Storage,
            "cannot encrypt account sync data",
            error.to_string(),
        )
    })
}

/// Decrypt a cloud account record locally and restore its credentials without exposing them to UI code.
#[tauri::command]
pub fn restore_account_sync_payload(
    account_id: String,
    payload: Vec<u8>,
    password: String,
    state: State<'_, SharedState>,
) -> IpcResult<Account> {
    if password.chars().count() < 12 {
        return Err(IpcError::invalid(
            "Пароль синхронизации должен содержать не менее 12 символов",
        ));
    }
    let clear = ml_storage::secrets::decrypt(&payload, password.as_bytes()).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "cannot decrypt account sync data",
            error.to_string(),
        )
    })?;
    let mut record: AccountSyncPayload = serde_json::from_slice(&clear).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::InvalidInput,
            "cloud account record is invalid",
            error.to_string(),
        )
    })?;
    if record.account.id != account_id
        || !valid_nickname(&record.account.username)
        || uuid::Uuid::parse_str(&record.account.uuid).is_err()
        || record.account.uuid_prefix != record.account.uuid.replace('-', "")
        || record
            .secrets
            .as_ref()
            .is_some_and(|value| value.account_id != account_id)
    {
        return Err(IpcError::invalid(
            "Облачная запись игрового аккаунта повреждена",
        ));
    }
    record.account.selected = false;
    if let Some(secrets) = record.secrets {
        state.storage().secrets().put(secrets).map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot restore account credentials",
                error.to_string(),
            )
        })?;
    }
    Ok(record.account)
}

/// Save the server reservation status for an offline account.
#[tauri::command]
pub fn set_nickname_claim_status(
    account_id: String,
    nickname: String,
    status: String,
    state: State<'_, SharedState>,
) -> IpcResult<()> {
    if !matches!(status.as_str(), "reserved" | "conflict") {
        return Err(IpcError::invalid("Недопустимый статус резерва ника"));
    }
    let account = state
        .storage()
        .accounts()
        .get(&account_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the account",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found(format!("there is no account `{account_id}`")))?;
    if account.kind != AccountType::Offline || nickname.len() > 64 {
        return Err(IpcError::invalid(
            "Резерв относится только к оффлайн-аккаунту",
        ));
    }
    state
        .storage()
        .nickname_claim_set(&account_id, &nickname, &status)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot save nickname status",
                error.to_string(),
            )
        })?;
    Ok(())
}

/// Clear the local record after the server confirms a queued nickname release.
#[tauri::command]
pub fn clear_nickname_claim(account_id: String, state: State<'_, SharedState>) -> IpcResult<()> {
    let storage = state.storage();
    let pending = storage
        .nickname_claims()
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read nickname status",
                error.to_string(),
            )
        })?
        .iter()
        .any(|claim| claim.account_id == account_id && claim.status == "release_pending");
    if !pending {
        return Err(IpcError::invalid(
            "Для этого аккаунта нет ожидающего освобождения ника",
        ));
    }
    storage
        .nickname_claim_remove(&account_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot clear nickname status",
                error.to_string(),
            )
        })?;
    Ok(())
}

/// Persist a release retry when server reservation succeeded but local creation failed.
#[tauri::command]
pub fn queue_nickname_release(
    account_id: String,
    nickname: String,
    state: State<'_, SharedState>,
) -> IpcResult<()> {
    if !account_id.starts_with("offline-") || account_id.len() > 48 || nickname.len() > 64 {
        return Err(IpcError::invalid("Недопустимый идентификатор резерва ника"));
    }
    state
        .storage()
        .nickname_claim_set(&account_id, &nickname, "release_pending")
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot queue nickname release",
                error.to_string(),
            )
        })?;
    Ok(())
}

fn valid_nickname(value: &str) -> bool {
    (3..=16).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
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
        nickname_claims: storage.nickname_claims().map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read nickname reservations",
                error.to_string(),
            )
        })?,
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

    let account = storage
        .accounts()
        .get(&account_id)
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Storage,
                "cannot read the account",
                error.to_string(),
            )
        })?
        .ok_or_else(|| IpcError::not_found(format!("there is no account `{account_id}`")))?;
    if account.kind == AccountType::Offline {
        let claim_status = storage
            .nickname_claims()
            .map_err(|error| {
                IpcError::with_detail(
                    ErrorCode::Storage,
                    "cannot read nickname status",
                    error.to_string(),
                )
            })?
            .into_iter()
            .find(|claim| claim.account_id == account_id)
            .map(|claim| claim.status);
        match claim_status.as_deref() {
            Some("reserved") => {
                storage
                    .nickname_claim_queue_release(&account_id)
                    .map_err(|error| {
                        IpcError::with_detail(
                            ErrorCode::Storage,
                            "cannot queue nickname release",
                            error.to_string(),
                        )
                    })?;
            }
            Some("conflict") => {
                storage
                    .nickname_claim_remove(&account_id)
                    .map_err(|error| {
                        IpcError::with_detail(
                            ErrorCode::Storage,
                            "cannot clear nickname conflict",
                            error.to_string(),
                        )
                    })?;
            }
            _ => {}
        }
    }

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
    fn offline_nicknames_follow_the_global_registry_format() {
        assert!(valid_nickname("Min_Player7"));
        assert!(!valid_nickname("ab"));
        assert!(!valid_nickname("имя"));
        assert!(!valid_nickname("bad name"));
        assert!(!valid_nickname("abcdefghijklmnopq"));
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

    #[test]
    fn a_microsoft_reauth_replaces_local_secrets_without_refresh_tokens() {
        let (_dir, state) = state();
        let storage = state.storage();
        let account = Account::new(
            "microsoft-11111111-2222-3333-4444-555555555555",
            "11111111-2222-3333-4444-555555555555",
            "Player",
            AccountType::Microsoft,
        );
        storage.accounts().upsert(&account).unwrap();

        for (token, expires_at) in [("old-token", 100), ("new-token", 200)] {
            storage
                .secrets()
                .put(ml_core::AccountSecrets {
                    account_id: account.id.clone(),
                    access_token: Some(token.into()),
                    refresh_token: None,
                    client_token: None,
                    session_server: None,
                    xuid: Some("12345".into()),
                    client_id: Some("application-id".into()),
                    expires_at: Some(expires_at),
                })
                .unwrap();
        }

        let secrets = storage
            .secrets()
            .get(&account.id)
            .unwrap()
            .expect("the reauthenticated credentials are saved");
        assert_eq!(secrets.access_token.as_deref(), Some("new-token"));
        assert_eq!(secrets.expires_at, Some(200));
        assert_eq!(secrets.refresh_token, None);
        assert_eq!(secrets.client_token, None);
    }

    #[test]
    fn minecraft_handoff_uses_the_rust_account_secrets_wire_format() {
        let handoff: MinecraftHandoffResponse = serde_json::from_value(serde_json::json!({
            "payload": {
                "account": {
                    "id": "microsoft-11111111-2222-3333-4444-555555555555",
                    "uuid": "11111111-2222-3333-4444-555555555555",
                    "username": "Player",
                    "kind": "microsoft",
                    "uuid_prefix": "11111111222233334444555555555555",
                    "selected": false,
                    "created_at": "2026-10-28T00:00:00.000Z"
                },
                "secrets": {
                    "account_id": "microsoft-11111111-2222-3333-4444-555555555555",
                    "access_token": "minecraft-access-token",
                    "xuid": "12345",
                    "client_id": "application-id",
                    "expires_at": 1_793_203_200
                }
            }
        }))
        .unwrap();

        assert_eq!(handoff.payload.secrets.expires_at, Some(1_793_203_200));
        assert_eq!(handoff.payload.secrets.refresh_token, None);
        assert_eq!(handoff.payload.secrets.client_token, None);
    }
}
