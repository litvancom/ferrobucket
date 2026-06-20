//! Sidebar SSR component — 220px fixed navigation panel.
//!
//! Renders: app name, bucket nav list (active bucket = accent left border),
//! settings link, ThemeToggle island, and SidebarStatus island.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. SSR-only.

use leptos::prelude::*;

use crate::islands::{SidebarStatus, ThemeToggle};

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
pub fn Sidebar(
    #[prop(default = Vec::new())] buckets: Vec<String>,
    #[prop(default = String::new())] active_bucket: String,
) -> impl IntoView {
    let active = StoredValue::new(active_bucket);

    view! {
        <nav
            aria-label="Main navigation"
            style="width:220px;flex-shrink:0;background:var(--surface);\
                border-right:1px solid var(--border);display:flex;\
                flex-direction:column;height:100vh;position:sticky;\
                top:0;overflow-y:auto;"
        >
            // App name / logo
            <div style="padding:24px 16px 16px;border-bottom:1px solid var(--border);">
                <span style="font-size:16px;font-weight:600;color:var(--text);\
                    letter-spacing:-0.01em;">
                    "ferrobucket"
                </span>
            </div>

            // Buckets section
            <div style="flex:1;padding:8px 0;overflow-y:auto;">
                <div style="padding:8px 16px 4px;">
                    <span style="font-size:12px;font-weight:400;color:var(--text-muted);\
                        text-transform:uppercase;letter-spacing:0.05em;">
                        "Buckets"
                    </span>
                </div>
                <ul style="list-style:none;margin:0;padding:0;">
                    {buckets.into_iter().map(|name| {
                        let is_active = name == active.get_value();
                        let href = format!("/ui/buckets/{}", name);
                        let name_display = name.clone();
                        view! {
                            <li>
                                <a
                                    href=href
                                    style=move || {
                                        let base = "display:block;padding:6px 16px;\
                                            font-size:14px;text-decoration:none;\
                                            white-space:nowrap;overflow:hidden;\
                                            text-overflow:ellipsis;\
                                            transition:background-color 150ms ease,\
                                            color 150ms ease;".to_string();
                                        if is_active {
                                            format!("{base}color:var(--accent);\
                                                border-left:2px solid var(--accent);\
                                                padding-left:14px;background:var(--surface-raised);")
                                        } else {
                                            format!("{base}color:var(--text);\
                                                border-left:2px solid transparent;\
                                                padding-left:14px;")
                                        }
                                    }
                                >
                                    {name_display}
                                </a>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                // Link to bucket list (home)
                <div style="padding:4px 16px 8px;">
                    <a
                        href="/ui"
                        style="font-size:12px;color:var(--text-muted);\
                            text-decoration:none;display:block;padding:4px 0;\
                            transition:color 150ms ease;"
                    >
                        "+ All buckets"
                    </a>
                </div>
            </div>

            // Bottom section: settings + theme toggle + status
            <div style="border-top:1px solid var(--border);padding:12px 0;">
                // Settings link
                <a
                    href="/ui/settings"
                    style="display:flex;align-items:center;gap:8px;\
                        padding:8px 16px;font-size:14px;color:var(--text-muted);\
                        text-decoration:none;transition:color 150ms ease,\
                        background-color 150ms ease;"
                >
                    // Settings gear icon (Lucide)
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="16" height="16" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor"
                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
                        <circle cx="12" cy="12" r="3"/>
                    </svg>
                    "Settings"
                </a>

                // Theme toggle island (hydrated, persists via localStorage)
                <div style="padding:4px 16px;">
                    <ThemeToggle />
                </div>

                // Status indicator island (hydrated, calls check_status_fn — fixes GAP-04-05)
                <div style="padding:8px 16px 4px;">
                    <SidebarStatus />
                </div>
            </div>
        </nav>
    }
}
