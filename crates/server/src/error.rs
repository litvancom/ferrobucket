use s3s::{s3_error, S3Error};
use ferrobucket_storage::StorageError;

/// Map a StorageError to an S3Error with the correct S3 wire error code (D-08, corrected).
///
/// Corrected from CONTEXT.md: `KeyTooLongError` (not `KeyTooLong`) and
/// `BucketAlreadyOwnedByYou` (not `BucketAlreadyExists`) per
/// `s3s-0.13.0/src/error/generated.rs`.
pub fn map_storage_err(err: StorageError) -> S3Error {
    match err {
        StorageError::NoSuchBucket(_) => s3_error!(NoSuchBucket),
        StorageError::NoSuchKey(_) => s3_error!(NoSuchKey),
        StorageError::BucketNotEmpty(_) => s3_error!(BucketNotEmpty),
        StorageError::BucketAlreadyExists(_) => s3_error!(BucketAlreadyOwnedByYou),
        StorageError::InvalidBucketName(n) => s3_error!(InvalidBucketName, "invalid bucket name: {n}"),
        StorageError::KeyTooLong { .. } => s3_error!(KeyTooLongError),
        StorageError::InvalidKey => s3_error!(InvalidRequest, "invalid key"),
        StorageError::InvalidContinuationToken => s3_error!(InvalidArgument, "invalid continuation token"),
        StorageError::RangeNotSatisfiable => s3_error!(InvalidRange),
        StorageError::NoSuchUpload(_) => s3_error!(NoSuchUpload),
        StorageError::InvalidPartNumber(n) => s3_error!(InvalidArgument, "part number must be positive: {n}"),
        StorageError::Io(e) => s3_error!(e, InternalError, "I/O error"),
    }
}
