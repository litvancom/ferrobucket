use leptos::prelude::*;

use crate::app::App;

/// HTML shell function for server-side rendering.
///
/// Returns the full `<!DOCTYPE html>` document with:
/// - `<HydrationScripts options islands=true/>` for islands-mode hydration
/// - IBM Plex Sans + IBM Plex Mono Google Fonts links
/// - Inline `<style>` with the CSS-variable palette (dark `:root` + `[data-theme="light"]`)
/// - `<body data-theme>` defaulting to dark (no attribute = dark per `:root` palette)
/// - `<App/>` rendered server-side; islands hydrated by the WASM loader
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
                    href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400&family=IBM+Plex+Sans:wght@400;600&display=swap"
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
            // data-theme attribute: absent = dark (matches :root palette).
            // The ThemeToggle island sets data-theme="light" for the light theme.
            <body>
                <App/>
            </body>
        </html>
    }
}
