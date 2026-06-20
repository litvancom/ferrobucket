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

/// BucketListPage SSR component (`/ui`).
///
/// Calls `list_buckets_fn` via a `Resource` (SSR-rendered). Uses `Suspense` to
/// render LoadingState while pending and the table once resolved.
#[component]
pub fn BucketListPage() -> impl IntoView {
    let buckets = Resource::new(
        || (),
        |_| async move { list_buckets_fn().await },
    );

    view! {
        <div style="padding:32px 32px 32px;">
            // Page header
            <div style="display:flex;align-items:center;\
                justify-content:space-between;margin-bottom:24px;">
                <h1 style="font-size:16px;font-weight:600;color:var(--text);\
                    margin:0;line-height:1.3;">
                    "Buckets"
                </h1>
                // "Create Bucket" accent button — opens CreateBucketModal island
                <CreateBucketModal />
            </div>

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
                                    <div style="padding:24px;color:var(--destructive);\
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
    }
}
