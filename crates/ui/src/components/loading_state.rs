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
        <div style="display:flex;flex-direction:column;align-items:center;\
            justify-content:center;gap:12px;padding:48px 32px;">
            <svg width="20" height="20" viewBox="0 0 16 16" fill="none"
                style="color:var(--faint);animation:spin .7s linear infinite;">
                <path d="M8 1.6a6.4 6.4 0 1 0 6.4 6.4" stroke="currentColor"
                    stroke-width="1.4" stroke-linecap="round"/>
            </svg>
            <span style="font-size:11px;font-weight:600;letter-spacing:.4px;\
                color:var(--faint);text-transform:uppercase;">"Loading\u{2026}"</span>
        </div>
    }
}
