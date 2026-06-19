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

/// Write ObjectMeta as a JSON sidecar file (REQ-object-persistence).
/// Caller must ensure this is written AFTER the object body rename (D-04 crash-safe ordering).
pub async fn write_sidecar(
    path: &std::path::Path,
    meta: &ObjectMeta,
) -> Result<(), crate::StorageError> {
    let json = serde_json::to_string(meta).map_err(|e| {
        crate::StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    tokio::fs::write(path, json.as_bytes())
        .await
        .map_err(crate::StorageError::Io)
}

/// Read ObjectMeta from a JSON sidecar file.
/// A missing sidecar is treated as NoSuchKey (RESEARCH.md Open Question 3).
pub async fn read_sidecar(
    path: &std::path::Path,
) -> Result<ObjectMeta, crate::StorageError> {
    let bytes = tokio::fs::read(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::StorageError::NoSuchKey(path.display().to_string())
        } else {
            crate::StorageError::Io(e)
        }
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| crate::StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
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
        // RFC3339 round-trip: compare unix timestamps (subsecond may differ by RFC3339 rounding)
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
