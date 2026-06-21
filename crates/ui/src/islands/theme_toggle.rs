//! ThemeToggle island — toggles `data-theme` on `<html>`, persists in `localStorage`.
//!
//! Security invariant (DEC-ui-ssr / T-04-07A): only the `"theme"` key is written to
//! localStorage. No credentials, signing keys, or secret material touch this island.
//! `data-theme` is set to the literal string `"dark"` or removed (never user input).

use leptos::prelude::*;

/// Apply `theme` to the document element.
///
/// Light (default): removes `data-theme` attribute (absent = light per :root palette).
/// Dark: sets `data-theme="dark"` (override).
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

/// ThemeToggle island: sun/moon button that flips between dark (default) and light themes.
///
/// On mount, reads `localStorage["theme"]` and applies it. On click, toggles, writes
/// `localStorage["theme"]`, and updates `data-theme` on `<html>`. Dark is the default
/// (no attribute on `<html>`; absent = dark per :root palette — DEC-ui-theme-default).
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

    // Boxed 34x34 icon-button (template lines ~91-94).
    // Template: isDark → moon icon; isLight → sun icon. Color via currentColor.
    view! {
        <style>
            ".fb-theme-btn:hover{color:var(--text) !important;border-color:var(--border-2) !important}"
        </style>
        <button
            class="theme-toggle fb-theme-btn"
            aria-label="Toggle theme"
            title="Toggle theme"
            on:click=toggle
            style="width:34px;height:34px;flex:none;display:flex;align-items:center;\
                justify-content:center;border:1px solid var(--border);border-radius:6px;\
                background:var(--surface);color:var(--dim);cursor:pointer"
        >
            {move || {
                if theme.get() == "light" {
                    // Sun icon — currently in light mode, click to switch to dark
                    view! {
                        <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="3.1" stroke="currentColor" stroke-width="1.2"/><path d="M8 1v1.6M8 13.4V15M15 8h-1.6M2.6 8H1M12.95 3.05l-1.13 1.13M4.18 11.82l-1.13 1.13M12.95 12.95l-1.13-1.13M4.18 4.18 3.05 3.05" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
                    }
                    .into_any()
                } else {
                    // Moon icon — currently in dark mode, click to switch to light
                    view! {
                        <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><path d="M13.5 9.2A5.4 5.4 0 0 1 6.8 2.5 5.5 5.5 0 1 0 13.5 9.2Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/></svg>
                    }
                    .into_any()
                }
            }}
        </button>
    }
}
