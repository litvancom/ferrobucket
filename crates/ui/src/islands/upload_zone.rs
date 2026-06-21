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

    // Track in-flight uploads so we can refresh the (server-rendered) object list
    // once everything settles. `pending` counts uploads not yet terminal; `any_done`
    // records whether at least one succeeded. When pending hits 0 with a success, we
    // reload — the object table is SSR-only and won't otherwise show the new file(s).
    let (pending, set_pending) = signal(0u32);
    let (any_done, set_any_done) = signal(false);
    Effect::new(move |_| {
        if pending.get() == 0 && any_done.get() {
            #[cfg(feature = "hydrate")]
            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        }
    });

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

                    set_pending.update(|n| *n += 1);
                    if file_size <= PART_SIZE_BYTES {
                        // Small file: single XHR with byte progress.
                        let (read_progress, set_progress) = signal(0.0f64);
                        let (entry, set_status) = FileEntry::new_small(entry_id, read_progress);
                        let name_entry = FileEntryName { id: entry_id, name: file_name };
                        se.update(|v| v.push((entry, name_entry)));
                        upload_small_file(file, bkt, key, set_progress, entry_id /* kept for tracing */, set_status, set_pending, set_any_done);
                    } else {
                        // Large file: real multipart slicing.
                        let (read_part, set_part) = signal((0u32, 0u32));
                        let (entry, set_status) = FileEntry::new_multipart(entry_id, read_part);
                        let name_entry = FileEntryName { id: entry_id, name: file_name };
                        se.update(|v| v.push((entry, name_entry)));
                        spawn_local(upload_multipart(file, bkt, key, set_part, set_status, set_pending, set_any_done));
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
        // ── Drop zone (dashed-accent overlay style from template) ───────────────
        <div
            style=move || {
                let (bc, bg) = if drag_over.get() {
                    ("var(--accent)", "var(--accent-dim)")
                } else {
                    ("var(--border-2)", "var(--surface)")
                };
                format!(
                    "border:2px dashed {bc};border-radius:12px;padding:24px;\
                    display:flex;flex-direction:column;align-items:center;\
                    justify-content:center;gap:12px;cursor:pointer;\
                    transition:border-color .15s ease,background-color .15s ease;\
                    background:{bg};"
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
            <svg width="34" height="34" viewBox="0 0 16 16" fill="none"
                style=move || format!(
                    "color:{};",
                    if drag_over.get() { "var(--accent)" } else { "var(--faint)" }
                )>
                <path d="M8 11V3M4.5 6.2 8 2.7l3.5 3.5M3 13h10" stroke="currentColor"
                    stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            <div style="font-size:14px;font-weight:600;color:var(--text);">
                "Drop files here or click to upload"
            </div>
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
    // Mono summary: "{done}/{total}" finished.
    let summary = move || {
        let v = entries.get();
        let total = v.len();
        let done = v
            .iter()
            .filter(|(e, _)| e.status.get() != UploadStatus::InProgress)
            .count();
        format!("{done}/{total}")
    };

    view! {
        <Show when=move || !entries.get().is_empty()>
            <div
                style="position:fixed;right:20px;bottom:20px;width:380px;\
                    max-width:calc(100vw - 40px);background:var(--panel);\
                    border:1px solid var(--border-2);border-radius:11px;\
                    box-shadow:var(--shadow);overflow:hidden;z-index:45;"
            >
                // ── Header: title + mono summary + clear button ──────────────────
                <div style="display:flex;align-items:center;gap:9px;\
                    padding:12px 15px;border-bottom:1px solid var(--border);">
                    <div style="font-size:13px;font-weight:600;">"Uploads"</div>
                    <div style="font-family:'IBM Plex Mono',monospace;\
                        font-size:11px;color:var(--faint);">{summary}</div>
                    <button
                        aria-label="Clear finished uploads"
                        style="margin-left:auto;width:24px;height:24px;display:flex;\
                            align-items:center;justify-content:center;border:none;\
                            border-radius:5px;background:transparent;color:var(--faint);\
                            cursor:pointer;"
                        on:click=move |_| {
                            set_entries.update(|v| {
                                v.retain(|(e, _)| e.status.get() == UploadStatus::InProgress)
                            });
                        }
                    >
                        <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
                    </button>
                </div>
                // ── Scrollable list ──────────────────────────────────────────────
                <div style="max-height:280px;overflow-y:auto;">
                    <For
                        each=move || entries.get()
                        key=|(e, _)| e.id
                        children=move |(entry, name_entry)| {
                            let file_name = name_entry.name.clone();
                            let status = entry.status;
                            let progress = entry.progress;

                            let icon_color = move || match status.get() {
                                UploadStatus::Done => "var(--success)",
                                UploadStatus::Error => "var(--danger)",
                                UploadStatus::InProgress => "var(--faint)",
                            };

                            // Progress bar fill (0–100).
                            let pct = move || match progress {
                                ProgressInfo::Small(sig) => sig.get(),
                                ProgressInfo::Multipart(sig) => {
                                    let (cur, total) = sig.get();
                                    if total == 0 { 0.0 } else { cur as f64 / total as f64 * 100.0 }
                                }
                            };
                            // Bar fill color follows status (accent in-flight, success/danger terminal).
                            let bar_color = move || match status.get() {
                                UploadStatus::Done => "var(--success)",
                                UploadStatus::Error => "var(--danger)",
                                UploadStatus::InProgress => "var(--accent)",
                            };
                            // Right label: "{N}%" for both small and multipart.
                            let right = move || format!("{:.0}%", pct());
                            // Optional part label: "Uploading part {N}/{M}" for multipart (D-07).
                            let part_label = move || match progress {
                                ProgressInfo::Small(_) => String::new(),
                                ProgressInfo::Multipart(sig) => {
                                    let (cur, total) = sig.get();
                                    if cur == 0 {
                                        "Starting\u{2026}".to_string()
                                    } else {
                                        format!("Uploading part {cur}/{total}")
                                    }
                                }
                            };
                            let show_part = move || matches!(progress, ProgressInfo::Multipart(_));

                            view! {
                                <div style="padding:11px 15px;border-bottom:1px solid var(--border);">
                                    <div style="display:flex;align-items:center;gap:9px;margin-bottom:7px;">
                                        // Status icon (spinner/✓/✗)
                                        <div style=move || format!(
                                            "width:18px;height:18px;flex:none;display:flex;\
                                            align-items:center;justify-content:center;color:{};",
                                            icon_color()
                                        )>
                                            {move || match status.get() {
                                                UploadStatus::Done => view! {
                                                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><path d="M3.5 8.2 6.5 11l6-6.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
                                                }.into_any(),
                                                UploadStatus::Error => view! {
                                                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
                                                }.into_any(),
                                                UploadStatus::InProgress => view! {
                                                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" style="animation:spin .7s linear infinite;"><path d="M8 1.6a6.4 6.4 0 1 0 6.4 6.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
                                                }.into_any(),
                                            }}
                                        </div>
                                        // Mono filename (truncated)
                                        <span style="flex:1;min-width:0;\
                                            font-family:'IBM Plex Mono',monospace;font-size:12px;\
                                            color:var(--text);white-space:nowrap;overflow:hidden;\
                                            text-overflow:ellipsis;">
                                            {file_name}
                                        </span>
                                        // Right label ("{N}%")
                                        <span style="font-family:'IBM Plex Mono',monospace;\
                                            font-size:11px;color:var(--faint);flex:none;">
                                            {right}
                                        </span>
                                    </div>
                                    // 4px progress bar w/ colored fill
                                    <div style="height:4px;border-radius:3px;\
                                        background:var(--surface-2);overflow:hidden;">
                                        <div style=move || format!(
                                            "height:100%;width:{:.1}%;background:{};\
                                            border-radius:3px;transition:width .25s;",
                                            pct(), bar_color()
                                        ) />
                                    </div>
                                    // Optional part label
                                    <Show when=show_part>
                                        <div style="font-family:'IBM Plex Mono',monospace;\
                                            font-size:10.5px;color:var(--faint);margin-top:5px;">
                                            {part_label}
                                        </div>
                                    </Show>
                                </div>
                            }
                        }
                    />
                </div>
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
    set_pending: WriteSignal<u32>,
    set_any_done: WriteSignal<bool>,
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
        set_any_done.set(true);
        set_pending.update(|n| *n = n.saturating_sub(1));
    }) as Box<dyn FnMut()>);
    xhr.set_onload(Some(load_cb.as_ref().unchecked_ref()));
    load_cb.forget();

    // Error.
    let error_cb = Closure::wrap(Box::new(move || {
        set_status.set(UploadStatus::Error);
        set_pending.update(|n| *n = n.saturating_sub(1));
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
    set_pending: WriteSignal<u32>,
    set_any_done: WriteSignal<bool>,
) {
    // Decrement the in-flight counter exactly once when this upload settles.
    let settle = move || set_pending.update(|n| *n = n.saturating_sub(1));

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
            settle();
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
        settle();
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
        Ok(_) => {
            set_status.set(UploadStatus::Done);
            set_any_done.set(true);
        }
        Err(_) => set_status.set(UploadStatus::Error),
    }
    settle();
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
