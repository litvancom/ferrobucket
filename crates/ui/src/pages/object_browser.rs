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
    // Blocking: in-order SSR so the row islands (SlideOver, delete ConfirmModal) hydrate.
    let listing = Resource::new_blocking(
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
            // Page header — back icon-button + breadcrumb (mono) + prefix filter
            <header style="display:flex;align-items:center;gap:14px;\
                padding:14px 28px;border-bottom:1px solid var(--border);flex-shrink:0;">
                // Back-to-buckets icon button
                <a
                    href="/ui"
                    title="Back to buckets"
                    aria-label="Back to buckets"
                    style="width:30px;height:30px;flex:none;display:flex;align-items:center;\
                        justify-content:center;border:1px solid var(--border);border-radius:7px;\
                        background:var(--surface);color:var(--dim);cursor:pointer;\
                        text-decoration:none;transition:color 150ms ease,border-color 150ms ease;"
                    onmouseover="this.style.color='var(--text)';this.style.borderColor='var(--border-2)'"
                    onmouseout="this.style.color='var(--dim)';this.style.borderColor='var(--border)'"
                >
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                        <path d="M9.5 3.5 5 8l4.5 4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </a>

                // Breadcrumb (prefix nav — clickable mono segments)
                <Breadcrumb
                    bucket=bucket()
                    prefix=prefix()
                />

                // Prefix filter (search box) — server-side GET form redirect
                <div style="margin-left:auto;display:flex;align-items:center;gap:10px;flex:none;">
                    <form
                        method="get"
                        action=move || base_href()
                        class="fb-search"
                        style="display:flex;align-items:center;gap:7px;padding:7px 11px;\
                            border:1px solid var(--border-2);border-radius:7px;\
                            background:var(--surface);width:220px;\
                            transition:border-color 150ms ease,box-shadow 150ms ease;"
                    >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" style="flex:none;color:var(--faint);" aria-hidden="true">
                            <circle cx="7" cy="7" r="4.3" stroke="currentColor" stroke-width="1.2"/>
                            <path d="M10.3 10.3 14 14" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
                        </svg>
                        <input
                            type="text"
                            name="prefix"
                            value=move || prefix()
                            placeholder="Filter by prefix\u{2026}"
                            spellcheck="false"
                            style="flex:1;min-width:0;border:none;background:transparent;\
                                color:var(--text);font-family:'IBM Plex Mono',monospace;\
                                font-size:12.5px;outline:none;"
                        />
                        // Clear filter link (only when a prefix is active)
                        {move || {
                            let p = prefix();
                            if !p.is_empty() {
                                Either::Left(view! {
                                    <a
                                        href=base_href()
                                        title="Clear filter"
                                        aria-label="Clear filter"
                                        style="flex:none;display:flex;align-items:center;\
                                            justify-content:center;color:var(--faint);\
                                            text-decoration:none;cursor:pointer;"
                                        onmouseover="this.style.color='var(--text)'"
                                        onmouseout="this.style.color='var(--faint)'"
                                    >
                                        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                                            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
                                        </svg>
                                    </a>
                                })
                            } else {
                                Either::Right(())
                            }
                        }}
                    </form>
                </div>
            </header>

            // Upload island (drag-and-drop zone + fixed-bottom progress panel in
            // ONE hydrated island — D-06, D-07, D-08; GAP-04-01 fix: zone + panel
            // share one locally-owned entries signal, no cross-island use_context).
            <div style="padding:18px 28px 0;flex-shrink:0;">
                <UploadIsland bucket=bucket() prefix=prefix() />
            </div>

            // Object table with Suspense for SSR loading state
            <div style="flex:1;overflow-y:auto;min-height:0;padding:18px 28px 40px;">
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
                                            // Bordered card wrapping table + footer (template radius:9px)
                                            <div style="border:1px solid var(--border);\
                                                border-radius:9px;overflow:hidden;background:var(--surface);">
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
                                        <div style="padding:24px;color:var(--danger);\
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
