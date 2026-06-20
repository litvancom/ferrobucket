//! SlideOver island — right-anchored 400px object-detail panel.
//!
//! The island manages its own open/close state AND loads real object metadata
//! via `head_object_fn` (REQ-ui-object-detail, GAP-04-01). Actions: Download,
//! Copy Presigned URL (calls `presign_fn` server-side, URL-string-only in WASM),
//! and Delete (inline confirm flow, no cross-island signal).
//!
//! Architecture invariant (DEC-ui-ssr, T-04-09A, T-04-09D):
//!   - Single island macro — no cross-island WriteSignal.
//!   - presign_fn is a #[server] fn; only the returned URL String enters WASM.
//!   - No presign/hmac/sigv4/secret_key code in this file.
//!   - All browser/DOM/clipboard calls are #[cfg(feature = "hydrate")]-gated.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::server_fns::objects::{delete_object_fn, head_object_fn};
use crate::server_fns::presign::presign_fn;

/// Format bytes into human-readable string (mirrors object_table.rs).
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

/// SlideOver island — right-side panel for object detail.
///
/// Props (all serializable):
/// - `bucket`: bucket name.
/// - `object_key`: object key.
///
/// On open, loads real metadata via `head_object_fn` (GAP-04-01 fix).
/// Actions: Download, Copy Presigned URL (presign_fn + clipboard), Delete (inline modal).
#[island]
pub fn SlideOver(bucket: String, object_key: String) -> impl IntoView {
    let (open, set_open) = signal(false);
    let (loading, set_loading) = signal(false);
    let (delete_confirm_open, set_delete_confirm_open) = signal(false);
    let (deleting, set_deleting) = signal(false);
    let (presign_copied, set_presign_copied) = signal(false);
    let (presigning, set_presigning) = signal(false);

    // None = not yet loaded; Some(Ok(detail)) or Some(Err(msg)) after first open.
    let (metadata, set_metadata) =
        signal::<Option<Result<crate::types::ObjectDetail, String>>>(None);

    let bucket_sv = StoredValue::new(bucket.clone());
    let object_key_sv = StoredValue::new(object_key.clone());

    // Display title is the filename portion of the key.
    let title = {
        let k = object_key.clone();
        k.rsplit('/').next().unwrap_or(&object_key).to_string()
    };
    let title_sv = StoredValue::new(title);

    // Open handler: fetch metadata on first open.
    let handle_open = move |_| {
        set_open.set(true);
        if metadata.get().is_none() {
            set_loading.set(true);
            let bkt = bucket_sv.get_value();
            let key = object_key_sv.get_value();
            spawn_local(async move {
                let result = head_object_fn(bkt, key).await;
                set_metadata.set(Some(result.map_err(|e| e.to_string())));
                set_loading.set(false);
            });
        }
    };
    let handle_close = move |_| {
        set_open.set(false);
        set_delete_confirm_open.set(false);
    };
    let handle_backdrop = move |_| {
        set_open.set(false);
        set_delete_confirm_open.set(false);
    };

    // Copy Presigned URL: calls presign_fn (server-side signing), writes returned
    // URL string to clipboard, shows the UI-SPEC affordance. No signing in WASM.
    let handle_presign = move |_| {
        set_presigning.set(true);
        set_presign_copied.set(false);
        let bkt = bucket_sv.get_value();
        let key = object_key_sv.get_value();
        spawn_local(async move {
            let result = presign_fn(bkt, key).await;
            set_presigning.set(false);
            if let Ok(url) = result {
                // Write URL string to clipboard (browser API, hydrate-gated).
                #[cfg(feature = "hydrate")]
                {
                    if let Some(window) = web_sys::window() {
                        let clipboard = window.navigator().clipboard();
                        let _ = wasm_bindgen_futures::JsFuture::from(
                            clipboard.write_text(&url),
                        )
                        .await;
                        set_presign_copied.set(true);
                    }
                }
                // Under ssr compilation (no browser), suppress unused-variable warning.
                #[cfg(not(feature = "hydrate"))]
                let _ = url;
            }
        });
    };

    // Delete: open inline confirm, then call delete_object_fn on confirm.
    let handle_delete_open = move |_| set_delete_confirm_open.set(true);
    let handle_delete_dismiss = move |_| set_delete_confirm_open.set(false);
    let handle_delete_confirm = move |_| {
        set_deleting.set(true);
        let bkt = bucket_sv.get_value();
        let key = object_key_sv.get_value();
        spawn_local(async move {
            let result = delete_object_fn(bkt, key).await;
            set_deleting.set(false);
            if result.is_ok() {
                set_open.set(false);
                set_delete_confirm_open.set(false);
                #[cfg(feature = "hydrate")]
                if let Some(window) = web_sys::window() {
                    let _ = window.location().reload();
                }
            }
        });
    };

    view! {
        // Trigger: clicking the object key name opens the slide-over.
        <button
            on:click=handle_open
            style="background:none;border:none;cursor:pointer;\
                color:var(--accent);font-size:13px;\
                font-family:'IBM Plex Mono',monospace;\
                padding:4px 8px;border-radius:4px;\
                text-align:left;max-width:280px;overflow:hidden;\
                text-overflow:ellipsis;white-space:nowrap;"
        >
            {move || {
                let k = object_key_sv.get_value();
                k.rsplit('/').next().map(|s| s.to_string()).unwrap_or(k)
            }}
        </button>

        <Show when=move || open.get()>
            // Backdrop (closes panel on click outside)
            <div
                style="position:fixed;inset:0;z-index:400;"
                on:click=handle_backdrop
            />

            // Slide-over panel: 400px, right-anchored, full height (UI-SPEC Screen 3).
            <div
                style="position:fixed;top:0;right:0;bottom:0;width:400px;\
                    background:var(--surface);border-left:1px solid var(--border);\
                    z-index:401;display:flex;flex-direction:column;\
                    box-shadow:-4px 0 24px rgba(0,0,0,0.4);"
                on:click=|e| e.stop_propagation()
            >
                // Header
                <div
                    style="display:flex;align-items:center;justify-content:space-between;\
                        padding:24px;border-bottom:1px solid var(--border);flex-shrink:0;"
                >
                    <h2
                        style="font-size:16px;font-weight:600;color:var(--text);\
                            margin:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;"
                    >
                        {move || title_sv.get_value()}
                    </h2>
                    // Close button — 36px touch target (UI-SPEC)
                    <button
                        aria-label="Close panel"
                        on:click=handle_close
                        style="background:none;border:none;cursor:pointer;\
                            color:var(--text-muted);width:36px;height:36px;\
                            display:flex;align-items:center;justify-content:center;\
                            border-radius:4px;flex-shrink:0;\
                            transition:background-color 150ms ease,color 150ms ease;"
                    >
                        // X icon (Lucide)
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16" height="16"
                            viewBox="0 0 24 24"
                            fill="none" stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round" stroke-linejoin="round"
                        >
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                    </button>
                </div>

                // Body: loading → error → metadata + actions + preview
                <div style="flex:1;overflow-y:auto;padding:24px;">
                    <Show when=move || loading.get()>
                        <p
                            style="font-size:13px;font-family:'IBM Plex Mono',monospace;\
                                color:var(--text-muted);margin:0;"
                        >
                            "Loading\u{2026}"
                        </p>
                    </Show>

                    <Show when=move || !loading.get()>
                        {move || match metadata.get() {
                            // Metadata not yet set (shouldn't occur after open fires)
                            None => view! {
                                <p
                                    style="font-size:13px;font-family:'IBM Plex Mono',\
                                        monospace;color:var(--text-muted);margin:0;"
                                >
                                    "Loading\u{2026}"
                                </p>
                            }.into_any(),

                            // Error loading metadata
                            Some(Err(ref e)) => {
                                let msg = e.clone();
                                view! {
                                    <p style="font-size:13px;color:var(--danger);margin:0;">
                                        {format!("Error: {msg}")}
                                    </p>
                                }.into_any()
                            },

                            // Success: render metadata list + actions + inline preview
                            Some(Ok(ref detail)) => {
                                let d = detail.clone();
                                let size_label = format!(
                                    "{} ({} bytes)",
                                    fmt_size(d.size),
                                    d.size
                                );
                                let dl_bkt = bucket_sv.get_value();
                                let dl_key = object_key_sv.get_value();
                                let prev_bkt = bucket_sv.get_value();
                                let prev_key = object_key_sv.get_value();
                                let prev_ct = d.content_type.clone();
                                let prev_size = d.size;

                                view! {
                                    <div>
                                        // Metadata section (UI-SPEC Screen 3, IBM Plex Mono 13px)
                                        <div style="margin-bottom:24px;">
                                            // Full key
                                            <div style="margin-bottom:12px;">
                                                <div
                                                    style="font-size:12px;color:var(--text-muted);\
                                                        margin-bottom:4px;"
                                                >
                                                    "Key"
                                                </div>
                                                <div
                                                    style="font-size:13px;\
                                                        font-family:'IBM Plex Mono',monospace;\
                                                        color:var(--text);word-break:break-all;"
                                                >
                                                    {d.key.clone()}
                                                </div>
                                            </div>
                                            // Size (human-readable + bytes)
                                            <div style="margin-bottom:12px;">
                                                <div
                                                    style="font-size:12px;color:var(--text-muted);\
                                                        margin-bottom:4px;"
                                                >
                                                    "Size"
                                                </div>
                                                <div
                                                    style="font-size:13px;\
                                                        font-family:'IBM Plex Mono',monospace;\
                                                        color:var(--text);"
                                                >
                                                    {size_label}
                                                </div>
                                            </div>
                                            // Content-Type
                                            <div style="margin-bottom:12px;">
                                                <div
                                                    style="font-size:12px;color:var(--text-muted);\
                                                        margin-bottom:4px;"
                                                >
                                                    "Content-Type"
                                                </div>
                                                <div
                                                    style="font-size:13px;\
                                                        font-family:'IBM Plex Mono',monospace;\
                                                        color:var(--text);"
                                                >
                                                    {d.content_type.clone()}
                                                </div>
                                            </div>
                                            // ETag
                                            <div style="margin-bottom:12px;">
                                                <div
                                                    style="font-size:12px;color:var(--text-muted);\
                                                        margin-bottom:4px;"
                                                >
                                                    "ETag"
                                                </div>
                                                <div
                                                    style="font-size:13px;\
                                                        font-family:'IBM Plex Mono',monospace;\
                                                        color:var(--text);word-break:break-all;"
                                                >
                                                    {d.etag.clone()}
                                                </div>
                                            </div>
                                            // Last Modified (RFC3339)
                                            <div style="margin-bottom:12px;">
                                                <div
                                                    style="font-size:12px;color:var(--text-muted);\
                                                        margin-bottom:4px;"
                                                >
                                                    "Last Modified"
                                                </div>
                                                <div
                                                    style="font-size:13px;\
                                                        font-family:'IBM Plex Mono',monospace;\
                                                        color:var(--text);"
                                                >
                                                    {d.last_modified.clone()}
                                                </div>
                                            </div>
                                        </div>

                                        // Actions (UI-SPEC Screen 3): Download | Copy Presigned URL | Delete
                                        <div
                                            style="display:flex;flex-direction:column;\
                                                gap:8px;margin-bottom:24px;"
                                        >
                                            // Download (ghost button — link, browser download)
                                            <a
                                                href=format!("/ui/download/{dl_bkt}/{dl_key}")
                                                download=""
                                                style="display:inline-flex;align-items:center;\
                                                    gap:6px;background:none;\
                                                    border:1px solid var(--border);\
                                                    color:var(--text);border-radius:4px;\
                                                    padding:8px 12px;font-size:14px;\
                                                    text-decoration:none;cursor:pointer;\
                                                    transition:background-color 150ms ease,\
                                                    border-color 150ms ease;"
                                            >
                                                // Download icon (Lucide)
                                                <svg
                                                    xmlns="http://www.w3.org/2000/svg"
                                                    width="14" height="14"
                                                    viewBox="0 0 24 24"
                                                    fill="none" stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    aria-hidden="true"
                                                >
                                                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                                                    <polyline points="7 10 12 15 17 10"/>
                                                    <line x1="12" y1="15" x2="12" y2="3"/>
                                                </svg>
                                                "Download"
                                            </a>

                                            // Copy Presigned URL (calls presign_fn server-side,
                                            // URL-string-only received in WASM — T-04-09A)
                                            <div>
                                                <button
                                                    on:click=handle_presign
                                                    disabled=move || presigning.get()
                                                    style="display:inline-flex;align-items:center;\
                                                        gap:6px;background:none;\
                                                        border:1px solid var(--border);\
                                                        color:var(--text);border-radius:4px;\
                                                        padding:8px 12px;font-size:14px;\
                                                        cursor:pointer;width:100%;\
                                                        transition:background-color 150ms ease,\
                                                        border-color 150ms ease;"
                                                >
                                                    // Link icon (Lucide)
                                                    <svg
                                                        xmlns="http://www.w3.org/2000/svg"
                                                        width="14" height="14"
                                                        viewBox="0 0 24 24"
                                                        fill="none" stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                        stroke-linejoin="round"
                                                        aria-hidden="true"
                                                    >
                                                        <path d="M10 13a5 5 0 0 0 7.54.54l3-3\
                                                            a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
                                                        <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3\
                                                            a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
                                                    </svg>
                                                    {move || if presigning.get() {
                                                        "Copying\u{2026}"
                                                    } else {
                                                        "Copy Presigned URL"
                                                    }}
                                                </button>
                                                // Affordance: "Presigned URL copied (expires in 15 min)"
                                                // (UI-SPEC Copywriting Contract, --warn color)
                                                <Show when=move || presign_copied.get()>
                                                    <p
                                                        style="font-size:12px;color:var(--warn);\
                                                            margin:6px 0 0 0;"
                                                    >
                                                        "Presigned URL copied (expires in 15 min)"
                                                    </p>
                                                </Show>
                                            </div>

                                            // Delete (ghost, destructive color — opens inline confirm)
                                            <button
                                                on:click=handle_delete_open
                                                style="display:inline-flex;align-items:center;\
                                                    gap:6px;background:none;\
                                                    border:1px solid var(--danger-bd);\
                                                    color:var(--danger);border-radius:4px;\
                                                    padding:8px 12px;font-size:14px;\
                                                    cursor:pointer;\
                                                    transition:background-color 150ms ease,\
                                                    border-color 150ms ease;"
                                            >
                                                // Trash icon (Lucide)
                                                <svg
                                                    xmlns="http://www.w3.org/2000/svg"
                                                    width="14" height="14"
                                                    viewBox="0 0 24 24"
                                                    fill="none" stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    aria-hidden="true"
                                                >
                                                    <polyline points="3 6 5 6 21 6"/>
                                                    <path d="M19 6l-1 14a2 2 0 0 1-2 2H8\
                                                        a2 2 0 0 1-2-2L5 6"/>
                                                    <path d="M10 11v6"/>
                                                    <path d="M14 11v6"/>
                                                    <path d="M9 6V4a1 1 0 0 1 1-1h4\
                                                        a1 1 0 0 1 1 1v2"/>
                                                </svg>
                                                "Delete"
                                            </button>
                                        </div>

                                        // Inline preview (UI-SPEC Screen 3, size-gated — T-04-09C)
                                        <div style="border-top:1px solid var(--border);padding-top:16px;">
                                            <crate::components::InlinePreview
                                                bucket=prev_bkt
                                                key=prev_key
                                                content_type=prev_ct
                                                size=prev_size
                                            />
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }}
                    </Show>
                </div>
            </div>

            // Inline delete-confirmation modal (inside the island — T-04-09D: single hydration root).
            <Show when=move || delete_confirm_open.get()>
                <div
                    style="position:fixed;inset:0;background:rgba(0,0,0,0.6);z-index:600;\
                        display:flex;align-items:center;justify-content:center;"
                    on:click=handle_delete_dismiss
                >
                    <div
                        style="background:var(--surface);border:1px solid var(--border);\
                            border-radius:8px;padding:32px;max-width:440px;width:100%;\
                            margin:0 16px;z-index:601;\
                            box-shadow:0 8px 32px rgba(0,0,0,0.5);"
                        on:click=|e| e.stop_propagation()
                    >
                        <h2
                            style="font-size:16px;font-weight:600;color:var(--text);\
                                margin:0 0 12px 0;line-height:1.3;"
                        >
                            {move || format!("Delete \"{}\"?", title_sv.get_value())}
                        </h2>
                        <p
                            style="font-size:14px;color:var(--text-muted);\
                                margin:0 0 24px 0;line-height:1.5;"
                        >
                            "This action cannot be undone."
                        </p>
                        <div style="display:flex;gap:8px;justify-content:flex-end;">
                            <button
                                on:click=handle_delete_dismiss
                                disabled=move || deleting.get()
                                style="background:none;border:1px solid var(--border);\
                                    color:var(--text);border-radius:4px;padding:8px 16px;\
                                    font-size:14px;cursor:pointer;\
                                    transition:background-color 150ms ease,\
                                    border-color 150ms ease;"
                            >
                                "Keep File"
                            </button>
                            <button
                                on:click=handle_delete_confirm
                                disabled=move || deleting.get()
                                style="background:var(--danger);border:1px solid var(--danger);\
                                    color:#fff;border-radius:4px;padding:8px 16px;\
                                    font-size:14px;cursor:pointer;font-weight:600;\
                                    transition:background-color 150ms ease,\
                                    border-color 150ms ease;"
                            >
                                {move || if deleting.get() { "Deleting\u{2026}" } else { "Delete Object" }}
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        </Show>
    }
}
