//! Shared domain model for MinLauncher.
//!
//! This crate is the vocabulary layer: every other crate speaks in the types
//! defined here so that the launcher, the storage engine and the UI can never
//! disagree about what a "version", an "account" or a "rule" is.
//!
//! It deliberately contains **no** I/O beyond filesystem path construction,
//! no async runtime, and no third-party networking, which keeps it cheap to
//! compile and trivial to test.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod account;
pub mod config;
pub mod error;
pub mod paths;
pub mod rules;
pub mod version;

pub use account::{Account, AccountSecrets, AccountType};
pub use config::{DownloadConfig, JvmConfig, LauncherConfig};
pub use error::{Error, Result};
pub use paths::{APP_DIR_NAME, AppPaths};
pub use rules::{
    OsArch, OsCondition, OsName, Platform, Rule, RuleAction, RuleOutcome, VersionRange, evaluate,
};
pub use version::{JavaRuntime, VersionKind, VersionMeta, comparable, components};

/// Crate version, surfaced to the UI so it can display "About" information
/// without hard-coding the version in the frontend.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
