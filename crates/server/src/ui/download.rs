//! Streaming download handler for `/ui/download/{bucket}/{*key}`.
//!
//! Reads from `FsStorage.get_object` and streams bytes to the browser with
//! `Content-Disposition: attachment`. Never buffers the object in memory
//! (RESEARCH Pitfall 6 / T-04-09 mitigate).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use ferrobucket_storage::Storage;
use futures::StreamExt;

use crate::ui::AppState;

/// GET /ui/download/{bucket}/{*key}
///
/// Streams the object bytes from `FsStorage.get_object` to the browser.
/// Sets `Content-Disposition: attachment; filename="{basename}"` so the browser
/// triggers a download instead of attempting inline rendering.
///
/// Returns 404 if the object does not exist.
/// NEVER buffers the entire stream into memory (T-04-09 mitigate, low-RAM differentiator).
pub async fn download_handler(
    Path((bucket, key)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    match state.storage.get_object(&bucket, &key, None).await {
        Ok((meta, stream)) => {
            // Derive the filename from the last path segment.
            let filename = key.split('/').next_back().unwrap_or(&key);
            let disposition = format!("attachment; filename=\"{filename}\"");

            // Adapt io::Result<Bytes> → Stream<Item = Result<Bytes, String>>
            // for axum Body::from_stream (RESEARCH Pattern 6 / Patterns.md Streaming Body Bridge).
            // Never collects the stream into a Vec (T-04-09, grep gate).
            let adapted = stream.map(|r| r.map_err(|e| e.to_string()));
            let body = Body::from_stream(adapted);

            let content_type: String = meta.content_type;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                content_type
                    .parse()
                    .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                disposition.parse().unwrap(),
            );

            (headers, body).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
