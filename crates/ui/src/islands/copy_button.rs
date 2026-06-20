//! CopyButton island — writes a string to the Clipboard API.
//!
//! Per UI-SPEC Copywriting Contract: "Presigned URL copied (expires in 15 min)".
//! The presigned URL string arrives as a server-minted prop (never generated
//! inside this island — DEC-ui-ssr / D-05).
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.
//! The only signed artifact that reaches the browser is a URL String from the server fn.

use leptos::prelude::*;

/// CopyButton island.
///
/// Props (all serializable):
/// - `text`: the string to copy to clipboard (a presigned URL from the server).
#[island]
pub fn CopyButton(text: String) -> impl IntoView {
    let (copied, set_copied) = signal(false);
    let text = StoredValue::new(text);

    let handle_copy = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let text_val = text.get_value();
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&text_val);
            }
        }
        set_copied.set(true);
        // Reset after 2s.
        #[cfg(feature = "hydrate")]
        {
            let _ = leptos::leptos_dom::helpers::set_timeout(
                move || {
                    set_copied.set(false);
                },
                std::time::Duration::from_millis(2000),
            );
        }
    };

    view! {
        <button
            on:click=handle_copy
            style="background:none;border:1px solid var(--border);\
                color:var(--text);border-radius:4px;padding:6px 12px;\
                font-size:14px;cursor:pointer;display:inline-flex;align-items:center;gap:4px;\
                transition:background-color 150ms ease,border-color 150ms ease;"
        >
            // Clipboard SVG icon (Lucide)
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
            >
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
            {move || {
                if copied.get() {
                    "Presigned URL copied (expires in 15 min)"
                } else {
                    "Copy Presigned URL"
                }
            }}
        </button>
    }
}
