//! Integration tests for ferrobucket-server.
//!
//! Strategy (deviation from plan — aws-sdk-s3 incompatible with s3s hmac pin):
//!   - In-process tests: a minimal reqwest-based client with hand-rolled AWS SigV4
//!     header auth (`Authorization: AWS4-HMAC-SHA256 ...`), exercising the real s3s
//!     wire protocol over plain HTTP to 127.0.0.1:<port>.
//!   - Real aws CLI test: `#[ignore]` gated; verifies mb/cp/ls/rm/rb path-style.
//!
//! All crypto uses crates already in the locked tree:
//!   hmac =0.13.0-rc.5, sha2 =0.11.0-rc.5, hex =0.4.3  (no new conflicts).

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use ferrobucket_server::FerrobucketS3;
use ferrobucket_storage::FsStorage;
use hex;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, Method, Response};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::task::JoinHandle;

// ─── SigV4 constants ──────────────────────────────────────────────────────────

const ACCESS_KEY: &str = "dev";
const SECRET_KEY: &str = "devsecret";
const REGION: &str = "us-east-1";
const SERVICE: &str = "s3";

// ─── Server harness ───────────────────────────────────────────────────────────

/// Spawn ferrobucket in-process on an OS-assigned port.
///
/// Returns:
///  - `JoinHandle` (to abort the server after the test)
///  - `SocketAddr` (the bound address)
///  - `TempDir`    (must stay alive to keep the data directory; drop AFTER the test)
///
/// When `anonymous = true` no `SimpleAuth` is installed — unsigned requests succeed.
/// When `anonymous = false` `SimpleAuth::from_single("dev", "devsecret")` is installed.
async fn start_server(anonymous: bool) -> (JoinHandle<()>, SocketAddr, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FsStorage::new(dir.path());
    let adapter = FerrobucketS3::new(storage);

    let mut builder = s3s::service::S3ServiceBuilder::new(adapter);
    // D-07: conditional auth — mirror main.rs exactly.
    if !anonymous {
        builder.set_auth(s3s::auth::SimpleAuth::from_single(
            ACCESS_KEY.to_owned(),
            SECRET_KEY.to_owned(),
        ));
    }
    let s3_service = builder.build();

    async fn handle_s3_error(err: s3s::HttpError) -> axum::response::Response {
        use axum::response::IntoResponse;
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("{err:?}"),
        )
            .into_response()
    }

    let s3_wrapped =
        axum::error_handling::HandleError::new(s3_service, handle_s3_error);
    let app = axum::Router::new().fallback_service(s3_wrapped);

    // Bind :0 — OS assigns a free port; avoids collisions in parallel test runs.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    (handle, addr, dir)
}

// ─── AWS SigV4 signing ───────────────────────────────────────────────────────

/// Hex-encode a SHA-256 hash.
fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// HMAC-SHA256.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Format `SystemTime` as `YYYYMMDDTHHMMSSZ` (amzdate) and `YYYYMMDD` (datestamp).
fn amz_timestamps(t: SystemTime) -> (String, String) {
    let secs = t.duration_since(UNIX_EPOCH).unwrap().as_secs();
    // Decompose into calendar components (no external date lib needed).
    let amzdate = format_utc_datetime(secs);
    let datestamp = amzdate[..8].to_owned(); // YYYYMMDD
    (amzdate, datestamp)
}

/// Minimal UTC formatter: returns `YYYYMMDDTHHMMSSZ`.
fn format_utc_datetime(unix_secs: u64) -> String {
    // Days since epoch → calendar date using Gregorian proleptic calendar.
    let (y, mo, d) = days_to_ymd(unix_secs / 86400);
    let rem = unix_secs % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        y, mo, d, hh, mm, ss
    )
}

/// Convert days-since-epoch to (year, month, day), all 1-based.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm: https://www.researchgate.net/publication/316558298
    // (Julian day number approach)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Build a signed `reqwest::Request` using AWS SigV4 header authentication.
///
/// Parameters:
///   - `method`: HTTP verb
///   - `url`: full URL including path and any query string
///   - `bucket`: S3 bucket (used only for path construction — already in `url`)
///   - `body`: request body bytes
///   - `content_type`: value for Content-Type header (empty string to omit)
///   - `extra_headers`: additional headers to include and sign (key, value pairs)
///   - `access_key` / `secret_key`: AWS credentials
///
/// Signed headers: `content-type` (when non-empty), `host`, `x-amz-content-sha256`,
/// `x-amz-date`. All lowercase, sorted alphabetically (canonical form).
#[allow(clippy::too_many_arguments)]
async fn signed_request(
    client: &Client,
    method: Method,
    url: &str,
    body: Vec<u8>,
    content_type: &str,
    extra_headers: &[(&str, &str)],
    access_key: &str,
    secret_key: &str,
) -> reqwest::RequestBuilder {
    let parsed = reqwest::Url::parse(url).expect("url");
    let host = format!(
        "{}:{}",
        parsed.host_str().unwrap_or("127.0.0.1"),
        parsed.port().unwrap_or(80)
    );
    let path = parsed.path();
    let query = parsed.query().unwrap_or("");

    let now = SystemTime::now();
    let (amzdate, datestamp) = amz_timestamps(now);
    let payload_hash = sha256_hex(&body);

    // Build sorted signed-header set.
    // Always sign: host, x-amz-content-sha256, x-amz-date.
    // Optionally sign: content-type (when non-empty).
    let mut header_pairs: Vec<(String, String)> = vec![
        ("host".to_owned(), host.clone()),
        ("x-amz-content-sha256".to_owned(), payload_hash.clone()),
        ("x-amz-date".to_owned(), amzdate.clone()),
    ];
    if !content_type.is_empty() {
        header_pairs.push(("content-type".to_owned(), content_type.to_owned()));
    }
    for (k, v) in extra_headers {
        header_pairs.push((k.to_lowercase(), v.to_string()));
    }
    header_pairs.sort_by(|a, b| a.0.cmp(&b.0));
    // Deduplicate in case caller accidentally provided a duplicate.
    header_pairs.dedup_by(|a, b| a.0 == b.0);

    let signed_headers: String = header_pairs
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers: String = header_pairs
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();

    // Canonical URI: URL-encode each path segment (but keep `/`).
    let canonical_uri = if path.is_empty() { "/" } else { path };

    // Canonical query string: sort by name.
    let canonical_querystring = {
        let mut qs_pairs: Vec<(&str, &str)> = query
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|kv| {
                let mut it = kv.splitn(2, '=');
                let k = it.next().unwrap_or("");
                let v = it.next().unwrap_or("");
                (k, v)
            })
            .collect();
        qs_pairs.sort();
        qs_pairs
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    };

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        canonical_uri,
        canonical_querystring,
        canonical_headers,
        signed_headers,
        payload_hash
    );

    let credential_scope =
        format!("{}/{}/s3/aws4_request", datestamp, REGION);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amzdate,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );

    // Derive the signing key: HMAC chain.
    let k_date = hmac_sha256(
        format!("AWS4{}", secret_key).as_bytes(),
        datestamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, REGION.as_bytes());
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        access_key, credential_scope, signed_headers, signature
    );

    let mut req = client.request(method, url).body(body);
    req = req.header("host", &host);
    req = req.header("x-amz-date", &amzdate);
    req = req.header("x-amz-content-sha256", &payload_hash);
    req = req.header("authorization", &authorization);
    if !content_type.is_empty() {
        req = req.header("content-type", content_type);
    }
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    req
}

/// Convenience: send a signed PUT for a small object.
async fn put_object(
    client: &Client,
    addr: SocketAddr,
    bucket: &str,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Response {
    let url = format!("http://{}/{}/{}", addr, bucket, key);
    signed_request(
        client,
        Method::PUT,
        &url,
        body,
        content_type,
        &[],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("PUT send")
}

/// Convenience: send a signed GET.
async fn get_object(
    client: &Client,
    addr: SocketAddr,
    bucket: &str,
    key: &str,
    range: Option<&str>,
) -> Response {
    let url = format!("http://{}/{}/{}", addr, bucket, key);
    let extra: Vec<(&str, &str)> = if let Some(r) = range {
        vec![("range", r)]
    } else {
        vec![]
    };
    signed_request(
        client,
        Method::GET,
        &url,
        vec![],
        "",
        &extra,
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("GET send")
}

/// Convenience: send a signed HEAD.
async fn head_object(
    client: &Client,
    addr: SocketAddr,
    bucket: &str,
    key: &str,
) -> Response {
    let url = format!("http://{}/{}/{}", addr, bucket, key);
    signed_request(
        client,
        Method::HEAD,
        &url,
        vec![],
        "",
        &[],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("HEAD send")
}

/// Create a bucket via signed PUT (path-style: PUT /<bucket>).
async fn create_bucket(client: &Client, addr: SocketAddr, bucket: &str) -> Response {
    let url = format!("http://{}/{}", addr, bucket);
    signed_request(
        client,
        Method::PUT,
        &url,
        vec![],
        "",
        &[],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("CreateBucket send")
}

/// DeleteObjects via signed POST with an XML payload.
async fn delete_objects(
    client: &Client,
    addr: SocketAddr,
    bucket: &str,
    keys: &[&str],
) -> Response {
    let objects_xml: String = keys
        .iter()
        .map(|k| format!("<Object><Key>{}</Key></Object>", k))
        .collect::<Vec<_>>()
        .join("");
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Delete>{}</Delete>",
        objects_xml
    )
    .into_bytes();
    // POST /<bucket>?delete — path-style DeleteObjects.
    let url = format!("http://{}/{}?delete", addr, bucket);

    let body_len = body.len().to_string();

    signed_request(
        client,
        Method::POST,
        &url,
        body,
        "application/xml",
        &[("content-length", &body_len)],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("DeleteObjects send")
}

/// ListObjectsV2 via signed GET with ?list-type=2.
async fn list_objects_v2(
    client: &Client,
    addr: SocketAddr,
    bucket: &str,
) -> Response {
    // GET /<bucket>?list-type=2 — path-style ListObjectsV2.
    let url = format!("http://{}/{}?list-type=2", addr, bucket);
    signed_request(
        client,
        Method::GET,
        &url,
        vec![],
        "",
        &[],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("ListObjectsV2 send")
}

// ─── In-process tests — credentialed harness ─────────────────────────────────

/// Round-trip: put a small object, get it back, verify body bytes and ETag (DEC-etag: MD5).
#[tokio::test]
async fn test_put_get_object() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    // Create bucket.
    let resp = create_bucket(&client, addr, "test-bucket").await;
    assert!(
        resp.status().is_success(),
        "create_bucket failed: {}",
        resp.status()
    );

    // PUT object.
    let payload: Vec<u8> = b"hello ferrobucket".to_vec();
    let resp = put_object(
        &client,
        addr,
        "test-bucket",
        "hello.txt",
        payload.clone(),
        "text/plain",
    )
    .await;
    assert_eq!(
        resp.status(),
        200,
        "put_object failed: {}",
        resp.status()
    );

    // GET object.
    let resp = get_object(&client, addr, "test-bucket", "hello.txt", None).await;
    assert_eq!(
        resp.status(),
        200,
        "get_object failed: {}",
        resp.status()
    );

    // Verify body.
    let body_bytes = resp.bytes().await.expect("body bytes");
    assert_eq!(
        body_bytes.as_ref(),
        payload.as_slice(),
        "get_object body mismatch"
    );

    handle.abort();
}

/// HeadObject: verify content_length, content_type, etag, last_modified are all present.
#[tokio::test]
async fn test_head_object_metadata() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    create_bucket(&client, addr, "meta-bucket").await;

    let payload: Vec<u8> = b"metadata test payload".to_vec();
    let resp = put_object(
        &client,
        addr,
        "meta-bucket",
        "obj.bin",
        payload.clone(),
        "application/octet-stream",
    )
    .await;
    assert_eq!(resp.status(), 200, "put failed: {}", resp.status());

    let resp = head_object(&client, addr, "meta-bucket", "obj.bin").await;
    assert_eq!(resp.status(), 200, "head failed: {}", resp.status());

    // content-length must equal payload size.
    let cl = resp
        .headers()
        .get("content-length")
        .expect("content-length header missing")
        .to_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert_eq!(cl, payload.len(), "content-length mismatch");

    // content-type must match what we PUT.
    let ct = resp
        .headers()
        .get("content-type")
        .expect("content-type header missing")
        .to_str()
        .unwrap();
    assert!(
        ct.contains("application/octet-stream"),
        "content-type wrong: {}",
        ct
    );

    // etag must be present.
    let etag = resp
        .headers()
        .get("etag")
        .expect("etag header missing")
        .to_str()
        .unwrap();
    assert!(!etag.is_empty(), "etag empty");

    // last-modified must be present.
    let lm = resp
        .headers()
        .get("last-modified")
        .expect("last-modified header missing")
        .to_str()
        .unwrap();
    assert!(!lm.is_empty(), "last-modified empty");

    // HEAD body must be empty.
    let body_bytes = resp.bytes().await.expect("bytes");
    assert!(body_bytes.is_empty(), "HEAD body must be empty");

    handle.abort();
}

/// Ranged GET → 206 Partial Content; unsatisfiable range → 416.
#[tokio::test]
async fn test_ranged_get() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    create_bucket(&client, addr, "range-bucket").await;

    // PUT a 1000-byte object.
    let payload: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
    put_object(
        &client,
        addr,
        "range-bucket",
        "bigobj",
        payload.clone(),
        "application/octet-stream",
    )
    .await;

    // GET bytes=0-499 → expect 206, 500 bytes, Content-Range header.
    let resp = get_object(
        &client,
        addr,
        "range-bucket",
        "bigobj",
        Some("bytes=0-499"),
    )
    .await;
    assert_eq!(
        resp.status(),
        206,
        "expected 206 Partial Content, got {}",
        resp.status()
    );

    let content_range = resp
        .headers()
        .get("content-range")
        .expect("content-range header missing")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        content_range, "bytes 0-499/1000",
        "content-range wrong: {}",
        content_range
    );

    let body_bytes = resp.bytes().await.expect("range body");
    assert_eq!(body_bytes.len(), 500, "expected 500 bytes for range 0-499");
    assert_eq!(
        body_bytes.as_ref(),
        &payload[..500],
        "range body content mismatch"
    );

    // Unsatisfiable range → 416.
    let resp416 = get_object(
        &client,
        addr,
        "range-bucket",
        "bigobj",
        Some("bytes=99999-"),
    )
    .await;
    assert_eq!(
        resp416.status(),
        416,
        "expected 416 for unsatisfiable range, got {}",
        resp416.status()
    );

    handle.abort();
}

/// DeleteObjects: two real keys + one never-created; all three appear in Deleted[].
/// Then ListObjectsV2 shows bucket empty of those keys (D-05 idempotency).
#[tokio::test]
async fn test_delete_objects_idempotent() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    create_bucket(&client, addr, "del-bucket").await;

    // PUT two objects.
    put_object(
        &client,
        addr,
        "del-bucket",
        "key-a",
        b"aaa".to_vec(),
        "text/plain",
    )
    .await;
    put_object(
        &client,
        addr,
        "del-bucket",
        "key-b",
        b"bbb".to_vec(),
        "text/plain",
    )
    .await;

    // DeleteObjects: two real + one non-existent.
    let resp = delete_objects(
        &client,
        addr,
        "del-bucket",
        &["key-a", "key-b", "key-never-existed"],
    )
    .await;
    assert!(
        resp.status().is_success(),
        "DeleteObjects failed: {}",
        resp.status()
    );

    let body = resp.text().await.expect("body");
    // All three keys should appear in <Deleted> (NoSuchKey treated as success).
    assert!(
        body.contains("key-a"),
        "key-a not in Deleted: {}",
        body
    );
    assert!(
        body.contains("key-b"),
        "key-b not in Deleted: {}",
        body
    );
    assert!(
        body.contains("key-never-existed"),
        "key-never-existed not in Deleted: {}",
        body
    );
    // <Error> block must NOT be present.
    assert!(
        !body.contains("<Error>"),
        "Errors[] not empty: {}",
        body
    );

    // ListObjectsV2 must show bucket empty.
    let list_resp = list_objects_v2(&client, addr, "del-bucket").await;
    let list_body = list_resp.text().await.expect("list body");
    assert!(
        !list_body.contains("key-a") && !list_body.contains("key-b"),
        "bucket not empty after DeleteObjects: {}",
        list_body
    );

    handle.abort();
}

// ─── Auth contract tests ──────────────────────────────────────────────────────

/// Valid SigV4 → 200; wrong secret → 403 (REQ-sigv4-auth, T-02-10).
#[tokio::test]
async fn test_sigv4_auth() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    // Good creds → CreateBucket succeeds (200).
    let url = format!("http://{}/auth-bucket", addr);
    let resp = signed_request(
        &client,
        Method::PUT,
        &url,
        vec![],
        "",
        &[],
        ACCESS_KEY,
        SECRET_KEY,
    )
    .await
    .send()
    .await
    .expect("good creds send");
    assert!(
        resp.status().is_success(),
        "valid creds should succeed, got {}",
        resp.status()
    );

    // Bad secret → 403.
    let url = format!("http://{}/auth-bucket2", addr);
    let resp_bad = signed_request(
        &client,
        Method::PUT,
        &url,
        vec![],
        "",
        &[],
        ACCESS_KEY,
        "wrongsecret",
    )
    .await
    .send()
    .await
    .expect("bad creds send");
    assert_eq!(
        resp_bad.status(),
        403,
        "invalid creds should return 403, got {}",
        resp_bad.status()
    );

    handle.abort();
}

/// Anonymous server (no SimpleAuth) accepts unsigned requests (D-07, T-02-11).
#[tokio::test]
async fn test_anonymous_mode() {
    let (handle, addr, _dir) = start_server(true).await;
    let client = Client::new();

    // Send an unsigned (no Authorization header) PUT for CreateBucket.
    let url = format!("http://{}/anon-bucket", addr);
    let resp = client
        .put(&url)
        .header("x-amz-content-sha256", sha256_hex(&[]))
        .header("x-amz-date", {
            let (d, _) = amz_timestamps(SystemTime::now());
            d
        })
        .send()
        .await
        .expect("unsigned send");
    assert!(
        resp.status().is_success(),
        "anonymous mode should accept unsigned request, got {}",
        resp.status()
    );

    // Confirm the same bucket op would fail on a credentialed server (showing
    // that anonymous mode is opt-in, not the default).
    let (handle2, addr2, _dir2) = start_server(false).await;
    let resp_denied = client
        .put(format!("http://{}/anon-bucket2", addr2))
        .header("x-amz-content-sha256", sha256_hex(&[]))
        .header("x-amz-date", {
            let (d, _) = amz_timestamps(SystemTime::now());
            d
        })
        .send()
        .await
        .expect("unsigned on credentialed server");
    assert_eq!(
        resp_denied.status(),
        403,
        "credentialed server should reject unsigned request"
    );

    handle.abort();
    handle2.abort();
}

/// Region-agnostic: sign with eu-west-1, connect to a us-east-1 server → success (D-06).
#[tokio::test]
async fn test_region_agnostic() {
    let (handle, addr, _dir) = start_server(false).await;
    let client = Client::new();

    // Sign with a different region name.
    let url = format!("http://{}/region-test", addr);
    let body: Vec<u8> = vec![];
    let payload_hash = sha256_hex(&body);

    let now = SystemTime::now();
    let (amzdate, datestamp) = amz_timestamps(now);
    let foreign_region = "eu-west-1";

    let host = format!("127.0.0.1:{}", addr.port());

    let credential_scope =
        format!("{}/{}/s3/aws4_request", datestamp, foreign_region);
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, payload_hash, amzdate
    );
    let canonical_request = format!(
        "PUT\n/region-test\n\n{}\n{}\n{}",
        canonical_headers, signed_headers, payload_hash
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amzdate,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac_sha256(
        format!("AWS4{}", SECRET_KEY).as_bytes(),
        datestamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, foreign_region.as_bytes());
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        ACCESS_KEY, credential_scope, signed_headers, signature
    );

    let resp = client
        .put(&url)
        .header("host", &host)
        .header("x-amz-date", &amzdate)
        .header("x-amz-content-sha256", &payload_hash)
        .header("authorization", &authorization)
        .send()
        .await
        .expect("region-agnostic send");

    assert!(
        resp.status().is_success(),
        "region-agnostic signing should succeed (D-06), got {}",
        resp.status()
    );

    handle.abort();
}

// ─── Real aws CLI subprocess test (gated, #[ignore]) ─────────────────────────

/// Integration test: real `aws` CLI against an in-process server.
///
/// Tests `mb`, `cp` (upload + download), `ls`, `rm`, `rb` using path-style
/// addressing (`--endpoint-url http://127.0.0.1:<port>`).
///
/// Skip-if-absent: if `aws --version` fails, print a skip message and return 0
/// (so `cargo test -p ferrobucket-server` is always green without the CLI).
///
/// To run explicitly:
///   cargo test -p ferrobucket-server -- --ignored test_aws_cli
#[tokio::test]
#[ignore]
async fn test_aws_cli() {
    // Skip-if-absent guard.
    if std::process::Command::new("aws")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("aws CLI not installed; skipping test_aws_cli");
        return;
    }

    let (handle, addr, _dir) = start_server(false).await;

    /// Run an `aws s3` command against the in-process server asynchronously.
    /// Uses `tokio::process::Command` to avoid blocking the tokio executor thread.
    async fn run_aws_cmd(
        args: &[&str],
        addr: SocketAddr,
    ) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
        let mut cmd = tokio::process::Command::new("aws");
        cmd.args(args)
            .arg("--endpoint-url")
            .arg(format!("http://{}", addr))
            .env("AWS_ACCESS_KEY_ID", ACCESS_KEY)
            .env("AWS_SECRET_ACCESS_KEY", SECRET_KEY)
            .env("AWS_REGION", REGION)
            // Suppress the virtual-hosted style warning for custom endpoints.
            .env("AWS_EC2_METADATA_DISABLED", "true");
        let out = cmd.output().await.expect("aws subprocess");
        (out.status, out.stdout, out.stderr)
    }

    // 1. mb — create bucket.
    let (status, _out, err) = run_aws_cmd(&["s3", "mb", "s3://ferro-cli-test"], addr).await;
    assert!(
        status.success(),
        "aws s3 mb failed:\n{}",
        String::from_utf8_lossy(&err)
    );

    // 2. cp — upload a small file.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), b"aws cli test content").expect("write tempfile");

    let src_path = tmp.path().to_str().unwrap().to_owned();
    let (status, _out, err) = run_aws_cmd(
        &["s3", "cp", &src_path, "s3://ferro-cli-test/obj.txt"],
        addr,
    )
    .await;
    assert!(
        status.success(),
        "aws s3 cp (upload) failed:\n{}",
        String::from_utf8_lossy(&err)
    );

    // 3. ls — list bucket, verify obj.txt is listed.
    let (status, out, err) =
        run_aws_cmd(&["s3", "ls", "s3://ferro-cli-test"], addr).await;
    assert!(
        status.success(),
        "aws s3 ls failed:\n{}",
        String::from_utf8_lossy(&err)
    );
    let ls_output = String::from_utf8_lossy(&out);
    assert!(
        ls_output.contains("obj.txt"),
        "obj.txt not found in ls output:\n{}",
        ls_output
    );

    // 4. cp — download and verify round-trip content.
    let down_tmp = tempfile::NamedTempFile::new().expect("download tempfile");
    let down_path = down_tmp.path().to_str().unwrap().to_owned();
    let (status, _out, err) = run_aws_cmd(
        &["s3", "cp", "s3://ferro-cli-test/obj.txt", &down_path],
        addr,
    )
    .await;
    assert!(
        status.success(),
        "aws s3 cp (download) failed:\n{}",
        String::from_utf8_lossy(&err)
    );
    let downloaded = std::fs::read(&down_path).expect("read downloaded");
    assert_eq!(
        downloaded,
        b"aws cli test content",
        "round-trip content mismatch"
    );

    // 5. rm — remove the object.
    let (status, _out, err) =
        run_aws_cmd(&["s3", "rm", "s3://ferro-cli-test/obj.txt"], addr).await;
    assert!(
        status.success(),
        "aws s3 rm failed:\n{}",
        String::from_utf8_lossy(&err)
    );

    // 6. rb — remove the (now-empty) bucket.
    let (status, _out, err) =
        run_aws_cmd(&["s3", "rb", "s3://ferro-cli-test"], addr).await;
    assert!(
        status.success(),
        "aws s3 rb failed:\n{}",
        String::from_utf8_lossy(&err)
    );

    handle.abort();
}
