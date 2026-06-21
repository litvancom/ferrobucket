//! CopyButton island — writes a string to the Clipboard API.
//!
//! The copy-label and copied-feedback text are caller-supplied props so the
//! button can be reused for any field (endpoint copy, presigned URL copy, etc.).
//! No label text is hardcoded; callers supply both `copy_label` and `copied_label`.
//!
//! Security invariant: no presign/hmac/secret/sigv4 code. No credentials in this island.
//! The only signed artifact that reaches the browser is a URL String from the server fn.

use leptos::prelude::*;

/// CopyButton island.
///
/// Props (all serializable):
/// - `text`:         the string to copy to clipboard.
/// - `copy_label`:   button label shown before copy (e.g. "Copy endpoint").
/// - `copied_label`: feedback text shown after copy (e.g. "Endpoint copied").
#[island]
pub fn CopyButton(
    text: String,
    copy_label: String,
    copied_label: String,
) -> impl IntoView {
    let (copied, set_copied) = signal(false);
    let text = StoredValue::new(text);
    let copy_label = StoredValue::new(copy_label);
    let copied_label = StoredValue::new(copied_label);

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
        // Scoped hover (template Copy buttons darken on hover via accent-2).
        <style>".copy-btn:hover{background:var(--accent-2)}"</style>
        <button
            aria-label=move || copy_label.get_value()
            on:click=handle_copy
            class="copy-btn"
            style="flex:none;display:flex;align-items:center;gap:6px;\
                padding:6px 10px;border:none;border-radius:5px;\
                background:var(--accent);color:#fff;font-family:inherit;\
                font-size:12px;font-weight:600;cursor:pointer;\
                transition:background-color 150ms ease;"
        >
            // Clipboard SVG icon (template style — white stroke on accent)
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                <rect x="5" y="5" width="8" height="8" rx="1.4" stroke="#fff" stroke-width="1.3" />
                <path d="M3 11V4.4C3 3.6 3.6 3 4.4 3H11" stroke="#fff" stroke-width="1.3" />
            </svg>
            {move || {
                if copied.get() {
                    copied_label.get_value()
                } else {
                    copy_label.get_value()
                }
            }}
        </button>
    }
}
