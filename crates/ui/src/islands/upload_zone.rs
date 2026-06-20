//! UploadZone island — drag-and-drop + click-to-select file upload.
//!
//! Routes files by size:
//!   - ≤ 8 MiB: single XHR POST with `upload.onprogress` byte progress (Pattern 4).
//!   - > 8 MiB: browser-side `Blob.slice` multipart loop (Pattern 5) with REAL
//!     "part N/M" label driven by actual chunk POSTs (D-07, T-04-13).
//!
//! Multipart endpoint contract (from 04-03-SUMMARY.md):
//!   create:   POST /ui/upload/{bucket}/{key}?action=create          → uploadId
//!   part N:   POST /ui/upload/{bucket}/{key}?uploadId=X&partNumber=N → ETag header
//!   complete: POST /ui/upload/{bucket}/{key}?uploadId=X&action=complete  (JSON [1,2,...])
//!   abort:    POST /ui/upload/{bucket}/{key}?uploadId=X&action=abort
//!
//! Security invariant (DEC-ui-ssr, criterion 5):
//! NO presign/hmac/secret/sigv4 code in this island.
//! NO ferrobucket-storage import (Pitfall 3 — compiles to WASM).
//! Islands only POST raw bytes to /ui/upload.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::islands::upload_panel::{
    FileEntry, FileEntryName, ProgressInfo, UploadStatus, next_entry_id,
};

/// 8 MiB threshold and part size (D-08).
/// Named clearly: 8 * 1024 * 1024 bytes = 8_388_608 bytes = 8 MiB.
pub const PART_SIZE_BYTES: u32 = 8 * 1024 * 1024; // 8_388_608

/// UploadIsland — a SINGLE `#[island]` that owns the upload zone (drag-drop +
/// click-to-select) AND the bottom progress panel.
///
/// GAP-04-01 fix: the zone and panel were previously two separate islands that
/// tried to share a `WriteSignal<Vec<(FileEntry, FileEntryName)>>` via
/// `use_context()`. A `WriteSignal` cannot cross a Leptos island hydration
/// boundary, so `process_files` always early-returned and no upload ever fired.
/// Now ONE island owns the entries signal LOCALLY and passes it directly —
/// the entries write-handle is never shared across an island boundary.
///
/// Props (all serializable — no signals/callbacks as props):
/// - `bucket`: the target bucket name.
/// - `prefix`: the current object key prefix.
#[island]
pub fn UploadIsland(bucket: String, prefix: String) -> impl IntoView {
    let (drag_over, set_drag_over) = signal(false);
    let bucket = StoredValue::new(bucket);
    let prefix = StoredValue::new(prefix);

    // File list owned LOCALLY (GAP-04-01): never crosses an island boundary.
    let (entries, set_entries) = signal::<Vec<(FileEntry, FileEntryName)>>(Vec::new());

    let process_files = move |file_list: web_sys::FileList| {
        #[cfg(feature = "hydrate")]
        {
            let se = set_entries;
            let n = file_list.length();
            for i in 0..n {
                if let Some(file) = file_list.get(i) {
                    let key = {
                        let pfx = prefix.get_value();
                        let name = file.name();
                        if pfx.is_empty() { name } else { format!("{pfx}{name}") }
                    };
                    let bkt = bucket.get_value();
                    let file_name = file.name();
                    let file_size = file.size() as u32;
                    let entry_id = next_entry_id();

                    if file_size <= PART_SIZE_BYTES {
                        // Small file: single XHR with byte progress.
                        let (read_progress, set_progress) = signal(0.0f64);
                        let (entry, set_status) = FileEntry::new_small(entry_id, read_progress);
                        let name_entry = FileEntryName { id: entry_id, name: file_name };
                        se.update(|v| v.push((entry, name_entry)));
                        upload_small_file(file, bkt, key, set_progress, entry_id /* kept for tracing */, set_status);
                    } else {
                        // Large file: real multipart slicing.
                        let (read_part, set_part) = signal((0u32, 0u32));
                        let (entry, set_status) = FileEntry::new_multipart(entry_id, read_part);
                        let name_entry = FileEntryName { id: entry_id, name: file_name };
                        se.update(|v| v.push((entry, name_entry)));
                        spawn_local(upload_multipart(file, bkt, key, set_part, set_status));
                    }
                }
            }
        }
    };

    let on_dragover = move |e: web_sys::DragEvent| {
        e.prevent_default();
        set_drag_over.set(true);
    };
    let on_dragleave = move |_: web_sys::DragEvent| {
        set_drag_over.set(false);
    };
    let on_drop = {
        let pf = process_files;
        move |e: web_sys::DragEvent| {
            e.prevent_default();
            set_drag_over.set(false);
            #[cfg(feature = "hydrate")]
            {
                if let Some(dt) = e.data_transfer() {
                    if let Some(fl) = dt.files() {
                        pf(fl);
                    }
                }
            }
        }
    };

    // Click-to-select via hidden file input.
    let node_ref = NodeRef::<leptos::html::Input>::new();
    let on_input = {
        let pf = process_files;
        move |e: web_sys::Event| {
            #[cfg(feature = "hydrate")]
            {
                use wasm_bindgen::JsCast;
                if let Some(input) = e
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                {
                    if let Some(fl) = input.files() {
                        pf(fl);
                    }
                }
            }
        }
    };

    view! {
        // ── Drop zone ──────────────────────────────────────────────────────────
        <div
            style=move || {
                let bc = if drag_over.get() { "var(--accent)" } else { "var(--border)" };
                let bg = if drag_over.get() { "var(--surface-raised)" } else { "transparent" };
                format!(
                    "border:2px dashed {bc};border-radius:8px;padding:24px;\
                    text-align:center;cursor:pointer;\
                    transition:border-color 150ms ease,background-color 150ms ease;\
                    background-color:{bg};"
                )
            }
            on:dragover=on_dragover
            on:dragleave=on_dragleave
            on:drop=on_drop
            on:click=move |_| {
                if let Some(el) = node_ref.get() {
                    let _ = el.click();
                }
            }
        >
            <p style="font-size:14px;color:var(--text-muted);margin:0;">
                "Drop files here or click to upload"
            </p>
            <input
                node_ref=node_ref
                type="file"
                multiple=true
                style="display:none;"
                on:change=on_input
            />
        </div>

        // ── Bottom progress panel (same island; reads the LOCAL entries signal) ──
        {upload_progress_panel(entries, set_entries)}
    }
}

/// Render the fixed-bottom upload progress panel from the island-local entries
/// signal. Lives in the SAME island as the drop zone (GAP-04-01) — the signals
/// are passed in directly, never via `use_context` across an island boundary.
fn upload_progress_panel(
    entries: ReadSignal<Vec<(FileEntry, FileEntryName)>>,
    set_entries: WriteSignal<Vec<(FileEntry, FileEntryName)>>,
) -> impl IntoView {
    view! {
        <Show when=move || !entries.get().is_empty()>
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
                            set_entries.update(|v| {
                                v.retain(|(e, _)| e.status.get() == UploadStatus::InProgress)
                            });
                        }
                    >
                        "Clear all"
                    </button>
                </div>
                <For
                    each=move || entries.get()
                    key=|(e, _)| e.id
                    children=move |(entry, name_entry)| {
                        let entry_id = entry.id;
                        let file_name = name_entry.name.clone();
                        let status = entry.status;
                        let progress = entry.progress;

                        let dismiss = move |_| {
                            set_entries.update(|v| v.retain(|(e, _)| e.id != entry_id));
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

// ── Small-file XHR upload ──────────────────────────────────────────────────────

/// Upload a file ≤ 8 MiB as a single XHR POST to `/ui/upload/{bucket}/{key}`.
///
/// `set_progress` advances via `upload.onprogress` (Pattern 4).
/// All closures kept alive with `.forget()` (Pitfall 4).
#[cfg(feature = "hydrate")]
fn upload_small_file(
    file: web_sys::File,
    bucket: String,
    key: String,
    set_progress: WriteSignal<f64>,
    _entry_id: u32,
    set_status: WriteSignal<UploadStatus>,
) {
    use wasm_bindgen::prelude::*;

    let xhr = match web_sys::XmlHttpRequest::new() {
        Ok(x) => x,
        Err(_) => return,
    };
    let upload_obj = match xhr.upload() {
        Ok(u) => u,
        Err(_) => return,
    };

    // Progress closure — `.forget()` keeps it alive for XHR lifetime (Pitfall 4).
    let progress_cb = Closure::wrap(Box::new(move |e: web_sys::ProgressEvent| {
        if e.length_computable() && e.total() > 0.0 {
            set_progress.set(e.loaded() / e.total() * 100.0);
        }
    }) as Box<dyn FnMut(_)>);
    upload_obj.set_onprogress(Some(progress_cb.as_ref().unchecked_ref()));
    progress_cb.forget(); // Pitfall 4

    // Load (success).
    let load_cb = Closure::wrap(Box::new(move || {
        set_progress.set(100.0);
        set_status.set(UploadStatus::Done);
    }) as Box<dyn FnMut()>);
    xhr.set_onload(Some(load_cb.as_ref().unchecked_ref()));
    load_cb.forget();

    // Error.
    let error_cb = Closure::wrap(Box::new(move || {
        set_status.set(UploadStatus::Error);
    }) as Box<dyn FnMut()>);
    xhr.set_onerror(Some(error_cb.as_ref().unchecked_ref()));
    error_cb.forget();

    let url = format!("/ui/upload/{bucket}/{key}");
    let _ = xhr.open("POST", &url);
    use wasm_bindgen::JsCast;
    let blob: &web_sys::Blob = file.as_ref();
    let _ = xhr.send_with_opt_blob(Some(blob));
}

// ── Multipart upload ──────────────────────────────────────────────────────────

/// Upload a file > 8 MiB via the multipart endpoint.
///
/// `set_part` is driven by REAL chunk POSTs (D-07, T-04-13).
/// Part label in UploadPanel is bound to the resulting signal — never faked.
#[cfg(feature = "hydrate")]
async fn upload_multipart(
    file: web_sys::File,
    bucket: String,
    key: String,
    set_part: WriteSignal<(u32, u32)>,
    set_status: WriteSignal<UploadStatus>,
) {

    let file_size = file.size(); // f64

    // 8 MiB per part (D-08). Explicit constant name.
    let part_size: f64 = PART_SIZE_BYTES as f64; // 8 * 1024 * 1024 = 8_388_608

    let num_parts = ((file_size / part_size).ceil() as u32).max(1);

    // Step 1: create multipart upload → uploadId.
    let create_url = format!("/ui/upload/{bucket}/{key}?action=create");
    let upload_id = match post_text(&create_url, None).await {
        Ok(id) => id.trim().to_string(),
        Err(_) => {
            set_status.set(UploadStatus::Error);
            return;
        }
    };

    // Step 2: upload each 8 MiB chunk.
    let mut part_numbers: Vec<i32> = Vec::with_capacity(num_parts as usize);
    let mut failed = false;

    for part_num in 1u32..=num_parts {
        let start = (part_num - 1) as f64 * part_size;
        let end = (part_num as f64 * part_size).min(file_size);

        // Browser-side slicing (Pattern 5 — Blob.slice_with_f64_and_f64).
        let blob: &web_sys::Blob = file.as_ref();
        let chunk = match blob.slice_with_f64_and_f64(start, end) {
            Ok(b) => b,
            Err(_) => {
                failed = true;
                break;
            }
        };

        // Update part signal with REAL part number BEFORE posting (D-07, T-04-13).
        // This is what drives the "Uploading part N/M" label in UploadPanel.
        set_part.set((part_num, num_parts));

        let part_url = format!(
            "/ui/upload/{bucket}/{key}?uploadId={upload_id}&partNumber={part_num}"
        );
        match post_blob(&chunk, &part_url).await {
            Ok(_) => {
                // Collect part numbers ascending (Pitfall 7).
                part_numbers.push(part_num as i32);
            }
            Err(_) => {
                failed = true;
                break;
            }
        }
    }

    if failed {
        // Abort on failure (D-07).
        let abort_url =
            format!("/ui/upload/{bucket}/{key}?uploadId={upload_id}&action=abort");
        let _ = post_text(&abort_url, None).await;
        set_status.set(UploadStatus::Error);
        return;
    }

    // Step 3: complete. Parts already in ascending order (sequential loop, Pitfall 7).
    let body_json = format!(
        "[{}]",
        part_numbers
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let complete_url =
        format!("/ui/upload/{bucket}/{key}?uploadId={upload_id}&action=complete");
    match post_text(&complete_url, Some(&body_json)).await {
        Ok(_) => set_status.set(UploadStatus::Done),
        Err(_) => set_status.set(UploadStatus::Error),
    }
}

/// POST a blob via fetch API and return response text.
#[cfg(feature = "hydrate")]
async fn post_blob(blob: &web_sys::Blob, url: &str) -> Result<String, ()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or(())?;
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    let js_val: &wasm_bindgen::JsValue = blob.as_ref();
    init.set_body(js_val);
    let req = web_sys::Request::new_with_str_and_init(url, &init).map_err(|_| ())?;
    let resp_val =
        JsFuture::from(window.fetch_with_request(&req)).await.map_err(|_| ())?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| ())?;
    if !resp.ok() {
        return Err(());
    }
    let text_val =
        JsFuture::from(resp.text().map_err(|_| ())?).await.map_err(|_| ())?;
    Ok(text_val.as_string().unwrap_or_default())
}

/// POST with optional text body and return response text.
#[cfg(feature = "hydrate")]
async fn post_text(url: &str, body: Option<&str>) -> Result<String, ()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or(())?;
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    if let Some(b) = body {
        let js_str = wasm_bindgen::JsValue::from_str(b);
        init.set_body(&js_str);
    }
    let req = web_sys::Request::new_with_str_and_init(url, &init).map_err(|_| ())?;
    let resp_val =
        JsFuture::from(window.fetch_with_request(&req)).await.map_err(|_| ())?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| ())?;
    if !resp.ok() {
        return Err(());
    }
    let text_val =
        JsFuture::from(resp.text().map_err(|_| ())?).await.map_err(|_| ())?;
    Ok(text_val.as_string().unwrap_or_default())
}
