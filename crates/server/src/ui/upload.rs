//! Streaming upload handler for `POST /ui/upload/{bucket}/{*key}`.
//!
//! Dispatch logic (controlled by query params):
//! 1. `?uploadId=<id>&partNumber=<n>` → multipart part upload (D-07)
//! 2. `?uploadId=<id>&action=complete` → complete multipart upload
//! 3. `?uploadId=<id>&action=abort`    → abort multipart upload (D-07 cleanup)
//! 4. `?action=create`                 → create multipart upload, return uploadId
//! 5. (no params)                      → small-file single streaming PUT (D-06)
//!
//! NEVER uses `axum::extract::Multipart` — the upload body is raw bytes, not
//! multipart/form-data (RESEARCH Pitfall 6). Raw body streaming maps directly
//! to `FsStorage.put_object` / `upload_part`.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use ferrobucket_storage::Storage;
use futures::StreamExt;
use serde::Deserialize;

use crate::ui::AppState;

/// Query parameters for the upload endpoint.
///
/// All fields are optional so the handler can dispatch on their presence.
#[derive(Deserialize, Debug)]
pub struct UploadParams {
    /// Multipart upload ID (returned by `create_multipart_upload`).
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,

    /// Part number (1-based) for `upload_part`.
    #[serde(rename = "partNumber")]
    pub part_number: Option<i32>,

    /// Action — `"complete"`, `"abort"`, or `"create"`.
    /// Takes precedence over `partNumber` when `upload_id` is also present.
    pub action: Option<String>,
}

/// POST /ui/upload/{bucket}/{*key}
///
/// Dispatches based on query parameters:
/// - `action=create`                          → `create_multipart_upload`; body ignored
/// - `uploadId=<id>&partNumber=<n>`           → `upload_part` (streaming raw bytes)
/// - `uploadId=<id>&action=complete`          → `complete_multipart_upload` (body = JSON `[1,2,3]`)
/// - `uploadId=<id>&action=abort`             → `abort_multipart_upload`; body ignored
/// - (no params)                              → small-file `put_object` (streaming raw bytes)
///
/// Raw-body streaming is used throughout — NOT `axum::extract::Multipart` (Pitfall 6).
pub async fn upload_handler(
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<UploadParams>,
    State(state): State<AppState>,
    body: Body,
) -> Response {
    match &params.action {
        // ── 1. Create multipart upload ────────────────────────────────────────
        Some(action) if action == "create" => {
            // Derive content-type from the query or leave None; can be extended later.
            let content_type: Option<String> = None;
            match state.storage.create_multipart_upload(&bucket, &key, content_type).await {
                Ok(upload_id) => (StatusCode::OK, upload_id).into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }

        // ── 2. Complete multipart upload ──────────────────────────────────────
        Some(action) if action == "complete" => {
            let Some(upload_id) = params.upload_id else {
                return (StatusCode::BAD_REQUEST, "missing uploadId").into_response();
            };
            // Body contains a JSON array of part numbers, e.g. [1, 2, 3].
            let bytes = axum::body::to_bytes(body, 1024 * 1024).await;
            let parts: Vec<i32> = match bytes {
                Ok(b) => match serde_json::from_slice(&b) {
                    Ok(v) => v,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, format!("invalid parts JSON: {e}"))
                            .into_response();
                    }
                },
                Err(e) => {
                    return (StatusCode::BAD_REQUEST, format!("failed to read body: {e}"))
                        .into_response();
                }
            };
            match state.storage.complete_multipart_upload(&bucket, &key, &upload_id, parts).await {
                Ok(_meta) => StatusCode::OK.into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }

        // ── 3. Abort multipart upload ─────────────────────────────────────────
        Some(action) if action == "abort" => {
            let Some(upload_id) = params.upload_id else {
                return (StatusCode::BAD_REQUEST, "missing uploadId").into_response();
            };
            match state.storage.abort_multipart_upload(&bucket, &upload_id).await {
                Ok(()) => StatusCode::OK.into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }

        // ── 4. Multipart part upload ──────────────────────────────────────────
        _ if params.upload_id.is_some() && params.part_number.is_some() => {
            let upload_id = params.upload_id.unwrap();
            let part_number = params.part_number.unwrap();

            // Adapt axum Body data stream → io::Result<Bytes> (mirror of body_to_stream in s3_impl.rs).
            // Raw bytes — NOT axum::extract::Multipart (Pitfall 6).
            let stream = body.into_data_stream().map(|r| {
                r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            });

            match state.storage.upload_part(&bucket, &upload_id, part_number, stream).await {
                Ok(etag) => (StatusCode::OK, [(header::ETAG, etag)]).into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }

        // ── 5. Small-file single PUT ──────────────────────────────────────────
        _ => {
            // Extract Content-Type from request headers (passed through the State-less body param).
            // For the small-file path the browser sets it automatically on XHR.
            // We don't have access to headers here; pass None and FsStorage will use
            // application/octet-stream as the fallback (same as S3 adapter).
            let content_type: Option<String> = None;

            // Adapt axum Body → io::Result<Bytes> stream.
            let stream = body.into_data_stream().map(|r| {
                r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            });

            match state.storage.put_object(&bucket, &key, stream, content_type).await {
                Ok(_meta) => StatusCode::OK.into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }
    }
}
