use std::pin::Pin;
use bytes::Bytes;
use futures::Stream;
use thiserror::Error;

pub mod encode;
pub mod etag;
pub mod fs;
pub mod list;
pub mod meta;

pub use encode::{decode_key, encode_key};
pub use meta::{BucketInfo, ObjectMeta};

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("no such bucket: {0}")]
    NoSuchBucket(String),

    #[error("no such key: {0}")]
    NoSuchKey(String),

    #[error("bucket not empty: {0}")]
    BucketNotEmpty(String),

    #[error("bucket already exists: {0}")]
    BucketAlreadyExists(String),

    #[error("invalid bucket name: {0}")]
    InvalidBucketName(String),

    #[error("key too long after encoding (limit: 255 bytes): {key}")]
    KeyTooLong { key: String },

    #[error("invalid key")]
    InvalidKey,

    #[error("invalid continuation token")]
    InvalidContinuationToken,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Phase-1 lean subset of the Storage trait (D-05).
/// Native async fn in trait — Rust 1.75+ RPITIT; no #[async_trait] needed.
pub trait Storage: Send + Sync {
    async fn list_buckets(&self) -> Result<Vec<BucketInfo>, StorageError>;

    async fn create_bucket(&self, name: &str) -> Result<(), StorageError>;

    async fn delete_bucket(&self, name: &str) -> Result<(), StorageError>;

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: impl Stream<Item = std::io::Result<Bytes>> + Send,
        content_type: Option<String>,
    ) -> Result<ObjectMeta, StorageError>;

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
    >;

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMeta, StorageError>;

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError>;

    async fn list_objects_v2(
        &self,
        bucket: &str,
        req: crate::list::ListV2Req,
    ) -> Result<crate::list::ListV2Res, StorageError>;
}
