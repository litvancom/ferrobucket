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
        <div style="display:flex;flex-direction:column;align-items:center;\
            justify-content:center;padding:64px 32px;text-align:center;">
            <h2 style="font-size:16px;font-weight:600;color:var(--text);margin:0 0 8px 0;\
                line-height:1.3;">
                {heading}
            </h2>
            <p style="font-size:14px;color:var(--text-muted);margin:0 0 24px 0;\
                line-height:1.5;max-width:320px;">
                {body}
            </p>
            // CTA: either a link, a slot island, or nothing
            {cta_slot.map(|slot| {
                view! { <div>{slot()}</div> }.into_any()
            }).or_else(|| {
                cta_href.map(|href| {
                    let label = cta_label.clone().unwrap_or_default();
                    view! {
                        <a
                            href=href
                            style="display:inline-block;background:var(--accent);\
                                color:#fff;border-radius:4px;padding:8px 16px;\
                                font-size:14px;font-weight:600;text-decoration:none;\
                                transition:background-color 150ms ease;"
                        >
                            {label}
                        </a>
                    }.into_any()
                })
            })}
        </div>
    }
}
