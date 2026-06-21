//! ConfirmModal island — centered destructive-action confirmation overlay.
//!
//! Used for: delete bucket, delete object.
//! The island manages its own open/close state.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

/// Which delete action this modal performs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConfirmAction {
    DeleteBucket,
    DeleteObject,
}

/// ConfirmModal island.
///
/// Props (all serializable):
/// - `action`: which action to confirm.
/// - `name`: the bucket/object name shown in the title.
/// - `bucket`: bucket name.
/// - `object_key`: object key (empty for bucket delete).
/// - `aria_label`: accessible label for the icon-only trigger button (UI-SPEC accessibility).
///   Pass an empty string to omit the attribute.
#[island]
pub fn ConfirmModal(
    action: ConfirmAction,
    name: String,
    bucket: String,
    object_key: String,
    aria_label: String,
) -> impl IntoView {
    let (open, set_open) = signal(false);
    let (loading, set_loading) = signal(false);

    let action = StoredValue::new(action);
    let name = StoredValue::new(name);
    let bucket = StoredValue::new(bucket);
    let object_key = StoredValue::new(object_key);
    let aria_label = StoredValue::new(aria_label);

    let title = move || {
        let n = name.get_value();
        match action.get_value() {
            ConfirmAction::DeleteBucket => format!("Delete bucket \"{n}\"?"),
            ConfirmAction::DeleteObject => format!("Delete \"{n}\"?"),
        }
    };
    let message = move || match action.get_value() {
        ConfirmAction::DeleteBucket => {
            "This action cannot be undone. All objects inside will be lost.".to_string()
        }
        ConfirmAction::DeleteObject => "This action cannot be undone.".to_string(),
    };
    let confirm_label = move || match action.get_value() {
        ConfirmAction::DeleteBucket => "Delete Bucket".to_string(),
        ConfirmAction::DeleteObject => "Delete Object".to_string(),
    };
    let dismiss_label = move || match action.get_value() {
        ConfirmAction::DeleteBucket => "Keep Bucket".to_string(),
        ConfirmAction::DeleteObject => "Keep File".to_string(),
    };

    let handle_open = move |_| set_open.set(true);
    let handle_dismiss = move |_| set_open.set(false);
    let handle_backdrop = move |_| set_open.set(false);

    let handle_confirm = move |_| {
        set_loading.set(true);
        let bkt = bucket.get_value();
        let k = object_key.get_value();
        let act = action.get_value();
        spawn_local(async move {
            let result = match act {
                ConfirmAction::DeleteBucket => {
                    crate::server_fns::buckets::delete_bucket_fn(bkt).await
                }
                ConfirmAction::DeleteObject => {
                    crate::server_fns::objects::delete_object_fn(bkt, k).await
                }
            };
            set_loading.set(false);
            if result.is_ok() {
                set_open.set(false);
                #[cfg(feature = "hydrate")]
                if let Some(window) = web_sys::window() {
                    let _ = window.location().reload();
                }
            }
        });
    };

    view! {
        // Scoped hover states for the row trash trigger + modal footer buttons (template style-hover)
        <style>
            ".fb-confirm-trigger:hover{background:var(--danger-dim);color:var(--danger)}\
             .fb-confirm-cancel:hover{background:var(--hover)}\
             .fb-confirm-yes:hover{filter:brightness(1.1)}"
        </style>

        // Delete trigger button (row action — borderless ghost; aria-label required for accessibility)
        <button
            class="fb-confirm-trigger"
            on:click=handle_open
            aria-label=move || {
                let lbl = aria_label.get_value();
                if lbl.is_empty() { None } else { Some(lbl) }
            }
            style="width:28px;height:28px;flex:none;display:flex;align-items:center;justify-content:center;\
                border:none;border-radius:6px;background:transparent;color:var(--faint);cursor:pointer"
        >
            // Trash icon (template inline svg)
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path
                    d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5"
                    stroke="currentColor"
                    stroke-width="1.2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
            </svg>
        </button>

        <Show when=move || open.get()>
            // Overlay
            <div
                style="position:fixed;inset:0;background:rgba(0,0,0,.45);display:flex;align-items:center;\
                    justify-content:center;animation:overlayIn .15s ease;z-index:50"
                on:click=handle_backdrop
            >
                // Dialog
                <div
                    style="width:420px;max-width:92vw;background:var(--panel);border:1px solid var(--border-2);\
                        border-radius:12px;box-shadow:var(--shadow);animation:modalIn .18s cubic-bezier(.2,.7,.3,1)"
                    on:click=|e| e.stop_propagation()
                >
                    <div style="display:flex;gap:13px;padding:20px 20px 16px">
                        // Danger icon tile
                        <div style="width:34px;height:34px;flex:none;border-radius:9px;background:var(--danger-dim);\
                            display:flex;align-items:center;justify-content:center;color:var(--danger)">
                            <svg width="17" height="17" viewBox="0 0 16 16" fill="none">
                                <path d="M8 1.5 15 14H1z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>
                                <path d="M8 6v3.4M8 11.4v.1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
                            </svg>
                        </div>
                        <div>
                            <div style="font-size:16px;font-weight:600;letter-spacing:-.2px">
                                {title}
                            </div>
                            <div style="font-size:13px;color:var(--dim);margin-top:4px;line-height:1.5">
                                {message}
                            </div>
                        </div>
                    </div>
                    <div style="display:flex;justify-content:flex-end;gap:9px;padding:14px 20px;border-top:1px solid var(--border)">
                        <button
                            class="fb-confirm-cancel"
                            on:click=handle_dismiss
                            disabled=move || loading.get()
                            style="padding:8px 15px;border:1px solid var(--border-2);border-radius:7px;\
                                background:transparent;color:var(--text);font-family:inherit;\
                                font-size:13px;font-weight:500;cursor:pointer"
                        >
                            {dismiss_label}
                        </button>
                        <button
                            class="fb-confirm-yes"
                            on:click=handle_confirm
                            disabled=move || loading.get()
                            style="padding:8px 15px;border:none;border-radius:7px;background:var(--danger);\
                                color:#fff;font-family:inherit;font-size:13px;font-weight:600;cursor:pointer"
                        >
                            {confirm_label}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
