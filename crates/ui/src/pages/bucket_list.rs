//! BucketListPage — SSR page for `/ui` (home screen).
//!
//! Priority screen 1 (UI-SPEC Focal Points):
//! - `list_buckets_fn` Resource for SSR-rendered BucketTable
//! - "Create Bucket" accent button (top-right) opening CreateBucketModal island
//! - LoadingState while pending, EmptyState when bucket list is empty
//! - Success/delete toasts via Toast island context
//!
//! Security invariant: SSR page. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;
use leptos::either::Either;

use crate::components::{BucketTable, EmptyState, LoadingState};
use crate::islands::CreateBucketModal;
use crate::server_fns::buckets::list_buckets_fn;

/// Format bytes as human-readable string (B, KB, MB, GB, TB).
fn fmt_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    }
}

/// BucketListPage SSR component (`/ui`).
///
/// Calls `list_buckets_fn` via a `Resource` (SSR-rendered). Uses `Suspense` to
/// render LoadingState while pending and the table once resolved.
#[component]
pub fn BucketListPage() -> impl IntoView {
    // Blocking resource: renders SSR in-order (no out-of-order streaming placeholder
    // swap), which is required for the island modals on this page (CreateBucketModal,
    // per-row ConfirmModal) to hydrate correctly.
    let buckets = Resource::new_blocking(
        || (),
        |_| async move { list_buckets_fn().await },
    );

    // Reactive subtitle: "N buckets · X total" derived from the resolved resource.
    let subtitle = move || {
        buckets.get().and_then(|r| r.ok()).map(|rows| {
            let count = rows.len();
            let total: u64 = rows.iter().map(|r| r.total_size).sum();
            format!("{count} buckets · {} total", fmt_size(total))
        })
    };

    view! {
        <div style="display:flex;flex-direction:column;height:100%;">
            // Page header — title + subtitle + Create bucket accent button
            <header style="display:flex;align-items:center;gap:16px;\
                padding:18px 28px;border-bottom:1px solid var(--border);">
                <div>
                    <h1 style="margin:0;font-size:18px;font-weight:600;\
                        letter-spacing:-.3px;color:var(--text);">
                        "Buckets"
                    </h1>
                    <div style="font-size:12px;color:var(--faint);margin-top:2px;">
                        <Suspense>{move || subtitle()}</Suspense>
                    </div>
                </div>
                // "Create Bucket" accent button — opens CreateBucketModal island
                <div style="margin-left:auto;">
                    <CreateBucketModal />
                </div>
            </header>

            // Content scroll area
            <div style="flex:1;overflow:auto;padding:18px 28px 40px;">
            // Bucket table with Suspense for SSR loading state
            <Suspense fallback=|| view! { <LoadingState /> }>
                {move || {
                    buckets.get().map(|result| {
                        match result {
                            Ok(rows) if rows.is_empty() => {
                                Either::Left(view! {
                                    <EmptyState
                                        heading="No buckets yet".to_string()
                                        body="Create your first bucket to start storing objects.".to_string()
                                        cta_slot=Some(Box::new(|| view! { <CreateBucketModal /> }.into_any()))
                                    />
                                })
                            }
                            Ok(rows) => {
                                Either::Right(
                                    view! { <BucketTable rows=rows /> }.into_any()
                                )
                            }
                            Err(e) => {
                                Either::Right(view! {
                                    <div style="padding:24px;color:var(--danger);\
                                        font-size:14px;">
                                        {format!("Something went wrong. Refresh the page or check the server logs. ({e})")}
                                    </div>
                                }.into_any())
                            }
                        }
                    })
                }}
            </Suspense>
            </div>
        </div>
    }
}
