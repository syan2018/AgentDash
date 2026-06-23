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

/// 宿主拥有的 Canvas module operation。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleCanvasHostAction {
    BindData,
}

/// operation 的来源专属派发分量。
///
/// `origin` 是给人/UI 看的扁平标签；`dispatch` 承载 invoke 元工具据以**直接路由**的
/// 结构化分量，由聚合层（`build_workspace_modules`）在构造 operation 时一并填好。
/// invoke 据 `dispatch` 派发，**不再字符串拆 `operation_key`**（避免 channel method
/// 名含驼峰时的反解析脆弱）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceModuleOperationDispatch {
    /// extension runtime action：直接以 `action_key` 走 RuntimeGateway。
    RuntimeAction { action_key: String },
    /// extension protocol channel method：走 ExtensionRuntimeChannelInvoker，不经 action_key。
    ProtocolChannel {
        channel_key: String,
        method_name: String,
    },
    /// 宿主 Canvas 资产操作：走 application use case，不进入 iframe/runtime action。
    HostCanvas {
        canvas_action: WorkspaceModuleCanvasHostAction,
    },
    /// builtin module operation：预留，本轮 invoke 返回 unimplemented。
    Builtin { builtin_key: String },
}

/// 单个 operation（extension action / protocol channel method / host canvas / builtin 同构呈现）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct WorkspaceModuleOperation {
    pub operation_key: String,
    /// "runtime_action" | "protocol_channel" | "host_canvas" | "builtin"。
    pub origin: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub permission_summary: Vec<String>,
    /// 来源专属路由分量，invoke 据此直接派发（不拆 operation_key）。
    pub dispatch: WorkspaceModuleOperationDispatch,
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
    /// 可选展示上下文；HTTP 用户打开只校验归属，Agent 工具路径负责运行时授权。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
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
