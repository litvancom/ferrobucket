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
        <div style="display:flex;flex-direction:column;height:100%">

            // ── Header: "Connection" + subtitle + live status pill ────────────
            <header style="display:flex;align-items:center;gap:16px;padding:18px 28px;border-bottom:1px solid var(--border)">
                <div>
                    <h1 style="margin:0;font-size:18px;font-weight:600;letter-spacing:-.3px">
                        "Connection"
                    </h1>
                    <div style="font-size:12px;color:var(--faint);margin-top:2px">
                        "S3-compatible endpoint configuration"
                    </div>
                </div>
                // Status pill — resolves from check_status_fn
                <div style="margin-left:auto">
                    <Suspense fallback=|| view! {}>
                        {move || {
                            status.get().map(|result| {
                                let (writable, message) = match result {
                                    Ok(s) => (s.writable, s.message),
                                    Err(_) => (false, "Unable to check status.".to_string()),
                                };
                                let dot = if writable { "var(--success)" } else { "var(--warn)" };
                                view! {
                                    <div style="display:flex;align-items:center;gap:8px;padding:6px 12px;border:1px solid var(--border);border-radius:20px;background:var(--surface)">
                                        <span
                                            aria-hidden="true"
                                            style=format!(
                                                "width:7px;height:7px;border-radius:50%;background:{dot};flex:none;"
                                            )
                                        />
                                        <span style="font-size:12px;color:var(--dim)">
                                            {message}
                                        </span>
                                    </div>
                                }
                            })
                        }}
                    </Suspense>
                </div>
            </header>

            // ── Scroll area ───────────────────────────────────────────────────
            <div style="flex:1;overflow:auto;padding:24px 28px 48px">
            <div style="max-width:680px">

            // ── Server-side credentials info banner ───────────────────────────
            <div style="display:flex;gap:9px;padding:11px 14px;margin-bottom:22px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--dim);font-size:12.5px;line-height:1.5">
                // Info icon
                <svg
                    width="15"
                    height="15"
                    viewBox="0 0 16 16"
                    fill="none"
                    style="flex:none;margin-top:1px;color:var(--accent)"
                    aria-hidden="true"
                >
                    <circle cx="8" cy="8" r="6.3" stroke="currentColor" stroke-width="1.2" />
                    <path d="M8 7.2v3.6M8 5.1v.1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                </svg>
                <p style="margin:0">
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
                                    <div style="border:1px solid var(--border);border-radius:10px;background:var(--surface);overflow:hidden">

                                        // Endpoint
                                        <div style="padding:16px 18px;border-bottom:1px solid var(--border)">
                                            <label style="display:block;font-size:12px;font-weight:600;color:var(--dim);margin-bottom:7px">
                                                "Endpoint URL"
                                            </label>
                                            <CopyField
                                                value=info.endpoint.clone()
                                                label="S3 endpoint URL".to_string()
                                                copy_label="Copy endpoint".to_string()
                                                copied_label="Endpoint copied".to_string()
                                            />
                                            <div style="font-size:11.5px;color:var(--faint);margin-top:6px">
                                                "For local backends like MinIO, use the host:port your server exposes."
                                            </div>
                                        </div>

                                        // Region + Force Path Style — 2-col grid
                                        <div style="display:grid;grid-template-columns:1fr 1fr;border-bottom:1px solid var(--border)">
                                            // Region
                                            <div style="padding:16px 18px;border-right:1px solid var(--border)">
                                                <label style="display:block;font-size:12px;font-weight:600;color:var(--dim);margin-bottom:7px">
                                                    "Region"
                                                </label>
                                                <div style="width:100%;padding:9px 11px;border:1px solid var(--border-2);border-radius:7px;background:var(--bg);color:var(--text);font-family:'IBM Plex Mono',monospace;font-size:13px">
                                                    {info.region.clone()}
                                                </div>
                                            </div>
                                            // Force Path Style — read-only locked-ON toggle (D-10)
                                            <div style="padding:16px 18px;display:flex;align-items:center;justify-content:space-between;gap:12px">
                                                <div>
                                                    <div style="font-size:12px;font-weight:600;color:var(--text);margin-bottom:2px">
                                                        "Force path-style"
                                                    </div>
                                                    <div style="font-size:11.5px;color:var(--faint)">
                                                        "Locked ON for MinIO & local endpoints"
                                                    </div>
                                                </div>
                                                // Locked-ON toggle switch (visual, non-interactive — D-10)
                                                <div
                                                    aria-label="Force path-style: on (locked)"
                                                    style="width:42px;height:24px;flex:none;border-radius:13px;background:var(--accent);position:relative;cursor:not-allowed"
                                                >
                                                    <span style="position:absolute;top:3px;left:21px;width:18px;height:18px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,.3)"></span>
                                                </div>
                                            </div>
                                        </div>

                                        // Credentials sub-panel — managed server-side (var(--surface-2))
                                        <div style="padding:16px 18px;background:var(--surface-2)">
                                            <div style="font-size:11px;font-weight:600;letter-spacing:.4px;color:var(--faint);text-transform:uppercase;margin-bottom:12px">
                                                "Credentials · managed server-side"
                                            </div>
                                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
                                                // Access Key ID (public identifier — never the signing key)
                                                <div>
                                                    <label style="display:block;font-size:12px;font-weight:600;color:var(--dim);margin-bottom:7px">
                                                        "Access key ID"
                                                    </label>
                                                    <div style="display:flex;align-items:center;gap:8px;padding:9px 11px;border:1px solid var(--border);border-radius:7px;background:var(--bg);font-family:'IBM Plex Mono',monospace;font-size:13px;color:var(--dim)">
                                                        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" style="flex:none">
                                                            <rect x="3" y="7" width="10" height="7" rx="1.4" stroke="currentColor" stroke-width="1.2" />
                                                            <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" stroke="currentColor" stroke-width="1.2" />
                                                        </svg>
                                                        {info.access_key_id.clone().unwrap_or_else(|| "anonymous".to_string())}
                                                    </div>
                                                </div>
                                                // Masked secret indicator — fixed bullets, no value (T-04-17, D-10)
                                                // ConnectionInfo carries no credential material; this is visual-only.
                                                <div>
                                                    <label style="display:block;font-size:12px;font-weight:600;color:var(--dim);margin-bottom:7px">
                                                        "Secret access key"
                                                    </label>
                                                    <div style="display:flex;align-items:center;gap:8px;padding:9px 11px;border:1px solid var(--border);border-radius:7px;background:var(--bg);font-family:'IBM Plex Mono',monospace;font-size:13px;color:var(--faint)">
                                                        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" style="flex:none">
                                                            <rect x="3" y="7" width="10" height="7" rx="1.4" stroke="currentColor" stroke-width="1.2" />
                                                            <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" stroke="currentColor" stroke-width="1.2" />
                                                        </svg>
                                                        "••••••••••••••••"
                                                    </div>
                                                </div>
                                            </div>
                                        </div>

                                        // Data directory path
                                        <div style="padding:16px 18px;border-top:1px solid var(--border)">
                                            <label style="display:block;font-size:12px;font-weight:600;color:var(--dim);margin-bottom:7px">
                                                "Data Directory"
                                            </label>
                                            <div style="width:100%;padding:9px 11px;border:1px solid var(--border-2);border-radius:7px;background:var(--bg);color:var(--text);font-family:'IBM Plex Mono',monospace;font-size:13px;word-break:break-all">
                                                {info.data_dir.clone()}
                                            </div>
                                        </div>

                                    </div>
                                }.into_any()
                            }
                            Err(e) => {
                                view! {
                                    <div style="color:var(--danger);font-size:14px;">
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

            </div>// end max-width wrapper
            </div>// end scroll area
        </div>
    }
}
