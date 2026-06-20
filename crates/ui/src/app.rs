use leptos::prelude::*;
use leptos_router::{
    components::{Outlet, ParentRoute, Route, Router, Routes},
    path,
};

use crate::components::Sidebar;
use crate::islands::Toast;
use crate::pages::{BucketListPage, ObjectBrowserPage, SettingsPage};

/// Root Leptos application component.
///
/// Defines the router with all routes nested under the `/ui` parent prefix
/// (D-01: UI served at `/ui` on the same port as the S3 API).
///
/// RESEARCH A2: defining all routes inside `<ParentRoute path=path!("/ui")>`
/// causes `generate_route_list(App)` to emit paths starting with `/ui/…`,
/// so the S3 fallback never sees `/ui/*` requests. Verified by Wave-0 compile check.
///
/// Pitfall 1 (RESEARCH.md): do NOT use `axum::Router::nest("/ui", …)` to wrap
/// Leptos routes — that creates a double `/ui/ui/` prefix. The `/ui` prefix lives
/// here, inside the Leptos Router tree.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Not found"</p> }>
                <ParentRoute path=path!("/ui") view=Shell>
                    <Route path=path!("") view=BucketListPage />
                    <Route path=path!("buckets/:bucket") view=ObjectBrowserPage />
                    <Route path=path!("settings") view=SettingsPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

/// Layout shell wrapping sidebar + main content area + toast stack.
///
/// The Shell renders the 220px Sidebar (SSR-only), the main content area
/// (`<Outlet />` — child routes), and the Toast island (hydrated).
///
/// Sidebar receives no bucket list here (no server fn call from shell — pages
/// handle their own data). Sidebar shows only the settings link + theme toggle
/// + status at the shell level. The bucket nav list is populated on each page.
#[component]
fn Shell() -> impl IntoView {
    view! {
        <div class="app-shell">
            // 220px fixed sidebar (SSR)
            <Sidebar />
            // Main content area — child route renders here
            <main class="main-content" style="overflow-y:auto;">
                <Outlet />
            </main>
            // Toast island (top-right stack, hydrated)
            <Toast />
        </div>
    }
}
