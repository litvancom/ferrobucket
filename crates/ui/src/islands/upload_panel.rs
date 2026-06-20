//! UploadPanel island — bottom bar with reactive per-file progress rows.
//!
//! Renders one row per in-flight or recently-completed file:
//!   - filename, 4px accent progress bar, percentage or "Uploading part N/M" label,
//!     status icon (spinner / ✓ / ✗), dismiss × button.
//!
//! Upload progress label copy (UI-SPEC Copywriting Contract):
//!   - Small file: "{N}%"
//!   - Multipart:  "Uploading part {N}/{M}"
//!
//! The "part N/M" label is driven by the real `(part_num, num_parts)` signal from
//! `upload_multipart` (D-07, T-04-13) — never derived from byte progress.
//!
//! Security invariant (DEC-ui-ssr, criterion 5):
//! NO presign/hmac/secret/sigv4 code in this island.
//! NO ferrobucket-storage import (Pitfall 3 — compiles to WASM).

use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ENTRY_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a unique ID for a new file entry.
pub fn next_entry_id() -> u32 {
    NEXT_ENTRY_ID.fetch_add(1, Ordering::Relaxed)
}

/// Upload lifecycle status.
#[derive(Clone, PartialEq, Debug)]
pub enum UploadStatus {
    InProgress,
    Done,
    Error,
}

/// Progress kind: concrete enum using reactive signals.
/// Avoids Box<dyn Fn> to preserve thread-safety bound required by Leptos.
#[derive(Clone, Copy)]
pub enum ProgressInfo {
    /// Small file (≤ 8 MiB): progress 0.0–100.0.
    Small(ReadSignal<f64>),
    /// Large file (> 8 MiB): (current_part, total_parts).
    Multipart(ReadSignal<(u32, u32)>),
}

// Re-export as UploadKind for backward compat.
pub use ProgressInfo as UploadKind;

/// One in-flight or recently-completed file entry.
#[derive(Clone, Copy)]
pub struct FileEntry {
    pub id: u32,
    pub progress: ProgressInfo,
    pub status: ReadSignal<UploadStatus>,
    // For dismissal and status update by upload_zone.
    pub set_status: WriteSignal<UploadStatus>,
}

/// Name is stored separately to avoid Copy constraint on String.
#[derive(Clone)]
pub struct FileEntryName {
    pub id: u32,
    pub name: String,
}

impl FileEntry {
    pub fn new_small(
        id: u32,
        progress: ReadSignal<f64>,
    ) -> (Self, WriteSignal<UploadStatus>) {
        let (status, set_status) = signal(UploadStatus::InProgress);
        (
            Self {
                id,
                progress: ProgressInfo::Small(progress),
                status,
                set_status,
            },
            set_status,
        )
    }

    pub fn new_multipart(
        id: u32,
        part: ReadSignal<(u32, u32)>,
    ) -> (Self, WriteSignal<UploadStatus>) {
        let (status, set_status) = signal(UploadStatus::InProgress);
        (
            Self {
                id,
                progress: ProgressInfo::Multipart(part),
                status,
                set_status,
            },
            set_status,
        )
    }
}

/// UploadPanel island — bottom fixed bar with per-file progress rows.
///
/// Reads `Vec<(FileEntry, FileEntryName)>` from reactive context. Pages provide
/// the context via `provide_context` before rendering UploadZone + UploadPanel.
#[island]
pub fn UploadPanel() -> impl IntoView {
    let entries: Option<ReadSignal<Vec<(FileEntry, FileEntryName)>>> = use_context();
    let set_entries: Option<WriteSignal<Vec<(FileEntry, FileEntryName)>>> = use_context();

    view! {
        <Show when=move || entries.map(|e| !e.get().is_empty()).unwrap_or(false)>
            <div
                style="position:fixed;bottom:0;left:0;right:0;\
                    background:var(--surface);border-top:1px solid var(--border);\
                    z-index:300;max-height:220px;overflow-y:auto;padding:12px 16px;"
            >
                <div style="display:flex;align-items:center;\
                    justify-content:space-between;margin-bottom:8px;">
                    <span style="font-size:12px;color:var(--text-muted);">"Uploads"</span>
                    <button
                        style="background:none;border:none;cursor:pointer;\
                            font-size:12px;color:var(--text-muted);"
                        on:click=move |_| {
                            if let Some(se) = set_entries {
                                se.update(|v| {
                                    v.retain(|(e, _)| e.status.get() == UploadStatus::InProgress)
                                });
                            }
                        }
                    >
                        "Clear all"
                    </button>
                </div>
                <For
                    each=move || entries.map(|e| e.get()).unwrap_or_default()
                    key=|(e, _)| e.id
                    children=move |(entry, name_entry)| {
                        let entry_id = entry.id;
                        let file_name = name_entry.name.clone();
                        let status = entry.status;
                        let progress = entry.progress;

                        let dismiss = move |_| {
                            if let Some(se) = set_entries {
                                se.update(|v| v.retain(|(e, _)| e.id != entry_id));
                            }
                        };

                        let status_icon = move || match status.get() {
                            UploadStatus::Done => "\u{2713}",     // ✓
                            UploadStatus::Error => "\u{2717}",    // ✗
                            UploadStatus::InProgress => "\u{2026}", // …
                        };
                        let icon_color = move || match status.get() {
                            UploadStatus::Done => "var(--success)",
                            UploadStatus::Error => "var(--destructive)",
                            UploadStatus::InProgress => "var(--text-muted)",
                        };

                        // Progress bar fill (0–100) and label.
                        let pct = move || match progress {
                            ProgressInfo::Small(sig) => sig.get(),
                            ProgressInfo::Multipart(sig) => {
                                let (cur, total) = sig.get();
                                if total == 0 { 0.0 } else { cur as f64 / total as f64 * 100.0 }
                            }
                        };
                        // Label: "{N}%" for small; "Uploading part {N}/{M}" for multipart (D-07).
                        let label = move || match progress {
                            ProgressInfo::Small(sig) => format!("{:.0}%", sig.get()),
                            ProgressInfo::Multipart(sig) => {
                                let (cur, total) = sig.get();
                                if cur == 0 {
                                    "Starting\u{2026}".to_string()
                                } else {
                                    format!("Uploading part {cur}/{total}")
                                }
                            }
                        };

                        let is_done = move || status.get() != UploadStatus::InProgress;

                        view! {
                            <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">
                                // Filename (truncated, IBM Plex Sans 14px)
                                <span style="font-size:14px;color:var(--text);flex:1;\
                                    overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0;">
                                    {file_name}
                                </span>
                                // 4px accent progress bar
                                <div style="width:80px;height:4px;background:var(--border);\
                                    border-radius:2px;flex-shrink:0;">
                                    <div style=move || format!(
                                        "height:4px;background:var(--accent);border-radius:2px;\
                                        width:{:.1}%;transition:width 150ms ease;",
                                        pct()
                                    ) />
                                </div>
                                // Label ("{N}%" or "Uploading part N/M")
                                <span style="font-size:12px;color:var(--text-muted);\
                                    flex-shrink:0;white-space:nowrap;min-width:80px;text-align:right;">
                                    {label}
                                </span>
                                // Status icon (spinner/✓/✗)
                                <span style=move || format!(
                                    "font-size:14px;color:{};flex-shrink:0;",
                                    icon_color()
                                )>
                                    {status_icon}
                                </span>
                                // Dismiss × (only when done/error)
                                <Show when=is_done>
                                    <button
                                        aria-label="Dismiss"
                                        on:click=dismiss
                                        style="background:none;border:none;cursor:pointer;\
                                            color:var(--text-muted);font-size:14px;flex-shrink:0;"
                                    >
                                        {"\u{00d7}"}
                                    </button>
                                </Show>
                            </div>
                        }
                    }
                />
            </div>
        </Show>
    }
}
