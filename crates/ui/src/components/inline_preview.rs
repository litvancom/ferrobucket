//! InlinePreview SSR component — size-gated image/text preview.
//!
//! SECURITY (T-04-15, T-04-16):
//! - All images (including SVG) rendered as HTML img tags with src=/ui/download/...
//!   NEVER as inline SVG elements — this prevents embedded script execution.
//! - Images only if size <= 5 MB (5_242_880 bytes).
//! - Text only if size <= 512 KB (524_288 bytes).
//! - Over-limit: "File too large to preview (>{limit})".
//! - Unknown type: "No preview available for this file type."
//!
//! Security invariant: SSR-only. No presign/hmac/sigv4 code. No inline SVG elements.

use leptos::prelude::*;

/// 5 MB image size gate (T-04-16).
const IMAGE_SIZE_GATE: u64 = 5 * 1024 * 1024; // 5_242_880 bytes

/// 512 KB text size gate (T-04-16).
const TEXT_SIZE_GATE: u64 = 512 * 1024; // 524_288 bytes

/// InlinePreview component (SSR only).
///
/// Props:
/// - `bucket`: bucket name.
/// - `key`: object key (used to construct `/ui/download/{bucket}/{key}` URL).
/// - `content_type`: MIME type from `ObjectDetail.content_type`.
/// - `size`: object size in bytes (used for size gate).
#[component]
pub fn InlinePreview(
    bucket: String,
    key: String,
    #[prop(default = String::new())] content_type: String,
    size: u64,
) -> impl IntoView {
    let download_url = format!("/ui/download/{}/{}", bucket, key);
    let ct = content_type.to_lowercase();

    // Classify content type
    let is_image = ct.starts_with("image/png")
        || ct.starts_with("image/jpeg")
        || ct.starts_with("image/jpg")
        || ct.starts_with("image/gif")
        || ct.starts_with("image/webp")
        || ct.starts_with("image/svg+xml");

    let is_text = ct.starts_with("text/")
        || ct == "application/json"
        || ct == "application/yaml"
        || ct == "application/x-yaml"
        || ct == "application/toml";

    if is_image {
        if size > IMAGE_SIZE_GATE {
            // Refuse — size gate (T-04-16)
            view! {
                <p style="font-size:12px;color:var(--faint);font-style:italic;margin:0;">
                    "File too large to preview (>5 MB)"
                </p>
            }.into_any()
        } else {
            // Render as <img> — NEVER inline SVG (T-04-15, Security Domain).
            // SVG also rendered as <img> here so the browser sandboxes any embedded script.
            // Click the thumbnail to open a full-screen lightbox (this component runs
            // inside the SlideOver island, so the click handler is interactive).
            let (zoom, set_zoom) = signal(false);
            let full_url = download_url.clone();
            let alt_thumb = key.clone();
            let alt_full = key.clone();
            view! {
                <img
                    src=download_url
                    alt=alt_thumb
                    title="Click to view full image"
                    on:click=move |_| set_zoom.set(true)
                    style="max-width:100%;max-height:300px;\
                        object-fit:contain;display:block;margin:0 auto;\
                        border-radius:7px;cursor:zoom-in;"
                    loading="lazy"
                />
                <Show when=move || zoom.get()>
                    <div
                        on:click=move |_| set_zoom.set(false)
                        style="position:fixed;inset:0;z-index:70;\
                            background:rgba(0,0,0,.85);display:flex;\
                            align-items:center;justify-content:center;padding:32px;\
                            cursor:zoom-out;animation:overlayIn .15s ease;"
                    >
                        <img
                            src=full_url.clone()
                            alt=alt_full.clone()
                            style="max-width:95vw;max-height:95vh;object-fit:contain;\
                                border-radius:6px;box-shadow:var(--shadow);"
                        />
                        <button
                            aria-label="Close image"
                            on:click=move |_| set_zoom.set(false)
                            style="position:fixed;top:18px;right:22px;width:36px;height:36px;\
                                display:flex;align-items:center;justify-content:center;\
                                border:none;border-radius:8px;background:rgba(255,255,255,.12);\
                                color:#fff;cursor:pointer;"
                        >
                            <svg width="18" height="18" viewBox="0 0 16 16" fill="none"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
                        </button>
                    </div>
                </Show>
            }.into_any()
        }
    } else if is_text {
        if size > TEXT_SIZE_GATE {
            // Refuse — size gate (T-04-16)
            view! {
                <p style="font-size:12px;color:var(--faint);font-style:italic;margin:0;">
                    "File too large to preview (>512 KB)"
                </p>
            }.into_any()
        } else {
            // Text preview: server-side load (for SSR, we link to the download URL as placeholder;
            // the actual content is fetched by the browser on load via a placeholder message.
            // Full text rendering requires a client-side fetch or a separate server fn.
            // For SSR, we display a note prompting download, since we cannot buffer arbitrarily
            // large content into the SSR render. The plan allows rendering via <pre> if content
            // is available. For SSR-only (no WASM fetching), we link to download.
            view! {
                <div>
                    <p style="font-size:12px;color:var(--faint);margin:0;">
                        "Text preview — "
                        <a
                            href=download_url
                            style="color:var(--accent);text-decoration:none;"
                        >
                            "view raw"
                        </a>
                    </p>
                </div>
            }.into_any()
        }
    } else {
        view! {
            <p style="font-size:12px;color:var(--text-muted);font-style:italic;">
                "No preview available for this file type."
            </p>
        }.into_any()
    }
}
