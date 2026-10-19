//! Typed repositories over the SQLite schema.
//!
//! Each repository owns one slice of the schema and exposes it as Rust types
//! from [`ml_core`]. No other part of the workspace writes SQL, so the
//! database layout can change without touching the rest of the launcher.

pub mod accounts;
pub mod assets;
pub mod benchmarks;
pub mod downloads;
pub mod instances;
pub mod jres;
pub mod libraries;
pub mod versions;

use rusqlite::Error as SqliteError;

use ml_core::Error;

/// Wrap a SQLite error with the operation that produced it.
///
/// Keeping the context at the call site is what makes a storage failure
/// diagnosable from a log line alone.
pub(crate) fn storage_error(context: &str, source: SqliteError) -> Error {
    Error::Storage(format!("{context}: {source}"))
}

/// Convert an optional integer column into an optional `u32`.
///
/// Negative and oversized values are treated as absent rather than silently
/// wrapping around.
pub(crate) fn to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

/// Convert a boolean column into a `bool`.
pub(crate) fn to_bool(value: i64) -> bool {
    value != 0
}
