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
            padding:10px 16px;font-size:12px;color:var(--faint);\
            border-top:1px solid var(--border);">
            // Page indicator (mono) — left
            <span style="font-family:'IBM Plex Mono',monospace;">
                {format!("Page {}", page_num)}
            </span>

            // Prev / Next controls — right
            <div style="display:flex;align-items:center;gap:6px;">
                {if has_prev {
                    view! {
                        <a
                            href=prev_href
                            style="padding:4px 9px;border:1px solid var(--border);\
                                border-radius:6px;background:var(--surface);color:var(--text);\
                                font-family:inherit;font-size:12px;text-decoration:none;cursor:pointer;"
                        >
                            "Prev"
                        </a>
                    }.into_any()
                } else {
                    view! {
                        <span
                            aria-disabled="true"
                            style="padding:4px 9px;border:1px solid var(--border);\
                                border-radius:6px;background:var(--surface);color:var(--faint);\
                                font-family:inherit;font-size:12px;cursor:not-allowed;"
                        >
                            "Prev"
                        </span>
                    }.into_any()
                }}

                {if has_next {
                    view! {
                        <a
                            href=next_href
                            style="padding:4px 9px;border:1px solid var(--border);\
                                border-radius:6px;background:var(--surface);color:var(--text);\
                                font-family:inherit;font-size:12px;text-decoration:none;cursor:pointer;"
                        >
                            "Next"
                        </a>
                    }.into_any()
                } else {
                    view! {
                        <span
                            aria-disabled="true"
                            style="padding:4px 9px;border:1px solid var(--border);\
                                border-radius:6px;background:var(--surface);color:var(--faint);\
                                font-family:inherit;font-size:12px;cursor:not-allowed;"
                        >
                            "Next"
                        </span>
                    }.into_any()
                }}
            </div>
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
