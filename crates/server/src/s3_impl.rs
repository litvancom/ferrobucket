use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use axum::http::StatusCode;
use s3s::dto::*;
use s3s::{s3_error, S3, S3Request, S3Response, S3Result};
use time::OffsetDateTime;

use ferrobucket_storage::list::ListV2Req;
use ferrobucket_storage::{ByteRange, FsStorage, StorageError, Storage};

use crate::error::map_storage_err;

// ─── Adapter struct ───────────────────────────────────────────────────────────

pub struct FerrobucketS3 {
    /// Held concretely — `Storage` uses RPITIT (not object-safe).
    storage: FsStorage,
}

impl FerrobucketS3 {
    pub fn new(storage: FsStorage) -> Self {
        Self { storage }
    }
}

// ─── Body bridge helpers ──────────────────────────────────────────────────────

/// Convert an incoming `StreamingBlob` (implements `Stream<Item = Result<Bytes, StdError>>`)
/// into a `Stream<Item = io::Result<Bytes>>` for `Storage::put_object`.
fn body_to_stream(
    blob: StreamingBlob,
) -> impl futures::Stream<Item = std::io::Result<Bytes>> + Send {
    use futures::StreamExt;
    blob.map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
}

/// Convert a storage stream into `StreamingBlob` for GetObject response output.
///
/// `StreamingBlob::wrap` maps any `Stream<Item = Result<Bytes, E>>` where E: Error.
fn stream_to_blob(
    stream: Pin<Box<dyn futures::Stream<Item = std::io::Result<Bytes>> + Send>>,
) -> StreamingBlob {
    // std::io::Error implements std::error::Error + Send + Sync, so StreamingBlob::wrap works.
    // But the stream is `Send` only, not `Sync`. Use StreamingBlob::new with a manual ByteStream
    // wrapper instead — or re-box with Sync bound.
    //
    // Since DynByteStream requires `Send + Sync`, we need to add Sync.
    // The storage streams (ReaderStream etc.) are not Sync, so we box them with
    // a wrapper that implements Sync by wrapping in a Mutex — but that's complex.
    //
    // Simpler: use `StreamingBlob::wrap` which takes `Send + Sync + 'static`.
    // Our storage stream is `Send` but not necessarily `Sync`.
    // Re-box adding Sync via a newtype that implements Sync.
    //
    // However, looking at s3s-0.13.0/src/dto/streaming_blob.rs, `StreamingBlob::wrap`
    // requires `Send + Sync + 'static`. The FsStorage streams (ReaderStream) are not Sync.
    //
    // Use `s3s::Body::http_body_unsync` instead, which doesn't require Sync, then convert
    // the Body to StreamingBlob via `StreamingBlob::from(Body)`.
    use futures::StreamExt;
    use http_body::Frame;
    use http_body_util::StreamBody;

    let frames = stream.map(|r| {
        r.map(Frame::data)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync + 'static> { Box::new(e) })
    });
    let body = s3s::Body::http_body_unsync(StreamBody::new(frames));
    // StreamingBlob implements From<Body>
    StreamingBlob::from(body)
}

/// Convert `time::OffsetDateTime` → `s3s::dto::Timestamp`.
///
/// `Timestamp` wraps `time::OffsetDateTime` and has `From<OffsetDateTime>` (verified from
/// `s3s-0.13.0/src/dto/timestamp.rs` line 45).
fn to_s3_timestamp(dt: OffsetDateTime) -> Timestamp {
    Timestamp::from(dt)
}

/// Translate `s3s::dto::Range` → `ferrobucket_storage::ByteRange` (DEC-storage-decoupled).
///
/// The conversion lives HERE (not in the storage crate) so the storage crate remains
/// free of any s3s types.
///
/// `s3s::dto::Range` variants (from `s3s-0.13.0/src/dto/range.rs`):
/// - `Int { first, last: None }` → open-ended (`bytes=first-`)
/// - `Int { first, last: Some(last) }` → inclusive range (`bytes=first-last`)
/// - `Suffix { length }` → suffix range (`bytes=-length`)
fn s3_range_to_byte_range(r: &Range) -> ByteRange {
    match *r {
        Range::Int { first, last: None } => ByteRange::open_from(first),
        Range::Int { first, last: Some(last) } => ByteRange::full_from(first, last),
        Range::Suffix { length } => ByteRange::suffix(length),
    }
}

// ─── S3 trait implementation ──────────────────────────────────────────────────

#[async_trait]
impl S3 for FerrobucketS3 {
    // ── PutObject ─────────────────────────────────────────────────────────────

    async fn put_object(&self, req: S3Request<PutObjectInput>) -> S3Result<S3Response<PutObjectOutput>> {
        let PutObjectInput { bucket, key, body, content_type, .. } = req.input;
        let blob = body.ok_or_else(|| s3_error!(InvalidRequest, "missing body"))?;
        let stream = body_to_stream(blob);
        let meta = self
            .storage
            .put_object(&bucket, &key, stream, content_type)
            .await
            .map_err(map_storage_err)?;
        let output = PutObjectOutput {
            e_tag: Some(ETag::Strong(meta.etag)),
            ..Default::default()
        };
        Ok(S3Response::new(output))
    }

    // ── GetObject ─────────────────────────────────────────────────────────────

    async fn get_object(&self, req: S3Request<GetObjectInput>) -> S3Result<S3Response<GetObjectOutput>> {
        let GetObjectInput { bucket, key, range, .. } = req.input;

        // Translate s3s Range -> internal ByteRange (DEC-storage-decoupled).
        let internal_range: Option<ByteRange> = range.as_ref().map(s3_range_to_byte_range);
        let range_requested = range.is_some();

        let (meta, stream) = self
            .storage
            .get_object(&bucket, &key, internal_range)
            .await
            .map_err(map_storage_err)?;

        let blob = stream_to_blob(stream);

        // Full size is the complete object size (meta.size is always the full size per plan 02-01).
        let full_size = meta.size;

        let mut output = GetObjectOutput {
            body: Some(blob),
            e_tag: Some(ETag::Strong(meta.etag)),
            content_type: Some(meta.content_type),
            content_length: Some(full_size as i64),
            last_modified: Some(to_s3_timestamp(meta.last_modified)),
            accept_ranges: Some("bytes".to_owned()),
            ..Default::default()
        };

        if range_requested {
            // Build Content-Range from the resolved window.
            // Resolve the range against the full size to compute the window length.
            // FsStorage already performed the seek; we just need the resolved coordinates
            // for the Content-Range header and correct content_length.
            let window = range
                .as_ref()
                .and_then(|r| s3_range_to_byte_range(r).resolve(full_size));

            if let Some(resolved) = window {
                // A zero-length window (e.g. a suffix range on a zero-length object, where
                // resolve() returns Some({start:0, length:0})) has no satisfiable bytes:
                // fall through to a 200 with the full (empty) body rather than emitting a
                // malformed "206 Content-Range: bytes 0-0/0" (RFC 9110 incorrect). WR-01.
                if resolved.length > 0 {
                    let start = resolved.start;
                    // end is inclusive last byte.
                    let end = resolved.start + resolved.length.saturating_sub(1);
                    output.content_range = Some(format!("bytes {start}-{end}/{full_size}"));
                    output.content_length = Some(resolved.length as i64);
                    return Ok(S3Response::with_status(output, StatusCode::PARTIAL_CONTENT));
                }
            }
            // If the range produced no satisfiable window (None, or the zero-length case
            // above), fall through to a 200 response with the full (possibly empty) body.
        }

        Ok(S3Response::new(output))
    }

    // ── HeadObject ───────────────────────────────────────────────────────────

    async fn head_object(&self, req: S3Request<HeadObjectInput>) -> S3Result<S3Response<HeadObjectOutput>> {
        let HeadObjectInput { bucket, key, .. } = req.input;
        let meta = self
            .storage
            .head_object(&bucket, &key)
            .await
            .map_err(map_storage_err)?;
        let output = HeadObjectOutput {
            e_tag: Some(ETag::Strong(meta.etag)),
            content_type: Some(meta.content_type),
            content_length: Some(meta.size as i64),
            last_modified: Some(to_s3_timestamp(meta.last_modified)),
            ..Default::default()
        };
        Ok(S3Response::new(output))
    }

    // ── DeleteObject ──────────────────────────────────────────────────────────

    async fn delete_object(&self, req: S3Request<DeleteObjectInput>) -> S3Result<S3Response<DeleteObjectOutput>> {
        let DeleteObjectInput { bucket, key, .. } = req.input;
        self.storage
            .delete_object(&bucket, &key)
            .await
            .map_err(map_storage_err)?;
        Ok(S3Response::new(DeleteObjectOutput::default()))
    }

    // ── DeleteObjects (D-05: S3-faithful idempotent) ──────────────────────────

    async fn delete_objects(
        &self,
        req: S3Request<DeleteObjectsInput>,
    ) -> S3Result<S3Response<DeleteObjectsOutput>> {
        let DeleteObjectsInput { bucket, delete, .. } = req.input;
        let quiet = delete.quiet.unwrap_or(false);
        let mut deleted: Vec<DeletedObject> = Vec::new();
        let mut errors: Vec<s3s::dto::Error> = Vec::new();

        for obj in delete.objects {
            // ObjectIdentifier.key is ObjectKey = String (not Option<String>).
            let key = obj.key;
            match self.storage.delete_object(&bucket, &key).await {
                Ok(()) | Err(StorageError::NoSuchKey(_)) => {
                    // D-05: NoSuchKey is treated as success (S3 idempotency).
                    if !quiet {
                        deleted.push(DeletedObject { key: Some(key), ..Default::default() });
                    }
                }
                Err(e) => {
                    errors.push(s3s::dto::Error {
                        code: Some("InternalError".to_owned()),
                        key: Some(key),
                        message: Some(e.to_string()),
                        ..Default::default()
                    });
                }
            }
        }

        let output = DeleteObjectsOutput {
            deleted: if deleted.is_empty() { None } else { Some(deleted) },
            errors: if errors.is_empty() { None } else { Some(errors) },
            ..Default::default()
        };
        Ok(S3Response::new(output))
    }

    // ── CreateBucket ──────────────────────────────────────────────────────────

    async fn create_bucket(&self, req: S3Request<CreateBucketInput>) -> S3Result<S3Response<CreateBucketOutput>> {
        let CreateBucketInput { bucket, .. } = req.input;
        self.storage.create_bucket(&bucket).await.map_err(map_storage_err)?;
        Ok(S3Response::new(CreateBucketOutput::default()))
    }

    // ── DeleteBucket ──────────────────────────────────────────────────────────

    async fn delete_bucket(&self, req: S3Request<DeleteBucketInput>) -> S3Result<S3Response<DeleteBucketOutput>> {
        let DeleteBucketInput { bucket, .. } = req.input;
        self.storage.delete_bucket(&bucket).await.map_err(map_storage_err)?;
        Ok(S3Response::new(DeleteBucketOutput::default()))
    }

    // ── ListBuckets ───────────────────────────────────────────────────────────

    async fn list_buckets(&self, _req: S3Request<ListBucketsInput>) -> S3Result<S3Response<ListBucketsOutput>> {
        let infos = self.storage.list_buckets().await.map_err(map_storage_err)?;
        let buckets: Vec<Bucket> = infos
            .into_iter()
            .map(|b| Bucket {
                name: Some(b.name),
                creation_date: Some(to_s3_timestamp(b.created_at)),
                ..Default::default()
            })
            .collect();
        let output = ListBucketsOutput {
            buckets: Some(buckets),
            ..Default::default()
        };
        Ok(S3Response::new(output))
    }

    // ── HeadBucket ────────────────────────────────────────────────────────────

    async fn head_bucket(&self, req: S3Request<HeadBucketInput>) -> S3Result<S3Response<HeadBucketOutput>> {
        let HeadBucketInput { bucket, .. } = req.input;
        // Check existence by listing buckets and searching for the name.
        let infos = self.storage.list_buckets().await.map_err(map_storage_err)?;
        if infos.iter().any(|b| b.name == bucket) {
            Ok(S3Response::new(HeadBucketOutput::default()))
        } else {
            Err(s3_error!(NoSuchBucket))
        }
    }

    // ── ListObjectsV2 ─────────────────────────────────────────────────────────

    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        let ListObjectsV2Input {
            bucket,
            prefix,
            delimiter,
            max_keys,
            continuation_token,
            ..
        } = req.input;

        let list_req = ListV2Req {
            prefix,
            delimiter,
            max_keys: max_keys.map(|n| n as usize),
            continuation_token,
        };

        let res = self
            .storage
            .list_objects_v2(&bucket, list_req)
            .await
            .map_err(map_storage_err)?;

        let contents: Vec<Object> = res
            .objects
            .into_iter()
            .map(|o| Object {
                key: Some(o.key),
                size: Some(o.size as i64),
                e_tag: Some(ETag::Strong(o.etag)),
                last_modified: Some(to_s3_timestamp(o.last_modified)),
                ..Default::default()
            })
            .collect();

        let common_prefixes: Vec<CommonPrefix> = res
            .common_prefixes
            .into_iter()
            .map(|p| CommonPrefix { prefix: Some(p), ..Default::default() })
            .collect();

        let key_count = (contents.len() + common_prefixes.len()) as i32;

        let output = ListObjectsV2Output {
            contents: if contents.is_empty() { None } else { Some(contents) },
            common_prefixes: if common_prefixes.is_empty() { None } else { Some(common_prefixes) },
            is_truncated: Some(res.is_truncated),
            next_continuation_token: res.next_continuation_token,
            key_count: Some(key_count),
            name: Some(bucket),
            ..Default::default()
        };

        Ok(S3Response::new(output))
    }
}
