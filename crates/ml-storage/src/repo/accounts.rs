//! Repository for player accounts.
//!
//! Only the *public* half of an account lives here. Access and refresh tokens
//! are stored separately, encrypted, in the secrets file — see
//! [`crate::secrets`].

use rusqlite::{OptionalExtension, Row, params};

use ml_core::{Account, AccountType, Error, Result};

use crate::db::{PooledConnection, SqlitePool, now_iso8601};
use crate::repo::{storage_error, to_bool};

/// Reads and writes player accounts.
#[derive(Debug, Clone)]
pub struct AccountRepo {
    pool: SqlitePool,
}

impl AccountRepo {
    /// Create a repository backed by the given pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn conn(&self) -> Result<PooledConnection> {
        crate::db::connection(&self.pool)
    }

    /// Insert or update an account.
    pub fn upsert(&self, account: &Account) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO accounts (id, uuid, username, kind, uuid_prefix, selected, created_at, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                 uuid = excluded.uuid,
                 username = excluded.username,
                 kind = excluded.kind,
                 uuid_prefix = excluded.uuid_prefix,
                 last_used = excluded.last_used",
            params![
                account.id,
                account.uuid,
                account.username,
                kind_to_db(account.kind),
                account.uuid_prefix,
                i64::from(account.selected),
                if account.created_at.is_empty() { now_iso8601() } else { account.created_at.clone() },
                Option::<String>::None,
            ],
        )
        .map_err(|e| storage_error("cannot upsert account", &e))?;
        Ok(())
    }

    /// Atomically persist an offline account and its granted nickname claim.
    pub fn upsert_reserved_offline(&self, account: &Account) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot start transaction", &e))?;
        tx.execute(
            "INSERT INTO accounts (id, uuid, username, kind, uuid_prefix, selected, created_at, last_used)
             VALUES (?1, ?2, ?3, 'offline', ?4, ?5, ?6, NULL)
             ON CONFLICT(id) DO UPDATE SET uuid = excluded.uuid, username = excluded.username,
               kind = excluded.kind, uuid_prefix = excluded.uuid_prefix, last_used = excluded.last_used",
            params![account.id, account.uuid, account.username, account.uuid_prefix, i64::from(account.selected),
                if account.created_at.is_empty() { now_iso8601() } else { account.created_at.clone() }],
        ).map_err(|e| storage_error("cannot upsert reserved account", &e))?;
        tx.execute(
            "INSERT INTO nickname_claims (account_id, nickname, status, updated_at) VALUES (?1, ?2, 'reserved', ?3)
             ON CONFLICT(account_id) DO UPDATE SET nickname = excluded.nickname, status = 'reserved', updated_at = excluded.updated_at",
            params![account.id, account.username, now_iso8601()],
        ).map_err(|e| storage_error("cannot save reserved nickname", &e))?;
        tx.commit()
            .map_err(|e| storage_error("cannot commit reserved account", &e))?;
        Ok(())
    }

    /// Fetch a single account.
    pub fn get(&self, id: &str) -> Result<Option<Account>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, uuid, username, kind, uuid_prefix, selected, created_at
                 FROM accounts WHERE id = ?1",
                params![id],
                row_to_account,
            )
            .optional()
            .map_err(|e| storage_error("cannot read account", &e))?;
        Ok(row)
    }

    /// List all accounts, most recently used first.
    pub fn list(&self) -> Result<Vec<Account>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, uuid, username, kind, uuid_prefix, selected, created_at
                 FROM accounts
                 ORDER BY last_used IS NULL, last_used DESC, username ASC",
            )
            .map_err(|e| storage_error("cannot prepare statement", &e))?;
        let rows = stmt
            .query_map([], row_to_account)
            .map_err(|e| storage_error("cannot query accounts", &e))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| storage_error("cannot read accounts", &e))
    }

    /// The account the user has selected, if any.
    pub fn selected(&self) -> Result<Option<Account>> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, uuid, username, kind, uuid_prefix, selected, created_at
                 FROM accounts WHERE selected = 1 LIMIT 1",
                [],
                row_to_account,
            )
            .optional()
            .map_err(|e| storage_error("cannot read selected account", &e))?;
        Ok(row)
    }

    /// Select an account, clearing the selection on every other account.
    pub fn set_selected(&self, id: &str) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| storage_error("cannot start transaction", &e))?;
        tx.execute("UPDATE accounts SET selected = 0", [])
            .map_err(|e| storage_error("cannot clear selection", &e))?;
        let changed = tx
            .execute(
                "UPDATE accounts SET selected = 1, last_used = ?2 WHERE id = ?1",
                params![id, now_iso8601()],
            )
            .map_err(|e| storage_error("cannot select account", &e))?;
        tx.commit()
            .map_err(|e| storage_error("cannot commit transaction", &e))?;
        if changed == 0 {
            return Err(Error::UnknownVersion(id.to_string()));
        }
        Ok(())
    }

    /// Delete an account.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])
            .map_err(|e| storage_error("cannot delete account", &e))?;
        Ok(changed > 0)
    }

    /// Number of stored accounts.
    pub fn count(&self) -> Result<i64> {
        let conn = self.conn()?;
        conn.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .map_err(|e| storage_error("cannot count accounts", &e))
    }
}

fn kind_to_db(kind: AccountType) -> &'static str {
    match kind {
        AccountType::Microsoft => "microsoft",
        AccountType::Mojang => "mojang",
        AccountType::Offline => "offline",
        AccountType::MinLauncher => "minlauncher",
        AccountType::ThirdParty => "third_party",
    }
}

fn kind_from_db(raw: &str) -> AccountType {
    match raw {
        "microsoft" => AccountType::Microsoft,
        "mojang" => AccountType::Mojang,
        "third_party" => AccountType::ThirdParty,
        "minlauncher" => AccountType::MinLauncher,
        // "offline" and anything the launcher does not know.
        _ => AccountType::Offline,
    }
}

fn row_to_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    let kind: String = row.get(3)?;
    Ok(Account {
        id: row.get(0)?,
        uuid: row.get(1)?,
        username: row.get(2)?,
        kind: kind_from_db(&kind),
        uuid_prefix: row.get(4)?,
        selected: to_bool(row.get(5)?),
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn repo() -> (Database, AccountRepo) {
        let db = Database::open_in_memory().unwrap();
        let repo = AccountRepo::new(db.pool().clone());
        (db, repo)
    }

    #[test]
    fn upsert_and_read_back() {
        let (_db, repo) = repo();
        let account = Account::new("id-1", "1234-5678", "Steve", AccountType::Microsoft);
        repo.upsert(&account).unwrap();

        let read = repo.get("id-1").unwrap().unwrap();
        assert_eq!(read.uuid, "1234-5678");
        assert_eq!(read.kind, AccountType::Microsoft);
        assert_eq!(read.uuid_prefix, "12345678");
    }

    #[test]
    fn selection_is_exclusive() {
        let (_db, repo) = repo();
        repo.upsert(&Account::new("a", "u1", "Alpha", AccountType::Offline))
            .unwrap();
        repo.upsert(&Account::new("b", "u2", "Beta", AccountType::Offline))
            .unwrap();

        repo.set_selected("a").unwrap();
        assert_eq!(repo.selected().unwrap().unwrap().id, "a");

        repo.set_selected("b").unwrap();
        let selected = repo.selected().unwrap().unwrap();
        assert_eq!(selected.id, "b");
        assert_eq!(repo.count().unwrap(), 2);
    }

    #[test]
    fn offline_account_is_offline_capable() {
        let (_db, repo) = repo();
        repo.upsert(&Account::new("a", "u1", "Alpha", AccountType::Offline))
            .unwrap();
        assert!(repo.get("a").unwrap().unwrap().kind.is_offline_capable());
    }

    #[test]
    fn delete_removes_the_account() {
        let (_db, repo) = repo();
        repo.upsert(&Account::new("a", "u1", "Alpha", AccountType::Offline))
            .unwrap();
        assert!(repo.delete("a").unwrap());
        assert!(!repo.delete("a").unwrap());
        assert!(repo.get("a").unwrap().is_none());
    }
}
