use serde::{Deserialize, Serialize};

/// 平台独有事件 — Codex 原生协议未覆盖的语义在此扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PlatformEvent {
    /// Connector 绑定了底层执行器 session（用于 follow-up / resume）。
    ExecutorSessionBound {
        executor_session_id: String,
    },

    /// Hook 运行时追踪条目。
    HookTrace(HookTracePayload),

    /// 平台元信息更新（系统消息、能力变更等）。
    SessionMetaUpdate {
        key: String,
        value: serde_json::Value,
    },
}

/// Hook trace payload — 对应原 `hook_trace_notification.rs` 产出的信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookTracePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
