//! SlideOver island — right-anchored 400px object-detail panel.
//!
//! The island manages its own open/close state. The `bucket` and `object_key`
//! are passed as serializable string props; the panel renders a placeholder body
//! for the page (Plan 05) to extend via server components (non-island children
//! are SSR-rendered and passed through from a containing server component).
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.

use leptos::prelude::*;

/// SlideOver island — right-side panel for object detail.
///
/// Props (all serializable):
/// - `bucket`: bucket name.
/// - `object_key`: object key.
#[island]
pub fn SlideOver(bucket: String, object_key: String) -> impl IntoView {
    let (open, set_open) = signal(false);
    let bucket = StoredValue::new(bucket);
    let object_key_stored = StoredValue::new(object_key.clone());

    // Display title is the filename portion of the key.
    let title = {
        let k = object_key.clone();
        k.rsplit('/').next().unwrap_or(&object_key).to_string()
    };
    let title = StoredValue::new(title);

    let handle_open = move |_| set_open.set(true);
    let handle_close = move |_| set_open.set(false);
    let handle_backdrop = move |_| set_open.set(false);

    view! {
        // Trigger: row click or action button (page uses this island by key)
        <button
            on:click=handle_open
            style="background:none;border:none;cursor:pointer;\
                color:var(--accent);font-size:14px;padding:4px 8px;border-radius:4px;"
        >
            {move || {
                let k = object_key_stored.get_value();
                k.rsplit('/').next().map(|s| s.to_string()).unwrap_or(k)
            }}
        </button>

        <Show when=move || open.get()>
            // Backdrop
            <div
                style="position:fixed;inset:0;z-index:400;"
                on:click=handle_backdrop
            />
            // Slide-over panel (400px, right-anchored, full height — UI-SPEC)
            <div
                style="position:fixed;top:0;right:0;bottom:0;width:400px;\
                    background:var(--surface);border-left:1px solid var(--border);\
                    z-index:401;display:flex;flex-direction:column;\
                    box-shadow:-4px 0 24px rgba(0,0,0,0.4);"
                on:click=|e| e.stop_propagation()
            >
                // Header
                <div style="display:flex;align-items:center;justify-content:space-between;\
                    padding:24px;border-bottom:1px solid var(--border);flex-shrink:0;">
                    <h2 style="font-size:16px;font-weight:600;color:var(--text);margin:0;\
                        overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                        {move || title.get_value()}
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
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <line x1="18" y1="6" x2="6" y2="18" />
                            <line x1="6" y1="6" x2="18" y2="18" />
                        </svg>
                    </button>
                </div>
                // Body — Plan 05 pages use head_object_fn to load metadata
                <div style="flex:1;overflow-y:auto;padding:24px;">
                    <p style="font-size:13px;font-family:'IBM Plex Mono',monospace;color:var(--text-muted);margin:0 0 8px 0;">
                        {move || format!("{}/{}", bucket.get_value(), object_key_stored.get_value())}
                    </p>
                    <p style="font-size:12px;color:var(--text-muted);margin:0;">
                        "Loading metadata\u{2026}"
                    </p>
                </div>
            </div>
        </Show>
    }
}
