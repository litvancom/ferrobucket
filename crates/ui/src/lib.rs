pub mod app;
pub mod shell;

/// Bucket list, object browser, settings pages — filled by Plan 05.
pub mod pages;

/// Interactive islands (#[island]) — upload zone, modals, theme toggle, toasts, etc.
/// Filled by Plan 03 and Plan 04.
pub mod islands;

/// SSR-only reusable components (table, breadcrumb, empty state, etc.).
/// Filled by Plan 05.
pub mod components;

/// Server functions — storage calls from #[server] macros.
/// Filled by Plan 02.
pub mod server_fns;

pub use app::App;
pub use shell::shell;

/// WASM hydrate entry point.
///
/// Called automatically by the browser when the WASM module loads
/// (`#[wasm_bindgen(start)]` attribute). Installs the panic hook for
/// readable console errors, then mounts the Leptos App into the existing
/// SSR-rendered body.
///
/// Gated to `#[cfg(feature = "hydrate")]` so this function is never
/// compiled into the server binary (DEC-ui-ssr: no signing or credentials
/// in the WASM/browser path).
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
