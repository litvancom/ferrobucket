//! SettingsPage — SSR page for `/ui/settings`.
//!
//! Read-only connection view (D-10). No editable fields.
//! - `get_config_fn` Resource → ConnectionInfo
//! - `check_status_fn` Resource → StatusInfo
//! - CopyField for endpoint URL
//! - Force Path Style displayed as read-only badge (always `true`)
//!
//! SECURITY (T-04-17, D-10): Only the access key ID (public identifier) is rendered.
//! The signing key is never accessed here. `ConnectionInfo` has no credential material.
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
        <div style="padding:32px;">
            // Screen title: "Connection" (UI-SPEC)
            <h1 style="font-size:16px;font-weight:600;color:var(--text);\
                margin:0 0 24px 0;line-height:1.3;">
                "Connection"
            </h1>

            // Connection info block
            <Suspense fallback=|| view! { <LoadingState /> }>
                {move || {
                    config.get().map(|result| {
                        match result {
                            Ok(info) => {
                                view! {
                                    <div style="display:flex;flex-direction:column;\
                                        gap:16px;max-width:640px;">

                                        // Endpoint
                                        <div>
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:6px;\
                                                font-weight:400;">
                                                "Endpoint"
                                            </label>
                                            <CopyField
                                                value=info.endpoint.clone()
                                                label="S3 endpoint URL".to_string()
                                            />
                                        </div>

                                        // Region
                                        <div>
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:6px;\
                                                font-weight:400;">
                                                "Region"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;">
                                                {info.region.clone()}
                                            </div>
                                        </div>

                                        // Access Key ID (public identifier only — never the signing key)
                                        <div>
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:6px;\
                                                font-weight:400;">
                                                "Access Key ID"
                                            </label>
                                            <div style="font-family:'IBM Plex Mono',monospace;\
                                                font-size:13px;color:var(--text);\
                                                padding:8px 12px;background:var(--mono-bg);\
                                                border:1px solid var(--border);border-radius:4px;">
                                                {info.access_key_id.clone().unwrap_or_else(|| "anonymous".to_string())}
                                            </div>
                                        </div>

                                        // Force Path Style — read-only badge, always true (D-10)
                                        <div>
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:6px;\
                                                font-weight:400;">
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

                                        // Data directory path
                                        <div>
                                            <label style="display:block;font-size:12px;\
                                                color:var(--text-muted);margin-bottom:6px;\
                                                font-weight:400;">
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

            // Status indicator section
            <div style="margin-top:32px;max-width:640px;">
                <h2 style="font-size:14px;font-weight:600;color:var(--text);\
                    margin:0 0 12px 0;">
                    "Status"
                </h2>
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
