//! JSON-backed stores for data that is small, human-editable and rarely
//! queried: launcher settings and encrypted account secrets.
//!
//! Every write is atomic: the document is written to a temporary file and
//! then renamed over the target, so a crash mid-write can never leave a
//! half-written settings file behind.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};

use ml_core::{Error, Result};

/// A JSON document persisted next to the database.
#[derive(Debug)]
pub struct JsonStore<T> {
    path: PathBuf,
    _marker: PhantomData<T>,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    /// Create a store backed by the given file.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// The file this store is backed by.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the document, returning the default value when the file does not
    /// exist yet.
    ///
    /// # Errors
    /// Fails if the file exists but cannot be read or parsed.
    pub fn load(&self) -> Result<T> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) if raw.trim().is_empty() => Ok(T::default()),
            Ok(raw) => serde_json::from_str(&raw).map_err(|e| Error::Json {
                source_name: self.path.display().to_string(),
                message: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(Error::io(&self.path, e)),
        }
    }

    /// Write the document atomically.
    ///
    /// # Errors
    /// Fails if the document cannot be serialised, the directory cannot be
    /// created, or the file cannot be written.
    pub fn save(&self, value: &T) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let serialized = serde_json::to_string_pretty(value).map_err(|e| Error::Json {
            source_name: self.path.display().to_string(),
            message: e.to_string(),
        })?;

        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, serialized.as_bytes()).map_err(|e| Error::io(&temporary, e))?;
        std::fs::rename(&temporary, &self.path).map_err(|e| {
            // Clean up the temporary file so a failed write does not leave
            // debris behind.
            let _ = std::fs::remove_file(&temporary);
            Error::io(&self.path, e)
        })?;
        Ok(())
    }

    /// Load the document, mutate it and write it back.
    ///
    /// # Errors
    /// Propagates any error from [`JsonStore::load`] or [`JsonStore::save`].
    pub fn update<T2>(&self, mutate: impl FnOnce(&mut T) -> T2) -> Result<T2> {
        let mut value = self.load()?;
        let result = mutate(&mut value);
        self.save(&value)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn missing_file_yields_default() {
        let dir = tempfile::tempdir().unwrap();
        let store: JsonStore<Sample> = JsonStore::new(dir.path().join("absent.json"));
        assert_eq!(store.load().unwrap(), Sample::default());
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store: JsonStore<Sample> = JsonStore::new(dir.path().join("s.json"));
        store
            .save(&Sample {
                name: "x".into(),
                count: 3,
            })
            .unwrap();
        assert_eq!(
            store.load().unwrap(),
            Sample {
                name: "x".into(),
                count: 3
            }
        );
    }

    #[test]
    fn update_mutates_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let store: JsonStore<Sample> = JsonStore::new(dir.path().join("s.json"));
        store.update(|s| s.count += 1).unwrap();
        assert_eq!(store.load().unwrap().count, 1);
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let store: JsonStore<Sample> = JsonStore::new(&path);
        store.save(&Sample::default()).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
