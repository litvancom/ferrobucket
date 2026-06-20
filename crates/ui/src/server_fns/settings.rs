//! Settings server functions: read-only connection view + live writable-status check.
//!
//! Implements D-10: the Settings screen is a read-only connection view.
//! No editable fields. No credential material in returned DTOs.
//!
//! Copy strings match UI-SPEC Copywriting Contract exactly:
//! - healthy: "Server up — data directory present and writable"
//! - unhealthy: "Data directory not writable"

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::server_fns::state::AppState;
use crate::types::{ConnectionInfo, StatusInfo};

/// Build a `ConnectionInfo` from an `AppState` reference.
///
/// Factored out as a plain `fn` so it can be unit-tested without going through
/// the full `#[server]` machinery (Task 3 acceptance criterion).
///
/// SECURITY: `access_key_id` (the public ID) is included; the signing key
/// field from `AppState` is never accessed here.
#[cfg(feature = "ssr")]
pub fn build_connection_info(state: &AppState) -> ConnectionInfo {
    ConnectionInfo {
        endpoint: state.endpoint.clone(),
        region: state.region.clone(),
        access_key_id: state.access_key_id.clone(),
        force_path_style: true, // locked ON per D-10
        data_dir: state.data_root.display().to_string(),
    }
}

/// Return the read-only connection configuration (D-10, REQ-ui-settings).
///
/// Returns `access_key_id` (the public identifier) and always sets
/// `force_path_style = true` (locked ON for ferrobucket). Never returns the
/// signing key.
#[server]
pub async fn get_config_fn() -> Result<ConnectionInfo, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        Ok(build_connection_info(&state))
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Perform a live writable-status probe of the data directory (D-10, REQ-ui-settings).
///
/// Attempts to create and remove a temporary marker file under `data_root`.
/// Returns:
/// - `{ writable: true,  message: "Server up — data directory present and writable" }`
///   on success.
/// - `{ writable: false, message: "Data directory not writable" }` on any failure.
///
/// Copy strings match UI-SPEC Copywriting Contract exactly (see section above).
#[server]
pub async fn check_status_fn() -> Result<StatusInfo, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        let info = probe_writable(&state.data_root).await;
        Ok(info)
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Probe whether `data_root` is present and writable by creating + removing a
/// temporary file. Returns a `StatusInfo` with the UI-SPEC copy strings.
#[cfg(feature = "ssr")]
async fn probe_writable(data_root: &std::path::Path) -> StatusInfo {
    let probe_path = data_root.join(".ferrobucket-writable-probe");
    let writable = tokio::fs::write(&probe_path, b"probe")
        .await
        .is_ok();
    if writable {
        let _ = tokio::fs::remove_file(&probe_path).await;
        StatusInfo {
            writable: true,
            message: "Server up — data directory present and writable".to_owned(),
        }
    } else {
        StatusInfo {
            writable: false,
            message: "Data directory not writable".to_owned(),
        }
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;
    use ferrobucket_storage::FsStorage;
    use leptos::config::LeptosOptions;

    fn make_app_state(data_root: std::path::PathBuf) -> AppState {
        let leptos_options = LeptosOptions::builder()
            .output_name("ferrobucket-ui")
            .build();
        AppState {
            storage: Arc::new(FsStorage::new(&data_root)),
            leptos_options,
            endpoint: "http://127.0.0.1:9000".to_owned(),
            region: "us-east-1".to_owned(),
            access_key_id: Some("DEVKEYID".to_owned()),
            secret_key: "DEVSIGNING_KEY_NEVER_SERIALIZED".to_owned(),
            data_root,
            anonymous: false,
        }
    }

    /// Test that `build_connection_info` returns the configured endpoint, region,
    /// and access_key_id — and that the result does NOT contain the signing key.
    #[test]
    fn settings_fn_connection_info_no_credential_material() {
        let dir = tempdir().unwrap();
        let state = make_app_state(dir.path().to_path_buf());

        let info = build_connection_info(&state);

        // Configured values are returned.
        assert_eq!(info.endpoint, "http://127.0.0.1:9000");
        assert_eq!(info.region, "us-east-1");
        assert_eq!(info.access_key_id, Some("DEVKEYID".to_owned()));

        // force_path_style is always true (locked ON, D-10).
        assert!(info.force_path_style, "force_path_style must be true (locked ON, D-10)");

        // Verify the serialized JSON does not contain the signing key value.
        let json = serde_json::to_string(&info).unwrap();
        assert!(
            !json.contains("DEVSIGNING_KEY_NEVER_SERIALIZED"),
            "Serialized ConnectionInfo must not contain the signing key: {json}"
        );
        // Signing key must not appear anywhere in the struct fields.
        assert!(
            !json.contains("signing"),
            "Serialized ConnectionInfo must not mention 'signing': {json}"
        );
    }

    /// Test that `probe_writable` returns the healthy status for a writable temp dir.
    #[tokio::test]
    async fn settings_fn_writable_probe_healthy() {
        let dir = tempdir().unwrap();
        let info = probe_writable(dir.path()).await;

        assert!(info.writable, "writable must be true for a temp dir");
        assert_eq!(
            info.message,
            "Server up — data directory present and writable",
            "Copy string must match UI-SPEC exactly"
        );
    }

    /// Test that `probe_writable` returns the unhealthy status for a non-existent path.
    #[tokio::test]
    async fn settings_fn_writable_probe_missing_dir() {
        let info = probe_writable(std::path::Path::new("/nonexistent/path/that/does/not/exist")).await;

        assert!(!info.writable, "writable must be false for non-existent path");
        assert_eq!(
            info.message,
            "Data directory not writable",
            "Copy string must match UI-SPEC exactly"
        );
    }
}
