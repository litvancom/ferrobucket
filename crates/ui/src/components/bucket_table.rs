//! BucketTable SSR component — table of buckets with create + delete actions.
//!
//! Columns: Name | Created | Object Count | Total Size | Actions
//! - Name: clickable → /ui/buckets/{name}
//! - Numeric/date cells: IBM Plex Mono 13px (--text-muted)
//! - Delete: ConfirmModal island with aria-label="Delete bucket {name}"
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

use crate::islands::confirm_modal::{ConfirmAction, ConfirmModal};
use crate::types::BucketRow;

/// Format an RFC 3339 timestamp ("2026-06-20T18:09:40Z") as a calendar date
/// ("2026-06-20") to match the design template's clean date column.
fn fmt_date(ts: &str) -> String {
    ts.get(..10).unwrap_or(ts).to_string()
}

/// Format bytes as human-readable string (B, KB, MB, GB, TB).
fn fmt_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    }
}

/// BucketTable component (SSR only).
///
/// Props:
/// - `rows`: list of `BucketRow` from `list_buckets_fn`.
#[component]
pub fn BucketTable(rows: Vec<BucketRow>) -> impl IntoView {
    view! {
        // Bordered rounded card containing a grid table (template: buckets view)
        <div
            style="border:1px solid var(--border);border-radius:9px;overflow:hidden;\
                background:var(--surface);"
            aria-label="Bucket list"
        >
            // Header row — uppercase faint, grid columns: 1fr 150px 110px 130px 44px
            <div style="display:grid;grid-template-columns:1fr 150px 110px 130px 44px;\
                gap:0;padding:9px 16px;border-bottom:1px solid var(--border);\
                font-size:11px;font-weight:600;letter-spacing:.4px;color:var(--faint);\
                text-transform:uppercase;">
                <div>"Name"</div>
                <div>"Created"</div>
                <div style="text-align:right;">"Objects"</div>
                <div style="text-align:right;">"Size"</div>
                <div></div>
            </div>
            {rows.into_iter().map(|row| {
                let href = format!("/ui/buckets/{}", row.name);
                let name_display = row.name.clone();
                // aria-label for accessibility (icon-only delete button — UI-SPEC)
                let delete_label = format!("Delete bucket {}", row.name);
                let count_str = row.object_count.to_string();
                let size_str = fmt_size(row.total_size);

                view! {
                    // Data row — grid, hover surface-2
                    <div
                        style="display:grid;grid-template-columns:1fr 150px 110px 130px 44px;\
                            gap:0;align-items:center;padding:11px 16px;\
                            border-bottom:1px solid var(--border);\
                            transition:background-color 150ms ease;"
                        onmouseover="this.style.backgroundColor='var(--surface-2)'"
                        onmouseout="this.style.backgroundColor=''"
                    >
                        // Name column — bucket icon (accent) + clickable mono name
                        <div style="display:flex;align-items:center;gap:10px;min-width:0;">
                            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" style="flex:none;color:var(--accent);">
                                <ellipse cx="8" cy="4" rx="5.3" ry="2" stroke="currentColor" stroke-width="1.2"/>
                                <path d="M2.7 4v8c0 1.1 2.37 2 5.3 2s5.3-.9 5.3-2V4M2.7 8c0 1.1 2.37 2 5.3 2s5.3-.9 5.3-2" stroke="currentColor" stroke-width="1.2"/>
                            </svg>
                            <a
                                href=href
                                style="font-family:'IBM Plex Mono',monospace;font-size:13px;\
                                    font-weight:500;color:var(--text);text-decoration:none;\
                                    overflow:hidden;text-overflow:ellipsis;white-space:nowrap;"
                            >
                                {name_display}
                            </a>
                        </div>
                        // Created — mono, dim
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--dim);">
                            {fmt_date(&row.created)}
                        </div>
                        // Object count — mono, dim, right-aligned
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--dim);text-align:right;">
                            {count_str}
                        </div>
                        // Total size — mono, text, right-aligned
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--text);text-align:right;">
                            {size_str}
                        </div>
                        // Actions column: delete via ConfirmModal island
                        <div style="display:flex;justify-content:flex-end;">
                            <ConfirmModal
                                action=ConfirmAction::DeleteBucket
                                name=row.name.clone()
                                bucket=row.name.clone()
                                object_key=String::new()
                                aria_label=delete_label
                            />
                        </div>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}
