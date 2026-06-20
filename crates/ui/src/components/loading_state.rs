//! LoadingState SSR component — centered "Loading…" placeholder.
//!
//! Copy: "Loading…" (centered, --text-muted, label role 12px).
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

/// LoadingState component (SSR only).
#[component]
pub fn LoadingState() -> impl IntoView {
    view! {
        <div style="display:flex;align-items:center;justify-content:center;\
            padding:48px 32px;">
            <span style="font-size:12px;color:var(--text-muted);">"Loading\u{2026}"</span>
        </div>
    }
}
