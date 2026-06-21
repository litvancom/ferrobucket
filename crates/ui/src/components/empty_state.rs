//! EmptyState SSR component — centered empty placeholder with optional CTA.
//!
//! Heading + body + optional CTA button (rendered as an island slot or a plain link).
//! Copy strings from UI-SPEC Copywriting Contract.
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

/// EmptyState component (SSR only).
///
/// Props:
/// - `heading`: heading text (e.g. "No buckets yet").
/// - `body`: body text (e.g. "Create your first bucket to start storing objects.").
/// - `cta_label`: optional CTA button label (renders a slot if present).
/// - `cta_href`: optional CTA link href (renders an `<a>` if present).
#[component]
pub fn EmptyState(
    heading: String,
    body: String,
    #[prop(default = None)] cta_label: Option<String>,
    #[prop(default = None)] cta_href: Option<String>,
    /// Optional children slot (e.g. a CreateBucketModal island).
    #[prop(default = None)] cta_slot: Option<Children>,
) -> impl IntoView {
    view! {
        // Dashed-border rounded card (template empty-state pattern)
        <div style="border:1px dashed var(--border-2);border-radius:10px;\
            padding:64px 24px;text-align:center;background:var(--surface);">
            // Rounded icon tile (halo)
            <div style="width:48px;height:48px;margin:0 auto 16px;border-radius:11px;\
                background:var(--surface-2);display:flex;align-items:center;\
                justify-content:center;color:var(--faint);">
                <svg width="22" height="22" viewBox="0 0 16 16" fill="none">
                    <ellipse cx="8" cy="4" rx="5.3" ry="2" stroke="currentColor" stroke-width="1.2"/>
                    <path d="M2.7 4v8c0 1.1 2.37 2 5.3 2s5.3-.9 5.3-2V4" stroke="currentColor" stroke-width="1.2"/>
                </svg>
            </div>
            <div style="font-size:15px;font-weight:600;color:var(--text);\
                margin-bottom:5px;">
                {heading}
            </div>
            <div style="font-size:13px;color:var(--faint);margin-bottom:18px;">
                {body}
            </div>
            // CTA: either a link, a slot island, or nothing
            {cta_slot.map(|slot| {
                view! { <div>{slot()}</div> }.into_any()
            }).or_else(|| {
                cta_href.map(|href| {
                    let label = cta_label.clone().unwrap_or_default();
                    view! {
                        <a
                            href=href
                            style="display:inline-flex;align-items:center;gap:7px;\
                                padding:8px 14px;border:none;border-radius:7px;\
                                background:var(--accent);color:#fff;\
                                font-family:inherit;font-size:13px;font-weight:600;\
                                text-decoration:none;cursor:pointer;"
                        >
                            {label}
                        </a>
                    }.into_any()
                })
            })}
        </div>
    }
}
