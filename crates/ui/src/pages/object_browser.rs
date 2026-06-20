//! ObjectBrowserPage — SSR page for `/ui/buckets/{bucket}[?prefix=…]`.
//!
//! Priority screen 2 (UI-SPEC Focal Points):
//! - Reads `bucket` from route param, `prefix` and `continuation` from query
//! - `list_objects_fn` Resource for SSR-rendered ObjectTable (folders then objects)
//! - Breadcrumb prefix nav
//! - Folders from ObjectListing.folders (CommonPrefixes) rendered first in ObjectTable
//! - UploadZone (drag-drop, props bucket+prefix) + UploadPanel
//! - PaginationBar (Previous/Next via next_token) — D-09
//! - Prefix filter input (server-side redirect on submit)
//! - Download links: `/ui/download/{bucket}/{key}` (D-04, Plan 03)
//! - Object row click opens SlideOver island (via ObjectTable)
//!
//! Object-detail slide-over content is rendered by the SlideOver island (Plan 04).
//! presign_fn is available in this module for server-fn calls from the browser
//! (the #[server] macro routes calls over HTTP from islands like CopyButton).
//!
//! Security invariant: SSR page. No presign/hmac/sigv4 code here.
//! Signing happens exclusively in presign_fn (server-side, #[server] gate).

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use leptos_router::hooks::use_query_map;
use leptos::either::Either;

// presign_fn: server fn for presigned URL minting (D-05, 900s TTL, server-side only).
// Available to islands on this page for "Copy Presigned URL" actions via #[server] HTTP.
#[allow(unused_imports)]
use crate::server_fns::presign::presign_fn;

use crate::components::{Breadcrumb, EmptyState, LoadingState, ObjectTable, PaginationBar};
use crate::islands::UploadIsland;
use crate::server_fns::objects::list_objects_fn;

/// ObjectBrowserPage SSR component (`/ui/buckets/{bucket}`).
///
/// Download action: links to `/ui/download/{bucket}/{key}` (D-04).
/// Presigned URL action: calls `presign_fn` server-side via island HTTP round-trip.
#[component]
pub fn ObjectBrowserPage() -> impl IntoView {
    // Read route params
    let params = use_params_map();
    let query = use_query_map();

    let bucket = move || params.read().get("bucket").unwrap_or_default();
    let prefix = move || query.read().get("prefix").unwrap_or_default();
    let continuation = move || query.read().get("continuation").unwrap_or_default();

    // Listing Resource — reacts to bucket/prefix/continuation changes
    let listing = Resource::new(
        move || (bucket(), prefix(), continuation()),
        |(b, p, c)| async move {
            list_objects_fn(
                b,
                if p.is_empty() { None } else { Some(p) },
                if c.is_empty() { None } else { Some(c) },
            )
            .await
        },
    );

    // Page base href for pagination bar and prefix filter
    let base_href = move || format!("/ui/buckets/{}", bucket());

    view! {
        <div style="display:flex;flex-direction:column;height:100%;">
            // Page header
            <div style="padding:24px 32px 0;flex-shrink:0;">
                <div style="display:flex;align-items:center;\
                    justify-content:space-between;margin-bottom:16px;">
                    <h1 style="font-size:16px;font-weight:600;color:var(--text);\
                        margin:0;line-height:1.3;overflow:hidden;\
                        text-overflow:ellipsis;white-space:nowrap;">
                        {move || bucket()}
                    </h1>
                    // "Upload Files" accent hint (drag-drop zone below does the work)
                    <span style="font-size:12px;color:var(--text-muted);">
                        "Drag files below to upload"
                    </span>
                </div>

                // Breadcrumb (prefix nav — clickable segments, --accent text)
                <Breadcrumb
                    bucket=bucket()
                    prefix=prefix()
                />
            </div>

            // Prefix filter input (server-side GET form redirect)
            <div style="padding:16px 32px 8px;flex-shrink:0;">
                <form
                    method="get"
                    action=move || base_href()
                    style="display:flex;gap:8px;"
                >
                    <input
                        type="text"
                        name="prefix"
                        value=move || prefix()
                        placeholder="Filter by prefix\u{2026}"
                        style="flex:1;background:var(--surface);color:var(--text);\
                            border:1px solid var(--border);border-radius:4px;\
                            padding:6px 12px;font-family:'IBM Plex Mono',monospace;\
                            font-size:13px;outline:none;max-width:400px;\
                            transition:border-color 150ms ease;"
                    />
                    <button
                        type="submit"
                        style="background:var(--surface);color:var(--text);\
                            border:1px solid var(--border);border-radius:4px;\
                            padding:6px 12px;font-size:13px;cursor:pointer;\
                            transition:background-color 150ms ease,border-color 150ms ease;"
                    >
                        "Filter"
                    </button>
                    // Clear filter link
                    {move || {
                        let p = prefix();
                        if !p.is_empty() {
                            Either::Left(view! {
                                <a
                                    href=base_href()
                                    style="display:inline-flex;align-items:center;\
                                        padding:6px 12px;font-size:13px;color:var(--text-muted);\
                                        text-decoration:none;border:1px solid var(--border);\
                                        border-radius:4px;transition:color 150ms ease;"
                                >
                                    "Clear"
                                </a>
                            })
                        } else {
                            Either::Right(())
                        }
                    }}
                </form>
            </div>

            // Upload island (drag-and-drop zone + fixed-bottom progress panel in
            // ONE hydrated island — D-06, D-07, D-08; GAP-04-01 fix: zone + panel
            // share one locally-owned entries signal, no cross-island use_context).
            <div style="padding:0 32px 16px;flex-shrink:0;">
                <UploadIsland bucket=bucket() prefix=prefix() />
            </div>

            // Object table with Suspense for SSR loading state
            <div style="flex:1;overflow-y:auto;min-height:0;">
                <Suspense fallback=|| view! { <LoadingState /> }>
                    {move || {
                        listing.get().map(|result| {
                            match result {
                                Ok(listing) => {
                                    let is_empty = listing.folders.is_empty()
                                        && listing.objects.is_empty();
                                    let has_prefix = !listing.prefix.is_empty();
                                    let next_token = listing.next_token.clone();
                                    let pfx = listing.prefix.clone();
                                    let bkt = bucket();
                                    let bkt_href = base_href();
                                    let pfx2 = pfx.clone();

                                    // Build download link for first object (used in table rows
                                    // via ObjectTable → /ui/download/{bucket}/{key} — D-04)
                                    let _download_base = format!("/ui/download/{}", bkt.clone());

                                    Either::Left(if is_empty {
                                        if has_prefix {
                                            view! {
                                                <EmptyState
                                                    heading="No objects with this prefix.".to_string()
                                                    body="Try a different prefix or clear the filter.".to_string()
                                                    cta_href=Some(bkt_href)
                                                    cta_label=Some("Clear filter".to_string())
                                                />
                                            }.into_any()
                                        } else {
                                            view! {
                                                <EmptyState
                                                    heading="This bucket is empty".to_string()
                                                    body="Upload files or drag them here to get started.".to_string()
                                                />
                                            }.into_any()
                                        }
                                    } else {
                                        view! {
                                            <div>
                                                // ObjectTable: folders first (CommonPrefixes),
                                                // then object rows with Download/Copy/Delete.
                                                // Download links use /ui/download/{bucket}/{key} (D-04).
                                                <ObjectTable
                                                    bucket=bkt.clone()
                                                    prefix=pfx2.clone()
                                                    folders=listing.folders
                                                    objects=listing.objects
                                                />
                                                // PaginationBar: Previous/Next via continuation
                                                // token (D-09 — not infinite scroll)
                                                <PaginationBar
                                                    base_href=bkt_href
                                                    prefix=pfx2
                                                    next_token=next_token
                                                />
                                            </div>
                                        }.into_any()
                                    })
                                }
                                Err(e) => {
                                    Either::Right(view! {
                                        <div style="padding:24px;color:var(--destructive);\
                                            font-size:14px;">
                                            {format!("Something went wrong. Refresh the page or \
                                                check the server logs. ({e})")}
                                        </div>
                                    })
                                }
                            }
                        })
                    }}
                </Suspense>
            </div>
        </div>
    }
}
