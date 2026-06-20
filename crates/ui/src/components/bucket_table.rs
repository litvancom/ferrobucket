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
        <table
            style="width:100%;border-collapse:collapse;"
            aria-label="Bucket list"
        >
            <thead>
                <tr style="border-bottom:1px solid var(--border);">
                    <th style="text-align:left;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Name"
                    </th>
                    <th style="text-align:left;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Created"
                    </th>
                    <th style="text-align:right;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Objects"
                    </th>
                    <th style="text-align:right;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Total Size"
                    </th>
                    <th style="text-align:right;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Actions"
                    </th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(|row| {
                    let href = format!("/ui/buckets/{}", row.name);
                    let name_display = row.name.clone();
                    // aria-label for accessibility (icon-only delete button — UI-SPEC)
                    let delete_label = format!("Delete bucket {}", row.name);
                    let count_str = row.object_count.to_string();
                    let size_str = fmt_size(row.total_size);

                    view! {
                        <tr
                            style="border-bottom:1px solid var(--border);\
                                transition:background-color 150ms ease;"
                            onmouseover="this.style.backgroundColor='var(--surface-raised)'"
                            onmouseout="this.style.backgroundColor=''"
                        >
                            // Name column — clickable, IBM Plex Sans 14px
                            <td style="padding:8px 16px;">
                                <a
                                    href=href
                                    style="color:var(--accent);text-decoration:none;\
                                        font-size:14px;font-weight:400;"
                                >
                                    {name_display}
                                </a>
                            </td>
                            // Created — IBM Plex Mono 13px, --text-muted
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;color:var(--text-muted);">
                                {row.created}
                            </td>
                            // Object count — IBM Plex Mono 13px, --text-muted, right-aligned
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;color:var(--text-muted);text-align:right;">
                                {count_str}
                            </td>
                            // Total size — IBM Plex Mono 13px, --text-muted, right-aligned
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;color:var(--text-muted);text-align:right;">
                                {size_str}
                            </td>
                            // Actions column: delete via ConfirmModal island
                            <td style="padding:8px 16px;text-align:right;">
                                <ConfirmModal
                                    action=ConfirmAction::DeleteBucket
                                    name=row.name.clone()
                                    bucket=row.name.clone()
                                    object_key=String::new()
                                    aria_label=delete_label
                                />
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}
