//! ObjectTable SSR component — table of folders + objects with row actions.
//!
//! Columns: Name | Size | Last Modified | Actions
//! - Folder rows first (from ObjectListing.folders / CommonPrefixes), then object rows.
//! - Folder name: IBM Plex Mono 13px, --accent, clickable → navigate to prefix.
//! - Object name: IBM Plex Mono 13px.
//! - Row actions: Download | Copy Presigned URL | Delete.
//! - Delete: ConfirmModal island with aria-label="Delete object {key}".
//!
//! Security invariant: SSR-only. No presign/hmac/secret/sigv4 code.

use leptos::prelude::*;

use crate::islands::confirm_modal::{ConfirmAction, ConfirmModal};
use crate::islands::{SlideOver};
use crate::types::ObjectRow;

/// Format an RFC 3339 timestamp ("2026-06-20T18:10:05Z") as "2026-06-20 18:10"
/// (date + minute) to match the design template's Last Modified column.
fn fmt_datetime(ts: &str) -> String {
    ts.get(..16).map(|d| d.replace('T', " ")).unwrap_or_else(|| ts.to_string())
}

/// Format bytes as human-readable string.
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

/// ObjectTable component (SSR only).
///
/// Props:
/// - `bucket`: the current bucket name.
/// - `prefix`: the current prefix (for breadcrumb context).
/// - `folders`: common-prefix folder entries from `ObjectListing.folders`.
/// - `objects`: direct object rows from `ObjectListing.objects`.
#[component]
pub fn ObjectTable(
    bucket: String,
    #[prop(default = String::new())] prefix: String,
    #[prop(default = Vec::new())] folders: Vec<String>,
    #[prop(default = Vec::new())] objects: Vec<ObjectRow>,
) -> impl IntoView {
    let bucket = StoredValue::new(bucket);
    let _prefix = StoredValue::new(prefix);

    view! {
        <div role="table" aria-label="Object list">
            // Header row (grid) — uppercase faint labels
            <div
                role="row"
                style="display:grid;grid-template-columns:1fr 120px 170px 80px;gap:0;\
                    padding:9px 16px;border-bottom:1px solid var(--border);\
                    font-size:11px;font-weight:600;letter-spacing:.4px;\
                    color:var(--faint);text-transform:uppercase;"
            >
                <div>"Name"</div>
                <div style="text-align:right;">"Size"</div>
                <div style="text-align:right;">"Last modified"</div>
                <div style="text-align:right;">"Actions"</div>
            </div>

            // Folder rows first (CommonPrefixes from ObjectListing.folders)
            {folders.into_iter().map(|folder| {
                let display = folder.trim_end_matches('/').rsplit('/').next()
                    .map(|s| format!("{s}/"))
                    .unwrap_or_else(|| folder.clone());
                let href = format!("/ui/buckets/{}?prefix={}",
                    bucket.get_value(),
                    urlencoding_simple(&folder));
                view! {
                    <a
                        href=href
                        role="row"
                        style="display:grid;grid-template-columns:1fr 120px 170px 80px;gap:0;\
                            align-items:center;padding:10px 16px;\
                            border-bottom:1px solid var(--border);cursor:pointer;\
                            text-decoration:none;transition:background-color 150ms ease;"
                        onmouseover="this.style.backgroundColor='var(--surface-2)'"
                        onmouseout="this.style.backgroundColor=''"
                    >
                        // Folder name — folder icon (accent) + mono name + "/"
                        <div style="display:flex;align-items:center;gap:10px;min-width:0;">
                            <svg
                                width="16" height="16" viewBox="0 0 16 16" fill="none"
                                style="flex:none;color:var(--accent);"
                                aria-hidden="true"
                            >
                                <path d="M1.5 4.2c0-.6.5-1.1 1.1-1.1h3l1.4 1.6h6.4c.6 0 1.1.5 1.1 1.1v6.4c0 .6-.5 1.1-1.1 1.1H2.6c-.6 0-1.1-.5-1.1-1.1z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
                            </svg>
                            <span style="font-family:'IBM Plex Mono',monospace;font-size:13px;\
                                font-weight:500;color:var(--text);overflow:hidden;\
                                text-overflow:ellipsis;white-space:nowrap;">
                                {display}
                            </span>
                        </div>
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--faint);text-align:right;">"—"</div>
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--faint);text-align:right;">"—"</div>
                        // Chevron
                        <div style="display:flex;justify-content:flex-end;color:var(--faint);">
                            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                                <path d="M6 3.5 10.5 8 6 12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </a>
                }
            }).collect_view()}

            // Object rows
            {objects.into_iter().map(|obj| {
                let key = obj.key.clone();
                let display_name = key.rsplit('/').next().unwrap_or(&key).to_string();
                let download_href = format!("/ui/download/{}/{}", bucket.get_value(), key);
                // aria-label for accessibility (icon-only delete button — UI-SPEC)
                let delete_label = format!("Delete object {}", obj.key);
                let size_str = fmt_size(obj.size);

                view! {
                    <div
                        role="row"
                        style="display:grid;grid-template-columns:1fr 120px 170px 80px;gap:0;\
                            align-items:center;padding:10px 16px;\
                            border-bottom:1px solid var(--border);transition:background-color 150ms ease;"
                        onmouseover="this.style.backgroundColor='var(--surface-2)'"
                        onmouseout="this.style.backgroundColor=''"
                    >
                        // Name — file icon + mono name (SlideOver island is the trigger)
                        <div style="display:flex;align-items:center;gap:10px;min-width:0;">
                            <span style="flex:none;display:flex;color:var(--dim);" aria-hidden="true">
                                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                                    <path d="M3.5 1.5h5L12.5 5.5v9h-9z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
                                    <path d="M8.5 1.5v4h4" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
                                </svg>
                            </span>
                            <span style="font-family:'IBM Plex Mono',monospace;font-size:13px;\
                                color:var(--text);overflow:hidden;text-overflow:ellipsis;\
                                white-space:nowrap;min-width:0;">
                                <SlideOver
                                    bucket=bucket.get_value()
                                    object_key=obj.key.clone()
                                />
                            </span>
                        </div>
                        // Size — mono, --dim, right-aligned
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--dim);text-align:right;">
                            {size_str}
                        </div>
                        // Last modified — mono, --dim, right-aligned
                        <div style="font-family:'IBM Plex Mono',monospace;font-size:12px;\
                            color:var(--dim);text-align:right;">
                            {fmt_datetime(&obj.last_modified)}
                        </div>
                        // Actions: Download | Delete (row-action icon buttons)
                        <div style="display:flex;align-items:center;justify-content:flex-end;gap:2px;">
                            // Download link (D-04: /ui/download/{bucket}/{key})
                            <a
                                href=download_href
                                download=display_name
                                title="Download"
                                aria-label=format!("Download {}", obj.key)
                                style="width:26px;height:26px;display:flex;align-items:center;\
                                    justify-content:center;border:none;border-radius:6px;\
                                    background:transparent;color:var(--faint);cursor:pointer;\
                                    text-decoration:none;transition:color 150ms ease,\
                                    background-color 150ms ease;"
                                onmouseover="this.style.backgroundColor='var(--hover)';this.style.color='var(--text)'"
                                onmouseout="this.style.backgroundColor='transparent';this.style.color='var(--faint)'"
                            >
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                                    <path d="M8 2.5v8M4.8 7.5 8 10.7l3.2-3.2M3 13h10" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                                </svg>
                            </a>
                            // Delete via ConfirmModal island with aria-label
                            <ConfirmModal
                                action=ConfirmAction::DeleteObject
                                name=obj.key.clone()
                                bucket=bucket.get_value()
                                object_key=obj.key.clone()
                                aria_label=delete_label
                            />
                        </div>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

/// Simple percent-encode a string for use in a URL query parameter.
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
