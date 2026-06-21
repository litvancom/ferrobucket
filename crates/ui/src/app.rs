use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::Sidebar;
use crate::islands::Toast;
use crate::pages::{BucketListPage, ObjectBrowserPage, SettingsPage};

/// Root Leptos application component.
///
/// Routes are FLAT and each route view is a dedicated layout component that
/// instantiates its page **directly** (`<BucketListPage/>`), not through a
/// parent-route `<Outlet/>` nor through `children`/`AnyView`.
///
/// WHY (islands hydration): with `hydrate_islands()`, `#[island]` components only
/// hydrate when they live inside a directly-instantiated component in the route
/// view (the same way `<Sidebar/>`'s islands hydrate). Islands reached through
/// type-erased rendering — a parent-route `<Outlet/>` or a `children: Children`
/// (`Box<dyn FnOnce() -> AnyView>`) slot — do NOT get their event handlers
/// attached, so the create-bucket / delete confirmation modals never open. Each
/// page is therefore a direct child element of its route's layout component.
///
/// All paths start with `/ui` so `generate_route_list(App)` emits `/ui/…` and the
/// S3 fallback never sees UI requests. Do NOT use `axum::Router::nest("/ui", …)`.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Not found"</p> }>
                <Route path=path!("/ui") view=BucketsRoute />
                <Route path=path!("/ui/buckets/:bucket") view=ObjectsRoute />
                <Route path=path!("/ui/settings") view=SettingsRoute />
            </Routes>
        </Router>
    }
}

/// Shared chrome styles (kept as constants so each route component stays small).
const SHELL: &str = "display:flex;height:100%;width:100%;overflow:hidden;\
    background:var(--bg);color:var(--text);font-size:13px;line-height:1.45;\
    transform:translateZ(0)";
const MAIN: &str = "flex:1;min-width:0;display:flex;flex-direction:column;overflow:hidden";

#[component]
fn BucketsRoute() -> impl IntoView {
    view! {
        <div style=SHELL>
            <Sidebar />
            <main style=MAIN>
                <BucketListPage />
            </main>
            <Toast />
        </div>
    }
}

#[component]
fn ObjectsRoute() -> impl IntoView {
    view! {
        <div style=SHELL>
            <Sidebar />
            <main style=MAIN>
                <ObjectBrowserPage />
            </main>
            <Toast />
        </div>
    }
}

#[component]
fn SettingsRoute() -> impl IntoView {
    view! {
        <div style=SHELL>
            <Sidebar />
            <main style=MAIN>
                <SettingsPage />
            </main>
            <Toast />
        </div>
    }
}
