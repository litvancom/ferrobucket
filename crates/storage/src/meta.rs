use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub etag: String,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Stub — returns Err so RED tests fail on missing implementation.
pub async fn write_sidecar(
    _path: &std::path::Path,
    _meta: &ObjectMeta,
) -> Result<(), crate::StorageError> {
    Err(crate::StorageError::InvalidKey)
}

/// Stub — returns Err so RED tests fail on missing implementation.
pub async fn read_sidecar(
    _path: &std::path::Path,
) -> Result<ObjectMeta, crate::StorageError> {
    Err(crate::StorageError::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use time::OffsetDateTime;

    fn make_meta() -> ObjectMeta {
        ObjectMeta {
            key: "test/key.txt".to_owned(),
            size: 42,
            content_type: "text/plain".to_owned(),
            etag: "abc123".to_owned(),
            last_modified: OffsetDateTime::now_utc(),
        }
    }

    #[tokio::test]
    async fn sidecar_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("meta.json");
        let meta = make_meta();

        write_sidecar(&path, &meta).await.unwrap();
        let loaded = read_sidecar(&path).await.unwrap();

        assert_eq!(loaded.key, meta.key);
        assert_eq!(loaded.size, meta.size);
        assert_eq!(loaded.content_type, meta.content_type);
        assert_eq!(loaded.etag, meta.etag);
        // RFC3339 round-trip: compare as formatted strings (subsecond precision may vary)
        assert_eq!(
            loaded.last_modified.unix_timestamp(),
            meta.last_modified.unix_timestamp()
        );
    }

    #[tokio::test]
    async fn sidecar_missing_is_nosuchkey() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        match read_sidecar(&path).await {
            Err(crate::StorageError::NoSuchKey(_)) => {} // expected
            other => panic!("expected NoSuchKey, got {:?}", other),
        }
    }
}
