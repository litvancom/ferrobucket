use std::path::{Path, PathBuf};
use std::pin::Pin;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use crate::meta::{BucketInfo, read_sidecar, write_sidecar};
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
}

/// Validate an S3 bucket name against strict DNS-safe rules (D-09, RESEARCH.md §"Bucket Name Validation").
/// Accepts: 3–63 chars, only `[a-z0-9.-]`, no leading/trailing hyphen, no consecutive dots.
pub fn validate_bucket_name(name: &str) -> Result<(), StorageError> {
    let n = name.len();
    if !(3..=63).contains(&n) {
        return Err(StorageError::InvalidBucketName(name.to_owned()));
    }
    let valid_chars = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    let no_leading_trailing_hyphen = !name.starts_with('-') && !name.ends_with('-');
    let no_consecutive_dots = !name.contains("..");
    if !valid_chars || !no_leading_trailing_hyphen || !no_consecutive_dots {
        return Err(StorageError::InvalidBucketName(name.to_owned()));
    }
    Ok(())
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

    let etag = hasher.finalize();
    let target = objects_dir.join(encoded_key);

    // rename(2) can block under filesystem pressure — run on the blocking thread pool (D-04, D-08).
    tokio::task::spawn_blocking(move || std::fs::rename(&tmp_path, &target))
        .await
        .map_err(|_| StorageError::Io(std::io::Error::other("spawn_blocking join error")))?
        .map_err(StorageError::Io)?;

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
                    if let Ok(info) = serde_json::from_slice::<BucketInfo>(&bytes) {
                        buckets.push(info);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Not a ferrobucket bucket directory — skip silently.
                }
                Err(e) => return Err(StorageError::Io(e)),
            }
        }
        Ok(buckets)
    }

    async fn create_bucket(&self, name: &str) -> Result<(), StorageError> {
        validate_bucket_name(name)?;
        let bucket_dir = self.bucket_path(name);
        if bucket_dir.exists() {
            return Err(StorageError::BucketAlreadyExists(name.to_owned()));
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

        tokio::fs::remove_dir_all(&bucket_dir)
            .await
            .map_err(StorageError::Io)?;
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
        let meta = read_sidecar(&self.meta_path(bucket, &encoded_key)).await?;
        let stream = stream_object(&self.object_path(bucket, &encoded_key)).await?;
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
        // Remove sidecar (best-effort; ignore NotFound).
        match tokio::fs::remove_file(&meta_path).await {
            Ok(()) | Err(_) => {}
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

        let (_, mut stream) = storage.get_object("test-bucket", "fox.txt").await.unwrap();
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
            .get_object("test-bucket", "empty.bin")
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
        match storage.get_object("test-bucket", "missing.txt").await {
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
            match storage.get_object(bad, "k").await {
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
        match storage.get_object("test-bucket", "to-delete.txt").await {
            Err(StorageError::NoSuchKey(_)) => {}
            Ok(_) => panic!("expected NoSuchKey after delete, got Ok"),
            Err(e) => panic!("expected NoSuchKey after delete, got Err({:?})", e),
        }
    }
}
