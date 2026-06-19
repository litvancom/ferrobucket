use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;

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
///
/// The write goes through a temp file in the same directory followed by an atomic
/// rename (CR-02). A plain `create+truncate+write` is NOT atomic: a crash or
/// concurrent reader during the write would observe a truncated/torn JSON file,
/// which `read_sidecar` would reject as `InvalidData`, making an object whose body
/// exists permanently unreadable (and, on overwrite, destroying the prior metadata).
pub async fn write_sidecar(
    path: &std::path::Path,
    meta: &ObjectMeta,
) -> Result<(), crate::StorageError> {
    let json = serde_json::to_vec(meta).map_err(|e| {
        crate::StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;

    // Temp file MUST be in the same directory as the target to avoid EXDEV on rename.
    let dir = path
        .parent()
        .ok_or_else(|| crate::StorageError::Io(std::io::Error::other("meta path has no parent")))?;
    let tmp = tempfile::Builder::new()
        .tempfile_in(dir)
        .map_err(crate::StorageError::Io)?;
    let (std_file, tmp_path) = tmp
        .keep()
        .map_err(|e| crate::StorageError::Io(std::io::Error::other(e.to_string())))?;

    // From here on, the temp file is persistent; clean it up on any error before rename.
    let write_result = async {
        let mut f = tokio::fs::File::from_std(std_file);
        f.write_all(&json).await.map_err(crate::StorageError::Io)?;
        f.flush().await.map_err(crate::StorageError::Io)?;
        Ok::<(), crate::StorageError>(())
    }
    .await;
    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e);
    }

    let target = path.to_path_buf();
    let tmp_for_rename = tmp_path.clone();
    let rename = tokio::task::spawn_blocking(move || std::fs::rename(&tmp_for_rename, &target))
        .await
        .map_err(|_| crate::StorageError::Io(std::io::Error::other("spawn_blocking join error")))?;
    if let Err(e) = rename {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(crate::StorageError::Io(e));
    }
    Ok(())
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
    async fn sidecar_overwrite_is_atomic_no_temp_leftovers() {
        // CR-02 regression: write_sidecar uses temp+rename, so overwriting an
        // existing sidecar leaves a single valid file (the new one) and no temp
        // leftovers in the directory.
        let dir = tempdir().unwrap();
        let path = dir.path().join("meta.json");

        let mut first = make_meta();
        first.size = 1;
        write_sidecar(&path, &first).await.unwrap();

        let mut second = make_meta();
        second.size = 999;
        write_sidecar(&path, &second).await.unwrap();

        // The sidecar must be valid and reflect the most recent write.
        let loaded = read_sidecar(&path).await.unwrap();
        assert_eq!(loaded.size, 999);

        // Exactly one file in the directory: no .tmp-* leftovers from the rename path.
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1, "temp files must not leak into the meta dir");
        assert_eq!(
            entries[0].file_name().to_string_lossy().as_ref(),
            "meta.json"
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
