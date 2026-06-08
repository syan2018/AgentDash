//! Workspace Module 单一 projection 契约。
//!
//! 把 enabled extension、visible canvas、built-in module 聚合为同一种 module
//! descriptor。`list` 返回摘要（无完整 schema），`describe` 返回含 input/output
//! schema 的完整 descriptor。该契约同时服务 Agent 工具与项目设置页 UI（单一
//! canonical，不做两套 DTO）。
//!
//! 数据流向：application `workspace_module` 聚合层把内部 `ExtensionRuntimeProjection`
//! 子投影 + `Canvas` 转换为这里的 DTO（内部投影类型不直接 derive serde/TS）。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Module 的来源类别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleKind {
    Extension,
    Canvas,
    Builtin,
}

/// Module 的就绪状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleStatusKind {
    Ready,
    Unavailable,
}

/// Module 状态 + 不可用原因。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkspaceModuleStatus {
    pub kind: WorkspaceModuleStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl WorkspaceModuleStatus {
    pub fn ready() -> Self {
        Self {
            kind: WorkspaceModuleStatusKind::Ready,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            kind: WorkspaceModuleStatusKind::Unavailable,
            reason: Some(reason.into()),
        }
    }
}

/// `list` 返回的摘要——不含完整 schema。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleSummary {
    /// 稳定 id：`ext:{extension_key}` / `canvas:{mount_id}` / `builtin:{key}`。
    pub module_id: String,
    pub kind: WorkspaceModuleKind,
    pub title: String,
    pub description: String,
    /// extension_key / canvas mount / builtin key。
    pub source: String,
    /// 有几个 UI entry 的简述（无则 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_summary: Option<String>,
    /// operation_key 列表（仅 key，不含 schema）。
    pub operation_summary: Vec<String>,
    /// module 级权限摘要（来自 extension permission 声明；canvas/builtin 暂空）。
    pub permission_summary: Vec<String>,
    pub status: WorkspaceModuleStatus,
}

/// 单个 UI 入口（webview / canvas / panel）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleUiEntry {
    pub view_key: String,
    /// "webview" | "canvas" | "panel"。
    pub renderer_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_scheme: Option<String>,
    pub title: String,
}

/// 单个 operation（extension action / protocol channel method / canvas / builtin
/// 同构呈现）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleOperation {
    pub operation_key: String,
    /// "runtime_action" | "protocol_channel" | "canvas" | "builtin"。
    pub origin: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub permission_summary: Vec<String>,
}

/// `describe` 返回的完整 descriptor。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleDescriptor {
    pub summary: WorkspaceModuleSummary,
    pub ui_entries: Vec<WorkspaceModuleUiEntry>,
    pub operations: Vec<WorkspaceModuleOperation>,
    /// 引用底层 runtime surface（如 extension_runtime / canvas mount）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_backing: Option<String>,
}
