//! Breadcrumb SSR component — clickable prefix segments.
//!
//! Renders: `{bucket} / {segment} / {segment} /`
//! Each segment is a clickable link (--dim text), current segment non-linked (--text, bold).
//! Separator `/` in --faint. Full path in IBM Plex Mono 13px.
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

/// Breadcrumb component (SSR only).
///
/// Props:
/// - `bucket`: the bucket name (first segment, clickable → /ui/buckets/{bucket}).
/// - `prefix`: the current prefix string (may be empty at bucket root).
#[component]
pub fn Breadcrumb(
    bucket: String,
    #[prop(default = String::new())] prefix: String,
) -> impl IntoView {
    // Build breadcrumb segments from the prefix.
    // e.g. prefix "a/b/c/" → segments ["a", "b", "c"]
    let segments: Vec<String> = if prefix.is_empty() {
        Vec::new()
    } else {
        prefix
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    };

    let bucket_href = format!("/ui/buckets/{}", bucket);
    let bucket_name = bucket.clone();

    view! {
        <nav
            aria-label="Breadcrumb"
            style="display:flex;align-items:center;flex-wrap:wrap;\
                gap:5px;font-family:'IBM Plex Mono',monospace;font-size:13px;min-width:0;"
        >
            // Bucket (root segment) — always clickable unless no prefix
            {if segments.is_empty() {
                view! {
                    <span style="color:var(--text);font-weight:600;padding:2px 3px;">{bucket_name}</span>
                }.into_any()
            } else {
                view! {
                    <a
                        href=bucket_href
                        style="border:none;background:transparent;color:var(--dim);\
                            text-decoration:none;font-family:'IBM Plex Mono',monospace;\
                            font-size:13px;font-weight:500;cursor:pointer;padding:2px 3px;"
                    >
                        {bucket_name}
                    </a>
                }.into_any()
            }}

            // Prefix segments
            {segments.iter().enumerate().map(|(i, seg)| {
                let is_last = i == segments.len() - 1;
                // Build href for this prefix level: join segments[0..=i] + "/"
                let prefix_parts: Vec<&str> = segments[..=i].iter().map(|s| s.as_str()).collect();
                let segment_prefix = format!("{}/", prefix_parts.join("/"));
                let seg_href = format!("/ui/buckets/{}?prefix={}", bucket.clone(), urlencoding_simple(&segment_prefix));
                let seg_display = seg.clone();

                view! {
                    <>
                        // Separator (--faint)
                        <span style="color:var(--faint);font-size:13px;">"/"</span>
                        // Segment — linked unless last (current = --text, bold)
                        {if is_last {
                            view! {
                                <span style="color:var(--text);font-weight:600;padding:2px 3px;">{seg_display}</span>
                            }.into_any()
                        } else {
                            view! {
                                <a
                                    href=seg_href
                                    style="border:none;background:transparent;color:var(--dim);\
                                        text-decoration:none;font-family:'IBM Plex Mono',monospace;\
                                        font-size:13px;font-weight:500;cursor:pointer;padding:2px 3px;"
                                >
                                    {seg_display}
                                </a>
                            }.into_any()
                        }}
                    </>
                }
            }).collect_view()}
        </nav>
    }
}

/// Percent-encode string for URL query parameter.
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
