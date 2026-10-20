//! The commands about the updates of the launcher itself.
//!
//! The updater asks a server what the newest release is, and puts the
//! package in place. Until that server exists there is no address to
//! ask, and the commands say so plainly: «сервер обновлений не
//! настроен» is a state of the launcher, not a failure of it.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{ErrorCode, IpcError, IpcResult};

/// What the launcher knows about an update of itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateView {
    /// Whether there is a server to ask at all.
    pub configured: bool,
    /// Whether a newer version is waiting.
    pub available: bool,
    /// The version that is installed.
    pub current_version: String,
    /// The version that is offered, when there is one.
    pub version: Option<String>,
    /// What the author wrote about the release.
    pub notes: Option<String>,
    /// The manifest of the release.
    ///
    /// It is what says which package to put in place, so it travels
    /// back to the installer: the version that is installed is exactly
    /// the one the player was shown.
    pub raw_json: Option<serde_json::Value>,
}

/// What the server announced.
struct Offer {
    version: String,
    notes: Option<String>,
    raw_json: serde_json::Value,
}

/// The three answers the server can give.
enum Answer {
    /// There is no server to ask.
    NoServer,
    /// The launcher is the newest one.
    UpToDate,
    /// A newer version is waiting.
    Newer(Offer),
}

/// Describe the answer of the server.
///
/// The three answers are the three states of the updater: the server is
/// not set up, the launcher is up to date, and a newer version is
/// waiting. They are told apart here, and nowhere else, so that the
/// interface has one shape to deal with.
fn describe(current_version: &str, answer: Answer) -> UpdateView {
    match answer {
        Answer::NoServer => UpdateView {
            configured: false,
            available: false,
            current_version: current_version.to_string(),
            version: None,
            notes: None,
            raw_json: None,
        },
        Answer::UpToDate => UpdateView {
            configured: true,
            available: false,
            current_version: current_version.to_string(),
            version: None,
            notes: None,
            raw_json: None,
        },
        Answer::Newer(offer) => UpdateView {
            configured: true,
            available: true,
            current_version: current_version.to_string(),
            version: Some(offer.version),
            notes: offer.notes,
            raw_json: Some(offer.raw_json),
        },
    }
}

/// Ask the server whether there is a newer version.
///
/// # Errors
/// Fails when the server cannot be reached, or when its answer cannot
/// be read.
#[tauri::command]
pub async fn check_for_update<R: Runtime>(app: AppHandle<R>) -> IpcResult<UpdateView> {
    let current_version = app.package_info().version.to_string();

    let updater = match app.updater() {
        Ok(updater) => updater,
        // No address to ask: the server of the updates is not set up.
        // That is a state of the launcher, not a failure of it.
        Err(tauri_plugin_updater::Error::EmptyEndpoints) => {
            return Ok(describe(&current_version, Answer::NoServer));
        }
        Err(error) => {
            return Err(IpcError::with_detail(
                ErrorCode::Network,
                "cannot ask for an update",
                error.to_string(),
            ));
        }
    };

    let answer = match updater.check().await {
        Ok(Some(update)) => Answer::Newer(Offer {
            version: update.version.clone(),
            notes: update.body.clone(),
            raw_json: update.raw_json.clone(),
        }),
        Ok(None) => Answer::UpToDate,
        Err(error) => {
            return Err(IpcError::with_detail(
                ErrorCode::Network,
                "cannot ask for an update",
                error.to_string(),
            ));
        }
    };

    Ok(describe(&current_version, answer))
}

/// Download the announced update and put it in place.
///
/// The manifest of the release comes back from the check, so that the
/// version that is installed is the one the player was shown.
///
/// # Errors
/// Fails when the package cannot be downloaded, or when its signature
/// does not match the key of the launcher.
#[tauri::command]
pub async fn install_update<R: Runtime>(
    app: AppHandle<R>,
    raw_json: serde_json::Value,
) -> IpcResult<()> {
    let updater = app.updater().map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "the server of the updates is not set up",
            error.to_string(),
        )
    })?;

    let update = updater.restore_update(raw_json).map_err(|error| {
        IpcError::with_detail(
            ErrorCode::Network,
            "cannot read the announced update",
            error.to_string(),
        )
    })?;

    let Some(update) = update else {
        // The release that was offered is not newer than the launcher
        // any more: there is nothing to put in place.
        return Ok(());
    };

    update
        .download_and_install(|_done, _total| {}, || {})
        .await
        .map_err(|error| {
            IpcError::with_detail(
                ErrorCode::Network,
                "cannot install the update",
                error.to_string(),
            )
        })?;

    // On Windows the installer takes over and starts the launcher again
    // by itself. Elsewhere the launcher has to start itself.
    #[cfg(not(windows))]
    app.request_restart();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launcher_without_a_server_says_so() {
        let view = describe("0.1.0", Answer::NoServer);

        assert!(!view.configured);
        assert!(!view.available);
        assert_eq!(view.current_version, "0.1.0");
        assert!(view.version.is_none());
    }

    #[test]
    fn a_launcher_that_is_the_newest_one_says_so() {
        let view = describe("0.1.0", Answer::UpToDate);

        assert!(view.configured);
        assert!(!view.available);
        assert_eq!(view.current_version, "0.1.0");
    }

    #[test]
    fn a_newer_version_is_described_with_its_manifest() {
        let manifest = serde_json::json!({ "version": "0.2.0" });
        let view = describe(
            "0.1.0",
            Answer::Newer(Offer {
                version: "0.2.0".into(),
                notes: Some("Faster.".into()),
                raw_json: manifest.clone(),
            }),
        );

        assert!(view.configured);
        assert!(view.available);
        assert_eq!(view.version.as_deref(), Some("0.2.0"));
        assert_eq!(view.notes.as_deref(), Some("Faster."));
        assert_eq!(view.raw_json, Some(manifest));
    }

    /// The command itself, called on a real application.
    ///
    /// The application is built on the mock runtime of Tauri: it has no
    /// window and needs no display, but it holds the plugin and the
    /// handle the command asks for. The plugin is given the same
    /// configuration the shipped one has — no endpoint, no key.
    #[test]
    fn the_command_answers_without_a_server() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".to_string(),
            serde_json::json!({ "endpoints": [], "pubkey": "" }),
        );

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();

        let view = tauri::async_runtime::block_on(check_for_update(app.handle().clone())).unwrap();

        // There is no address to ask, and that is a state, not a
        // failure: the command answers, and the answer says so.
        assert!(!view.configured);
        assert!(!view.available);
        assert_eq!(view.current_version, "0.1.0");
    }
}
