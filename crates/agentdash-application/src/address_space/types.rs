pub use agentdash_spi::mount::{
    ApplyPatchRequest, ApplyPatchResult, ExecRequest, ExecResult, ListOptions, ListResult,
    ReadResult, RuntimeFileEntry,
};

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
