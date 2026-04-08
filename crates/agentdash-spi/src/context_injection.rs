use std::path::{Path, PathBuf};

use agentdash_domain::context_source::ContextSourceRef;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum InjectionError {
    #[error("缺少工作区，无法解析来源: {0}")]
    MissingWorkspace(String),
    #[error("来源路径不存在: {0}")]
    PathNotFound(PathBuf),
    #[error("来源文件过大: {path} ({size} bytes)")]
    SourceTooLarge { path: PathBuf, size: u64 },
    #[error("不支持的文件类型: {0}")]
    UnsupportedFileType(PathBuf),
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML 解析失败: {0}")]
    Yaml(String),
    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    Append,
    Override,
}

#[derive(Debug, Clone)]
pub struct ContextFragment {
    pub slot: &'static str,
    pub label: &'static str,
    pub order: i32,
    pub strategy: MergeStrategy,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddressSpaceDescriptor {
    pub id: String,
    pub label: String,
    pub kind: agentdash_domain::context_source::ContextSourceKind,
    pub provider: String,
    pub supports: Vec<String>,
    pub selector: Option<SelectorHint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectorHint {
    pub trigger: Option<String>,
    pub placeholder: String,
    pub result_item_type: String,
}

pub struct AddressSpaceContext<'a> {
    pub mount_root: Option<&'a Path>,
    pub has_mcp: bool,
}

pub trait AddressSpaceDiscoveryProvider: Send + Sync {
    fn descriptor(&self, ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor>;
}

pub struct ResolveSourcesRequest<'a> {
    pub sources: &'a [ContextSourceRef],
    pub mount_root: Option<&'a Path>,
    pub base_order: i32,
}

pub struct ResolveSourcesOutput {
    pub fragments: Vec<ContextFragment>,
    pub warnings: Vec<String>,
}

pub trait SourceResolver: Send + Sync {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        mount_root: Option<&Path>,
        order: i32,
    ) -> Result<ContextFragment, InjectionError>;
}
