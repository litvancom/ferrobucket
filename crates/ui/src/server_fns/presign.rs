//! Presign server function: mint a presigned GET URL for an S3 object.
//!
//! # Crate-cycle resolution (DEC-storage-decoupled)
//!
//! `crates/ui` cannot depend on `crates/server` (that would be a cycle:
//! `server` → `ui` → `server`). The presign signer has been relocated to
//! `crates/storage::presign` (pure SigV4, no s3s types — allowed under
//! DEC-storage-decoupled). Both `crates/server` and `crates/ui` call it from
//! there. `crates/server` re-exports `presign_url` for backward compatibility
//! with the Phase-3 `presign` CLI subcommand.
//!
//! # Security (T-04-04, DEC-ui-ssr)
//!
//! The signing key is used inside this `#[server]` body (compiled `ssr`-only)
//! and is NEVER returned — only the URL `String` is returned. The signing key
//! is never serialized into any DTO.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::server_fns::state::AppState;

/// Generate a presigned GET URL for `bucket/key` with a 900-second TTL (D-05).
///
/// The signing key is read from `AppState.secret_key` server-side and is NEVER
/// returned or serialized — only the URL string is sent to the browser.
#[server]
pub async fn presign_fn(
    bucket: String,
    key: String,
) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        let path = format!("/{bucket}/{key}");
        let access_key = state.access_key_id.as_deref().unwrap_or("");
        // `state.endpoint` is scheme-included (`format!("http://{listen}")` in
        // server/src/main.rs), but `presign_url` prepends `http://{host}` itself.
        // Strip the scheme here so the minted URL has a single `http://` (GAP-04-07).
        let host = state
            .endpoint
            .strip_prefix("https://")
            .or_else(|| state.endpoint.strip_prefix("http://"))
            .unwrap_or(state.endpoint.as_str());
        let url = ferrobucket_storage::presign::presign_url(
            "GET",
            host,
            &path,
            900, // D-05: 900s TTL hard-coded
            access_key,
            &state.secret_key, // server-side only — NEVER returned
            &state.region,
        );
        Ok(url)
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}
