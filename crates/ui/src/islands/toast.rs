//! Toast island — bottom-center auto-dismiss notification stack.
//!
//! - Max 4 visible simultaneously.
//! - Success toasts auto-dismiss after 3s.
//! - Error toasts persist until the user dismisses them.
//! - Status icon colors: success=`--success`, error=`--danger`, info/copy=`--accent`.
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
            style="position:fixed;left:50%;bottom:24px;transform:translateX(-50%);\
                z-index:60;display:flex;flex-direction:column;gap:9px;\
                align-items:center;pointer-events:none;"
        >
            <For
                each=move || take_four(items.get())
                key=|t| t.id
                children=move |toast| {
                    let id = toast.id;
                    let is_success = toast.kind == ToastKind::Success;
                    let icon_color = match &toast.kind {
                        ToastKind::Success => "var(--success)",
                        ToastKind::Error => "var(--danger)",
                        ToastKind::Info => "var(--accent)",
                    };
                    let kind = toast.kind.clone();
                    let msg = toast.message.clone();

                    if is_success {
                        schedule_dismiss(id, set_items);
                    }

                    view! {
                        <div
                            style="display:flex;align-items:center;gap:9px;\
                                padding:10px 15px;background:var(--panel);\
                                border:1px solid var(--border-2);border-radius:9px;\
                                box-shadow:var(--shadow-sm);animation:toastIn .2s ease;\
                                font-size:13px;color:var(--text);pointer-events:auto;"
                        >
                            <span style=format!(
                                "width:16px;height:16px;flex:none;display:flex;\
                                align-items:center;justify-content:center;color:{icon_color};"
                            )>
                                {match kind {
                                    ToastKind::Success => view! {
                                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3.5 8.2 6.5 11l6-6.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
                                    }.into_any(),
                                    ToastKind::Error => view! {
                                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 1.7 14.5 13H1.5L8 1.7Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/><path d="M8 6v3M8 11h.01" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
                                    }.into_any(),
                                    ToastKind::Info => view! {
                                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6.3" stroke="currentColor" stroke-width="1.2"/><path d="M8 7.3v3.4M8 5h.01" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
                                    }.into_any(),
                                }}
                            </span>
                            <span style="flex:1;">{msg}</span>
                            <button
                                aria-label="Dismiss notification"
                                on:click=move |_| dismiss(id)
                                style="width:24px;height:24px;flex:none;display:flex;\
                                    align-items:center;justify-content:center;border:none;\
                                    border-radius:5px;background:transparent;\
                                    color:var(--faint);cursor:pointer;"
                            >
                                <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}
