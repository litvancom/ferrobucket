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
        <table
            style="width:100%;border-collapse:collapse;"
            aria-label="Object list"
        >
            <thead>
                <tr style="border-bottom:1px solid var(--border);">
                    <th style="text-align:left;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Name"
                    </th>
                    <th style="text-align:right;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Size"
                    </th>
                    <th style="text-align:left;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Last Modified"
                    </th>
                    <th style="text-align:right;padding:8px 16px;font-size:12px;\
                        font-weight:400;color:var(--text-muted);white-space:nowrap;">
                        "Actions"
                    </th>
                </tr>
            </thead>
            <tbody>
                // Folder rows first (CommonPrefixes from ObjectListing.folders)
                {folders.into_iter().map(|folder| {
                    let display = folder.trim_end_matches('/').rsplit('/').next()
                        .map(|s| format!("{s}/"))
                        .unwrap_or_else(|| folder.clone());
                    let href = format!("/ui/buckets/{}?prefix={}",
                        bucket.get_value(),
                        urlencoding_simple(&folder));
                    view! {
                        <tr
                            style="border-bottom:1px solid var(--border);\
                                transition:background-color 150ms ease;"
                            onmouseover="this.style.backgroundColor='var(--surface-raised)'"
                            onmouseout="this.style.backgroundColor=''"
                        >
                            // Folder name — folder icon + IBM Plex Mono 13px, --accent
                            <td style="padding:8px 16px;" colspan="3">
                                <a
                                    href=href
                                    style="display:inline-flex;align-items:center;gap:6px;\
                                        color:var(--accent);text-decoration:none;\
                                        font-family:'IBM Plex Mono',monospace;font-size:13px;"
                                >
                                    // Folder icon (Lucide)
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        width="14" height="14" viewBox="0 0 24 24"
                                        fill="none" stroke="currentColor"
                                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                                        style="color:var(--text-muted);flex-shrink:0;"
                                        aria-hidden="true"
                                    >
                                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                                    </svg>
                                    {display}
                                </a>
                            </td>
                            <td style="padding:8px 16px;text-align:right;" />
                        </tr>
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
                        <tr
                            style="border-bottom:1px solid var(--border);\
                                transition:background-color 150ms ease;"
                            onmouseover="this.style.backgroundColor='var(--surface-raised)'"
                            onmouseout="this.style.backgroundColor=''"
                        >
                            // Name — IBM Plex Mono 13px, SlideOver trigger
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;">
                                <SlideOver
                                    bucket=bucket.get_value()
                                    object_key=obj.key.clone()
                                />
                            </td>
                            // Size — IBM Plex Mono 13px, --text-muted, right-aligned
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;color:var(--text-muted);text-align:right;">
                                {size_str}
                            </td>
                            // Last Modified — IBM Plex Mono 13px, --text-muted
                            <td style="padding:8px 16px;font-family:'IBM Plex Mono',monospace;\
                                font-size:13px;color:var(--text-muted);">
                                {obj.last_modified}
                            </td>
                            // Actions: Download | Copy Presigned URL | Delete
                            <td style="padding:8px 16px;text-align:right;\
                                display:flex;align-items:center;justify-content:flex-end;gap:4px;">
                                // Download link (D-04: /ui/download/{bucket}/{key})
                                <a
                                    href=download_href
                                    download=display_name
                                    title="Download"
                                    aria-label=format!("Download {}", obj.key)
                                    style="display:inline-flex;align-items:center;\
                                        background:none;border:none;cursor:pointer;\
                                        color:var(--text-muted);padding:4px 8px;\
                                        border-radius:4px;font-size:13px;\
                                        text-decoration:none;\
                                        transition:color 150ms ease,\
                                        background-color 150ms ease;"
                                >
                                    // Download icon (Lucide)
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        width="14" height="14" viewBox="0 0 24 24"
                                        fill="none" stroke="currentColor"
                                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                                        aria-hidden="true"
                                    >
                                        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                                        <polyline points="7 10 12 15 17 10"/>
                                        <line x1="12" y1="15" x2="12" y2="3"/>
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
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
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
