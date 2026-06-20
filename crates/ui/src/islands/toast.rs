//! Toast island — top-right auto-dismiss notification stack.
//!
//! - Max 4 visible simultaneously.
//! - Success toasts auto-dismiss after 3s.
//! - Error toasts persist until the user dismisses them.
//! - Left border colors: success=`--success`, error=`--destructive`, info/copy=`--accent`.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Toast variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

/// A single toast entry.
#[derive(Clone, Debug)]
pub struct ToastItem {
    pub id: u32,
    pub kind: ToastKind,
    pub message: String,
}

/// Auto-dismiss helper — kept outside view! to avoid cfg parse issues.
#[cfg(feature = "hydrate")]
fn schedule_dismiss(id: u32, set_items: WriteSignal<Vec<ToastItem>>) {
    let _ = leptos::leptos_dom::helpers::set_timeout(
        move || {
            set_items.update(|v| v.retain(|t| t.id != id));
        },
        std::time::Duration::from_millis(3000),
    );
}

#[cfg(not(feature = "hydrate"))]
fn schedule_dismiss(_id: u32, _set_items: WriteSignal<Vec<ToastItem>>) {}

/// Take up to 4 items from the list.
fn take_four(items: Vec<ToastItem>) -> Vec<ToastItem> {
    items.into_iter().take(4).collect()
}

/// Toast island.
///
/// No props. Provides its `WriteSignal<Vec<ToastItem>>` via context so
/// other islands can fire toasts.
#[island]
pub fn Toast() -> impl IntoView {
    let (items, set_items) = signal::<Vec<ToastItem>>(Vec::new());

    // Provide push-signal via context.
    provide_context(set_items);

    let dismiss = move |id: u32| {
        set_items.update(|v| v.retain(|t| t.id != id));
    };

    view! {
        <div
            id="toast-stack"
            style="position:fixed;top:16px;right:16px;z-index:1000;\
                display:flex;flex-direction:column;gap:8px;max-width:360px;"
        >
            <For
                each=move || take_four(items.get())
                key=|t| t.id
                children=move |toast| {
                    let id = toast.id;
                    let is_success = toast.kind == ToastKind::Success;
                    let border_color = match &toast.kind {
                        ToastKind::Success => "var(--success)",
                        ToastKind::Error => "var(--destructive)",
                        ToastKind::Info => "var(--accent)",
                    };
                    let msg = toast.message.clone();

                    if is_success {
                        schedule_dismiss(id, set_items);
                    }

                    view! {
                        <div
                            style=format!(
                                "background:var(--surface);border:1px solid var(--border);\
                                border-left:3px solid {border_color};border-radius:4px;\
                                padding:12px 16px;display:flex;align-items:flex-start;\
                                gap:8px;font-size:14px;color:var(--text);\
                                box-shadow:0 2px 8px rgba(0,0,0,0.4);"
                            )
                        >
                            <span style="flex:1;">{msg}</span>
                            <button
                                aria-label="Dismiss notification"
                                on:click=move |_| dismiss(id)
                                style="background:none;border:none;cursor:pointer;\
                                    color:var(--text-muted);font-size:16px;line-height:1;\
                                    padding:0;flex-shrink:0;"
                            >
                                {"\u{00d7}"}
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}
