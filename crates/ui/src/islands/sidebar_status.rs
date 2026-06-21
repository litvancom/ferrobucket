//! SidebarStatus island — resolves the sidebar status dot via `check_status_fn`.
//!
//! This island owns a `Resource` over `check_status_fn`, which actually runs
//! client-side after hydration. That makes the sidebar status live (no longer
//! stuck on "Checking status…" — fixes GAP-04-05).
//!
//! Security invariant (DEC-ui-ssr / T-04-07A): no signing, presign, hmac, sigv4,
//! secret material, or `ferrobucket-storage` import in this island. It compiles
//! to WASM. `check_status_fn` returns only `{ writable, message }` UI copy strings.

use leptos::prelude::*;

use crate::server_fns::settings::check_status_fn;

/// Connection status card matching the design template's sidebar card:
/// pulsing dot + status label + mono secondary line.
#[component]
fn StatusCard(writable: bool, message: String) -> impl IntoView {
    let (label, dot) = if writable {
        ("Connected", "var(--success)")
    } else {
        ("Degraded", "var(--warn)")
    };
    view! {
        <div style="margin:0 12px 12px;padding:8px 10px;background:var(--surface);\
            border:1px solid var(--border);border-radius:7px;display:flex;\
            align-items:center;gap:8px">
            <span style=format!(
                "width:7px;height:7px;border-radius:50%;background:{dot};\
                box-shadow:0 0 0 3px var(--accent-dim);flex:none;\
                animation:pulse 2.4s ease-in-out infinite"
            )></span>
            <div style="min-width:0">
                <div style="font-size:11px;color:var(--dim);line-height:1.3">{label}</div>
                <div style="font-family:'IBM Plex Mono',monospace;font-size:11px;\
                    color:var(--text);white-space:nowrap;overflow:hidden;\
                    text-overflow:ellipsis">{message}</div>
            </div>
        </div>
    }
}

/// SidebarStatus island: resolves the sidebar status dot by calling `check_status_fn`.
///
/// Owns a `Resource` over `check_status_fn` (same pattern as `SettingsPage`).
/// Inside `<Suspense>`, renders the resolved status via the `StatusCard` connection card.
/// The fallback shows "Checking status…" until the resource resolves.
///
/// Because this is a hydrated island (not a plain SSR component), the resource
/// actually fires and resolves — fixing GAP-04-05 where the static prop
/// never called the server fn.
///
/// SECURITY: no signing/presign/hmac/sigv4/secret code; no ferrobucket-storage import.
#[island]
pub fn SidebarStatus() -> impl IntoView {
    let status = Resource::new(|| (), |_| async move { check_status_fn().await });

    view! {
        <Suspense fallback=|| view! {
            <StatusCard writable=true message="Checking status…".to_string() />
        }>
            {move || {
                status.get().map(|result| {
                    let (writable, message) = match result {
                        Ok(s) => (s.writable, s.message),
                        Err(_) => (false, "Unable to check status.".to_string()),
                    };
                    view! {
                        <StatusCard writable=writable message=message />
                    }
                })
            }}
        </Suspense>
    }
}
