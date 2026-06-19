use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

/// JSON sidecar stored at `<data>/.uploads/<upload-id>/_meta.json`.
/// Persists the values from CreateMultipartUpload so CompleteMultipartUpload
/// can write the final object without the client resending them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartMeta {
    pub bucket: String,
    pub key: String,
    pub content_type: Option<String>,
}

/// Write MultipartMeta as a JSON sidecar file.
/// Uses a temp file in the same directory followed by an atomic rename (avoids EXDEV
/// and ensures crash-safety — same pattern as meta.rs `write_sidecar`).
pub async fn write_multipart_meta(
    path: &std::path::Path,
    meta: &MultipartMeta,
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

/// Read MultipartMeta from a JSON sidecar file.
/// A missing sidecar is treated as NoSuchUpload (staging is a separate namespace from objects).
pub async fn read_multipart_meta(
    path: &std::path::Path,
) -> Result<MultipartMeta, crate::StorageError> {
    let bytes = tokio::fs::read(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::StorageError::NoSuchUpload(path.display().to_string())
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

    fn make_meta() -> MultipartMeta {
        MultipartMeta {
            bucket: "test-bucket".to_owned(),
            key: "test/key.txt".to_owned(),
            content_type: Some("text/plain".to_owned()),
        }
    }

    #[tokio::test]
    async fn multipart_meta_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("_meta.json");
        let meta = make_meta();

        write_multipart_meta(&path, &meta).await.unwrap();
        let loaded = read_multipart_meta(&path).await.unwrap();

        assert_eq!(loaded.bucket, meta.bucket);
        assert_eq!(loaded.key, meta.key);
        assert_eq!(loaded.content_type, meta.content_type);
    }

    #[tokio::test]
    async fn multipart_meta_none_content_type() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("_meta.json");
        let meta = MultipartMeta {
            bucket: "b".to_owned(),
            key: "k".to_owned(),
            content_type: None,
        };

        write_multipart_meta(&path, &meta).await.unwrap();
        let loaded = read_multipart_meta(&path).await.unwrap();
        assert_eq!(loaded.content_type, None);
    }

    #[tokio::test]
    async fn multipart_meta_missing_is_nosuchupload() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        match read_multipart_meta(&path).await {
            Err(crate::StorageError::NoSuchUpload(_)) => {} // expected
            other => panic!("expected NoSuchUpload, got {:?}", other),
        }
    }
}
