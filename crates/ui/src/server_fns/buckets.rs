//! Bucket server functions: list, create, delete (REQ-ui-bucket-list).
//!
//! All functions call `FsStorage` in-process via `expect_context::<AppState>()`
//! (D-03 — no S3-over-loopback). Errors are mapped to `ServerFnError::new`
//! (Open Question 3 — typed errors are a v2 refinement).

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::server_fns::state::AppState;
use crate::types::BucketRow;

/// List all buckets, deriving `object_count` and `total_size` for each.
///
/// `BucketInfo` from storage does not carry count or size — they are computed
/// by calling `list_objects_v2` with no delimiter to enumerate all objects
/// (REQ-ui-bucket-list: table shows both columns).
#[server]
pub async fn list_buckets_fn() -> Result<Vec<BucketRow>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::{ListV2Req, Storage};

        let state = expect_context::<AppState>();
        let buckets = state
            .storage
            .list_buckets()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let mut rows = Vec::with_capacity(buckets.len());
        for bucket in &buckets {
            // Derive count and total size by listing all objects (no delimiter = flat walk).
            let listing = state
                .storage
                .list_objects_v2(
                    &bucket.name,
                    ListV2Req {
                        prefix: None,
                        delimiter: None,
                        continuation_token: None,
                        max_keys: None,
                    },
                )
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            let object_count = listing.objects.len() as u64;
            let total_size = listing.objects.iter().map(|o| o.size).sum();

            rows.push(BucketRow {
                name: bucket.name.clone(),
                created: bucket.created_at.to_string(),
                object_count,
                total_size,
            });
        }

        Ok(rows)
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Create a bucket. Reserved-name and validation errors surface verbatim (D-02).
///
/// The reserved-name check (`"ui"`, `"pkg"`) and all DNS-safety rules run inside
/// `FsStorage::create_bucket` → `validate_bucket_name` — the server fn passes
/// the name through unchanged.
#[server]
pub async fn create_bucket_fn(name: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::Storage;

        let state = expect_context::<AppState>();
        state
            .storage
            .create_bucket(&name)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

/// Delete a bucket by name.
#[server]
pub async fn delete_bucket_fn(name: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ferrobucket_storage::Storage;

        let state = expect_context::<AppState>();
        state
            .storage
            .delete_bucket(&name)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }

    #[cfg(not(feature = "ssr"))]
    unreachable!()
}
