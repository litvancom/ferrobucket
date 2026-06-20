//! Upload progress data types — file-entry model + status/progress signals.
//!
//! Upload progress label copy (UI-SPEC Copywriting Contract):
//!   - Small file: "{N}%"
//!   - Multipart:  "Uploading part {N}/{M}"
//!
//! The "part N/M" label is driven by the real `(part_num, num_parts)` signal from
//! `upload_multipart` (D-07, T-04-13) — never derived from byte progress.
//!
//! GAP-04-01: the rendering of these entries now lives in `UploadIsland`
//! (`upload_zone.rs`) so the zone + progress panel share ONE island and ONE
//! locally-owned entries signal — no cross-island `WriteSignal`/`use_context`.
//!
//! Security invariant (DEC-ui-ssr, criterion 5):
//! NO presign/hmac/secret/sigv4 code here.
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
    // For dismissal and status update by the upload island.
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
