use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use bytes::Bytes;
use futures::{Stream, StreamExt, TryStreamExt};
use time::OffsetDateTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::meta::{BucketInfo, read_sidecar, write_sidecar};
use crate::multipart::{MultipartMeta, read_multipart_meta, write_multipart_meta};
use crate::{ObjectMeta, StorageError, Storage};
use crate::encode::encode_key;
use crate::list::{ListV2Req, ListV2Res};

/// Filesystem-backed S3-compatible storage (ARCHITECTURE §4.1 on-disk layout).
///
/// On-disk layout under `data_root`:
/// ```text
/// <bucket>/
///   .bucket.json            — BucketInfo JSON
///   objects/<encoded-key>   — object bytes (flat, no subdirs)
///   meta/<encoded-key>.json — ObjectMeta JSON
/// ```
///
/// Concurrency: no in-process lock map; correctness relies on filesystem atomicity
/// (temp+rename for PutObject). Documented v1 limitation (D-08).
pub struct FsStorage {
    data_root: PathBuf,
}

impl FsStorage {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self { data_root: data_root.into() }
    }

    fn bucket_path(&self, name: &str) -> PathBuf {
        self.data_root.join(name)
    }

    /// `<bucket>/objects/` — exposed pub(crate) so Plan 03's list free function can resolve it.
    pub(crate) fn objects_dir(&self, bucket: &str) -> PathBuf {
        self.bucket_path(bucket).join("objects")
    }

    /// `<bucket>/meta/` — exposed pub(crate) so Plan 03's list free function can resolve it.
    pub(crate) fn meta_dir(&self, bucket: &str) -> PathBuf {
        self.bucket_path(bucket).join("meta")
    }

    fn object_path(&self, bucket: &str, encoded_key: &str) -> PathBuf {
        self.objects_dir(bucket).join(encoded_key)
    }

    fn meta_path(&self, bucket: &str, encoded_key: &str) -> PathBuf {
        self.meta_dir(bucket).join(format!("{}.json", encoded_key))
    }

    /// `<data>/.uploads/` — staging root for multipart uploads (D-05a: never inside a bucket dir).
    fn uploads_root(&self) -> PathBuf {
        self.data_root.join(".uploads")
    }

    /// `<data>/.uploads/<upload_id>/` — staging directory for one upload.
    /// uploadIds are SERVER-GENERATED uuid v4 (`[0-9a-f-]` only) — no traversal risk (T-03-01).
    fn upload_dir(&self, upload_id: &str) -> PathBuf {
        self.uploads_root().join(upload_id)
    }

    /// `<data>/.uploads/<upload_id>/<part_number>` — on-disk path for one part.
    /// part_number is guarded to be > 0 before this is called (Pitfall 3, T-03-01).
    fn part_path(&self, upload_id: &str, part_number: i32) -> PathBuf {
        self.upload_dir(upload_id).join(part_number.to_string())
    }
}

/// Reserved bucket names: "ui" (the /ui Leptos route prefix, D-01) and "pkg" (the
/// cargo-leptos site-pkg-dir, D-02). Buckets with these names would shadow the console
/// route tree or static-asset prefix and become unreachable via the S3 API.
/// This is a deliberate S3 deviation — documented in README alongside DEC-etag/ACL deviations.
pub const RESERVED_BUCKET_NAMES: &[&str] = &["ui", "pkg"];

/// Validate an S3 bucket name against strict DNS-safe rules (D-09, RESEARCH.md §"Bucket Name Validation").
/// Accepts: 3–63 chars, only `[a-z0-9.-]`, no leading/trailing hyphen, no leading/trailing
/// dot, no consecutive dots, and not formatted as an IPv4 address (WR-07).
pub fn validate_bucket_name(name: &str) -> Result<(), StorageError> {
    // Reserved-name guard (D-02): must run before length check so "ui" (2 chars) and
    // "pkg" (3 chars) both produce the descriptive reserved-name error, not the generic
    // length error that would mislead the caller about the actual rejection reason.
    if RESERVED_BUCKET_NAMES.contains(&name) {
        return Err(StorageError::InvalidBucketName(format!(
            "'{name}' is reserved by the server and cannot be used as a bucket name"
        )));
    }
    let n = name.len();
    if !(3..=63).contains(&n) {
        return Err(StorageError::InvalidBucketName(name.to_owned()));
    }
    let valid_chars = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    let no_leading_trailing_hyphen = !name.starts_with('-') && !name.ends_with('-');
    // Leading/trailing dots produce directories with leading/trailing dots and are
    // rejected by S3 (WR-07).
    let no_leading_trailing_dot = !name.starts_with('.') && !name.ends_with('.');
    let no_consecutive_dots = !name.contains("..");
    // S3 rejects bucket names formatted as IPv4 addresses (e.g. 192.168.0.1).
    let not_ip_formatted = !is_ipv4_formatted(name);
    if !valid_chars
        || !no_leading_trailing_hyphen
        || !no_leading_trailing_dot
        || !no_consecutive_dots
        || !not_ip_formatted
    {
        return Err(StorageError::InvalidBucketName(name.to_owned()));
    }
    Ok(())
}

/// True if `name` looks like a dotted-decimal IPv4 address (four numeric octets).
fn is_ipv4_formatted(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u8>().is_ok()
        })
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Stream `body` to a temp file in `objects_dir` (same-dir avoids EXDEV, Pitfall 1),
/// compute inline MD5 ETag, then atomically rename into place. Returns `(etag, bytes_written)`.
async fn write_object_body(
    objects_dir: &Path,
    encoded_key: &str,
    mut body: impl Stream<Item = std::io::Result<Bytes>> + Unpin,
) -> Result<(String, u64), StorageError> {
    // Temp file MUST be in the same directory as the target to avoid EXDEV on rename (Pitfall 1).
    let tmp = tempfile::Builder::new()
        .tempfile_in(objects_dir)
        .map_err(StorageError::Io)?;
    let (std_file, tmp_path) = tmp
        .keep()
        .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?;

    // After keep() the temp file is persistent (no Drop cleanup), so any error on the
    // write path must best-effort remove it to avoid leaking invisible temp files into
    // objects/ that never decode and accumulate forever (WR-06).
    let write_result = async {
        let mut file = tokio::fs::File::from_std(std_file);
        let mut hasher = crate::etag::EtagHasher::new();
        let mut bytes_written: u64 = 0;
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(StorageError::Io)?;
            hasher.update(&chunk);
            bytes_written += chunk.len() as u64;
            file.write_all(&chunk).await.map_err(StorageError::Io)?;
        }
        file.flush().await.map_err(StorageError::Io)?;
        drop(file);
        Ok::<(String, u64), StorageError>((hasher.finalize(), bytes_written))
    }
    .await;

    let (etag, bytes_written) = match write_result {
        Ok(v) => v,
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }
    };

    let target = objects_dir.join(encoded_key);

    // rename(2) can block under filesystem pressure — run on the blocking thread pool (D-04, D-08).
    let tmp_for_rename = tmp_path.clone();
    let rename = tokio::task::spawn_blocking(move || std::fs::rename(&tmp_for_rename, &target))
        .await
        .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?;
    if let Err(e) = rename {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(StorageError::Io(e));
    }

    Ok((etag, bytes_written))
}

/// Open an object file for streaming read. Maps `NotFound` → `NoSuchKey`.
async fn stream_object(
    path: &Path,
) -> Result<Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>, StorageError> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StorageError::NoSuchKey(path.display().to_string())
        } else {
            StorageError::Io(e)
        }
    })?;
    Ok(Box::pin(ReaderStream::new(file)))
}

// ─── impl Storage ─────────────────────────────────────────────────────────────

#[allow(async_fn_in_trait)]
impl Storage for FsStorage {
    // ── Bucket methods ────────────────────────────────────────────────────────

    async fn list_buckets(&self) -> Result<Vec<BucketInfo>, StorageError> {
        let data_root = self.data_root.clone();
        // read_dir iteration is blocking — use spawn_blocking (PATTERNS.md §Async file operations).
        let entries = tokio::task::spawn_blocking(move || {
            let mut dirs = Vec::new();
            match std::fs::read_dir(&data_root) {
                Ok(rd) => {
                    for entry in rd.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            dirs.push(entry.path());
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            Ok(dirs)
        })
        .await
        .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?
        .map_err(StorageError::Io)?;

        let mut buckets = Vec::new();
        for dir in entries {
            let bucket_json = dir.join(".bucket.json");
            match tokio::fs::read(&bucket_json).await {
                Ok(bytes) => {
                    // A present-but-unparseable .bucket.json is corruption, not "not a
                    // bucket" (WR-05). Surface it instead of silently dropping the bucket,
                    // which would hide it from the API while delete/put still operate on it.
                    let info = serde_json::from_slice::<BucketInfo>(&bytes).map_err(|e| {
                        StorageError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("corrupt .bucket.json at {}: {e}", bucket_json.display()),
                        ))
                    })?;
                    buckets.push(info);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // No .bucket.json — not a ferrobucket bucket directory; skip silently.
                }
                Err(e) => return Err(StorageError::Io(e)),
            }
        }
        Ok(buckets)
    }

    async fn create_bucket(&self, name: &str) -> Result<(), StorageError> {
        validate_bucket_name(name)?;
        let bucket_dir = self.bucket_path(name);
        // Atomic ownership claim (WR-01): create_dir is non-recursive and fails with
        // AlreadyExists, so two concurrent create_bucket calls cannot both "succeed".
        // The data_root itself must exist first (create_dir does not create parents).
        if let Some(parent) = bucket_dir.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(StorageError::Io)?;
        }
        match tokio::fs::create_dir(&bucket_dir).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StorageError::BucketAlreadyExists(name.to_owned()));
            }
            Err(e) => return Err(StorageError::Io(e)),
        }
        // Create objects/ and meta/ subdirectories.
        tokio::fs::create_dir_all(self.objects_dir(name))
            .await
            .map_err(StorageError::Io)?;
        tokio::fs::create_dir_all(self.meta_dir(name))
            .await
            .map_err(StorageError::Io)?;
        // Write .bucket.json.
        let info = BucketInfo {
            name: name.to_owned(),
            created_at: OffsetDateTime::now_utc(),
        };
        let json = serde_json::to_string(&info)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        tokio::fs::write(bucket_dir.join(".bucket.json"), json.as_bytes())
            .await
            .map_err(StorageError::Io)?;
        Ok(())
    }

    async fn delete_bucket(&self, name: &str) -> Result<(), StorageError> {
        validate_bucket_name(name)?;
        let bucket_dir = self.bucket_path(name);
        if !bucket_dir.exists() {
            return Err(StorageError::NoSuchBucket(name.to_owned()));
        }
        // Check that objects/ is empty (S3 requires empty bucket for delete).
        let objects_dir = self.objects_dir(name);
        let objects_dir_clone = objects_dir.clone();
        let is_empty = tokio::task::spawn_blocking(move || {
            match std::fs::read_dir(&objects_dir_clone) {
                Ok(mut rd) => Ok(rd.next().is_none()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?
        .map_err(StorageError::Io)?;

        if !is_empty {
            return Err(StorageError::BucketNotEmpty(name.to_owned()));
        }

        // Remove the structural pieces individually rather than force-deleting the whole
        // tree (WR-02). `remove_dir` is non-recursive: if a concurrent put_object slipped
        // an object into objects/ after the emptiness check, remove_dir(objects) fails with
        // a non-empty error and the live data survives instead of being silently destroyed.
        // The metadata sidecar dir is removed afterwards (best-effort cleanup of an empty
        // meta/ left from prior deletes is fine; a non-empty meta/ surfaces as an error).
        let meta_dir = self.meta_dir(name);
        let bucket_json = bucket_dir.join(".bucket.json");

        // Remove a directory non-recursively, mapping a "directory not empty" failure to
        // BucketNotEmpty. ErrorKind for ENOTEMPTY is not stably exposed across Rust
        // versions, so on any non-NotFound remove failure we re-check whether the dir
        // still has entries: if so, the race materialized and we surface BucketNotEmpty.
        async fn remove_dir_guarded(
            dir: &Path,
            name: &str,
        ) -> Result<(), StorageError> {
            match tokio::fs::remove_dir(dir).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => {
                    let dir_owned = dir.to_path_buf();
                    let still_has_entries = tokio::task::spawn_blocking(move || {
                        std::fs::read_dir(&dir_owned)
                            .map(|mut rd| rd.next().is_some())
                            .unwrap_or(false)
                    })
                    .await
                    .unwrap_or(false);
                    if still_has_entries {
                        Err(StorageError::BucketNotEmpty(name.to_owned()))
                    } else {
                        Err(StorageError::Io(e))
                    }
                }
            }
        }

        remove_dir_guarded(&objects_dir, name).await?;
        // meta/ holds only sidecars; with objects/ empty it should be empty too.
        remove_dir_guarded(&meta_dir, name).await?;
        match tokio::fs::remove_file(&bucket_json).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StorageError::Io(e)),
        }
        match tokio::fs::remove_dir(&bucket_dir).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StorageError::Io(e)),
        }
        Ok(())
    }

    // ── Object methods ────────────────────────────────────────────────────────

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: impl Stream<Item = std::io::Result<Bytes>> + Send,
        content_type: Option<String>,
    ) -> Result<ObjectMeta, StorageError> {
        // Reject traversal-unsafe bucket names BEFORE any path arithmetic (CR-03):
        // an unvalidated `bucket` like "../x" or "/etc" escapes data_root via Path::join.
        validate_bucket_name(bucket)?;
        // Validate bucket exists.
        if !self.bucket_path(bucket).exists() {
            return Err(StorageError::NoSuchBucket(bucket.to_owned()));
        }
        let encoded_key = encode_key(key)?;
        let objects_dir = self.objects_dir(bucket);

        // Stream body to a temp file in the same directory, then atomically rename.
        // Body must be Unpin for StreamExt::next(); pin it on the stack.
        let body = std::pin::pin!(body);
        let (etag, size) = write_object_body(&objects_dir, &encoded_key, body).await?;

        // Build ObjectMeta and write sidecar AFTER the rename (crash-safe ordering, Pattern 7).
        let meta = ObjectMeta {
            key: key.to_owned(),
            size,
            content_type: content_type.unwrap_or_else(|| "application/octet-stream".to_owned()),
            etag,
            last_modified: OffsetDateTime::now_utc(),
        };
        write_sidecar(&self.meta_path(bucket, &encoded_key), &meta).await?;
        Ok(meta)
    }

    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<crate::range::ByteRange>,
    ) -> Result<
        (
            ObjectMeta,
            Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
        ),
        StorageError,
    > {
        validate_bucket_name(bucket)?;
        let encoded_key = encode_key(key)?;
        // Read sidecar first — a missing sidecar means NoSuchKey.
        // Range resolution happens AFTER the sidecar read so a missing key always
        // returns NoSuchKey (not RangeNotSatisfiable).
        let meta = read_sidecar(&self.meta_path(bucket, &encoded_key)).await?;

        let stream: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>> = match range {
            // None: full-object stream — identical to Phase 1 (non-regression).
            None => stream_object(&self.object_path(bucket, &encoded_key)).await?,

            // Some: resolve the range against the actual object length, then seek + bound.
            Some(byte_range) => {
                let resolved = byte_range
                    .resolve(meta.size)
                    .ok_or(StorageError::RangeNotSatisfiable)?;

                // Open the file, seek to the start of the window, and bound to `length`.
                // Security: `resolved.start + resolved.length <= meta.size` is guaranteed
                // by `ByteRange::resolve` (T-02-01, T-02-02) — no read past object bytes.
                let obj_path = self.object_path(bucket, &encoded_key);
                let mut file = tokio::fs::File::open(&obj_path).await.map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        StorageError::NoSuchKey(key.to_owned())
                    } else {
                        StorageError::Io(e)
                    }
                })?;
                file.seek(SeekFrom::Start(resolved.start))
                    .await
                    .map_err(StorageError::Io)?;

                // Bound the read to exactly `length` bytes before handing to ReaderStream.
                let bounded = file.take(resolved.length);
                Box::pin(ReaderStream::new(bounded))
            }
        };

        Ok((meta, stream))
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMeta, StorageError> {
        validate_bucket_name(bucket)?;
        let encoded_key = encode_key(key)?;
        // head reads the sidecar only — no body I/O needed (Open Question 1).
        read_sidecar(&self.meta_path(bucket, &encoded_key)).await
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        validate_bucket_name(bucket)?;
        let encoded_key = encode_key(key)?;
        let obj_path = self.object_path(bucket, &encoded_key);
        let meta_path = self.meta_path(bucket, &encoded_key);

        // Treat absent object as NoSuchKey (mirrors S3 HEAD semantics for head_object).
        // Note: S3 DeleteObject technically succeeds for absent keys, but ferrobucket v1
        // returns NoSuchKey for explicit deletes of missing objects. This can be loosened
        // in a future plan without breaking the Storage contract.
        match tokio::fs::remove_file(&obj_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::NoSuchKey(key.to_owned()));
            }
            Err(e) => return Err(StorageError::Io(e)),
        }
        // Remove sidecar. Only NotFound is ignored (WR-03): swallowing every error
        // would leak an orphaned sidecar after a real I/O failure, leaving HEAD
        // resolving while GET 404s for the now-bodyless key.
        match tokio::fs::remove_file(&meta_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StorageError::Io(e)),
        }
        Ok(())
    }

    async fn list_objects_v2(
        &self,
        bucket: &str,
        req: ListV2Req,
    ) -> Result<ListV2Res, StorageError> {
        validate_bucket_name(bucket)?;
        // Validate bucket exists.
        if !self.bucket_path(bucket).exists() {
            return Err(StorageError::NoSuchBucket(bucket.to_owned()));
        }
        // Thin delegation to crate::list::list_objects_v2 (Plan 03 fills the algorithm).
        crate::list::list_objects_v2(self, bucket, req).await
    }

    // ── Multipart upload methods ──────────────────────────────────────────────

    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<String>,
    ) -> Result<String, StorageError> {
        // Reject traversal-unsafe bucket names before any path arithmetic (T-03-02).
        validate_bucket_name(bucket)?;
        // Bucket must exist (same guard as put_object).
        if !self.bucket_path(bucket).exists() {
            return Err(StorageError::NoSuchBucket(bucket.to_owned()));
        }

        let upload_id = uuid::Uuid::new_v4().to_string();
        let dir = self.upload_dir(&upload_id);

        // Create the staging directory before writing the sidecar (Pitfall 4).
        tokio::fs::create_dir_all(&dir).await.map_err(StorageError::Io)?;

        // Write _meta.json atomically so Complete can resolve bucket/key/content_type.
        let meta = MultipartMeta {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            content_type,
        };
        let meta_path = dir.join("_meta.json");
        write_multipart_meta(&meta_path, &meta).await?;

        Ok(upload_id)
    }

    async fn upload_part(
        &self,
        _bucket: &str,
        upload_id: &str,
        part_number: i32,
        body: impl Stream<Item = std::io::Result<Bytes>> + Send,
    ) -> Result<String, StorageError> {
        // Guard: part numbers must be positive (Pitfall 3, T-03-01).
        if part_number <= 0 {
            return Err(StorageError::InvalidPartNumber(part_number));
        }

        let dir = self.upload_dir(upload_id);
        if !dir.exists() {
            return Err(StorageError::NoSuchUpload(upload_id.to_owned()));
        }

        // Stream body to a temp file in the staging dir (same-dir avoids EXDEV — Pitfall 1).
        let tmp = tempfile::Builder::new()
            .tempfile_in(&dir)
            .map_err(StorageError::Io)?;
        let (std_file, tmp_path) = tmp
            .keep()
            .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?;

        let part_target = self.part_path(upload_id, part_number);
        let body = std::pin::pin!(body);

        let write_result = async {
            let mut file = tokio::fs::File::from_std(std_file);
            let mut hasher = crate::etag::EtagHasher::new();
            let mut body = body;
            while let Some(chunk) = body.next().await {
                let chunk = chunk.map_err(StorageError::Io)?;
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(StorageError::Io)?;
            }
            file.flush().await.map_err(StorageError::Io)?;
            drop(file);
            Ok::<String, StorageError>(hasher.finalize())
        }
        .await;

        let etag = match write_result {
            Ok(e) => e,
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e);
            }
        };

        // Atomic rename into the part slot.
        let tmp_for_rename = tmp_path.clone();
        let rename = tokio::task::spawn_blocking(move || std::fs::rename(&tmp_for_rename, &part_target))
            .await
            .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?;
        if let Err(e) = rename {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(StorageError::Io(e));
        }

        Ok(etag)
    }

    async fn complete_multipart_upload(
        &self,
        _bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<i32>,
    ) -> Result<ObjectMeta, StorageError> {
        let upload_dir = self.upload_dir(upload_id);

        // Read MultipartMeta sidecar — NotFound maps to NoSuchUpload.
        let meta_path = upload_dir.join("_meta.json");
        let mp_meta = read_multipart_meta(&meta_path).await?;

        // Sort part numbers ascending — D-05b: assemble in order regardless of upload order.
        let mut sorted_parts = parts;
        sorted_parts.sort();
        let part_count = sorted_parts.len();

        // Build chained stream over sorted parts (D-05b streaming concat, never buffered).
        // Each part file is opened on demand and read via ReaderStream before the next opens.
        let staging_dir = upload_dir.clone();
        let chained = futures::stream::iter(sorted_parts)
            .then(move |part_num| {
                let path = staging_dir.join(part_num.to_string());
                async move {
                    let f = tokio::fs::File::open(&path).await?;
                    Ok::<_, std::io::Error>(ReaderStream::new(f))
                }
            })
            .try_flatten();

        // Assemble via the atomic write pattern (temp+rename), computing the assembled ETag.
        let objects_dir = self.objects_dir(&mp_meta.bucket);
        // Ensure objects_dir exists (bucket was validated at create_multipart_upload time).
        tokio::fs::create_dir_all(&objects_dir).await.map_err(StorageError::Io)?;

        let encoded_key = encode_key(key)?;

        let tmp = tempfile::Builder::new()
            .tempfile_in(&objects_dir)
            .map_err(StorageError::Io)?;
        let (std_file, tmp_path) = tmp
            .keep()
            .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?;

        let write_result = async {
            let mut file = tokio::fs::File::from_std(std_file);
            let mut hasher = crate::etag::EtagHasher::new();
            let mut bytes_written: u64 = 0;
            futures::pin_mut!(chained);
            while let Some(chunk) = chained.next().await {
                let chunk = chunk.map_err(StorageError::Io)?;
                hasher.update(&chunk);
                bytes_written += chunk.len() as u64;
                file.write_all(&chunk).await.map_err(StorageError::Io)?;
            }
            file.flush().await.map_err(StorageError::Io)?;
            drop(file);
            // D-07: AWS-shaped multipart ETag = <md5hex_of_assembled_bytes>-<part_count>
            let assembled_etag = format!("{}-{}", hasher.finalize(), part_count);
            Ok::<(String, u64), StorageError>((assembled_etag, bytes_written))
        }
        .await;

        let (assembled_etag, size) = match write_result {
            Ok(v) => v,
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e);
            }
        };

        // Atomic rename of assembled object into place.
        let target = objects_dir.join(&encoded_key);
        let tmp_for_rename = tmp_path.clone();
        let rename = tokio::task::spawn_blocking(move || std::fs::rename(&tmp_for_rename, &target))
            .await
            .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?;
        if let Err(e) = rename {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(StorageError::Io(e));
        }

        // Write sidecar AFTER object rename (crash-safe ordering, Pattern 7).
        let obj_meta = ObjectMeta {
            key: mp_meta.key.clone(),
            size,
            content_type: mp_meta.content_type.unwrap_or_else(|| "application/octet-stream".to_owned()),
            etag: assembled_etag,
            last_modified: OffsetDateTime::now_utc(),
        };
        write_sidecar(&self.meta_path(&mp_meta.bucket, &encoded_key), &obj_meta).await?;

        // Clean up staging directory AFTER successful object + sidecar write (D-05c).
        let _ = tokio::fs::remove_dir_all(&upload_dir).await;

        Ok(obj_meta)
    }

    async fn abort_multipart_upload(
        &self,
        _bucket: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        let dir = self.upload_dir(upload_id);
        tokio::fs::remove_dir_all(&dir).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NoSuchUpload(upload_id.to_owned())
            } else {
                StorageError::Io(e)
            }
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use tempfile::tempdir;

    // Helper: build a body stream from a byte slice.
    fn body_from(data: &'static [u8]) -> impl Stream<Item = std::io::Result<Bytes>> + Send {
        stream::iter(vec![Ok(Bytes::from_static(data))])
    }

    // Helper: build a body stream from owned bytes.
    fn body_owned(data: Vec<u8>) -> impl Stream<Item = std::io::Result<Bytes>> + Send {
        stream::iter(vec![Ok(Bytes::from(data))])
    }

    // ── Bucket name validation ────────────────────────────────────────────────

    /// REQ-ui-bucket-list / Wave-0 / VALIDATION.md: reserved bucket names are rejected
    /// with a clear, user-visible error containing "reserved".
    #[test]
    fn validate_bucket_name_reserved() {
        // "ui" is reserved (the /ui Leptos route prefix, D-01).
        match validate_bucket_name("ui") {
            Err(StorageError::InvalidBucketName(msg)) => {
                assert!(
                    msg.contains("reserved"),
                    "error message for 'ui' must contain 'reserved', got: {msg}"
                );
            }
            other => panic!("expected InvalidBucketName for 'ui', got {:?}", other),
        }

        // "pkg" is reserved (the cargo-leptos site-pkg-dir, D-02).
        match validate_bucket_name("pkg") {
            Err(StorageError::InvalidBucketName(msg)) => {
                assert!(
                    msg.contains("reserved"),
                    "error message for 'pkg' must contain 'reserved', got: {msg}"
                );
            }
            other => panic!("expected InvalidBucketName for 'pkg', got {:?}", other),
        }

        // Normal names must still pass (regression: existing DNS-safety checks unaffected).
        assert!(
            validate_bucket_name("my-bucket").is_ok(),
            "valid bucket name 'my-bucket' must still pass"
        );

        // The Display string for "ui" must contain "reserved".
        let err = validate_bucket_name("ui").unwrap_err();
        let display = err.to_string();
        assert!(
            display.contains("reserved"),
            "Display of InvalidBucketName for 'ui' must contain 'reserved', got: {display}"
        );
    }

    #[test]
    fn bucket_name_validation_accepts_valid() {
        assert!(validate_bucket_name("abc").is_ok());
        assert!(validate_bucket_name("my-bucket").is_ok());
        assert!(validate_bucket_name("a.b.c").is_ok());
        assert!(validate_bucket_name("a1b2c3").is_ok());
    }

    #[test]
    fn bucket_name_validation_rejects_too_short() {
        // 2 chars — must be rejected
        assert!(matches!(
            validate_bucket_name("ab"),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_too_long() {
        // 64 chars — must be rejected (max is 63)
        let name = "a".repeat(64);
        assert!(matches!(
            validate_bucket_name(&name),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_leading_hyphen() {
        assert!(matches!(
            validate_bucket_name("-lead"),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_trailing_hyphen() {
        assert!(matches!(
            validate_bucket_name("trail-"),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_consecutive_dots() {
        assert!(matches!(
            validate_bucket_name("a..b"),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_uppercase() {
        assert!(matches!(
            validate_bucket_name("MyBucket"),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    // ── Bucket CRUD ───────────────────────────────────────────────────────────

    #[test]
    fn bucket_name_validation_rejects_leading_trailing_dot() {
        // WR-07
        assert!(matches!(
            validate_bucket_name(".bucket"),
            Err(StorageError::InvalidBucketName(_))
        ));
        assert!(matches!(
            validate_bucket_name("bucket."),
            Err(StorageError::InvalidBucketName(_))
        ));
    }

    #[test]
    fn bucket_name_validation_rejects_ipv4() {
        // WR-07: IPv4-formatted names must be rejected.
        assert!(matches!(
            validate_bucket_name("192.168.0.1"),
            Err(StorageError::InvalidBucketName(_))
        ));
        assert!(matches!(
            validate_bucket_name("10.0.0.255"),
            Err(StorageError::InvalidBucketName(_))
        ));
        // Dotted names that are NOT all numeric octets remain valid.
        assert!(validate_bucket_name("a.b.c").is_ok());
        assert!(validate_bucket_name("1.2.3.4.5").is_ok());
    }

    #[tokio::test]
    async fn create_bucket_creates_dir() {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        storage.create_bucket("buk").await.unwrap();

        assert!(dir.path().join("buk/.bucket.json").exists());
        assert!(dir.path().join("buk/objects").is_dir());
        assert!(dir.path().join("buk/meta").is_dir());
    }

    #[tokio::test]
    async fn bucket_crud() {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());

        storage.create_bucket("my-bucket").await.unwrap();
        let buckets = storage.list_buckets().await.unwrap();
        assert!(buckets.iter().any(|b| b.name == "my-bucket"));

        storage.delete_bucket("my-bucket").await.unwrap();
        let buckets = storage.list_buckets().await.unwrap();
        assert!(!buckets.iter().any(|b| b.name == "my-bucket"));
    }

    #[tokio::test]
    async fn create_existing_bucket_errors() {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        storage.create_bucket("buk").await.unwrap();
        match storage.create_bucket("buk").await {
            Err(StorageError::BucketAlreadyExists(_)) => {}
            other => panic!("expected BucketAlreadyExists, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn list_buckets_surfaces_corrupt_metadata() {
        // WR-05 regression: a present-but-corrupt .bucket.json must surface an error,
        // not silently hide the bucket from list_buckets.
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        storage.create_bucket("good-bucket").await.unwrap();
        // Corrupt the metadata.
        std::fs::write(dir.path().join("good-bucket/.bucket.json"), b"{not valid json").unwrap();

        match storage.list_buckets().await {
            Err(StorageError::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => {}
            other => panic!("expected InvalidData for corrupt .bucket.json, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn delete_absent_bucket_errors() {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        match storage.delete_bucket("nonexistent").await {
            Err(StorageError::NoSuchBucket(_)) => {}
            other => panic!("expected NoSuchBucket, got {:?}", other),
        }
    }

    // delete_nonempty_bucket is tested in the object tests below.

    // ── Object methods ────────────────────────────────────────────────────────

    async fn make_storage_with_bucket() -> (tempfile::TempDir, FsStorage) {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        storage.create_bucket("test-bucket").await.unwrap();
        (dir, storage)
    }

    #[tokio::test]
    async fn put_then_get_metadata() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let meta = storage
            .put_object(
                "test-bucket",
                "hello.txt",
                body_from(b"hello world"),
                Some("text/plain".to_owned()),
            )
            .await
            .unwrap();

        let head = storage.head_object("test-bucket", "hello.txt").await.unwrap();
        assert_eq!(head.content_type, "text/plain");
        assert!(!head.etag.is_empty());
        assert_eq!(head.size, 11);
        assert!(head.last_modified.unix_timestamp() > 0);
        // returned ObjectMeta from put should also be consistent
        assert_eq!(meta.size, 11);
        assert_eq!(meta.content_type, "text/plain");
    }

    #[tokio::test]
    async fn put_get_bytes_roundtrip() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let data = b"the quick brown fox";
        storage
            .put_object("test-bucket", "fox.txt", body_from(data), None)
            .await
            .unwrap();

        let (_, mut stream) = storage.get_object("test-bucket", "fox.txt", None).await.unwrap();
        let mut received = Vec::new();
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, data);
    }

    #[tokio::test]
    async fn put_get_bytes_roundtrip_zero_byte() {
        let (_dir, storage) = make_storage_with_bucket().await;
        storage
            .put_object("test-bucket", "empty.bin", body_from(b""), None)
            .await
            .unwrap();

        let (meta, mut stream) = storage
            .get_object("test-bucket", "empty.bin", None)
            .await
            .unwrap();
        assert_eq!(meta.size, 0);

        use futures::StreamExt;
        let mut received = Vec::new();
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, b"");
    }

    #[tokio::test]
    async fn etag_is_md5_hex() {
        let (_dir, storage) = make_storage_with_bucket().await;
        // MD5("hello") = 5d41402abc4b2a76b9719d911017c592
        storage
            .put_object("test-bucket", "hi.txt", body_from(b"hello"), None)
            .await
            .unwrap();

        let meta = storage.head_object("test-bucket", "hi.txt").await.unwrap();
        assert_eq!(meta.etag, "5d41402abc4b2a76b9719d911017c592");
    }

    #[tokio::test]
    async fn get_missing_key_errors() {
        let (_dir, storage) = make_storage_with_bucket().await;
        match storage.get_object("test-bucket", "missing.txt", None).await {
            Err(StorageError::NoSuchKey(_)) => {}
            Ok(_) => panic!("expected NoSuchKey for get_object, got Ok"),
            Err(e) => panic!("expected NoSuchKey for get_object, got Err({:?})", e),
        }
        match storage.head_object("test-bucket", "missing.txt").await {
            Err(StorageError::NoSuchKey(_)) => {}
            Ok(_) => panic!("expected NoSuchKey for head_object, got Ok"),
            Err(e) => panic!("expected NoSuchKey for head_object, got Err({:?})", e),
        }
    }

    #[tokio::test]
    async fn put_object_atomic_no_partial() {
        let (_dir, storage) = make_storage_with_bucket().await;
        storage
            .put_object("test-bucket", "data.bin", body_owned(vec![1u8, 2, 3, 4]), None)
            .await
            .unwrap();

        // objects/ should have exactly one entry (the encoded key), no .tmp-* leftovers.
        let encoded = encode_key("data.bin").unwrap();
        let objects_dir = storage.objects_dir("test-bucket");
        let entries: Vec<_> = std::fs::read_dir(&objects_dir)
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name().to_string_lossy().as_ref(), encoded);
    }

    #[tokio::test]
    async fn delete_nonempty_bucket_errors_and_preserves_data() {
        // WR-02 regression: deleting a non-empty bucket must fail with BucketNotEmpty
        // and must NOT destroy the live object (no force remove_dir_all).
        let (_dir, storage) = make_storage_with_bucket().await;
        storage
            .put_object("test-bucket", "keep.txt", body_from(b"data"), None)
            .await
            .unwrap();

        match storage.delete_bucket("test-bucket").await {
            Err(StorageError::BucketNotEmpty(_)) => {}
            other => panic!("expected BucketNotEmpty, got {:?}", other),
        }

        // The object must still be readable.
        let head = storage.head_object("test-bucket", "keep.txt").await.unwrap();
        assert_eq!(head.size, 4);
    }

    #[tokio::test]
    async fn object_ops_reject_traversal_bucket_names() {
        // CR-03: object operations must validate the bucket name before any path
        // arithmetic, so a traversal/absolute bucket cannot escape data_root.
        let (_dir, storage) = make_storage_with_bucket().await;
        for bad in &["../other", "/etc", ".."] {
            match storage
                .put_object(bad, "k", body_from(b"x"), None)
                .await
            {
                Err(StorageError::InvalidBucketName(_)) => {}
                other => panic!("put_object({:?}) expected InvalidBucketName, got {:?}", bad, other),
            }
            match storage.get_object(bad, "k", None).await {
                Err(StorageError::InvalidBucketName(_)) => {}
                Err(e) => panic!("get_object({:?}) expected InvalidBucketName, got {:?}", bad, e),
                Ok(_) => panic!("get_object({:?}) expected InvalidBucketName, got Ok", bad),
            }
            match storage.head_object(bad, "k").await {
                Err(StorageError::InvalidBucketName(_)) => {}
                other => panic!("head_object({:?}) expected InvalidBucketName, got {:?}", bad, other),
            }
            match storage.delete_object(bad, "k").await {
                Err(StorageError::InvalidBucketName(_)) => {}
                other => panic!("delete_object({:?}) expected InvalidBucketName, got {:?}", bad, other),
            }
        }
    }

    #[tokio::test]
    async fn put_object_rejects_traversal_key() {
        // CR-03: a key like ".." must be rejected on the write path (encode_key guard),
        // never written to the objects-dir parent.
        let (_dir, storage) = make_storage_with_bucket().await;
        match storage
            .put_object("test-bucket", "..", body_from(b"x"), None)
            .await
        {
            Err(StorageError::InvalidKey) => {}
            other => panic!("expected InvalidKey for key '..', got {:?}", other),
        }
    }

    #[tokio::test]
    async fn put_object_stream_error_leaves_no_temp_files() {
        // WR-06 regression: a body stream that errors mid-write must not leak a
        // persistent temp file into objects/.
        let (_dir, storage) = make_storage_with_bucket().await;
        let err_body = stream::iter(vec![
            Ok(Bytes::from_static(b"partial")),
            Err(std::io::Error::other("boom")),
        ]);
        let res = storage
            .put_object("test-bucket", "broken.bin", err_body, None)
            .await;
        assert!(res.is_err(), "stream error must propagate");

        let objects_dir = storage.objects_dir("test-bucket");
        let entries: Vec<_> = std::fs::read_dir(&objects_dir).unwrap().flatten().collect();
        assert!(
            entries.is_empty(),
            "no temp files must remain after a failed put, found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn delete_object_removes_both() {
        let (_dir, storage) = make_storage_with_bucket().await;
        storage
            .put_object("test-bucket", "to-delete.txt", body_from(b"bye"), None)
            .await
            .unwrap();

        storage
            .delete_object("test-bucket", "to-delete.txt")
            .await
            .unwrap();

        // Both body and sidecar must be gone.
        let encoded = encode_key("to-delete.txt").unwrap();
        assert!(!storage.object_path("test-bucket", &encoded).exists());
        assert!(!storage.meta_path("test-bucket", &encoded).exists());

        // Subsequent get_object must return NoSuchKey.
        match storage.get_object("test-bucket", "to-delete.txt", None).await {
            Err(StorageError::NoSuchKey(_)) => {}
            Ok(_) => panic!("expected NoSuchKey after delete, got Ok"),
            Err(e) => panic!("expected NoSuchKey after delete, got Err({:?})", e),
        }
    }

    // ── Multipart upload tests (REQ-multipart) ────────────────────────────────

    #[tokio::test]
    async fn multipart_create_returns_upload_id() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "test.bin", None)
            .await
            .unwrap();
        // UUID v4 format: 8-4-4-4-12 hex chars with hyphens
        assert!(!upload_id.is_empty());
        assert_eq!(upload_id.len(), 36);
        assert!(upload_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[tokio::test]
    async fn multipart_create_absent_bucket_errors() {
        let dir = tempdir().unwrap();
        let storage = FsStorage::new(dir.path());
        match storage
            .create_multipart_upload("nonexistent", "k", None)
            .await
        {
            Err(StorageError::NoSuchBucket(_)) => {}
            other => panic!("expected NoSuchBucket, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn upload_part_zero_returns_invalid_part_number() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "k", None)
            .await
            .unwrap();
        match storage
            .upload_part("test-bucket", &upload_id, 0, body_from(b"x"))
            .await
        {
            Err(StorageError::InvalidPartNumber(0)) => {}
            other => panic!("expected InvalidPartNumber(0), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn upload_part_negative_returns_invalid_part_number() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "k", None)
            .await
            .unwrap();
        match storage
            .upload_part("test-bucket", &upload_id, -5, body_from(b"x"))
            .await
        {
            Err(StorageError::InvalidPartNumber(-5)) => {}
            other => panic!("expected InvalidPartNumber(-5), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn multipart_roundtrip_two_parts_in_order() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "assembled.bin", Some("application/octet-stream".to_owned()))
            .await
            .unwrap();

        // Upload part 1 and part 2.
        storage
            .upload_part("test-bucket", &upload_id, 1, body_from(b"part-one-"))
            .await
            .unwrap();
        storage
            .upload_part("test-bucket", &upload_id, 2, body_from(b"part-two"))
            .await
            .unwrap();

        // Complete with parts in natural order.
        let obj_meta = storage
            .complete_multipart_upload("test-bucket", "assembled.bin", &upload_id, vec![1, 2])
            .await
            .unwrap();

        // ETag must match <md5hex>-2 pattern (D-07).
        assert!(
            obj_meta.etag.ends_with("-2"),
            "assembled ETag must end with -2, got: {}",
            obj_meta.etag
        );
        let parts: Vec<&str> = obj_meta.etag.splitn(2, '-').collect();
        assert_eq!(parts[0].len(), 32, "ETag prefix must be 32-char MD5 hex");
        assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));

        // Object body must be part1 || part2.
        let (_, mut stream) = storage
            .get_object("test-bucket", "assembled.bin", None)
            .await
            .unwrap();
        let mut received = Vec::new();
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, b"part-one-part-two");
    }

    #[tokio::test]
    async fn multipart_roundtrip_out_of_order_parts_assembled_ascending() {
        // Parts submitted as [2, 1] must assemble in ascending order (1 then 2).
        let (_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "ordered.bin", None)
            .await
            .unwrap();

        storage
            .upload_part("test-bucket", &upload_id, 1, body_from(b"FIRST"))
            .await
            .unwrap();
        storage
            .upload_part("test-bucket", &upload_id, 2, body_from(b"SECOND"))
            .await
            .unwrap();

        // Complete with parts in REVERSE order — must still assemble ascending.
        let obj_meta = storage
            .complete_multipart_upload("test-bucket", "ordered.bin", &upload_id, vec![2, 1])
            .await
            .unwrap();

        assert!(obj_meta.etag.ends_with("-2"));

        let (_, mut stream) = storage
            .get_object("test-bucket", "ordered.bin", None)
            .await
            .unwrap();
        let mut received = Vec::new();
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        // Should be FIRST||SECOND, not SECOND||FIRST.
        assert_eq!(received, b"FIRSTSECOND");
    }

    #[tokio::test]
    async fn abort_removes_staging_directory() {
        let (root_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "to-abort.bin", None)
            .await
            .unwrap();

        storage
            .upload_part("test-bucket", &upload_id, 1, body_from(b"data"))
            .await
            .unwrap();

        // Verify staging dir exists before abort.
        let staging = root_dir.path().join(".uploads").join(&upload_id);
        assert!(staging.exists(), "staging dir must exist before abort");

        storage
            .abort_multipart_upload("test-bucket", &upload_id)
            .await
            .unwrap();

        assert!(!staging.exists(), "staging dir must not exist after abort");
    }

    #[tokio::test]
    async fn abort_absent_upload_returns_nosuchupload() {
        let (_dir, storage) = make_storage_with_bucket().await;
        match storage
            .abort_multipart_upload("test-bucket", "00000000-0000-0000-0000-000000000000")
            .await
        {
            Err(StorageError::NoSuchUpload(_)) => {}
            other => panic!("expected NoSuchUpload, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn complete_cleans_staging_directory() {
        let (root_dir, storage) = make_storage_with_bucket().await;
        let upload_id = storage
            .create_multipart_upload("test-bucket", "clean.bin", None)
            .await
            .unwrap();

        storage
            .upload_part("test-bucket", &upload_id, 1, body_from(b"hello"))
            .await
            .unwrap();

        let staging = root_dir.path().join(".uploads").join(&upload_id);
        assert!(staging.exists());

        storage
            .complete_multipart_upload("test-bucket", "clean.bin", &upload_id, vec![1])
            .await
            .unwrap();

        assert!(!staging.exists(), "staging dir must be cleaned up after complete");
    }

    // ── Ranged get_object tests (D-04, T-02-01) ───────────────────────────────

    /// Helper: collect stream bytes into a Vec.
    async fn collect_stream(
        stream: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
    ) -> Vec<u8> {
        use futures::StreamExt;
        let mut out = Vec::new();
        futures::pin_mut!(stream);
        while let Some(chunk) = stream.next().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    }

    // Non-regression: get_object(None) returns the full object byte-for-byte.
    #[tokio::test]
    async fn get_object_none_returns_full_object() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1000).collect();
        storage
            .put_object("test-bucket", "data.bin", body_owned(data.clone()), None)
            .await
            .unwrap();

        let (meta, stream) = storage
            .get_object("test-bucket", "data.bin", None)
            .await
            .unwrap();

        // meta.size must be the full object size.
        assert_eq!(meta.size, 1000);
        let received = collect_stream(stream).await;
        assert_eq!(received, data, "None range must return full object bytes");
    }

    // D-04 bytes=0-499 over 1000 bytes → exactly 500 bytes equal to bytes[0..500].
    #[tokio::test]
    async fn get_object_range_full_bytes_0_499() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1000).collect();
        storage
            .put_object("test-bucket", "data.bin", body_owned(data.clone()), None)
            .await
            .unwrap();

        let (meta, stream) = storage
            .get_object(
                "test-bucket",
                "data.bin",
                Some(crate::range::ByteRange::full_from(0, 499)),
            )
            .await
            .unwrap();

        // meta.size stays the FULL object size (the adapter sets Content-Range from the window).
        assert_eq!(meta.size, 1000, "meta.size must be full object size for ranged get");
        let received = collect_stream(stream).await;
        assert_eq!(received.len(), 500);
        assert_eq!(received, &data[0..500], "ranged get must return exact byte window");
    }

    // D-04 bytes=500- over 1000 bytes → bytes 500..1000 (500 bytes).
    #[tokio::test]
    async fn get_object_range_open_from_500() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1000).collect();
        storage
            .put_object("test-bucket", "data.bin", body_owned(data.clone()), None)
            .await
            .unwrap();

        let (_, stream) = storage
            .get_object(
                "test-bucket",
                "data.bin",
                Some(crate::range::ByteRange::open_from(500)),
            )
            .await
            .unwrap();

        let received = collect_stream(stream).await;
        assert_eq!(received.len(), 500);
        assert_eq!(received, &data[500..1000]);
    }

    // D-04 bytes=-200 over 1000 bytes → last 200 bytes.
    #[tokio::test]
    async fn get_object_range_suffix_200() {
        let (_dir, storage) = make_storage_with_bucket().await;
        let data: Vec<u8> = (0u8..=255u8).cycle().take(1000).collect();
        storage
            .put_object("test-bucket", "data.bin", body_owned(data.clone()), None)
            .await
            .unwrap();

        let (_, stream) = storage
            .get_object(
                "test-bucket",
                "data.bin",
                Some(crate::range::ByteRange::suffix(200)),
            )
            .await
            .unwrap();

        let received = collect_stream(stream).await;
        assert_eq!(received.len(), 200);
        assert_eq!(received, &data[800..1000]);
    }

    // D-04 unsatisfiable: open_from past EOF → RangeNotSatisfiable (distinct from NoSuchKey).
    #[tokio::test]
    async fn get_object_range_unsatisfiable_returns_range_not_satisfiable() {
        let (_dir, storage) = make_storage_with_bucket().await;
        storage
            .put_object("test-bucket", "data.bin", body_owned(vec![0u8; 1000]), None)
            .await
            .unwrap();

        match storage
            .get_object(
                "test-bucket",
                "data.bin",
                Some(crate::range::ByteRange::open_from(99999)),
            )
            .await
        {
            Err(StorageError::RangeNotSatisfiable) => {}
            Ok(_) => panic!("expected RangeNotSatisfiable, got Ok"),
            Err(e) => panic!("expected RangeNotSatisfiable, got {:?}", e),
        }
    }

    // Ranged get on a missing key returns NoSuchKey (range resolution after sidecar read).
    #[tokio::test]
    async fn get_object_range_missing_key_returns_no_such_key() {
        let (_dir, storage) = make_storage_with_bucket().await;

        match storage
            .get_object(
                "test-bucket",
                "missing.bin",
                Some(crate::range::ByteRange::full_from(0, 499)),
            )
            .await
        {
            Err(StorageError::NoSuchKey(_)) => {}
            Ok(_) => panic!("expected NoSuchKey for missing key with range, got Ok"),
            Err(e) => panic!("expected NoSuchKey for missing key with range, got {:?}", e),
        }
    }
}
