//! Integration smoke test for the UI upload/download handlers (Plan 03, Task 2).
//!
//! Tests the handlers by constructing a minimal axum Router over an in-process
//! FsStorage on a temp directory, then issuing requests via `axum_test` style
//! (using axum::body and hyper directly via `tower::ServiceExt`).
//!
//! Verified behaviors:
//! - Small file round-trip: PUT body → GET returns identical bytes
//! - Multipart round-trip: create + 2 parts + complete → assembled bytes match concatenation
//! - Abort cleanup: after abort, staging dir for uploadId is removed

use std::sync::Arc;
use std::path::PathBuf;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use ferrobucket_storage::FsStorage;
use ferrobucket_server::ui::{download_handler, upload_handler};
use ferrobucket_ui::server_fns::state::AppState;
use leptos::config::LeptosOptions;
use tower::util::ServiceExt;

/// Construct a minimal AppState over a temp FsStorage for testing.
fn make_test_state(data_root: PathBuf) -> AppState {
    AppState {
        storage: Arc::new(FsStorage::new(&data_root)),
        leptos_options: LeptosOptions::builder().output_name("ferrobucket-ui").build(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
        region: "us-east-1".to_owned(),
        access_key_id: Some("test-access-key".to_owned()),
        secret_key: "test-secret-key".to_owned(),
        data_root,
        anonymous: true,
    }
}

/// Build a minimal axum Router with just the upload and download handlers.
fn make_router(state: AppState) -> Router {
    Router::new()
        .route("/ui/upload/{bucket}/{*key}", post(upload_handler))
        .route("/ui/download/{bucket}/{*key}", get(download_handler))
        .with_state(state)
}

/// Helper to send a request and collect the response body.
async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Bytes) {
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, body)
}

// ── Test 1: Small file round-trip ─────────────────────────────────────────────

#[tokio::test]
async fn small_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();

    // Create the bucket first.
    let storage = FsStorage::new(dir.path());
    use ferrobucket_storage::Storage;
    storage.create_bucket("testbucket").await.unwrap();

    let state = make_test_state(dir.path().to_path_buf());
    let router = make_router(state);

    let payload = b"hello from small file upload test";

    // POST /ui/upload/testbucket/small.txt (no query params → small-file PUT)
    let upload_req = Request::builder()
        .method("POST")
        .uri("/ui/upload/testbucket/small.txt")
        .header("content-type", "text/plain")
        .body(Body::from(payload.as_ref()))
        .unwrap();

    let (status, _) = send(&router, upload_req).await;
    assert_eq!(status, StatusCode::OK, "small file upload should return 200");

    // GET /ui/download/testbucket/small.txt
    let download_req = Request::builder()
        .method("GET")
        .uri("/ui/download/testbucket/small.txt")
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(&router, download_req).await;
    assert_eq!(status, StatusCode::OK, "download should return 200");
    assert_eq!(body.as_ref(), payload.as_ref(), "downloaded bytes must match uploaded bytes");
}

// ── Test 2: Multipart round-trip ──────────────────────────────────────────────

#[tokio::test]
async fn multipart_roundtrip() {
    let dir = tempfile::tempdir().unwrap();

    let storage = FsStorage::new(dir.path());
    use ferrobucket_storage::Storage;
    storage.create_bucket("testbucket").await.unwrap();

    let state = make_test_state(dir.path().to_path_buf());
    let router = make_router(state);

    // Step 1: create multipart upload
    let create_req = Request::builder()
        .method("POST")
        .uri("/ui/upload/testbucket/multi.bin?action=create")
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(&router, create_req).await;
    assert_eq!(status, StatusCode::OK, "create should return 200");
    let upload_id = String::from_utf8(body.to_vec()).unwrap();
    assert!(!upload_id.is_empty(), "uploadId must not be empty");

    // Step 2: upload part 1
    let part1_data = b"PART_ONE_DATA_____";
    let part1_req = Request::builder()
        .method("POST")
        .uri(format!("/ui/upload/testbucket/multi.bin?uploadId={upload_id}&partNumber=1"))
        .body(Body::from(part1_data.as_ref()))
        .unwrap();
    let (status, _) = send(&router, part1_req).await;
    assert_eq!(status, StatusCode::OK, "part 1 upload should return 200");

    // Step 3: upload part 2
    let part2_data = b"PART_TWO_DATA_____";
    let part2_req = Request::builder()
        .method("POST")
        .uri(format!("/ui/upload/testbucket/multi.bin?uploadId={upload_id}&partNumber=2"))
        .body(Body::from(part2_data.as_ref()))
        .unwrap();
    let (status, _) = send(&router, part2_req).await;
    assert_eq!(status, StatusCode::OK, "part 2 upload should return 200");

    // Step 4: complete
    let parts_json = serde_json::to_vec(&vec![1i32, 2i32]).unwrap();
    let complete_req = Request::builder()
        .method("POST")
        .uri(format!("/ui/upload/testbucket/multi.bin?uploadId={upload_id}&action=complete"))
        .header("content-type", "application/json")
        .body(Body::from(parts_json))
        .unwrap();
    let (status, _) = send(&router, complete_req).await;
    assert_eq!(status, StatusCode::OK, "complete should return 200");

    // Step 5: download and verify bytes equal part1 + part2
    let download_req = Request::builder()
        .method("GET")
        .uri("/ui/download/testbucket/multi.bin")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&router, download_req).await;
    assert_eq!(status, StatusCode::OK, "download after complete should return 200");

    let mut expected = Vec::new();
    expected.extend_from_slice(part1_data);
    expected.extend_from_slice(part2_data);
    assert_eq!(body.as_ref(), expected.as_slice(), "multipart assembled bytes must equal part1 + part2");
}

// ── Test 3: Abort cleans up staging ──────────────────────────────────────────

#[tokio::test]
async fn abort_cleans_staging() {
    let dir = tempfile::tempdir().unwrap();

    let storage = FsStorage::new(dir.path());
    use ferrobucket_storage::Storage;
    storage.create_bucket("testbucket").await.unwrap();

    let state = make_test_state(dir.path().to_path_buf());
    let router = make_router(state);

    // Create multipart upload
    let create_req = Request::builder()
        .method("POST")
        .uri("/ui/upload/testbucket/will-abort.bin?action=create")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&router, create_req).await;
    assert_eq!(status, StatusCode::OK);
    let upload_id = String::from_utf8(body.to_vec()).unwrap();

    // Upload one part
    let part_req = Request::builder()
        .method("POST")
        .uri(format!("/ui/upload/testbucket/will-abort.bin?uploadId={upload_id}&partNumber=1"))
        .body(Body::from(b"some data".as_ref()))
        .unwrap();
    let (status, _) = send(&router, part_req).await;
    assert_eq!(status, StatusCode::OK);

    // The staging directory should exist now
    let staging_dir = dir.path().join(".uploads").join(&upload_id);
    assert!(staging_dir.exists(), "staging dir should exist after uploading a part");

    // Abort
    let abort_req = Request::builder()
        .method("POST")
        .uri(format!("/ui/upload/testbucket/will-abort.bin?uploadId={upload_id}&action=abort"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&router, abort_req).await;
    assert_eq!(status, StatusCode::OK, "abort should return 200");

    // Staging directory must be gone (no orphaned parts)
    assert!(!staging_dir.exists(), "staging dir must be removed after abort (no orphan parts)");
}
