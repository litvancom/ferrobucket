//! CreateBucketModal island — bucket name input with server-fn submit.
//!
//! On submit calls `create_bucket_fn`. On `Err`, renders the error string inline
//! below the field. The reserved-name message "This name is reserved by the server.
//! Choose a different name." flows through verbatim from D-02 / UI-SPEC Copywriting Contract.
//!
//! `create_bucket_fn` is the server fn from Plan 02; it is callable from islands
//! because the #[server] macro routes calls over HTTP.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::server_fns::buckets::create_bucket_fn;

/// CreateBucketModal island.
///
/// No required props — the island renders a "Create Bucket" trigger button
/// and manages the modal state internally.
#[island]
pub fn CreateBucketModal() -> impl IntoView {
    let (open, set_open) = signal(false);
    let (name, set_name) = signal(String::new());
    let (error, set_error) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);

    let handle_open = move |_| {
        set_name.set(String::new());
        set_error.set(None);
        set_open.set(true);
    };

    let handle_dismiss = move |_| {
        set_name.set(String::new());
        set_error.set(None);
        set_open.set(false);
    };

    let handle_submit = move |e: web_sys::SubmitEvent| {
        e.prevent_default();
        let bucket_name = name.get_untracked();
        if bucket_name.trim().is_empty() {
            set_error.set(Some("Bucket name cannot be empty.".to_string()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match create_bucket_fn(bucket_name).await {
                Ok(()) => {
                    set_loading.set(false);
                    set_name.set(String::new());
                    set_open.set(false);
                    // Reload to show the new bucket in the table and sidebar.
                    #[cfg(feature = "hydrate")]
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(e) => {
                    set_loading.set(false);
                    // Surface the error verbatim (D-02 reserved-name message flows through).
                    set_error.set(Some(e.to_string()));
                }
            }
        });
    };

    let handle_backdrop = move |_| {
        set_name.set(String::new());
        set_error.set(None);
        set_open.set(false);
    };

    view! {
        // "Create Bucket" trigger button (accent fill — UI-SPEC primary CTA)
        <button
            on:click=handle_open
            id="create-bucket-btn"
            style="background:var(--accent);border:1px solid var(--accent);\
                color:#fff;border-radius:4px;padding:8px 16px;\
                font-size:14px;cursor:pointer;font-weight:600;\
                transition:background-color 150ms ease,border-color 150ms ease;"
        >
            "Create Bucket"
        </button>

        // Modal
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
                    <h2 style="font-size:16px;font-weight:600;color:var(--text);margin:0 0 16px 0;">
                        "Create Bucket"
                    </h2>
                    <form on:submit=handle_submit>
                        <div style="margin-bottom:16px;">
                            <input
                                type="text"
                                placeholder="my-bucket"
                                prop:value=move || name.get()
                                on:input=move |e| set_name.set(event_target_value(&e))
                                disabled=move || loading.get()
                                style="width:100%;box-sizing:border-box;\
                                    background:var(--bg);border:1px solid var(--border);\
                                    color:var(--text);border-radius:4px;\
                                    padding:8px 12px;font-size:14px;font-family:inherit;\
                                    outline:none;\
                                    transition:border-color 150ms ease;"
                            />
                            // Inline error (D-02 reserved-name message flows through verbatim)
                            <Show when=move || error.get().is_some()>
                                <p style="margin:6px 0 0 0;font-size:12px;color:var(--destructive);">
                                    {move || error.get().unwrap_or_default()}
                                </p>
                            </Show>
                        </div>
                        <div style="display:flex;gap:8px;justify-content:flex-end;">
                            <button
                                type="button"
                                on:click=handle_dismiss
                                disabled=move || loading.get()
                                style="background:none;border:1px solid var(--border);\
                                    color:var(--text);border-radius:4px;padding:8px 16px;\
                                    font-size:14px;cursor:pointer;\
                                    transition:background-color 150ms ease,border-color 150ms ease;"
                            >
                                "Discard"
                            </button>
                            <button
                                type="submit"
                                disabled=move || loading.get()
                                style="background:var(--accent);border:1px solid var(--accent);\
                                    color:#fff;border-radius:4px;padding:8px 16px;\
                                    font-size:14px;cursor:pointer;font-weight:600;\
                                    transition:background-color 150ms ease,border-color 150ms ease;"
                            >
                                {move || if loading.get() { "Creating\u{2026}" } else { "Create" }}
                            </button>
                        </div>
                    </form>
                </div>
            </div>
        </Show>
    }
}
