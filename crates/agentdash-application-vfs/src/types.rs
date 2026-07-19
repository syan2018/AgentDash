pub use agentdash_platform_spi::platform::mount::{
    ApplyPatchRequest, ApplyPatchResult, BinaryReadResult, ExecRequest, ExecResult, ListOptions,
    ListResult, ReadResult, RuntimeFileEntry, ShellSessionOutputChunk, ShellSessionReadRequest,
    ShellSessionResizeRequest, ShellSessionSnapshot, ShellSessionTerminateRequest,
    ShellSessionTerminateResult, ShellSessionWriteRequest, ShellSessionWriteResult,
};

pub const RUNTIME_FILE_CONTENT_KIND_ATTR: &str = "content_kind";
pub const RUNTIME_FILE_MIME_TYPE_ATTR: &str = "mime_type";
pub const RUNTIME_FILE_CONTENT_KIND_TEXT: &str = "text";
pub const RUNTIME_FILE_CONTENT_KIND_BINARY: &str = "binary";

pub fn runtime_entry_content_kind(entry: &RuntimeFileEntry) -> Option<&str> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get(RUNTIME_FILE_CONTENT_KIND_ATTR))
        .and_then(|value| value.as_str())
}

pub fn runtime_entry_mime_type(entry: &RuntimeFileEntry) -> Option<&str> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get(RUNTIME_FILE_MIME_TYPE_ATTR))
        .and_then(|value| value.as_str())
}

pub fn runtime_entry_is_binary(entry: &RuntimeFileEntry) -> bool {
    runtime_entry_content_kind(entry) == Some(RUNTIME_FILE_CONTENT_KIND_BINARY)
}

pub fn runtime_text_file_attributes() -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        RUNTIME_FILE_CONTENT_KIND_ATTR.to_string(),
        serde_json::Value::String(RUNTIME_FILE_CONTENT_KIND_TEXT.to_string()),
    );
    attrs.insert(
        RUNTIME_FILE_MIME_TYPE_ATTR.to_string(),
        serde_json::Value::String("text/plain; charset=utf-8".to_string()),
    );
    attrs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub mount_id: String,
    pub path: String,
}

/// 跨 mount apply_patch 的聚合结果。
#[derive(Debug, Clone, Default)]
pub struct MultiMountPatchResult {
    /// 成功新增的路径（`mount_id://relative_path` 格式）。
    pub added: Vec<String>,
    /// 成功修改的路径。
    pub modified: Vec<String>,
    /// 成功删除的路径。
    pub deleted: Vec<String>,
    /// 单条目级别的失败记录。
    pub errors: Vec<PatchEntryError>,
}

#[derive(Debug, Clone)]
pub struct PatchEntryError {
    pub mount_id: String,
    pub path: String,
    pub message: String,
}
