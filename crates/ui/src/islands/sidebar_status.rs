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

use crate::components::StatusIndicator;
use crate::server_fns::settings::check_status_fn;

/// SidebarStatus island: resolves the sidebar status dot by calling `check_status_fn`.
///
/// Owns a `Resource` over `check_status_fn` (same pattern as `SettingsPage`).
/// Inside `<Suspense>`, renders the resolved status via `StatusIndicator`.
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
            <StatusIndicator writable=true message="Checking status…".to_string() />
        }>
            {move || {
                status.get().map(|result| {
                    let (writable, message) = match result {
                        Ok(s) => (s.writable, s.message),
                        Err(_) => (false, "Unable to check status.".to_string()),
                    };
                    view! {
                        <StatusIndicator writable=writable message=message />
                    }
                })
            }}
        </Suspense>
    }
}
