//! Sidebar SSR component — 252px fixed navigation panel.
//!
//! Renders: app name, bucket nav list (active bucket = accent left border),
//! settings link, ThemeToggle island, and SidebarStatus island.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. SSR-only.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::islands::{SidebarStatus, ThemeToggle};
use crate::server_fns::buckets::list_buckets_fn;

/// Sidebar component (SSR only).
///
/// Props:
/// - `buckets`: list of bucket names for the nav list.
/// - `active_bucket`: currently active bucket (accent highlight).
///
/// The sidebar status is resolved via the `SidebarStatus` hydrated island,
/// which calls `check_status_fn` client-side. This fixes GAP-04-05 where the
/// previous static `StatusIndicator` prop never called the server fn.
#[component]
pub fn Sidebar() -> impl IntoView {
    // Load the bucket list here (SSR-resolved) so the nav list + count populate on
    // every screen. Rendered once — its islands are NOT duplicated by a Suspense fallback.
    let buckets_res = Resource::new_blocking(|| (), |_| async move { list_buckets_fn().await });
    let names = move || {
        buckets_res
            .get()
            .and_then(|r| r.ok())
            .map(|rows| rows.into_iter().map(|b| b.name).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // Active bucket derived reactively from the URL (`/ui/buckets/{name}`), so the
    // highlight stays correct across both SSR and client-side navigation.
    let location = use_location();
    let active = Memo::new(move |_| {
        location
            .pathname
            .get()
            .strip_prefix("/ui/buckets/")
            .map(|rest| rest.split('/').next().unwrap_or("").to_string())
            .unwrap_or_default()
    });

    view! {
        // Scoped hover styles (template uses style-hover="…"; reproduced as :hover here)
        <style>
            ".fb-bucket-row:hover{background:var(--hover) !important;color:var(--text) !important}\
             .fb-settings-btn:hover{background:var(--hover) !important;color:var(--text) !important}\
             .fb-nav-btn:hover{background:var(--hover) !important;color:var(--text) !important}"
        </style>
        <aside
            aria-label="Main navigation"
            style="width:252px;flex:none;display:flex;flex-direction:column;\
                background:var(--panel);border-right:1px solid var(--border)"
        >
            // Logo box + name + version badge
            <div style="display:flex;align-items:center;gap:9px;padding:15px 16px 14px">
                <div style="width:24px;height:24px;border-radius:6px;background:var(--accent);\
                    display:flex;align-items:center;justify-content:center;flex:none">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="#fff" stroke-width="1.25" stroke-linejoin="round" stroke-linecap="round"><path d="M4.3 4Q8 1.3 11.7 4"/><ellipse cx="8" cy="4.5" rx="4.9" ry="1.5"/><path d="M3.3 4.7 4.7 12.3Q8 13.7 11.3 12.3L12.7 4.7"/></svg>
                </div>
                <div style="font-weight:700;font-size:14px;letter-spacing:-.2px">"ferrobucket"</div>
                <div style="margin-left:auto;font-family:'IBM Plex Mono',monospace;\
                    font-size:10px;color:var(--faint);border:1px solid var(--border);\
                    border-radius:4px;padding:1px 5px">"v0.4"</div>
            </div>

            // Connection status card island (hydrated, calls check_status_fn — fixes GAP-04-05)
            <SidebarStatus />

            // Buckets nav button with count
            <div style="padding:0 8px">
                <a
                    href="/ui"
                    class="fb-nav-btn"
                    style=move || {
                        let base = "width:100%;display:flex;align-items:center;gap:9px;\
                            padding:7px 10px;border:none;border-radius:6px;color:var(--text);\
                            font-family:inherit;font-size:13px;font-weight:500;cursor:pointer;\
                            text-align:left;text-decoration:none;box-sizing:border-box;".to_string();
                        if active.get().is_empty() {
                            format!("{base}background:var(--surface);")
                        } else {
                            format!("{base}background:transparent;")
                        }
                    }
                >
                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><ellipse cx="8" cy="4" rx="5.5" ry="2.2" stroke="currentColor" stroke-width="1.2"/><path d="M2.5 4v8c0 1.2 2.46 2.2 5.5 2.2s5.5-1 5.5-2.2V4M2.5 8c0 1.2 2.46 2.2 5.5 2.2s5.5-1 5.5-2.2" stroke="currentColor" stroke-width="1.2"/></svg>
                    "Buckets"
                    <span style="margin-left:auto;font-family:'IBM Plex Mono',monospace;\
                        font-size:11px;color:var(--faint)">
                        <Suspense>{move || Some(names().len())}</Suspense>
                    </span>
                </a>
            </div>

            // ALL BUCKETS section label
            <div style="font-size:10px;font-weight:600;letter-spacing:.6px;color:var(--faint);\
                text-transform:uppercase;padding:14px 18px 6px">"All buckets"</div>

            // Scrollable per-bucket mono list (Suspense wraps only <a> links — no islands here)
            <div style="flex:1;overflow-y:auto;padding:0 8px 8px">
                <Suspense>
                {move || names().into_iter().map(|name| {
                    let href = format!("/ui/buckets/{}", name);
                    let name_display = name.clone();
                    let name_row = name.clone();
                    let name_dot = name.clone();
                    view! {
                        <a
                            href=href
                            class="fb-bucket-row"
                            style=move || {
                                let is_active = active.get() == name_row;
                                let row_bg = if is_active { "var(--hover)" } else { "transparent" };
                                let row_color = if is_active { "var(--text)" } else { "var(--dim)" };
                                format!(
                                    "width:100%;display:flex;align-items:center;gap:8px;\
                                    padding:6px 10px;margin-bottom:1px;border:none;border-radius:6px;\
                                    background:{row_bg};color:{row_color};\
                                    font-family:'IBM Plex Mono',monospace;font-size:12px;cursor:pointer;\
                                    text-align:left;text-decoration:none;box-sizing:border-box;"
                                )
                            }
                        >
                            <span style=move || {
                                let dot_bg = if active.get() == name_dot { "var(--accent)" } else { "var(--border-2)" };
                                format!("width:5px;height:5px;border-radius:1px;background:{dot_bg};flex:none")
                            }></span>
                            <span style="min-width:0;overflow:hidden;text-overflow:ellipsis;\
                                white-space:nowrap;flex:1">{name_display}</span>
                        </a>
                    }
                }).collect_view()}
                </Suspense>
            </div>

            // Footer: Settings ghost button + boxed theme toggle
            <div style="border-top:1px solid var(--border);padding:8px;display:flex;\
                align-items:center;gap:6px">
                <a
                    href="/ui/settings"
                    class="fb-settings-btn"
                    style="flex:1;display:flex;align-items:center;gap:9px;padding:7px 10px;\
                        border:none;border-radius:6px;background:transparent;color:var(--dim);\
                        font-family:inherit;font-size:13px;cursor:pointer;text-align:left;\
                        text-decoration:none;box-sizing:border-box;"
                >
                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="2.2" stroke="currentColor" stroke-width="1.2"/><path d="M8 1.4v2M8 12.6v2M14.6 8h-2M3.4 8h-2M12.7 3.3l-1.4 1.4M4.7 11.3l-1.4 1.4M12.7 12.7l-1.4-1.4M4.7 4.7 3.3 3.3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
                    "Settings"
                </a>

                // Theme toggle island (hydrated, persists via localStorage)
                <ThemeToggle />
            </div>
        </aside>
    }
}
