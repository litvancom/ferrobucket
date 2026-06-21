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

/// Render a single metadata label/value row inside the bordered list.
/// `copyable` rows get a ghost copy icon-button that writes the value to the clipboard.
fn meta_row(label: &'static str, value: String, copyable: bool) -> impl IntoView {
    // StoredValue is Copy, so the click handler stays `Fn` (required by <Show> children).
    let copy_value = StoredValue::new(value.clone());
    let on_copy = move |_| {
        let _ = copy_value;
        #[cfg(feature = "hydrate")]
        {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&copy_value.get_value());
            }
        }
    };
    view! {
        <div
            style="display:flex;align-items:flex-start;gap:12px;\
                padding:10px 13px;border-bottom:1px solid var(--border);"
        >
            <div style="font-size:12px;color:var(--faint);width:96px;flex:none;">
                {label}
            </div>
            <div
                style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                    color:var(--text);word-break:break-all;flex:1;"
            >
                {value}
            </div>
            <Show when=move || copyable>
                <button
                    on:click=on_copy
                    class="so-copy-meta"
                    style="flex:none;width:24px;height:24px;display:flex;\
                        align-items:center;justify-content:center;border:none;\
                        border-radius:5px;background:transparent;color:var(--faint);\
                        cursor:pointer;transition:background-color 150ms ease,color 150ms ease;"
                >
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                        <rect
                            x="5" y="5" width="8" height="8" rx="1.4"
                            stroke="currentColor" stroke-width="1.2"
                        />
                        <path
                            d="M3 11V4.4C3 3.6 3.6 3 4.4 3H11"
                            stroke="currentColor" stroke-width="1.2"
                        />
                    </svg>
                </button>
            </Show>
        </div>
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
        // Scoped hover styles (template uses style-hover="..." — reproduced via CSS classes).
        <style>
            ".so-close:hover{background:var(--hover);color:var(--text)}\
             .so-copy-meta:hover{background:var(--hover);color:var(--text)}\
             .so-gen:hover{border-color:var(--accent-bd);color:var(--text)}\
             .so-download:hover{border-color:var(--accent-bd);background:var(--surface-2)}\
             .so-delete:hover{background:var(--danger);color:#fff}"
        </style>

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
            // Backdrop scrim (closes panel on click outside)
            <div
                style="position:fixed;inset:0;background:rgba(0,0,0,.42);\
                    animation:overlayIn .15s ease;z-index:40;"
                on:click=handle_backdrop
            />

            // Slide-over panel: 420px, right-anchored, full height.
            <div
                style="position:fixed;top:0;right:0;bottom:0;width:420px;max-width:92vw;\
                    background:var(--panel);border-left:1px solid var(--border);\
                    box-shadow:var(--shadow);display:flex;flex-direction:column;\
                    animation:panelIn .2s cubic-bezier(.2,.7,.3,1);z-index:41;"
                on:click=|e| e.stop_propagation()
            >
                // Header: icon tile + mono basename + type + close button
                <div
                    style="display:flex;align-items:flex-start;gap:10px;\
                        padding:16px 18px;border-bottom:1px solid var(--border);flex-shrink:0;"
                >
                    // Icon tile (file icon)
                    <div
                        style="width:30px;height:30px;flex:none;border-radius:7px;\
                            background:var(--surface-2);display:flex;align-items:center;\
                            justify-content:center;color:var(--accent);"
                    >
                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                            <path
                                d="M4 2h5l3 3v9H4z"
                                stroke="currentColor" stroke-width="1.2"
                                stroke-linecap="round" stroke-linejoin="round"
                            />
                            <path
                                d="M9 2v3h3"
                                stroke="currentColor" stroke-width="1.2"
                                stroke-linecap="round" stroke-linejoin="round"
                            />
                        </svg>
                    </div>
                    <div style="min-width:0;flex:1;">
                        <div
                            style="font-family:'IBM Plex Mono',monospace;font-size:13px;\
                                font-weight:600;word-break:break-all;line-height:1.4;\
                                color:var(--text);"
                        >
                            {move || title_sv.get_value()}
                        </div>
                        <div style="font-size:11.5px;color:var(--faint);margin-top:2px;">
                            "Object"
                        </div>
                    </div>
                    // Close button (ghost icon-button)
                    <button
                        aria-label="Close panel"
                        on:click=handle_close
                        class="so-close"
                        style="width:28px;height:28px;flex:none;display:flex;\
                            align-items:center;justify-content:center;border:none;\
                            border-radius:6px;background:transparent;color:var(--faint);\
                            cursor:pointer;transition:background-color 150ms ease,color 150ms ease;"
                    >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                            <path
                                d="M4 4l8 8M12 4l-8 8"
                                stroke="currentColor" stroke-width="1.4" stroke-linecap="round"
                            />
                        </svg>
                    </button>
                </div>

                // Body: loading → error → preview + metadata + presigned URL
                <div style="flex:1;overflow-y:auto;padding:18px;">
                    <Show when=move || loading.get()>
                        <p
                            style="font-size:13px;font-family:'IBM Plex Mono',monospace;\
                                color:var(--faint);margin:0;"
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
                                        monospace;color:var(--faint);margin:0;"
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

                            // Success: render preview card + metadata list + presigned URL
                            Some(Ok(ref detail)) => {
                                let d = detail.clone();
                                let size_label = format!(
                                    "{} ({} bytes)",
                                    fmt_size(d.size),
                                    d.size
                                );
                                let prev_bkt = bucket_sv.get_value();
                                let prev_key = object_key_sv.get_value();
                                let prev_ct = d.content_type.clone();
                                let prev_size = d.size;
                                let row_key = d.key.clone();
                                let row_ct = d.content_type.clone();
                                let row_etag = d.etag.clone();
                                // Date + minute ("2026-06-20 18:10") to match the design template.
                                let row_lm = d
                                    .last_modified
                                    .get(..16)
                                    .map(|s| s.replace('T', " "))
                                    .unwrap_or_else(|| d.last_modified.clone());

                                view! {
                                    <div>
                                        // Preview card (template: bordered card first)
                                        <div
                                            style="border:1px solid var(--border);border-radius:9px;\
                                                overflow:hidden;margin-bottom:18px;\
                                                background:var(--surface);padding:14px;"
                                        >
                                            <crate::components::InlinePreview
                                                bucket=prev_bkt
                                                key=prev_key
                                                content_type=prev_ct
                                                size=prev_size
                                            />
                                        </div>

                                        // Metadata section label
                                        <div
                                            style="font-size:11px;font-weight:600;\
                                                letter-spacing:.4px;color:var(--faint);\
                                                text-transform:uppercase;margin-bottom:10px;"
                                        >
                                            "Metadata"
                                        </div>
                                        // Bordered list of label/value rows
                                        <div
                                            style="border:1px solid var(--border);border-radius:9px;\
                                                background:var(--surface);overflow:hidden;"
                                        >
                                            {meta_row("Key", row_key, true)}
                                            {meta_row("Size", size_label, false)}
                                            {meta_row("Content-Type", row_ct, false)}
                                            {meta_row("ETag", row_etag, true)}
                                            {meta_row("Last Modified", row_lm, false)}
                                        </div>

                                        // Presigned URL section label
                                        <div
                                            style="font-size:11px;font-weight:600;\
                                                letter-spacing:.4px;color:var(--faint);\
                                                text-transform:uppercase;margin:18px 0 10px;"
                                        >
                                            "Presigned URL"
                                        </div>
                                        // Either the copied affordance or the dashed generate button.
                                        <Show
                                            when=move || presign_copied.get()
                                            fallback=move || view! {
                                                <button
                                                    on:click=handle_presign
                                                    disabled=move || presigning.get()
                                                    class="so-gen"
                                                    style="width:100%;display:flex;align-items:center;\
                                                        justify-content:center;gap:7px;padding:9px;\
                                                        border:1px dashed var(--border-2);\
                                                        border-radius:7px;background:transparent;\
                                                        color:var(--dim);font-family:inherit;\
                                                        font-size:12.5px;font-weight:500;cursor:pointer;\
                                                        transition:border-color 150ms ease,color 150ms ease;"
                                                >
                                                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                                                        <path
                                                            d="M6.5 9.5 9.5 6.5M7 5l1.3-1.3a2.5 2.5 0 0 1 3.5 3.5L10.5 8.5M9 11l-1.3 1.3a2.5 2.5 0 0 1-3.5-3.5L5.5 7.5"
                                                            stroke="currentColor" stroke-width="1.2"
                                                            stroke-linecap="round"
                                                        />
                                                    </svg>
                                                    {move || if presigning.get() {
                                                        "Generating\u{2026}"
                                                    } else {
                                                        "Generate presigned URL"
                                                    }}
                                                </button>
                                            }
                                        >
                                            <div>
                                                // Accent copy field (URL is in clipboard; show success row)
                                                <div
                                                    style="display:flex;align-items:center;gap:6px;\
                                                        border:1px solid var(--accent-bd);border-radius:7px;\
                                                        background:var(--bg);padding:4px 4px 4px 11px;"
                                                >
                                                    <span
                                                        style="flex:1;min-width:0;\
                                                            font-family:'IBM Plex Mono',monospace;\
                                                            font-size:11.5px;color:var(--dim);\
                                                            white-space:nowrap;overflow:hidden;\
                                                            text-overflow:ellipsis;"
                                                    >
                                                        "Presigned URL copied to clipboard"
                                                    </span>
                                                    <button
                                                        on:click=handle_presign
                                                        style="flex:none;display:flex;align-items:center;\
                                                            gap:6px;padding:6px 10px;border:none;\
                                                            border-radius:5px;background:var(--accent);\
                                                            color:#fff;font-family:inherit;font-size:12px;\
                                                            font-weight:600;cursor:pointer;"
                                                    >
                                                        <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                                                            <rect
                                                                x="5" y="5" width="8" height="8" rx="1.4"
                                                                stroke="#fff" stroke-width="1.3"
                                                            />
                                                            <path
                                                                d="M3 11V4.4C3 3.6 3.6 3 4.4 3H11"
                                                                stroke="#fff" stroke-width="1.3"
                                                            />
                                                        </svg>
                                                        "Copy"
                                                    </button>
                                                </div>
                                                // Expiry warning note
                                                <div
                                                    style="font-size:11.5px;color:var(--warn);\
                                                        margin-top:7px;display:flex;align-items:center;gap:6px;"
                                                >
                                                    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                                                        <circle
                                                            cx="8" cy="8" r="6"
                                                            stroke="currentColor" stroke-width="1.2"
                                                        />
                                                        <path
                                                            d="M8 4.8V8l2.2 1.4"
                                                            stroke="currentColor" stroke-width="1.2"
                                                            stroke-linecap="round"
                                                        />
                                                    </svg>
                                                    "Expires in 15 minutes"
                                                </div>
                                            </div>
                                        </Show>
                                    </div>
                                }.into_any()
                            }
                        }}
                    </Show>
                </div>

                // Footer: Download (secondary) + Delete (danger tinted)
                <div
                    style="display:flex;gap:9px;padding:14px 18px;\
                        border-top:1px solid var(--border);flex-shrink:0;"
                >
                    // Download (secondary button — link, browser download)
                    <a
                        href=move || {
                            let b = bucket_sv.get_value();
                            let k = object_key_sv.get_value();
                            format!("/ui/download/{b}/{k}")
                        }
                        download=""
                        class="so-download"
                        style="flex:1;display:flex;align-items:center;justify-content:center;\
                            gap:7px;padding:9px;border:1px solid var(--border-2);\
                            border-radius:7px;background:var(--surface);color:var(--text);\
                            font-family:inherit;font-size:13px;font-weight:500;cursor:pointer;\
                            text-decoration:none;\
                            transition:background-color 150ms ease,border-color 150ms ease;"
                    >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                            <path
                                d="M8 2.5v8M4.8 7.5 8 10.7l3.2-3.2M3 13h10"
                                stroke="currentColor" stroke-width="1.2"
                                stroke-linecap="round" stroke-linejoin="round"
                            />
                        </svg>
                        "Download"
                    </a>
                    // Delete (danger tinted — opens inline confirm)
                    <button
                        on:click=handle_delete_open
                        class="so-delete"
                        style="display:flex;align-items:center;justify-content:center;\
                            gap:7px;padding:9px 16px;border:1px solid var(--danger-bd);\
                            border-radius:7px;background:var(--danger-dim);color:var(--danger);\
                            font-family:inherit;font-size:13px;font-weight:500;cursor:pointer;\
                            transition:background-color 150ms ease,color 150ms ease;"
                    >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                            <path
                                d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5"
                                stroke="currentColor" stroke-width="1.2"
                                stroke-linecap="round" stroke-linejoin="round"
                            />
                        </svg>
                        "Delete"
                    </button>
                </div>
            </div>

            // Inline delete-confirmation modal (inside the island — T-04-09D: single hydration root).
            <Show when=move || delete_confirm_open.get()>
                <div
                    style="position:fixed;inset:0;background:rgba(0,0,0,.45);\
                        display:flex;align-items:center;justify-content:center;\
                        animation:overlayIn .15s ease;z-index:600;"
                    on:click=handle_delete_dismiss
                >
                    <div
                        style="width:420px;max-width:92vw;background:var(--panel);\
                            border:1px solid var(--border-2);border-radius:12px;\
                            box-shadow:var(--shadow);margin:0 16px;z-index:601;\
                            animation:modalIn .18s cubic-bezier(.2,.7,.3,1);"
                        on:click=|e| e.stop_propagation()
                    >
                        <div style="padding:18px 20px 4px;">
                            <div
                                style="font-size:16px;font-weight:600;letter-spacing:-.2px;\
                                    color:var(--text);line-height:1.3;"
                            >
                                {move || format!("Delete \"{}\"?", title_sv.get_value())}
                            </div>
                            <div
                                style="font-size:12.5px;color:var(--faint);\
                                    margin-top:3px;line-height:1.5;"
                            >
                                "This action cannot be undone."
                            </div>
                        </div>
                        <div
                            style="display:flex;justify-content:flex-end;gap:9px;\
                                padding:14px 20px;border-top:1px solid var(--border);margin-top:14px;"
                        >
                            <button
                                on:click=handle_delete_dismiss
                                disabled=move || deleting.get()
                                style="padding:8px 15px;border:1px solid var(--border-2);\
                                    border-radius:7px;background:transparent;color:var(--text);\
                                    font-family:inherit;font-size:13px;font-weight:500;cursor:pointer;"
                            >
                                "Keep File"
                            </button>
                            <button
                                on:click=handle_delete_confirm
                                disabled=move || deleting.get()
                                style="padding:8px 15px;border:none;border-radius:7px;\
                                    background:var(--danger);color:#fff;font-family:inherit;\
                                    font-size:13px;font-weight:600;cursor:pointer;"
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
