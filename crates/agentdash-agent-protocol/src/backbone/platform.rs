use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::context_frame::ContextFrameChanged;

/// 平台独有事件 — Codex 原生协议未覆盖的语义在此扩展。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PlatformEvent {
    /// Materialized platform context presentation changed.
    ContextFrameChanged(Box<ContextFrameChanged>),
    /// Connector 绑定了底层执行器 session（用于 follow-up / resume）。
    ExecutorSessionBound { executor_session_id: String },

    /// 来源执行器提供了已有会话标题（如 Codex `Thread.name`）。
    SourceSessionTitleUpdated {
        #[serde(skip_serializing_if = "Option::is_none")]
        executor_session_id: Option<String>,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        preview: Option<String>,
        source: String,
    },

    /// Hook 运行时追踪条目。
    HookTrace(Box<HookTracePayload>),

    /// 平台元信息更新（系统消息、能力变更等）。
    SessionMetaUpdate {
        key: String,
        value: serde_json::Value,
    },

    /// Provider attempt lifecycle status, used for model connection/retry UI.
    ProviderAttemptStatus(ProviderAttemptStatus),

    /// Bounded runtime terminal diagnostic captured before terminal convergence.
    RuntimeTerminalDiagnostic(RuntimeTerminalDiagnostic),

    /// Session projection was rewound to a stable boundary after a failed turn.
    SessionRewound(SessionRewound),

    /// AgentRun control-plane projection invalidation hint.
    ControlPlaneProjectionChanged(Box<ControlPlaneProjectionChanged>),

    /// 交互式终端输出流数据（路由到前端 xterm.js，不作为 chat entry 展示）。
    TerminalOutput { terminal_id: String, data: String },

    /// PTY/交互式终端生命周期变更（创建/退出/丢失/用户终止）。
    PtyTerminalStateChanged {
        terminal_id: String,
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTerminalDiagnostic {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ControlPlaneProjectionChanged {
    pub projection: ControlPlaneProjection,
    pub reason: ControlPlaneProjectionChangeReason,
    pub run_id: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_runtime_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_module_presentation: Option<ControlPlaneWorkspaceModulePresentation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneProjection {
    Workspace,
    AgentRunList,
    Mailbox,
    Waiting,
    Delivery,
    HookRuntime,
    ResourceSurface,
    Title,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneProjectionChangeReason {
    AgentRunLineageChanged,
    AgentRunShellChanged,
    AgentRunActivityChanged,
    MailboxStateChanged,
    WaitResolved,
    DeliveryTerminal,
    CompanionResult,
    HookEffectApplied,
    HookAutoResumeQueued,
    WorkspaceModulePresented,
    CapabilityStateChanged,
    ContextFrameChanged,
    TitleChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ControlPlaneWorkspaceModulePresentation {
    pub module_id: String,
    pub view_key: String,
    pub renderer_kind: String,
    pub presentation_uri: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ProviderAttemptStatus {
    pub turn_id: String,
    pub phase: ProviderAttemptPhase,
    pub attempt: u32,
    pub max_attempts: u32,
    #[serde(default)]
    pub will_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptPhase {
    Connecting,
    ConnectedWaitingFirstDelta,
    Streaming,
    RetryScheduled,
    Retrying,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionRewound {
    pub discarded_turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discarded_entry_index: Option<u32>,
    pub stable_event_seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_turn_id: Option<String>,
    pub reason: SessionRewindReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SessionRewindReason {
    ProviderRetry,
    ProviderFailure,
    RuntimeFailure,
}

/// Hook trace payload — 对应原 `hook_trace_notification.rs` 产出的信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceData {
    pub trigger: HookTraceTrigger,
    pub decision: String,
    pub sequence: u64,
    pub revision: u64,
    pub severity: HookTraceSeverity,
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
    #[serde(default)]
    pub effects_applied: bool,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTraceTrigger {
    SessionStart,
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    BeforeCompact,
    AfterCompact,
    BeforeProviderRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTraceSeverity {
    Error,
    Warning,
    Success,
    Info,
}

impl HookTraceTrigger {
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::UserPromptSubmit => "user_prompt_submit",
            Self::BeforeTool => "before_tool",
            Self::AfterTool => "after_tool",
            Self::AfterTurn => "after_turn",
            Self::BeforeStop => "before_stop",
            Self::SessionTerminal => "session_terminal",
            Self::BeforeSubagentDispatch => "before_subagent_dispatch",
            Self::AfterSubagentDispatch => "after_subagent_dispatch",
            Self::BeforeCompact => "before_compact",
            Self::AfterCompact => "after_compact",
            Self::BeforeProviderRequest => "before_provider_request",
        }
    }
}

impl HookTraceSeverity {
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Success => "success",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceCompletion {
    pub mode: String,
    pub satisfied: bool,
    pub advanced: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceDiagnostic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceInjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}
