//! Presigned URL generation for ferrobucket (SigV4 / AWS4-HMAC-SHA256).
//!
//! This module lives in `crates/storage` (not `crates/server`) so that BOTH
//! `crates/server` (presign CLI subcommand + Phase-3 S3 compatibility) and
//! `crates/ui` (server fn presign_fn, Plan 02) can import it without creating
//! a crate cycle (`crates/ui` → `crates/server` would be cyclic).
//!
//! The module contains no `s3s` types (satisfying DEC-storage-decoupled) — it
//! is pure SigV4 math using `hmac`, `sha2`, and `hex`.
//!
//! `crates/server` re-exports `presign_url` from here so existing callers
//! (the `presign` CLI subcommand, tests) remain unaffected.
//!
//! Security notes (T-03-05, T-03-06, T-03-07, T-04-04):
//! - The signing key is NEVER written into the output URL — only the derived
//!   HMAC signature is.
//! - `X-Amz-Expires` is included in the signed string-to-sign, making
//!   expiry unforgeable.
//! - `signed_headers = "host"` ONLY — never content-type (Pitfall 5).

use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

// ─── SigV4 primitives ────────────────────────────────────────────────────────

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
    let amzdate = format_utc_datetime(secs);
    let datestamp = amzdate[..8].to_owned(); // YYYYMMDD
    (amzdate, datestamp)
}

/// Minimal UTC formatter: returns `YYYYMMDDTHHMMSSZ`.
fn format_utc_datetime(unix_secs: u64) -> String {
    let (y, mo, d) = days_to_ymd(unix_secs / 86400);
    let rem = unix_secs % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{:04}{:02}{:02}T{:02}{:02}{:02}Z", y, mo, d, hh, mm, ss)
}

/// Convert days-since-epoch to (year, month, day), all 1-based.
///
/// Algorithm: https://www.researchgate.net/publication/316558298
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
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

// ─── Percent-encoding ─────────────────────────────────────────────────────────

/// Percent-encode a string for use in AWS SigV4 credential query parameters.
///
/// Encodes `/` as `%2F` (the only reserved character in a credential string
/// like `AKID/20240101/us-east-1/s3/aws4_request`). All other characters are
/// passed through unchanged.
fn percent_encode_credential(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        if ch == '/' {
            out.push_str("%2F");
        } else {
            out.push(ch);
        }
    }
    out
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Generate a query-string presigned URL (AWS SigV4) for any S3 verb.
///
/// # Parameters
/// - `method`: HTTP verb — `"GET"`, `"PUT"`, `"HEAD"`, or `"DELETE"` (D-03).
/// - `host`: host:port of the S3 endpoint, e.g. `"127.0.0.1:9000"`.
/// - `path`: path-style S3 path, e.g. `"/<bucket>/<key>"`.
/// - `expires_secs`: URL validity window in seconds. Default is 900 (D-02).
/// - `access_key`: AWS access key ID.
/// - `signing_key`: AWS signing key (never included in the URL).
/// - `region`: AWS region string, e.g. `"us-east-1"`.
///
/// # Returns
/// A fully-formed presigned URL including all `X-Amz-*` query parameters and
/// the `X-Amz-Signature` appended last (as required by SigV4).
///
/// # Security
/// The signing key drives the HMAC key-derivation chain and is never written
/// into the output URL (T-03-07). `X-Amz-Expires` is part of the signed
/// canonical request, making the expiry unforgeable (T-03-06).
pub fn presign_url(
    method: &str,
    host: &str,
    path: &str,
    expires_secs: u32,
    access_key: &str,
    signing_key: &str,
    region: &str,
) -> String {
    let (amzdate, datestamp) = amz_timestamps(SystemTime::now());

    // Credential scope and credential string.
    let credential_scope = format!("{datestamp}/{region}/s3/aws4_request");
    let credential = format!("{access_key}/{credential_scope}");

    // Build the five canonical query parameters, sorted alphabetically by key.
    // X-Amz-Signature is NOT included here — it is appended after signing.
    let canonical_qs = build_canonical_qs(&credential, &amzdate, expires_secs);

    // canonical request: method \n path \n canonical_qs \n "host:<host>\n" \n "host" \n UNSIGNED-PAYLOAD
    // signed_headers = "host" ONLY (Pitfall 5 — never include content-type in presigned URLs).
    let canonical_headers = format!("host:{host}\n");
    let signed_headers = "host";
    let payload_hash = "UNSIGNED-PAYLOAD"; // standard for presigned S3 URLs

    let canonical_request = format!(
        "{method}\n{path}\n{canonical_qs}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    // String to sign.
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amzdate}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    // Signing key chain: AWS4{signing_key} → datestamp → region → "s3" → "aws4_request".
    let k_date = hmac_sha256(format!("AWS4{signing_key}").as_bytes(), datestamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    format!("http://{host}{path}?{canonical_qs}&X-Amz-Signature={signature}")
}

/// Build the five canonical query parameters sorted alphabetically, joined as
/// `key=value&...` (without the trailing `X-Amz-Signature`).
fn build_canonical_qs(credential: &str, amzdate: &str, expires_secs: u32) -> String {
    // Credential contains `/` which must be percent-encoded in the query string.
    let encoded_credential = percent_encode_credential(credential);

    // The five params that s3s v4_check_presigned_url expects, sorted by key.
    let mut params = vec![
        ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_owned()),
        ("X-Amz-Credential", encoded_credential),
        ("X-Amz-Date", amzdate.to_owned()),
        ("X-Amz-Expires", expires_secs.to_string()),
        ("X-Amz-SignedHeaders", "host".to_owned()),
    ];
    params.sort_by(|a, b| a.0.cmp(b.0));

    params
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify structural properties of the generated presigned URL.
    ///
    /// A fixed SystemTime is hard to inject without changing the public API, so
    /// this test validates the stable structural properties: prefix, required
    /// query params, and signature format.
    #[test]
    fn test_presign_url_structure() {
        let url = presign_url(
            "GET",
            "127.0.0.1:9000",
            "/b/k",
            900,
            "dev",
            "devsigning",
            "us-east-1",
        );

        // URL must start with http://{host}{path}?
        assert!(
            url.starts_with("http://127.0.0.1:9000/b/k?"),
            "URL must start with http://127.0.0.1:9000/b/k?, got: {url}"
        );

        // Must contain the algorithm.
        assert!(
            url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"),
            "Missing X-Amz-Algorithm: {url}"
        );

        // Must sign only the host header (Pitfall 5).
        assert!(
            url.contains("X-Amz-SignedHeaders=host"),
            "Missing X-Amz-SignedHeaders=host: {url}"
        );

        // Must contain the default expiry (900s, D-02).
        assert!(
            url.contains("X-Amz-Expires=900"),
            "Missing X-Amz-Expires=900: {url}"
        );

        // Must end with X-Amz-Signature= followed by 64 hex characters.
        let sig_prefix = "X-Amz-Signature=";
        let sig_pos = url
            .rfind(sig_prefix)
            .expect("X-Amz-Signature not found in URL");
        let sig_value = &url[sig_pos + sig_prefix.len()..];
        assert_eq!(
            sig_value.len(),
            64,
            "Signature must be 64 hex chars, got {} chars: {}",
            sig_value.len(),
            sig_value
        );
        assert!(
            sig_value.chars().all(|c| c.is_ascii_hexdigit()),
            "Signature must be hex: {sig_value}"
        );
    }

    #[test]
    fn test_percent_encode_credential() {
        let cred = "AKID/20240101/us-east-1/s3/aws4_request";
        let encoded = percent_encode_credential(cred);
        assert_eq!(encoded, "AKID%2F20240101%2Fus-east-1%2Fs3%2Faws4_request");
    }

    #[test]
    fn test_format_utc_datetime_known_value() {
        // 2024-01-15 11:54:56 UTC = 1705319696 Unix seconds.
        let result = format_utc_datetime(1705319696);
        assert_eq!(result, "20240115T115456Z");
    }
}
