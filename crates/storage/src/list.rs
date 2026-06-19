/// Stub types for list.rs — filled in full by Plan 03.
/// Declared here so lib.rs can compile with list_objects_v2 in the Storage trait.
#[derive(Debug, Default)]
pub struct ListV2Req {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub continuation_token: Option<String>,
    pub max_keys: Option<usize>,
}

#[derive(Debug, Default)]
pub struct ListV2Res {
    pub objects: Vec<crate::meta::ObjectMeta>,
    pub common_prefixes: Vec<String>,
    pub next_continuation_token: Option<String>,
    pub is_truncated: bool,
}

/// Stub free function — Plan 02 wires the delegation call site here;
/// Plan 03 replaces this with the full listing algorithm.
pub async fn list_objects_v2(
    _storage: &crate::fs::FsStorage,
    _bucket: &str,
    _req: ListV2Req,
) -> Result<ListV2Res, crate::StorageError> {
    // Plan 03 fills this with the real listing algorithm.
    Ok(ListV2Res::default())
}
