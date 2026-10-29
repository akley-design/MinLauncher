//! Secure storage for the Supabase OAuth session used by the launcher.

use keyring::Entry;
use tauri::command;
use uuid::Uuid;

const SERVICE: &str = "com.minlaucher.app";
const CHUNK_BYTES: usize = 1800;
const MAX_VALUE_BYTES: usize = 64_000;

fn valid_storage_key(key: &str) -> bool {
    key.len() <= 80
        && (key.starts_with("sb-") || key.starts_with("ml-nickname-"))
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn entry(key: &str) -> Result<Entry, String> {
    if key.len() > 120
        || !(key.starts_with("sb-") || key.starts_with("ml-nickname-"))
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err("Недопустимый ключ хранилища авторизации.".into());
    }
    Entry::new(SERVICE, key)
        .map_err(|_| "Не удалось открыть системное хранилище учётных данных.".into())
}

fn manifest_key(key: &str) -> String {
    format!("{key}.manifest")
}

fn chunk_key(key: &str, generation: &str, index: usize) -> String {
    format!("{key}.{generation}.{index}")
}

fn read_manifest(key: &str) -> Result<Option<(String, usize)>, String> {
    if !valid_storage_key(key) {
        return Err("Недопустимый ключ хранилища авторизации.".into());
    }
    let value = match entry(&manifest_key(key))?.get_password() {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(_) => return Err("Не удалось прочитать сессию из системного хранилища.".into()),
    };
    let Some((generation, count)) = value.split_once(':') else {
        return Err("Повреждена сохранённая сессия MinLauncher.".into());
    };
    let count = count
        .parse::<usize>()
        .ok()
        .filter(|count| (1..=64).contains(count))
        .ok_or_else(|| "Повреждена сохранённая сессия MinLauncher.".to_string())?;
    if Uuid::parse_str(generation).is_err() {
        return Err("Повреждена сохранённая сессия MinLauncher.".into());
    }
    Ok(Some((generation.to_string(), count)))
}

fn remove_generation(key: &str, generation: &str, count: usize) {
    for index in 0..count {
        let Ok(entry) = entry(&chunk_key(key, generation, index)) else {
            continue;
        };
        let _ = entry.delete_credential();
    }
}

/// Read an OAuth session item from the operating system credential vault.
#[command]
pub fn identity_storage_get(key: String) -> Result<Option<String>, String> {
    let Some((generation, count)) = read_manifest(&key)? else {
        return Ok(None);
    };
    let mut value = String::new();
    for index in 0..count {
        let chunk = entry(&chunk_key(&key, &generation, index))?
            .get_password()
            .map_err(|_| "Не удалось прочитать сессию из системного хранилища.".to_string())?;
        value.push_str(&chunk);
    }
    Ok(Some(value))
}

/// Store an OAuth session item in the operating system credential vault.
#[command]
pub fn identity_storage_set(key: String, value: String) -> Result<(), String> {
    if !valid_storage_key(&key) {
        return Err("Недопустимый ключ хранилища авторизации.".into());
    }
    if value.len() > MAX_VALUE_BYTES {
        return Err("Слишком большой элемент сессии.".into());
    }
    let generation = Uuid::new_v4().to_string();
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    for character in value.chars() {
        if chunk.len() + character.len_utf8() > CHUNK_BYTES {
            chunks.push(std::mem::take(&mut chunk));
        }
        chunk.push(character);
    }
    if !chunk.is_empty() || chunks.is_empty() {
        chunks.push(chunk);
    }
    for (index, value) in chunks.iter().enumerate() {
        if entry(&chunk_key(&key, &generation, index))
            .and_then(|entry| {
                entry
                    .set_password(value)
                    .map_err(|_| "Не удалось сохранить сессию в системном хранилище.".into())
            })
            .is_err()
        {
            remove_generation(&key, &generation, chunks.len());
            return Err("Не удалось сохранить сессию в системном хранилище.".into());
        }
    }

    let previous = match read_manifest(&key) {
        Ok(previous) => previous,
        Err(error) => {
            remove_generation(&key, &generation, chunks.len());
            return Err(error);
        }
    };
    let manifest = entry(&manifest_key(&key))?;
    if manifest
        .set_password(&format!("{generation}:{}", chunks.len()))
        .is_err()
    {
        remove_generation(&key, &generation, chunks.len());
        return Err("Не удалось сохранить сессию в системном хранилище.".into());
    }
    if let Some((old_generation, old_count)) = previous {
        remove_generation(&key, &old_generation, old_count);
    }
    Ok(())
}

/// Remove an OAuth session item from the operating system credential vault.
#[command]
pub fn identity_storage_remove(key: String) -> Result<(), String> {
    let previous = read_manifest(&key)?;
    match entry(&manifest_key(&key))?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {
            if let Some((generation, count)) = previous {
                remove_generation(&key, &generation, count);
            }
            Ok(())
        }
        Err(_) => Err("Не удалось удалить сессию из системного хранилища.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{CHUNK_BYTES, valid_storage_key};

    #[test]
    fn storage_keys_are_limited_to_supabase_auth_names() {
        assert!(valid_storage_key("sb-project-auth-token"));
        assert!(valid_storage_key("sb-project-auth-token-code-verifier"));
        assert!(valid_storage_key(
            "ml-nickname-offline-b50ad385-829d-3141-a216-7e7d7539ba7f"
        ));
        assert!(!valid_storage_key("other-project-auth-token"));
        assert!(!valid_storage_key("sb-project-auth-token/../other"));
        assert!(!valid_storage_key("ml-nickname-../../other"));
        assert!(!valid_storage_key(&format!("sb-{}", "x".repeat(80))));
        const { assert!(CHUNK_BYTES < 2560) };
    }
}
