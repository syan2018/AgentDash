pub use agentdash_spi::mount::{
    ApplyPatchRequest, ApplyPatchResult, ExecRequest, ExecResult, ListOptions, ListResult,
    ReadResult, RuntimeFileEntry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub mount_id: String,
    pub path: String,
}
