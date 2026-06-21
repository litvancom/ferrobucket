use leptos::prelude::*;

use crate::app::App;

/// HTML shell function for server-side rendering.
///
/// Returns the full `<!DOCTYPE html>` document with:
/// - `<HydrationScripts options islands=true/>` for islands-mode hydration
/// - IBM Plex Sans + IBM Plex Mono Google Fonts links
/// - Inline `<style>` with the CSS-variable palette (light `:root` default + `:root[data-theme="dark"]` override)
/// - `<body>` with no `data-theme` attribute — absent attribute means light (the `:root` palette)
/// - `<App/>` rendered server-side; islands hydrated by the WASM loader
///
/// # Theme default (DEC-ui-theme-default)
/// `:root` holds the light palette — no attribute on `<html>` means light at first paint.
/// The ThemeToggle island sets `data-theme="dark"` when the user switches to dark.
///
/// # Pattern
/// Follows RESEARCH.md Pattern 8 (HydrationScripts islands=true, DECISIONS.md §3.5).
/// `islands=true` is required for the `#[island]` macro to work in the WASM target.
///
/// # Security (DEC-ui-ssr)
/// This function is called server-side only (ssr feature path). No credentials,
/// signing, or storage access here — the shell is pure HTML scaffolding.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"ferrobucket"</title>

                // IBM Plex Sans + IBM Plex Mono from Google Fonts.
                // Phase 5 can self-host these to remove the external dependency.
                <link rel="preconnect" href="https://fonts.googleapis.com"/>
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin=""/>
                <link
                    rel="stylesheet"
                    href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500;600&display=swap"
                />

                // CSS variable palette (dark default + light override).
                // Inlined so the palette is available before any external asset loads.
                // The full palette lives in styles.css; this ensures it is available SSR.
                <style>
                    {include_str!("styles.css")}
                </style>

                // HydrationScripts with islands=true: inserts the WASM loader.
                // Only #[island] components are compiled to WASM; #[component] stays SSR-only.
                // This minimises the WASM bundle size (REQ-ui-theming, idle-RAM goal).
                <HydrationScripts options=options.clone() islands=true />
            </head>
            // No data-theme attribute on <html>: absent attribute = dark (matches :root palette).
            // The ThemeToggle island sets data-theme="light" when the user chooses the light theme.
            <body>
                <App/>
            </body>
        </html>
    }
}
