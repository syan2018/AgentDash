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
    /// 稳定 id：`ext:{extension_key}` / `canvas:{canvas_mount_id}` / `builtin:{key}`。
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
    /// 可直接交给 WorkspacePanel 打开的展示 URI，例如 `canvas://cvs-dashboard`
    /// 或 extension panel 的 `<scheme>://panel`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_uri: Option<String>,
    /// 底层 renderer scheme。保留给 extension webview/panel 描述，Canvas 的 VFS
    /// 编辑 mount 不应通过该字段作为展示入口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_scheme: Option<String>,
    pub title: String,
}

/// Operation exposure target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleOperationVisibility {
    /// Panel/UI may use this operation, but Agent tools must not expose or invoke it.
    PanelOnly,
    /// Agent tools and panel/UI may both use this operation.
    AgentAndPanel,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkspaceModuleOperationRef {
    pub namespace: String,
    pub provider_key: String,
    pub operation_key: String,
    pub contract_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkspaceModuleOperationProvenance {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleOperationEffect {
    Read,
    LocalMutation,
    ExternalSideEffect,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleOperationReplayPolicy {
    NonReplayable,
    Idempotent,
    ReplaySafe,
}

/// Operation 调用就绪状态；它只描述当前 operation 是否可调用，
/// 与 module 可见性和 renderer loadability 分层。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleOperationReadinessKind {
    Ready,
    Unavailable,
}

/// 当前 runtime 中 operation 调用可用性的结构化诊断。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkspaceModuleOperationReadiness {
    pub kind: WorkspaceModuleOperationReadinessKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl WorkspaceModuleOperationReadiness {
    pub fn ready() -> Self {
        Self {
            kind: WorkspaceModuleOperationReadinessKind::Ready,
            code: None,
            message: None,
        }
    }

    pub fn unavailable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: WorkspaceModuleOperationReadinessKind::Unavailable,
            code: Some(code.into()),
            message: Some(message.into()),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.kind == WorkspaceModuleOperationReadinessKind::Ready
    }
}

/// 单个 operation（extension action / protocol method / host canvas / builtin 同构呈现）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleOperation {
    pub operation_ref: WorkspaceModuleOperationRef,
    pub operation_key: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub permission_summary: Vec<String>,
    pub visibility: WorkspaceModuleOperationVisibility,
    pub effect: WorkspaceModuleOperationEffect,
    pub replay_policy: WorkspaceModuleOperationReplayPolicy,
    pub provenance: WorkspaceModuleOperationProvenance,
    pub readiness: WorkspaceModuleOperationReadiness,
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

/// 用户或 Agent 请求展示某个 workspace module UI entry。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModulePresentRequest {
    pub module_id: String,
    pub view_key: String,
    /// 可选 delivery trace context；HTTP user-open 不依赖它写事件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

/// canonical workspace module presentation payload。
///
/// Agent tool event、tool result details 与 HTTP user-open response 共用该形状。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModulePresentation {
    pub module_id: String,
    pub view_key: String,
    pub renderer_kind: String,
    pub presentation_uri: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Value>,
}
