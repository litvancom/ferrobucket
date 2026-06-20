//! StatusIndicator SSR component — colored dot + status text.
//!
//! Colors: green dot for writable/healthy, warning dot for not writable.
//! Copy strings from UI-SPEC Copywriting Contract.

use leptos::prelude::*;

/// Status indicator (SSR only).
///
/// Props:
/// - `writable`: true = healthy (green dot), false = warning dot.
/// - `message`: the status copy string (from `check_status_fn`).
#[component]
pub fn StatusIndicator(
    writable: bool,
    message: String,
) -> impl IntoView {
    let dot_color = if writable { "var(--success)" } else { "var(--warning)" };

    view! {
        <div style="display:flex;align-items:center;gap:6px;">
            // Colored dot
            <span
                style=format!(
                    "width:8px;height:8px;border-radius:50%;\
                    background:{dot_color};flex-shrink:0;"
                )
                aria-hidden="true"
            />
            // Status copy (label role: 12px, --text-muted)
            <span style="font-size:12px;color:var(--text-muted);line-height:1.4;">
                {message}
            </span>
        </div>
    }
}
