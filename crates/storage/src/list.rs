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
