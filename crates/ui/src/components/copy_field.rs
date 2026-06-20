//! CopyField SSR component — read-only monospace text field + CopyButton island.
//!
//! Used for presigned URLs and the settings endpoint field.
//! The text value is rendered in IBM Plex Mono 13px on --mono-bg background.
//!
//! Security invariant: SSR-only wrapper. CopyButton island handles clipboard.

use leptos::prelude::*;

use crate::islands::CopyButton;

/// CopyField component (SSR only — CopyButton island is hydrated).
///
/// Props:
/// - `value`: the text to display and copy.
/// - `label`: accessible label for the field (screen readers).
#[component]
pub fn CopyField(
    value: String,
    #[prop(default = String::new())] label: String,
) -> impl IntoView {
    let copy_value = value.clone();

    view! {
        <div style="display:flex;align-items:center;gap:0;\
            border:1px solid var(--border);border-radius:4px;overflow:hidden;">
            // Read-only text area — IBM Plex Mono 13px on --mono-bg
            <input
                type="text"
                readonly
                value=value
                aria-label=label
                style="flex:1;background:var(--mono-bg);color:var(--text);\
                    font-family:'IBM Plex Mono',monospace;font-size:13px;\
                    border:none;padding:8px 12px;outline:none;\
                    min-width:0;cursor:text;"
            />
            // CopyButton island (hydrated)
            <CopyButton text=copy_value />
        </div>
    }
}
