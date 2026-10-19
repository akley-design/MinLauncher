//! Player accounts and the credential handling that surrounds them.

use serde::{Deserialize, Serialize};

/// The kind of account a player can sign in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    /// Microsoft / Mojang OAuth login.
    Microsoft,
    /// A legacy Mojang account.
    Mojang,
    /// An offline ("cracked") account with no authentication.
    Offline,
    /// A third-party Yggdrasil server, reached through authlib-injector.
    ThirdParty,
}

impl AccountType {
    /// Whether this account type can launch the game without a network round
    /// trip to Mojang.
    #[must_use]
    pub fn is_offline_capable(self) -> bool {
        matches!(self, Self::Offline)
    }
}

/// A player account as stored by the launcher.
///
/// Tokens are deliberately **not** part of this struct: they live in the
/// encrypted secrets file so that a database dump never leaks credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Stable identifier of the account inside the launcher.
    pub id: String,
    /// Player UUID.
    pub uuid: String,
    /// Player name.
    pub username: String,
    /// Account kind.
    pub kind: AccountType,
    /// UUID without the leading hyphens, as the legacy servers expect it.
    pub uuid_prefix: String,
    /// Whether the account is currently selected.
    pub selected: bool,
    /// When the account was created, ISO-8601.
    pub created_at: String,
}

impl Account {
    /// Create an account with sensible defaults.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        uuid: impl Into<String>,
        username: impl Into<String>,
        kind: AccountType,
    ) -> Self {
        let uuid = uuid.into();
        let uuid_prefix = uuid.replace('-', "");
        Self {
            id: id.into(),
            uuid,
            username: username.into(),
            kind,
            uuid_prefix,
            selected: false,
            created_at: String::new(),
        }
    }
}

/// The secret half of an account: everything that must not be persisted in
/// clear text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSecrets {
    /// Account this secret belongs to.
    pub account_id: String,
    /// OAuth access token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// OAuth refresh token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Client token used for token refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_token: Option<String>,
    /// Yggdrasil session server, for third-party accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_server: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_prefix_is_derived() {
        let a = Account::new(
            "a",
            "1234-5678-9abc-def0-1234-56789abc0000",
            "Steve",
            AccountType::Offline,
        );
        assert_eq!(a.uuid_prefix, "123456789abcdef0123456789abc0000");
    }

    #[test]
    fn secrets_do_not_leak_into_account_json() {
        let account = Account::new("a", "u", "Steve", AccountType::Microsoft);
        let json = serde_json::to_string(&account).unwrap();
        assert!(!json.contains("token"));
    }
}
