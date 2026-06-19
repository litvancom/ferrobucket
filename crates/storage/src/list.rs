use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use crate::StorageError;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ListV2Req {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub continuation_token: Option<String>,
    pub max_keys: Option<usize>,
}

#[derive(Debug, Default)]
pub struct ListV2Res {
    pub objects: Vec<crate::meta::ObjectMeta>,
    pub common_prefixes: Vec<String>,
    pub next_continuation_token: Option<String>,
    pub is_truncated: bool,
}

// ─── Continuation-token helpers ───────────────────────────────────────────────

/// Encode the last-returned key as a base64 URL-safe (no-pad) continuation token.
/// The token is opaque to callers; `decode_continuation_token` reverses it.
pub fn encode_continuation_token(last_key: &str) -> String {
    URL_SAFE_NO_PAD.encode(last_key.as_bytes())
}

/// Decode a base64 continuation token back to the last-returned key.
/// Returns `StorageError::InvalidContinuationToken` for any malformed input
/// (bad base64 or non-UTF-8 payload) — never panics (T-01-09).
pub fn decode_continuation_token(token: &str) -> Result<String, StorageError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| StorageError::InvalidContinuationToken)?;
    String::from_utf8(bytes).map_err(|_| StorageError::InvalidContinuationToken)
}

// ─── Listing algorithm ────────────────────────────────────────────────────────

/// List objects in a bucket, implementing the S3 ListObjectsV2 semantics (D-07).
///
/// Algorithm order (sort AFTER decode — Pitfall 3):
///   1. read_dir objects/ → decode each filename back to a key (skip undecoded — T-01-10)
///   2. sort_unstable() — filesystem readdir order ≠ S3 UTF-8 lex order (D-07)
///   3. apply continuation_token: retain keys > decoded token (resume-after)
///   4. apply prefix filter: retain keys that start_with(prefix)
///   5. derive CommonPrefixes + direct objects using delimiter
///   6. cap at max_keys (default 1000); set is_truncated + next_continuation_token
pub async fn list_objects_v2(
    storage: &crate::fs::FsStorage,
    bucket: &str,
    req: ListV2Req,
) -> Result<ListV2Res, StorageError> {
    let objects_dir = storage.objects_dir(bucket);
    let objects_dir_clone = objects_dir.clone();

    // ── Step 1: read_dir + decode (skip undecoded entries — T-01-10) ──────────
    let mut keys: Vec<String> = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        match std::fs::read_dir(&objects_dir_clone) {
            Ok(rd) => {
                for entry in rd.flatten() {
                    let fname = entry.file_name();
                    let fname_str = fname.to_string_lossy();
                    // decode_key: failures are silently skipped (T-01-10)
                    if let Ok(key) = crate::encode::decode_key(&fname_str) {
                        out.push(key);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok(out)
    })
    .await
    .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?
    .map_err(StorageError::Io)?;

    // ── Step 2: sort — MUST be after decode, NEVER trust readdir order (D-07) ─
    keys.sort_unstable();

    // ── Step 3: apply continuation token (resume-after semantics) ────────────
    if let Some(ref token) = req.continuation_token {
        let after = decode_continuation_token(token)?;
        keys.retain(|k| k.as_str() > after.as_str());
    }

    // ── Step 4: apply prefix filter ───────────────────────────────────────────
    if let Some(ref prefix) = req.prefix {
        keys.retain(|k| k.starts_with(prefix.as_str()));
    }

    // ── Step 5: derive CommonPrefixes + direct objects via delimiter ──────────
    let prefix_str = req.prefix.as_deref().unwrap_or("");
    let max_keys = req.max_keys.unwrap_or(1000);

    let mut common_prefixes: Vec<String> = Vec::new();
    // Pairs of (key, is_common_prefix) describing the ordered result before capping.
    // We need to track the last *key* emitted (not common-prefix) for the token.
    // We collect (key, kind) then cap.
    enum Item {
        Object(String),       // the key
        CommonPrefix(String), // the collapsed prefix string
    }

    let mut items: Vec<Item> = Vec::new();
    let mut seen_prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();

    for key in &keys {
        if let Some(ref delimiter) = req.delimiter {
            // Strip the request prefix from the key to find the first delimiter.
            let remainder = &key[prefix_str.len()..];
            if let Some(delim_pos) = remainder.find(delimiter.as_str()) {
                // Collapse into a CommonPrefix: prefix + segment_up_to_and_including_delimiter
                let cp = format!(
                    "{}{}",
                    prefix_str,
                    &remainder[..delim_pos + delimiter.len()]
                );
                if seen_prefixes.insert(cp.clone()) {
                    items.push(Item::CommonPrefix(cp));
                }
                // Keys under a CommonPrefix do NOT appear as direct objects.
                continue;
            }
        }
        // No delimiter match → direct object.
        items.push(Item::Object(key.clone()));
    }

    // ── Step 6: cap at max_keys ───────────────────────────────────────────────
    let is_truncated = items.len() > max_keys;
    let capped: Vec<Item> = items.into_iter().take(max_keys).collect();

    // Build the last key emitted (for next_continuation_token).
    // The token is based on the last *object key* in the capped result;
    // for a page ending on a CommonPrefix we use the last key that contributed to it.
    // S3 spec: token is based on the last key returned (object or last key under CP).
    // Simple implementation: use the last key from the pre-cap `keys` slice that
    // was consumed (i.e., the key at index max_keys - 1 in the original ordered keys).
    //
    // We track which original key index is the last consumed in `capped`.
    let mut last_consumed_key: Option<String> = None;
    {
        // Re-derive: walk keys again up to max_keys items the same way, track last key used.
        let mut count = 0usize;
        let mut seen2: std::collections::HashSet<String> = std::collections::HashSet::new();
        for key in &keys {
            if count >= max_keys {
                break;
            }
            if let Some(ref delimiter) = req.delimiter {
                let remainder = &key[prefix_str.len()..];
                if let Some(delim_pos) = remainder.find(delimiter.as_str()) {
                    let cp = format!(
                        "{}{}",
                        prefix_str,
                        &remainder[..delim_pos + delimiter.len()]
                    );
                    if seen2.insert(cp) {
                        count += 1;
                        last_consumed_key = Some(key.clone());
                    } else {
                        // Still in the same common prefix; update last_consumed_key
                        // so the token covers all keys up to the CP boundary.
                        last_consumed_key = Some(key.clone());
                    }
                    continue;
                }
            }
            count += 1;
            last_consumed_key = Some(key.clone());
        }
    }

    let next_continuation_token = if is_truncated {
        last_consumed_key
            .as_deref()
            .map(encode_continuation_token)
    } else {
        None
    };

    // ── Build output ──────────────────────────────────────────────────────────
    let meta_dir = storage.meta_dir(bucket);
    let mut objects: Vec<crate::meta::ObjectMeta> = Vec::new();
    common_prefixes.clear();

    for item in capped {
        match item {
            Item::Object(key) => {
                let encoded = crate::encode::encode_key(&key)?;
                let sidecar_path = meta_dir.join(format!("{}.json", encoded));
                let meta = crate::meta::read_sidecar(&sidecar_path).await?;
                objects.push(meta);
            }
            Item::CommonPrefix(cp) => {
                common_prefixes.push(cp);
            }
        }
    }

    // CommonPrefixes must be sorted (S3 returns them sorted).
    common_prefixes.sort_unstable();

    Ok(ListV2Res {
        objects,
        common_prefixes,
        next_continuation_token,
        is_truncated,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use tempfile::tempdir;
    use crate::fs::FsStorage;
    use crate::Storage;

    fn body(data: &'static [u8]) -> impl futures::Stream<Item = std::io::Result<Bytes>> + Send {
        stream::iter(vec![Ok(Bytes::from_static(data))])
    }

    const BUCKET: &str = "test-bkt";

    async fn make_storage() -> (tempfile::TempDir, FsStorage) {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        storage.create_bucket(BUCKET).await.unwrap();
        (dir, storage)
    }

    async fn put(storage: &FsStorage, key: &str) {
        storage
            .put_object(BUCKET, key, body(b"x"), None)
            .await
            .unwrap();
    }

    // ── Continuation-token unit tests ─────────────────────────────────────────

    #[test]
    fn continuation_token_roundtrip() {
        // ASCII keys — round-trip must hold
        for k in &["", "simple", "a/b/c", "with spaces", "100%"] {
            let decoded = decode_continuation_token(&encode_continuation_token(k))
                .unwrap_or_else(|_| panic!("round-trip failed for {:?}", k));
            assert_eq!(decoded, *k, "round-trip value mismatch for {:?}", k);
        }
        // Unicode key
        let unicode = "photos/2024/café/résumé.jpg";
        let decoded = decode_continuation_token(&encode_continuation_token(unicode)).unwrap();
        assert_eq!(decoded, unicode);
        // Malformed token returns InvalidContinuationToken, never panics
        assert!(
            matches!(
                decode_continuation_token("!!!not-base64"),
                Err(StorageError::InvalidContinuationToken)
            ),
            "expected InvalidContinuationToken for malformed token"
        );
    }

    // ── Listing behaviour tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn list_prefix_filter() {
        let (_dir, storage) = make_storage().await;
        for k in &["a", "ab", "b", "ba", "c"] {
            put(&storage, k).await;
        }

        let res = list_objects_v2(
            &storage,
            BUCKET,
            ListV2Req {
                prefix: Some("a".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let keys: Vec<&str> = res.objects.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["a", "ab"]);
        assert!(res.common_prefixes.is_empty());
        assert!(!res.is_truncated);
    }

    #[tokio::test]
    async fn list_delimiter_common_prefixes() {
        let (_dir, storage) = make_storage().await;
        for k in &["a/b", "a/c", "b/d"] {
            put(&storage, k).await;
        }

        let res = list_objects_v2(
            &storage,
            BUCKET,
            ListV2Req {
                delimiter: Some("/".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(res.objects.is_empty(), "no direct objects at root with /");
        let mut cps = res.common_prefixes.clone();
        cps.sort_unstable();
        assert_eq!(cps, vec!["a/", "b/"]);
        assert!(!res.is_truncated);
    }

    #[tokio::test]
    async fn list_sort_order() {
        let (_dir, storage) = make_storage().await;
        // Insert in non-sorted order
        for k in &["c", "a", "z", "b", "m"] {
            put(&storage, k).await;
        }

        let res = list_objects_v2(&storage, BUCKET, ListV2Req::default())
            .await
            .unwrap();

        let keys: Vec<&str> = res.objects.iter().map(|o| o.key.as_str()).collect();
        let mut expected = vec!["a", "b", "c", "m", "z"];
        expected.sort_unstable();
        assert_eq!(keys, expected, "keys must be UTF-8 lexicographic, not readdir order");
    }

    #[tokio::test]
    async fn list_pagination_no_gaps() {
        let (_dir, storage) = make_storage().await;
        let all_keys: Vec<String> = (0..10).map(|i| format!("key-{:02}", i)).collect();
        for k in &all_keys {
            put(&storage, k).await;
        }

        let mut seen: Vec<String> = Vec::new();
        let mut token: Option<String> = None;

        loop {
            let res = list_objects_v2(
                &storage,
                BUCKET,
                ListV2Req {
                    max_keys: Some(3),
                    continuation_token: token.clone(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

            for obj in &res.objects {
                seen.push(obj.key.clone());
            }

            if res.is_truncated {
                token = res.next_continuation_token;
                assert!(token.is_some(), "is_truncated but no next token");
            } else {
                break;
            }
        }

        let mut expected = all_keys.clone();
        expected.sort_unstable();
        assert_eq!(seen, expected, "pagination must cover every key exactly once");
    }

    #[tokio::test]
    async fn list_prefix_and_delimiter_combined() {
        let (_dir, storage) = make_storage().await;
        for k in &["photos/2024/a", "photos/2024/b", "photos/2023/c"] {
            put(&storage, k).await;
        }

        let res = list_objects_v2(
            &storage,
            BUCKET,
            ListV2Req {
                prefix: Some("photos/".to_owned()),
                delimiter: Some("/".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(res.objects.is_empty());
        let mut cps = res.common_prefixes.clone();
        cps.sort_unstable();
        assert_eq!(cps, vec!["photos/2023/", "photos/2024/"]);
    }
}
