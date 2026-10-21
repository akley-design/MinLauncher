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

    /// The value the game expects in `--userType`.
    ///
    /// The game reads this value and does nothing much with it, but it
    /// does read it: an unknown value is refused by the server of the
    /// session. Mojang's own launcher writes `msa` for the accounts of
    /// Microsoft and `legacy` for the accounts of Mojang.
    ///
    /// An account that is neither — an offline account or a third-party
    /// one — is `msa`, because that is the value the game of today
    /// accepts. A third-party server is told the kind of the account
    /// through its own injector, not through this argument.
    #[must_use]
    pub fn user_type(self) -> &'static str {
        match self {
            Self::Mojang => "legacy",
            Self::Microsoft | Self::Offline | Self::ThirdParty => "msa",
        }
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

/// The UUID an offline player gets.
///
/// The game derives it from the name in a fixed way, and a launcher that
/// derives it in another way would put an offline player into a
/// different world than the one the game expects.
///
/// The rule is the one of the game: the MD5 of `"OfflinePlayer:"` and the
/// name, with the version nibble set to 3 and the variant forced to the
/// one of RFC 4122. That is what `UUID.nameUUIDFromBytes` does in Java.
///
/// "Notch" gets `b50ad385-829d-3141-a216-7e7d7539ba7f`, and so does
/// every launcher that ever started the game with that name.
#[must_use]
pub fn offline_uuid(username: &str) -> String {
    use std::fmt::Write;

    use md5::{Digest, Md5};

    let mut digest = Md5::digest(format!("OfflinePlayer:{username}").as_bytes());
    let bytes = digest.as_mut_slice();

    // The version is 3 (a name based UUID) and the variant is the one
    // of RFC 4122.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut hex = String::with_capacity(32);
    for byte in bytes.iter() {
        let _ = write!(hex, "{byte:02x}");
    }

    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
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
    fn the_offline_uuid_is_the_one_the_game_derives() {
        // These are the values every launcher and the game itself agree
        // on. A different rule would put an offline player into another
        // world.
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(offline_uuid("jeb_"), "a762f560-4fce-3236-812a-b80efff0b62b");
        assert_eq!(
            offline_uuid(" Dinnerbone"),
            "b8171812-187a-3aaf-b369-d898db88c5c0"
        );
    }

    #[test]
    fn the_offline_uuid_is_a_name_based_uuid() {
        let uuid = offline_uuid("Someone");
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.matches('-').count(), 4);
        // Version 3 and the RFC 4122 variant.
        assert_eq!(uuid.as_bytes()[14], b'3');
        assert!(matches!(uuid.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn the_offline_uuid_is_stable_and_names_differ() {
        assert_eq!(offline_uuid("Steve"), offline_uuid("Steve"));
        assert_ne!(offline_uuid("Steve"), offline_uuid("Alex"));
    }

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

    #[test]
    fn the_user_type_follows_the_kind_of_the_account() {
        // The game refuses a user type it does not know, so the value
        // has to be the one the launcher of Mojang writes.
        assert_eq!(AccountType::Microsoft.user_type(), "msa");
        assert_eq!(AccountType::Mojang.user_type(), "legacy");
        // The accounts that are not Mojang's are the ones of today.
        assert_eq!(AccountType::Offline.user_type(), "msa");
        assert_eq!(AccountType::ThirdParty.user_type(), "msa");
    }
}
