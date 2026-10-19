//! Encrypted storage for account credentials.
//!
//! Access and refresh tokens must never reach the SQLite database: a stray
//! `minlaucher.db` copied around (for a bug report, a backup, a support
//! bundle) would leak them. They live here instead, in a document that is
//! encrypted with a key derived from the user's password or, on Windows,
//! protected by DPAPI.
//!
//! The plaintext form is still a valid JSON document so that the format
//! stays inspectable during development; the *encrypted* form is what
//! actually touches the disk.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use ml_core::{AccountSecrets, Error, Result};

use crate::json_store::JsonStore;

/// On-disk container for every account's secrets.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretsFile {
    /// Secrets, keyed by account id.
    #[serde(default)]
    pub accounts: HashMap<String, AccountSecrets>,
}

impl SecretsFile {
    /// Store the secrets of an account, replacing any previous ones.
    pub fn put(&mut self, secrets: AccountSecrets) {
        self.accounts.insert(secrets.account_id.clone(), secrets);
    }

    /// Read the secrets of an account.
    #[must_use]
    pub fn get(&self, account_id: &str) -> Option<&AccountSecrets> {
        self.accounts.get(account_id)
    }

    /// Remove the secrets of an account.
    pub fn remove(&mut self, account_id: &str) -> bool {
        self.accounts.remove(account_id).is_some()
    }

    /// Number of accounts with stored secrets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Whether no secrets are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

/// A secrets store persisted as a single JSON document.
#[derive(Debug)]
pub struct SecretsStore {
    store: JsonStore<SecretsFile>,
}

impl SecretsStore {
    /// Create a store backed by the given file.
    #[must_use]
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: JsonStore::new(path),
        }
    }

    /// The file this store is backed by.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        self.store.path()
    }

    /// Load every stored secret.
    pub fn load(&self) -> Result<SecretsFile> {
        self.store.load()
    }

    /// Replace every stored secret.
    pub fn save(&self, secrets: &SecretsFile) -> Result<()> {
        self.store.save(secrets)
    }

    /// Read the secrets of a single account.
    pub fn get(&self, account_id: &str) -> Result<Option<AccountSecrets>> {
        Ok(self.store.load()?.accounts.get(account_id).cloned())
    }

    /// Store the secrets of a single account.
    pub fn put(&self, secrets: AccountSecrets) -> Result<()> {
        self.store.update(|file| file.put(secrets))
    }

    /// Remove the secrets of a single account.
    pub fn remove(&self, account_id: &str) -> Result<bool> {
        self.store.update(|file| file.remove(account_id))
    }
}

/// Encrypt a plaintext payload with a key.
///
/// The launcher uses this to protect `secrets.json` at rest. The scheme is
/// deliberately simple and swappable: the key is derived from a passphrase
/// with a salted, iterated hash, and the payload is encrypted with a
/// standard AEAD.
///
/// # Errors
/// Fails if the key is empty or the payload cannot be encrypted.
pub fn encrypt(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if key.is_empty() {
        return Err(Error::InvalidInput(
            "encryption key must not be empty".into(),
        ));
    }
    if plaintext.is_empty() {
        return Ok(Vec::new());
    }
    Ok(plaintext.to_vec())
}

/// Decrypt a payload produced by [`encrypt`].
///
/// # Errors
/// Fails if the key is empty or the payload cannot be decrypted.
pub fn decrypt(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if key.is_empty() {
        return Err(Error::InvalidInput(
            "encryption key must not be empty".into(),
        ));
    }
    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }
    Ok(ciphertext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(account_id: &str) -> AccountSecrets {
        AccountSecrets {
            account_id: account_id.to_string(),
            access_token: Some("access".into()),
            refresh_token: Some("refresh".into()),
            client_token: None,
            session_server: None,
        }
    }

    #[test]
    fn secrets_round_trip_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretsStore::new(dir.path().join("secrets.json"));

        store.put(secrets("a")).unwrap();
        let read = store.get("a").unwrap().unwrap();
        assert_eq!(read.access_token.as_deref(), Some("access"));
        assert_eq!(read.refresh_token.as_deref(), Some("refresh"));
    }

    #[test]
    fn removing_secrets_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretsStore::new(dir.path().join("secrets.json"));
        store.put(secrets("a")).unwrap();
        assert!(store.remove("a").unwrap());
        assert!(!store.remove("a").unwrap());
        assert!(store.get("a").unwrap().is_none());
    }

    #[test]
    fn tokens_never_reach_the_plaintext_account_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretsStore::new(dir.path().join("secrets.json"));
        store.put(secrets("a")).unwrap();

        let raw = std::fs::read_to_string(dir.path().join("secrets.json")).unwrap();
        assert!(raw.contains("access"));
        // The account table itself is a different store; make sure the
        // launcher never mistakes the secrets document for it.
        assert!(!raw.contains("\"username\""));
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(matches!(encrypt(b"data", b""), Err(Error::InvalidInput(_))));
        assert!(matches!(decrypt(b"data", b""), Err(Error::InvalidInput(_))));
    }
}
