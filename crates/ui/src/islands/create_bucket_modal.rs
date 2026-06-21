//! CreateBucketModal island — bucket name input with server-fn submit.
//!
//! On submit calls `create_bucket_fn`. On `Err`, maps the raw error through
//! `friendly_create_error` before rendering: reserved-name errors become the
//! UI-SPEC Copywriting Contract sentence "This name is reserved by the server.
//! Choose a different name."; other errors strip the leaky server-fn prefix.
//!
//! `create_bucket_fn` is the server fn from Plan 02; it is callable from islands
//! because the #[server] macro routes calls over HTTP.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::server_fns::buckets::create_bucket_fn;

/// Map a raw `create_bucket_fn` error to user-friendly copy (UI-SPEC Copywriting Contract).
///
/// - If the lowercased message contains "reserved" → the exact reserved-name sentence.
/// - Otherwise → strip the leaky "error running server function:" prefix (and surrounding
///   whitespace) so internal framework text never reaches the UI.
fn friendly_create_error(raw: String) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("reserved") {
        "This name is reserved by the server. Choose a different name.".to_string()
    } else {
        // Strip the framework prefix if present, then trim whitespace.
        let stripped = raw
            .strip_prefix("error running server function:")
            .map(|s| s.trim())
            .unwrap_or(raw.trim());
        stripped.to_string()
    }
}

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
                    set_error.set(Some(friendly_create_error(e.to_string())));
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
        // Scoped styles: input focus ring + button hover states (template style-focus/style-hover)
        <style>
            "#create-bucket-btn:hover{background:var(--accent-2)}\
             .fb-create-input:focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-dim)}\
             .fb-create-cancel:hover{background:var(--hover)}\
             .fb-create-confirm:hover{background:var(--accent-2)}"
        </style>

        // "Create bucket" trigger button (accent fill — primary CTA)
        <button
            on:click=handle_open
            id="create-bucket-btn"
            style="display:flex;align-items:center;gap:7px;padding:8px 14px;border:none;border-radius:7px;\
                background:var(--accent);color:#fff;font-family:inherit;font-size:13px;font-weight:600;cursor:pointer"
        >
            "Create Bucket"
        </button>

        // Modal
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
                    <div style="padding:18px 20px 4px">
                        <div style="font-size:16px;font-weight:600;letter-spacing:-.2px">
                            "Create bucket"
                        </div>
                        <div style="font-size:12.5px;color:var(--faint);margin-top:3px">
                            "Names must be lowercase, 3\u{2013}63 chars, DNS-compatible."
                        </div>
                    </div>
                    <form on:submit=handle_submit>
                        <div style="padding:16px 20px 20px">
                            <input
                                type="text"
                                class="fb-create-input"
                                placeholder="my-bucket-name"
                                spellcheck="false"
                                prop:value=move || name.get()
                                on:input=move |e| set_name.set(event_target_value(&e))
                                disabled=move || loading.get()
                                style="width:100%;padding:10px 12px;border:1px solid var(--border-2);\
                                    border-radius:8px;background:var(--bg);color:var(--text);\
                                    font-family:'IBM Plex Mono',monospace;font-size:14px;outline:none"
                            />
                            // Inline error (friendly_create_error maps reserved-name → UI-SPEC copy)
                            <Show when=move || error.get().is_some()>
                                <div style="font-size:12px;color:var(--danger);margin-top:8px">
                                    {move || error.get().unwrap_or_default()}
                                </div>
                            </Show>
                        </div>
                        <div style="display:flex;justify-content:flex-end;gap:9px;padding:14px 20px;border-top:1px solid var(--border)">
                            <button
                                type="button"
                                class="fb-create-cancel"
                                on:click=handle_dismiss
                                disabled=move || loading.get()
                                style="padding:8px 15px;border:1px solid var(--border-2);border-radius:7px;\
                                    background:transparent;color:var(--text);font-family:inherit;\
                                    font-size:13px;font-weight:500;cursor:pointer"
                            >
                                "Discard"
                            </button>
                            <button
                                type="submit"
                                class="fb-create-confirm"
                                disabled=move || loading.get()
                                style="padding:8px 15px;border:none;border-radius:7px;background:var(--accent);\
                                    color:#fff;font-family:inherit;font-size:13px;font-weight:600;cursor:pointer"
                            >
                                {move || if loading.get() { "Creating\u{2026}" } else { "Create bucket" }}
                            </button>
                        </div>
                    </form>
                </div>
            </div>
        </Show>
    }
}
