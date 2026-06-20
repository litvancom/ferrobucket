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
        // Delete trigger button (destructive color, icon-only — aria-label required for accessibility)
        <button
            on:click=handle_open
            aria-label=move || {
                let lbl = aria_label.get_value();
                if lbl.is_empty() { None } else { Some(lbl) }
            }
            style="background:none;border:none;cursor:pointer;\
                color:var(--destructive);font-size:14px;padding:4px 8px;\
                border-radius:4px;transition:background-color 150ms ease;"
        >
            // Trash icon (Lucide)
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
            >
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
                <path d="M10 11v6" />
                <path d="M14 11v6" />
                <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
            </svg>
        </button>

        <Show when=move || open.get()>
            // Backdrop
            <div
                style="position:fixed;inset:0;background:rgba(0,0,0,0.6);z-index:500;\
                    display:flex;align-items:center;justify-content:center;"
                on:click=handle_backdrop
            >
                // Modal panel
                <div
                    style="background:var(--surface);border:1px solid var(--border);\
                        border-radius:8px;padding:32px;max-width:440px;width:100%;\
                        margin:0 16px;z-index:501;box-shadow:0 8px 32px rgba(0,0,0,0.5);"
                    on:click=|e| e.stop_propagation()
                >
                    <h2 style="font-size:16px;font-weight:600;color:var(--text);margin:0 0 12px 0;line-height:1.3;">
                        {title}
                    </h2>
                    <p style="font-size:14px;color:var(--text-muted);margin:0 0 24px 0;line-height:1.5;">
                        {message}
                    </p>
                    <div style="display:flex;gap:8px;justify-content:flex-end;">
                        <button
                            on:click=handle_dismiss
                            disabled=move || loading.get()
                            style="background:none;border:1px solid var(--border);\
                                color:var(--text);border-radius:4px;padding:8px 16px;\
                                font-size:14px;cursor:pointer;\
                                transition:background-color 150ms ease,border-color 150ms ease;"
                        >
                            {dismiss_label}
                        </button>
                        <button
                            on:click=handle_confirm
                            disabled=move || loading.get()
                            style="background:var(--destructive);border:1px solid var(--destructive);\
                                color:#fff;border-radius:4px;padding:8px 16px;\
                                font-size:14px;cursor:pointer;font-weight:600;\
                                transition:background-color 150ms ease,border-color 150ms ease;"
                        >
                            {confirm_label}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
