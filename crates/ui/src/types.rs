//! Shared serde DTOs for the web UI.
//!
//! All types here are usable in BOTH `ssr` and `hydrate` feature contexts — no
//! server-only types are included. Server functions return these types; pages
//! render them.
//!
//! SECURITY (T-04-03, DEC-ui-ssr): No DTO carries any credential material.
//! `ConnectionInfo` carries `access_key_id` (the public identifier only).

use serde::{Deserialize, Serialize};

/// One row in the bucket list table (REQ-ui-bucket-list).
///
/// `object_count` and `total_size` are DERIVED by calling `list_objects_v2` on
/// each bucket — `BucketInfo` from storage does not carry these fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BucketRow {
    pub name: String,
    /// RFC 3339 timestamp string (formatted from `BucketInfo.created_at`).
    pub created: String,
    /// Total number of objects in the bucket (derived, not stored).
    pub object_count: u64,
    /// Sum of object sizes in bytes (derived, not stored).
    pub total_size: u64,
}

/// One row in the object browser table (REQ-ui-object-browser).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectRow {
    /// S3 key string.
    pub key: String,
    /// Size in bytes (0 for folder rows).
    pub size: u64,
    /// RFC 3339 timestamp string (empty for folder rows).
    pub last_modified: String,
    /// True for CommonPrefix rows (folder rows), false for object rows.
    pub is_folder: bool,
}

/// Result of a `list_objects_fn` call (REQ-ui-object-browser, D-09 pagination).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectListing {
    /// CommonPrefix strings (folder rows), sorted.
    pub folders: Vec<String>,
    /// Direct object rows under the current prefix.
    pub objects: Vec<ObjectRow>,
    /// Opaque continuation token for the next page (`None` if last page).
    pub next_token: Option<String>,
    /// The prefix used for this listing request.
    pub prefix: String,
}

/// Full object metadata returned by `head_object_fn` (REQ-ui-object-detail).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectDetail {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub etag: String,
    /// RFC 3339 timestamp string.
    pub last_modified: String,
}

/// Read-only connection configuration returned by `get_config_fn` (D-10, REQ-ui-settings).
///
/// `force_path_style` is always `true` (locked ON per D-10).
/// Only the public key ID is included — no credential material.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub endpoint: String,
    pub region: String,
    /// Access key ID (the public identifier, shown in Settings). No credential material.
    pub access_key_id: Option<String>,
    /// Always `true` — ferrobucket uses path-style S3 URLs (locked, D-10).
    pub force_path_style: bool,
    /// Filesystem path to the data directory.
    pub data_dir: String,
}

/// Live writable-status result returned by `check_status_fn` (D-10, REQ-ui-settings).
///
/// Copy strings match UI-SPEC Copywriting Contract exactly:
/// - healthy: "Server up — data directory present and writable"
/// - unhealthy: "Data directory not writable"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusInfo {
    pub writable: bool,
    /// UI-SPEC copy string (see above).
    pub message: String,
}
