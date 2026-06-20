//! Object server functions: list, head, delete (REQ-ui-object-browser, REQ-ui-object-detail).
//!
//! All functions call `FsStorage` in-process via `expect_context::<AppState>()`
//! (D-03 — no S3-over-loopback).

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::server_fns::state::AppState;
#[cfg(feature = "ssr")]
use crate::server_fns::buckets::rfc3339;
use crate::types::{ObjectDetail, ObjectListing, ObjectRow};

/// List objects under `prefix` in `bucket`, using `"/"` as delimiter so that
/// subdirectory entries appear as `common_prefixes` (folder rows) instead of
/// individual objects.
///
/// `delimiter = Some("/")` is mandatory (REQ-ui-object-browser): folders must
/// come back as `common_prefixes`, not as flat object keys.
///
/// Supports D-09 pagination via `continuation_token` / `next_token`.
#[server]
pub async fn list_objects_fn(
    bucket: String,
    prefix: Option<String>,
    continuation_token: Option<String>,
) -> Result<ObjectListing, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::{ListV2Req, Storage};

        let state = expect_context::<AppState>();

        let req = ListV2Req {
            prefix: prefix.clone(),
            delimiter: Some("/".to_owned()), // D-09: always "/" so folders = CommonPrefixes
            continuation_token,
            max_keys: None,
        };

        let res = state
            .storage
            .list_objects_v2(&bucket, req)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let objects: Vec<ObjectRow> = res
            .objects
            .into_iter()
            .map(|o| ObjectRow {
                key: o.key,
                size: o.size,
                last_modified: rfc3339(o.last_modified),
                is_folder: false,
            })
            .collect();

        Ok(ObjectListing {
            folders: res.common_prefixes,
            objects,
            next_token: res.next_continuation_token,
            prefix: prefix.unwrap_or_default(),
        })
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Fetch full object metadata (REQ-ui-object-detail).
#[server]
pub async fn head_object_fn(
    bucket: String,
    key: String,
) -> Result<ObjectDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::Storage;

        let state = expect_context::<AppState>();
        let meta = state
            .storage
            .head_object(&bucket, &key)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        Ok(ObjectDetail {
            key: meta.key,
            size: meta.size,
            content_type: meta.content_type,
            etag: meta.etag,
            last_modified: rfc3339(meta.last_modified),
        })
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Delete an object by key.
#[server]
pub async fn delete_object_fn(
    bucket: String,
    key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::Storage;

        let state = expect_context::<AppState>();
        state
            .storage
            .delete_object(&bucket, &key)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}
