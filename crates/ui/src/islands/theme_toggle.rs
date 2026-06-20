//! ThemeToggle island — toggles `data-theme` on `<html>`, persists in `localStorage`.
//!
//! Security invariant (DEC-ui-ssr / T-04-07A): only the `"theme"` key is written to
//! localStorage. No credentials, signing keys, or secret material touch this island.
//! `data-theme` is set to the literal string `"dark"` or removed (never user input).

use leptos::prelude::*;

/// Apply `theme` to the document element.
///
/// Light (default): removes `data-theme` attribute (absent = light per :root palette).
/// Dark: sets `data-theme="dark"`.
#[cfg(feature = "hydrate")]
fn apply_theme(theme: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            if theme == "dark" {
                let _ = html.set_attribute("data-theme", "dark");
            } else {
                // Light default: no attribute needed (matches :root palette)
                let _ = html.remove_attribute("data-theme");
            }
        }
    }
}

/// ThemeToggle island: sun/moon button that flips between light (default) and dark themes.
///
/// On mount, reads `localStorage["theme"]` and applies it. On click, toggles, writes
/// `localStorage["theme"]`, and updates `data-theme` on `<html>`. Light is the default
/// (no attribute on `<html>`; absent = light per :root palette — DEC-ui-theme-default).
///
/// SECURITY: only the string `"light"` or `"dark"` is stored in localStorage. No credentials.
/// No signing, presign, hmac, sigv4, or secret_key code in this island.
#[island]
pub fn ThemeToggle() -> impl IntoView {
    let (theme, set_theme) = signal("light".to_string());

    // On mount: read persisted theme from localStorage and apply it.
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(saved)) = storage.get_item("theme") {
                        let saved_clone = saved.clone();
                        set_theme.set(saved);
                        apply_theme(&saved_clone);
                    }
                    // If no saved value, default is "light" — no attribute needed.
                }
            }
        }
    });

    let toggle = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let current = theme.get_untracked();
            let next = if current == "light" { "dark" } else { "light" };
            set_theme.set(next.to_string());
            apply_theme(next);
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item("theme", next);
                }
            }
        }
    };

    // Sun icon (light mode) vs moon icon (dark mode) — Lucide-style inline SVG paths.
    // When in light mode: show moon icon (click to switch to dark).
    // When in dark mode: show sun icon (click to switch to light).
    view! {
        <button
            class="theme-toggle"
            aria-label="Toggle theme"
            on:click=toggle
            style="background:none;border:none;cursor:pointer;padding:8px;color:var(--text);display:flex;align-items:center;justify-content:center;border-radius:4px;transition:background-color 150ms ease;"
        >
            {move || {
                if theme.get() == "light" {
                    // Moon icon — currently in light mode, click to switch to dark
                    view! {
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
                        </svg>
                    }
                    .into_any()
                } else {
                    // Sun icon — currently in dark mode, click to switch to light
                    view! {
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <circle cx="12" cy="12" r="5" />
                            <line x1="12" y1="1" x2="12" y2="3" />
                            <line x1="12" y1="21" x2="12" y2="23" />
                            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
                            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
                            <line x1="1" y1="12" x2="3" y2="12" />
                            <line x1="21" y1="12" x2="23" y2="12" />
                            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
                            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
                        </svg>
                    }
                    .into_any()
                }
            }}
        </button>
    }
}
