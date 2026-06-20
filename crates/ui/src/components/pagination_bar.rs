//! PaginationBar SSR component — Previous / Next buttons + page indicator.
//!
//! Driven by `next_token` (opaque continuation token from `list_objects_fn`).
//! Pagination style: "Previous" / "Next" buttons (D-09, NOT infinite scroll).
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

/// PaginationBar component (SSR only).
///
/// Props:
/// - `base_href`: base URL path for this page (e.g. "/ui/buckets/mybucket").
/// - `prefix`: current prefix query param (empty = root).
/// - `prev_token`: continuation token of the previous page (None at page 1).
/// - `next_token`: continuation token from `ObjectListing.next_token` (None = last page).
/// - `page_num`: current page number (1-based).
#[component]
pub fn PaginationBar(
    base_href: String,
    #[prop(default = String::new())] prefix: String,
    #[prop(default = None)] prev_token: Option<String>,
    #[prop(default = None)] next_token: Option<String>,
    #[prop(default = 1usize)] page_num: usize,
) -> impl IntoView {
    // Build query string helper
    let build_href = |token: Option<&str>| -> String {
        let mut parts = Vec::<String>::new();
        if !prefix.is_empty() {
            parts.push(format!("prefix={}", urlencoding_simple(&prefix)));
        }
        if let Some(t) = token {
            if !t.is_empty() {
                parts.push(format!("continuation={}", urlencoding_simple(t)));
            }
        }
        if parts.is_empty() {
            base_href.clone()
        } else {
            format!("{}?{}", base_href, parts.join("&"))
        }
    };

    let has_prev = prev_token.is_some();
    let has_next = next_token.is_some();
    let prev_href = build_href(prev_token.as_deref());
    let next_href = build_href(next_token.as_deref());

    view! {
        <div style="display:flex;align-items:center;justify-content:space-between;\
            padding:12px 16px;border-top:1px solid var(--border);">
            // Previous button
            {if has_prev {
                view! {
                    <a
                        href=prev_href
                        style="display:inline-flex;align-items:center;gap:6px;\
                            padding:6px 12px;font-size:14px;color:var(--text);\
                            text-decoration:none;border:1px solid var(--border);\
                            border-radius:4px;transition:background-color 150ms ease,\
                            border-color 150ms ease;"
                    >
                        // Left arrow icon (Lucide)
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="14" height="14" viewBox="0 0 24 24"
                            fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <polyline points="15 18 9 12 15 6"/>
                        </svg>
                        "Previous"
                    </a>
                }.into_any()
            } else {
                view! {
                    <span
                        aria-disabled="true"
                        style="display:inline-flex;align-items:center;gap:6px;\
                            padding:6px 12px;font-size:14px;color:var(--text-muted);\
                            border:1px solid var(--border);border-radius:4px;\
                            opacity:0.4;cursor:not-allowed;"
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="14" height="14" viewBox="0 0 24 24"
                            fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <polyline points="15 18 9 12 15 6"/>
                        </svg>
                        "Previous"
                    </span>
                }.into_any()
            }}

            // Page indicator
            <span style="font-size:12px;color:var(--text-muted);">
                {format!("Page {}", page_num)}
            </span>

            // Next button
            {if has_next {
                view! {
                    <a
                        href=next_href
                        style="display:inline-flex;align-items:center;gap:6px;\
                            padding:6px 12px;font-size:14px;color:var(--text);\
                            text-decoration:none;border:1px solid var(--border);\
                            border-radius:4px;transition:background-color 150ms ease,\
                            border-color 150ms ease;"
                    >
                        "Next"
                        // Right arrow icon (Lucide)
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="14" height="14" viewBox="0 0 24 24"
                            fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <polyline points="9 18 15 12 9 6"/>
                        </svg>
                    </a>
                }.into_any()
            } else {
                view! {
                    <span
                        aria-disabled="true"
                        style="display:inline-flex;align-items:center;gap:6px;\
                            padding:6px 12px;font-size:14px;color:var(--text-muted);\
                            border:1px solid var(--border);border-radius:4px;\
                            opacity:0.4;cursor:not-allowed;"
                    >
                        "Next"
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="14" height="14" viewBox="0 0 24 24"
                            fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <polyline points="9 18 15 12 9 6"/>
                        </svg>
                    </span>
                }.into_any()
            }}
        </div>
    }
}

/// Percent-encode string for use in URL query parameter values.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}
