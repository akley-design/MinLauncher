//! Error taxonomy shared by every `MinLauncher` crate.
//!
//! The tree is deliberately shallow: a small set of stable variants that the
//! UI can map onto user-facing messages, plus a catch-all for anything that
//! escapes a lower layer. Every error carries enough context to be actionable
//! without re-querying the caller.

use std::path::PathBuf;

/// Top-level error type used across the workspace.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem operation failed.
    #[error("filesystem error at `{path}`: {source}")]
    Io {
        /// Path that was being accessed.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// A downloaded or cached file did not match its expected hash.
    #[error("checksum mismatch for `{path}`: expected sha1 {expected}, got {actual}")]
    ChecksumMismatch {
        /// File that failed verification.
        path: PathBuf,
        /// Hash the manifest declared.
        expected: String,
        /// Hash that was actually computed.
        actual: String,
    },

    /// A remote resource could not be retrieved.
    #[error("network request to `{url}` failed: {source}")]
    Network {
        /// URL that was requested.
        url: String,
        /// Underlying transport error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The HTTP server answered with a non-success status.
    #[error("http {status} while fetching `{url}`")]
    HttpStatus {
        /// URL that was requested.
        url: String,
        /// Numeric status code.
        status: u16,
    },

    /// A JSON document could not be parsed.
    #[error("malformed json in `{source_name}`: {message}")]
    Json {
        /// Logical name of the document (e.g. `version_manifest`).
        source_name: String,
        /// Human readable parse error.
        message: String,
    },

    /// The database could not be opened or migrated.
    #[error("storage error: {0}")]
    Storage(String),

    /// A Minecraft version was requested but is not known to the launcher.
    #[error("unknown game version `{0}`")]
    UnknownVersion(String),

    /// The launcher could not find a suitable Java runtime.
    #[error("no compatible java runtime found (required: java {0}+)")]
    NoJavaRuntime(u32),

    /// A game process could not be spawned.
    #[error("failed to launch the game: {0}")]
    ProcessSpawn(String),

    /// The game process exited with a non-zero status.
    #[error("game exited with {code}{}", match .signal { Some(s) => format!(" (signal {s})"), None => String::new() })]
    GameExited {
        /// Process exit code.
        code: i32,
        /// Terminating signal, if any (POSIX only).
        signal: Option<i32>,
    },

    /// User-supplied data failed validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The requested feature is not implemented yet.
    #[error("not implemented: {0}")]
    Unimplemented(String),

    /// Anything that does not fit a more specific variant.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// Convenience alias used throughout the workspace.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Attach a filesystem path to an [`std::io::Error`].
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Build a [`Error::Json`] from a serde error.
    #[must_use]
    pub fn json(source_name: impl Into<String>, source: &serde_json::Error) -> Self {
        Self::Json {
            source_name: source_name.into(),
            message: source.to_string(),
        }
    }

    /// Build a [`Error::Network`] from any transport-ish error.
    #[must_use]
    pub fn network(
        url: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self::Network {
            url: url.into(),
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_keeps_path() {
        let err = Error::io(
            "/tmp/x",
            std::io::Error::new(std::io::ErrorKind::NotFound, "nope"),
        );
        assert!(err.to_string().contains("/tmp/x"));
    }

    #[test]
    fn game_exited_renders_signal() {
        let err = Error::GameExited {
            code: 1,
            signal: Some(9),
        };
        assert!(err.to_string().contains("signal 9"));
    }
}
