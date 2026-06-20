//! SettingsPage — SSR page for `/ui/settings`.
//!
//! Read-only connection view (D-10). No editable fields, no Save, no Test.
//! Layout follows `S3 Browser.dc.html` lines ~263-335 (Connection screen).
//!
//! - Header "Connection" + live status pill (check_status_fn)
//! - Server-side credentials info banner
//! - Bordered card: Endpoint (CopyField "Copy endpoint"), Region,
//!   Force Path Style (locked-ON badge), Access Key ID, Access Credentials (masked bullets), Data Directory
//!
//! SECURITY (T-04-17, D-10): `ConnectionInfo` carries no credential material.
//! The signing key renders only as a fixed masked indicator (bullets), never the value.
//!
//! Security invariant: SSR page. No presign/hmac/sigv4 code.

use leptos::prelude::*;

use crate::components::{CopyField, LoadingState, StatusIndicator};
use crate::server_fns::settings::{check_status_fn, get_config_fn};

/// SettingsPage SSR component (`/ui/settings`).
///
/// Displays read-only connection configuration and live status.
/// Screen title: "Connection" (UI-SPEC Copywriting Contract).
#[component]
pub fn SettingsPage() -> impl IntoView {
    let config = Resource::new(|| (), |_| async move { get_config_fn().await });
    let status = Resource::new(|| (), |_| async move { check_status_fn().await });

    view! {
        <div style="padding:32px;max-width:720px;">

            // ── Header row: "Connection" + live status pill ──────────────────
            <div style="display:flex;align-items:center;gap:12px;margin-bottom:24px;flex-wrap:wrap;">
                <h1 style="font-size:16px;font-weight:600;color:var(--text);margin:0;line-height:1.3;">
                    "Connection"
                </h1>
                // Status pill — resolves from check_status_fn
                <Suspense fallback=|| view! {}>
                    {move || {
                        status.get().map(|result| {
                            let (writable, message) = match result {
                                Ok(s) => (s.writable, s.message),
                                Err(_) => (false, "Unable to check status.".to_string()),
                            };
                            view! {
                                <span style=format!(
                                    "display:inline-flex;align-items:center;gap:5px;\
                                    padding:3px 10px;border-radius:12px;font-size:12px;\
                                    font-weight:500;border:1px solid {};\
                                    background:{};color:{};",
                                    if writable { "var(--success)" } else { "var(--warning)" },
                                    if writable { "color-mix(in srgb,var(--success) 12%,transparent)" } else { "color-mix(in srgb,var(--warning) 12%,transparent)" },
                                    if writable { "var(--success)" } else { "var(--warning)" },
                                )>
                                    <span
                                        aria-hidden="true"
                                        style=format!(
                                            "width:6px;height:6px;border-radius:50%;background:{};flex-shrink:0;",
                                            if writable { "var(--success)" } else { "var(--warning)" }
                                        )
                                    />
                                    {message}
                                </span>
                            }
                        })
                    }}
                </Suspense>
            </div>

            // ── Server-side credentials info banner ───────────────────────────
            <div style="display:flex;align-items:flex-start;gap:10px;\
                padding:12px 16px;border-radius:6px;\
                background:color-mix(in srgb,var(--accent) 8%,transparent);\
                border:1px solid color-mix(in srgb,var(--accent) 25%,transparent);\
                margin-bottom:24px;">
                // Info icon
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="15"
                    height="15"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="var(--accent)"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    style="flex-shrink:0;margin-top:1px;"
                    aria-hidden="true"
                >
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="8" x2="12" y2="12" />
                    <line x1="12" y1="16" x2="12.01" y2="16" />
                </svg>
                <p style="font-size:13px;color:var(--text-muted);margin:0;line-height:1.5;">
                    "Credentials are loaded from environment variables at server start. \
                    They are never sent to the browser."
                </p>
            </div>

            // ── Connection fields — bordered card ─────────────────────────────
            <Suspense fallback=|| view! { <LoadingState /> }>
                {move || {
                    config.get().map(|result| {
                        match result {
                            Ok(info) => {
                                view! {
                                    <div style="border:1px solid var(--border);border-radius:8px;\
                                        overflow:hidden;">

                                        // Endpoint
                                        <div style="padding:16px 20px;\
                                            border-bottom:1px solid var(--border);">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Endpoint URL"
                                            </label>
                                            <CopyField
                                                value=info.endpoint.clone()
                                                label="S3 endpoint URL".to_string()
                                                copy_label="Copy endpoint".to_string()
                                                copied_label="Endpoint copied".to_string()
                                            />
                                        </div>

                                        // Region
                                        <div style="padding:16px 20px;\
                                            border-bottom:1px solid var(--border);">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Region"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;">
                                                {info.region.clone()}
                                            </div>
                                        </div>

                                        // Force Path Style — read-only locked-ON badge (D-10)
                                        <div style="padding:16px 20px;\
                                            border-bottom:1px solid var(--border);">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Force Path Style"
                                            </label>
                                            <div style="display:inline-flex;align-items:center;\
                                                gap:8px;">
                                                <span style="font-family:'IBM Plex Mono',monospace;\
                                                    font-size:13px;color:var(--text);\
                                                    padding:4px 10px;background:var(--mono-bg);\
                                                    border:1px solid var(--border);border-radius:4px;\
                                                    cursor:not-allowed;">
                                                    "true"
                                                </span>
                                                <span style="font-size:12px;color:var(--text-muted);">
                                                    "(locked ON)"
                                                </span>
                                            </div>
                                        </div>

                                        // Access Key ID (public identifier — never the signing key)
                                        <div style="padding:16px 20px;\
                                            border-bottom:1px solid var(--border);">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Access Key ID"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;">
                                                {info.access_key_id.clone().unwrap_or_else(|| "anonymous".to_string())}
                                            </div>
                                        </div>

                                        // Masked credential indicator — fixed bullets, no value (T-04-17, D-10)
                                        // ConnectionInfo carries no credential material; this is a visual-only field.
                                        <div style="padding:16px 20px;\
                                            border-bottom:1px solid var(--border);">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Access Credentials"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text-muted);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;\
                                                letter-spacing:0.15em;">
                                                "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
                                            </div>
                                        </div>

                                        // Data directory path
                                        <div style="padding:16px 20px;">
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:8px;\
                                                font-weight:400;letter-spacing:0.02em;">
                                                "Data Directory"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;\
                                                word-break:break-all;">
                                                {info.data_dir.clone()}
                                            </div>
                                        </div>

                                    </div>
                                }.into_any()
                            }
                            Err(e) => {
                                view! {
                                    <div style="color:var(--destructive);font-size:14px;">
                                        {format!("Something went wrong loading connection info. ({e})")}
                                    </div>
                                }.into_any()
                            }
                        }
                    })
                }}
            </Suspense>

            // ── Detailed status (below card) ──────────────────────────────────
            <div style="margin-top:24px;">
                <Suspense fallback=|| view! { <LoadingState /> }>
                    {move || {
                        status.get().map(|result| {
                            match result {
                                Ok(s) => {
                                    view! {
                                        <StatusIndicator
                                            writable=s.writable
                                            message=s.message
                                        />
                                    }.into_any()
                                }
                                Err(_) => {
                                    view! {
                                        <StatusIndicator
                                            writable=false
                                            message="Unable to check status.".to_string()
                                        />
                                    }.into_any()
                                }
                            }
                        })
                    }}
                </Suspense>
            </div>

        </div>
    }
}
