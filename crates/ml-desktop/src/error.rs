//! The error that crosses the IPC boundary.
//!
//! A Tauri command returns a `Result<T, E>`, and `E` has to be
//! serialisable, because it travels to the frontend as JSON. The error of
//! [`ml_core`] is a rich Rust enum that cannot cross that boundary, so
//! every command converts it into an [`IpcError`]: a flat structure with
//! a stable machine-readable `code` and a message that can be shown to
//! a user as is.
//!
//! The `code` is what the frontend switches on. The message is what the
//! frontend shows.

use serde::{Deserialize, Serialize};

/// A failure that a command reports to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    /// The machine-readable kind of the failure.
    pub code: ErrorCode,
    /// The message for the user.
    pub message: String,
    /// The underlying cause, when there is one worth showing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The machine-readable kind of a failure.
///
/// The variants are the ones the frontend has to tell apart. They are
/// deliberately few: a frontend that has to handle thirty error codes
/// handles none of them properly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The request itself was wrong.
    InvalidInput,
    /// The thing that was asked for does not exist.
    NotFound,
    /// The database or the filesystem refused.
    Storage,
    /// The network refused.
    Network,
    /// The game could not be started.
    Launch,
    /// The feature is not implemented yet.
    Unimplemented,
    /// Something else went wrong.
    Internal,
}

impl IpcError {
    /// An error with a code and a message.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    /// An error with a code, a message and a cause.
    #[must_use]
    pub fn with_detail(
        code: ErrorCode,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Some(detail.into()),
        }
    }

    /// The error of a value that was rejected before anything was done.
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    /// The error of something that was not found.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(detail) => write!(f, "{} ({detail})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for IpcError {}

impl From<ml_core::Error> for IpcError {
    fn from(error: ml_core::Error) -> Self {
        use ml_core::Error as E;

        let code = match &error {
            E::InvalidInput(_) => ErrorCode::InvalidInput,
            E::UnknownVersion(_) => ErrorCode::NotFound,
            E::Io { .. } | E::ChecksumMismatch { .. } | E::Storage(_) | E::Json { .. } => {
                ErrorCode::Storage
            }
            E::Network { .. } | E::HttpStatus { .. } => ErrorCode::Network,
            E::ProcessSpawn(_) | E::GameExited { .. } | E::NoJavaRuntime(_) => ErrorCode::Launch,
            E::Unimplemented(_) => ErrorCode::Unimplemented,
            E::Other(_) => ErrorCode::Internal,
        };

        let message = error.to_string();
        Self::new(code, message)
    }
}

impl From<IpcError> for ml_core::Error {
    fn from(error: IpcError) -> Self {
        ml_core::Error::Other(error.into())
    }
}

impl From<std::io::Error> for IpcError {
    fn from(error: std::io::Error) -> Self {
        Self::with_detail(
            ErrorCode::Storage,
            "a filesystem operation failed",
            error.to_string(),
        )
    }
}

/// The result of a command.
pub type IpcResult<T> = std::result::Result<T, IpcError>;

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::Error as E;

    #[test]
    fn a_domain_error_keeps_its_kind() {
        let error: IpcError = E::UnknownVersion("26.3".into()).into();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(error.message.contains("26.3"));
    }

    #[test]
    fn the_kind_of_every_domain_error_is_mapped() {
        let cases = [
            (E::InvalidInput("x".into()), ErrorCode::InvalidInput),
            (E::Storage("x".into()), ErrorCode::Storage),
            (E::Unimplemented("x".into()), ErrorCode::Unimplemented),
        ];
        for (error, expected) in cases {
            assert_eq!(IpcError::from(error).code, expected);
        }
    }

    #[test]
    fn the_error_serialises_to_json() {
        let error = IpcError::with_detail(ErrorCode::Storage, "cannot open", "no such file");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"code\":\"storage\""), "{json}");
        assert!(json.contains("cannot open"), "{json}");

        let back: IpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, error);
    }

    #[test]
    fn an_error_without_a_detail_leaves_the_field_out() {
        let error = IpcError::invalid("bad version");
        let json = serde_json::to_string(&error).unwrap();
        assert!(!json.contains("detail"), "{json}");
    }

    #[test]
    fn the_display_shows_the_cause() {
        let error = IpcError::with_detail(ErrorCode::Storage, "cannot open", "no such file");
        assert_eq!(error.to_string(), "cannot open (no such file)");
    }
}
