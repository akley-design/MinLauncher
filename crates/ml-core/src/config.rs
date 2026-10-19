//! User-facing configuration, persisted as JSON next to the database.

use serde::{Deserialize, Serialize};

/// Download engine tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadConfig {
    /// Maximum number of simultaneous HTTP connections.
    pub max_concurrent: usize,
    /// Number of times a failed download is retried.
    pub max_retries: u32,
    /// HTTP timeout for a single request, in seconds.
    pub timeout_secs: u64,
    /// Whether to verify SHA-1 checksums of downloaded files.
    pub verify_checksums: bool,
    /// Bandwidth limit in kilobits per second; `0` means unlimited.
    pub bandwidth_limit_kbps: u64,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            // 16 connections saturates a typical home uplink without
            // tripping Mojang's rate limiting.
            max_concurrent: 16,
            max_retries: 3,
            timeout_secs: 30,
            verify_checksums: true,
            bandwidth_limit_kbps: 0,
        }
    }
}

/// JVM tuning exposed to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JvmConfig {
    /// Maximum heap size, e.g. `4G`; empty means "auto-detect".
    pub max_heap: String,
    /// Minimum heap size.
    pub min_heap: String,
    /// Garbage collector to request (`G1`, `Z`, `Shenandoah`); empty = auto.
    pub garbage_collector: String,
    /// Extra JVM arguments appended verbatim.
    pub extra_args: Vec<String>,
    /// Whether to enable the startup-profiling Java agent.
    pub enable_agent: bool,
}

impl Default for JvmConfig {
    fn default() -> Self {
        Self {
            max_heap: String::new(),
            min_heap: String::new(),
            garbage_collector: "G1".to_string(),
            extra_args: Vec::new(),
            enable_agent: true,
        }
    }
}

/// The launcher's complete settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    /// Version schema revision, for future migrations.
    pub schema_version: u32,
    /// Interface language tag, e.g. `ru-RU`.
    pub language: String,
    /// Whether the launcher hides to the tray when the window is closed.
    pub minimize_to_tray: bool,
    /// Whether the launcher starts with Windows.
    pub autostart: bool,
    /// Default game version for new instances.
    pub default_version: Option<String>,
    /// Default Java runtime component; `auto` derives it from the version.
    pub default_jre: String,
    /// Whether to show the launcher's own console window.
    pub show_console: bool,
    /// Download engine tuning.
    pub downloads: DownloadConfig,
    /// JVM tuning.
    pub jvm: JvmConfig,
    /// Whether to collect anonymous performance samples.
    pub telemetry_enabled: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            language: "ru-RU".to_string(),
            minimize_to_tray: true,
            autostart: false,
            default_version: None,
            default_jre: "auto".to_string(),
            show_console: false,
            downloads: DownloadConfig::default(),
            jvm: JvmConfig::default(),
            telemetry_enabled: false,
        }
    }
}

impl LauncherConfig {
    /// Load settings from a JSON document, falling back to defaults when the
    /// document is missing or malformed.
    #[must_use]
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Serialise the settings to pretty-printed JSON.
    ///
    /// # Errors
    /// Fails if the configuration contains values that cannot be serialised.
    pub fn to_json(&self) -> crate::Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| crate::Error::Json {
            source_name: "settings".into(),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let cfg = LauncherConfig::default();
        let json = cfg.to_json().unwrap();
        assert_eq!(LauncherConfig::from_json(&json), cfg);
    }

    #[test]
    fn malformed_json_falls_back_to_defaults() {
        let cfg = LauncherConfig::from_json("{ not json");
        assert_eq!(cfg.language, "ru-RU");
        assert_eq!(cfg.downloads.max_concurrent, 16);
    }

    #[test]
    fn partial_documents_keep_defaults_for_missing_fields() {
        let cfg = LauncherConfig::from_json(r#"{"language":"en-US"}"#);
        assert_eq!(cfg.language, "en-US");
        assert_eq!(cfg.downloads.max_concurrent, 16);
    }
}
