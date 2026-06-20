use leptos::prelude::*;
use leptos_router::{
    components::{Outlet, ParentRoute, Route, Router, Routes},
    path,
};

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
                    // Wave 0 stubs — real implementations in Plan 05 (Wave 1).
                    <Route path=path!("") view=BucketListPage />
                    <Route path=path!("buckets/:bucket") view=ObjectBrowserPage />
                    <Route path=path!("settings") view=SettingsPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

/// Layout shell wrapping sidebar + main content area.
///
/// Wave 0 stub — Plan 03 adds the real sidebar, theme toggle, and toast stack.
/// `<Outlet />` is where child routes render their content.
#[component]
fn Shell() -> impl IntoView {
    view! {
        <div class="app-shell">
            <main class="main-content">
                <Outlet />
            </main>
        </div>
    }
}

/// Bucket list page stub (`/ui`). Filled by Plan 05.
#[component]
fn BucketListPage() -> impl IntoView {
    view! { <h1>"Bucket List"</h1> }
}

/// Object browser page stub (`/ui/buckets/{bucket}`). Filled by Plan 05.
#[component]
fn ObjectBrowserPage() -> impl IntoView {
    view! { <h1>"Object Browser"</h1> }
}

/// Settings page stub (`/ui/settings`). Filled by Plan 05.
#[component]
fn SettingsPage() -> impl IntoView {
    view! { <h1>"Settings"</h1> }
}
