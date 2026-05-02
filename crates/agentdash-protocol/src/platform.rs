use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 平台独有事件 — Codex 原生协议未覆盖的语义在此扩展。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PlatformEvent {
    /// Connector 绑定了底层执行器 session（用于 follow-up / resume）。
    ExecutorSessionBound { executor_session_id: String },

    /// Hook 运行时追踪条目。
    HookTrace(HookTracePayload),

    /// 平台元信息更新（系统消息、能力变更等）。
    SessionMetaUpdate {
        key: String,
        value: serde_json::Value,
    },
}

/// Hook trace payload — 对应原 `hook_trace_notification.rs` 产出的信息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct HookTracePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HookTraceData>,
}

/// Hook trace 的结构化数据体。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceData {
    pub trigger: String,
    pub decision: String,
    pub sequence: u64,
    pub revision: u64,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub matched_rule_keys: Vec<String>,
    #[serde(default)]
    pub refresh_snapshot: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<HookTraceCompletion>,
    #[serde(default)]
    pub diagnostic_codes: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<HookTraceDiagnostic>,
    #[serde(default)]
    pub injections: Vec<HookTraceInjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceCompletion {
    pub mode: String,
    pub satisfied: bool,
    pub advanced: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceDiagnostic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceInjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}
