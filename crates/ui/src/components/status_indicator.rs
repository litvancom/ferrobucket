//! StatusIndicator SSR component — status pill (dot + text), template style.
//!
//! Colors: success (green) dot for writable/healthy, warn (amber) dot otherwise.
//! Copy strings from UI-SPEC Copywriting Contract.

use leptos::prelude::*;

/// Status indicator (SSR only).
///
/// Props:
/// - `writable`: true = healthy (success dot), false = warn dot.
/// - `message`: the status copy string (from `check_status_fn`).
#[component]
pub fn StatusIndicator(
    writable: bool,
    message: String,
) -> impl IntoView {
    let dot_color = if writable { "var(--success)" } else { "var(--warn)" };

    view! {
        // Status pill — rounded, bordered surface chip with a colored dot
        <div style="display:inline-flex;align-items:center;gap:8px;padding:6px 12px;border:1px solid var(--border);border-radius:20px;background:var(--surface)">
            // Colored dot
            <span
                style=format!(
                    "width:7px;height:7px;border-radius:50%;\
                    background:{dot_color};flex:none;"
                )
                aria-hidden="true"
            />
            // Status copy
            <span style="font-size:12px;color:var(--dim);line-height:1.45;">
                {message}
            </span>
        </div>
    }
}
