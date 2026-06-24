pub mod script;
pub mod trace;

use std::sync::Arc;

use agentdash_domain::workflow::{EffectiveSessionContract, LifecycleRunStatus};

use crate::CapabilityScope;

/// Well-known `action_type` string values for [`HookPendingAction`].
///
/// These identifiers form the protocol contract between the hook runtime,
/// companion tools, and workflow rules (including Rhai scripts).
/// All string comparisons against `action_type` must use these constants.
pub mod action_type {
    /// Companion result requires the parent session to review and explicitly
    /// adopt or reject before proceeding or stopping.
    pub const BLOCKING_REVIEW: &str = "blocking_review";
    /// Companion result requires the parent session to follow up;
    /// does not block stop but is delivered as a follow-up prompt.
    pub const FOLLOW_UP_REQUIRED: &str = "follow_up_required";
    /// Companion result is informational only; parent session may ignore.
    pub const SUGGESTION: &str = "suggestion";
}

/// Well-known model context usage bucket identifiers.
///
/// Runtime context producers write these values into `context_usage_kind`;
/// projection consumers only read that explicit marker.
pub mod context_usage_kind {
    pub const SYSTEM_DEVELOPER: &str = "system_developer";
    pub const CAPABILITY_STATE: &str = "capability_state";
    pub const SYSTEM_TOOLS: &str = "system_tools";
    pub const MCP_TOOLS: &str = "mcp_tools";
    pub const AGENTS: &str = "agents";
    pub const SKILLS: &str = "skills";
}
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

pub use crate::connector::SetDelta;

/// Session 的 run-derived 业务上下文。
///
/// Hook runtime 只消费由 LifecycleSubjectAssociation、LifecycleAgent 与
/// AgentFrame 投影出的业务上下文，RuntimeSession id 仅作为 trace key。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SubjectRunContext {
    pub project_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Session 的能力作用域（从 run context 推导）
    pub scope: CapabilityScope,
}

/// 统一的 Hook 注入单元。
/// 合并了原来的 `HookContextFragment`（上下文注入）、`HookConstraint`（硬约束）、
/// `HookPolicyView`（策略描述）三种类型。
/// 通过 `slot` 字段区分用途：
/// - `"context"` — 通用上下文片段（原 HookContextFragment）
/// - `"constraint"` — 硬约束，delegate 层可据此做 gate 判断（原 HookConstraint）
/// - `"workflow"` — workflow 相关注入
/// - 其它自定义 slot 值
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookInjection {
    /// 用途标识：`"context"` / `"constraint"` / `"workflow"` / 自定义
    pub slot: String,
    /// 注入内容（Markdown 文本）
    pub content: String,
    /// 溯源标签（如 `"builtin:workspace_path_safety"` / `"workflow:trellis_dev_task:implement"`）
    #[serde(default)]
    pub source: String,
}

/// 精简的诊断条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookDiagnosticEntry {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameHookSnapshot {
    #[serde(alias = "session_id")]
    pub runtime_adapter_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_context: Option<SubjectRunContext>,
    /// 溯源标签集（如 `["builtin:runtime_trace", "workflow:trellis_dev_task:implement"]`）
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 统一注入列表（合并了原 context_fragments + constraints + policies）
    #[serde(default)]
    pub injections: Vec<HookInjection>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SessionSnapshotMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshotMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workflow: Option<ActiveWorkflowMeta>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,

    /// 保留扩展口 — 非核心字段仍可用 JSON
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ActiveWorkflowMeta {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "lifecycle_id"
    )]
    pub workflow_graph_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_status: Option<LifecycleRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workflow_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workflow_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_contract: Option<EffectiveSessionContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,

    /// 当前 node 的 output port key 列表（来自 AgentProcedureContract）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_port_keys: Option<Vec<String>>,
    /// 当前 lifecycle run 中已写入的 port output key 列表
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fulfilled_port_keys: Option<Vec<String>>,
    /// 当前 node 的 gate collision 次数
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_collision_count: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameRuntimeSnapshot {
    #[serde(alias = "session_id")]
    pub runtime_adapter_session_id: String,
    pub revision: u64,
    pub snapshot: AgentFrameHookSnapshot,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default)]
    pub trace: Vec<HookTraceEntry>,
    #[serde(default)]
    pub pending_actions: Vec<HookPendingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookPendingAction {
    pub id: String,
    pub created_at_ms: i64,
    pub title: String,
    pub summary: String,
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub source: RuntimeEventSource,
    #[serde(default)]
    pub status: HookPendingActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_injected_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_kind: Option<HookPendingActionResolutionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_turn_id: Option<String>,
    #[serde(default)]
    pub injections: Vec<HookInjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventSource {
    RuntimeContextUpdate,
    CompanionResult,
}

impl RuntimeEventSource {
    pub const fn as_key(&self) -> &'static str {
        match self {
            Self::RuntimeContextUpdate => "runtime_context_update",
            Self::CompanionResult => "companion_result",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookTurnStartNotice {
    pub id: String,
    pub created_at_ms: i64,
    pub source: RuntimeEventSource,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_frame: Option<ContextFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextFrame {
    pub id: String,
    pub kind: String,
    pub source: RuntimeEventSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_mode: Option<String>,
    pub delivery_status: String,
    pub delivery_channel: String,
    pub message_role: String,
    pub rendered_text: String,
    #[serde(default)]
    pub sections: Vec<ContextFrameSection>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextFrameSection {
    Identity {
        title: String,
        summary: String,
        base_prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_prompt: Option<String>,
        mode: String,
        effective_prompt: String,
    },
    AssignmentContext {
        title: String,
        summary: String,
        #[serde(default)]
        fragments: Vec<RuntimeContextFragmentEntry>,
    },
    ContinuationContext {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner_context: Option<String>,
        transcript_markdown: String,
    },
    CapabilityKeyDelta {
        #[serde(default)]
        added_capabilities: Vec<String>,
        #[serde(default)]
        removed_capabilities: Vec<String>,
        #[serde(default)]
        effective_capabilities: Vec<String>,
    },
    ToolPathDelta {
        #[serde(default)]
        blocked_tool_paths: Vec<String>,
        #[serde(default)]
        unblocked_tool_paths: Vec<String>,
        #[serde(default)]
        whitelisted_tool_paths: Vec<String>,
        #[serde(default)]
        removed_whitelist_paths: Vec<String>,
    },
    McpServerDelta {
        #[serde(default)]
        added_mcp_servers: Vec<String>,
        #[serde(default)]
        removed_mcp_servers: Vec<String>,
        #[serde(default)]
        changed_mcp_servers: Vec<String>,
    },
    VfsDelta {
        #[serde(default)]
        vfs_mounts_added: Vec<String>,
        #[serde(default)]
        vfs_mounts_removed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_after: Option<String>,
    },
    ToolSchemaDelta {
        #[serde(default)]
        added_tools: Vec<RuntimeToolSchemaEntry>,
    },
    SkillDelta {
        #[serde(default)]
        added_skills: Vec<RuntimeSkillEntry>,
        #[serde(default)]
        removed_skills: Vec<RuntimeSkillEntry>,
        #[serde(default)]
        changed_skills: Vec<RuntimeSkillEntry>,
    },
    CompanionAgentRosterDelta {
        #[serde(default)]
        added_agents: Vec<RuntimeCompanionAgentEntry>,
        #[serde(default)]
        removed_agent_keys: Vec<String>,
        #[serde(default)]
        changed_agents: Vec<RuntimeCompanionAgentEntry>,
        #[serde(default)]
        effective_agents: Vec<RuntimeCompanionAgentEntry>,
    },
    SystemNotice {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    PendingAction {
        title: String,
        summary: String,
        action_id: String,
        action_type: String,
        status: String,
        revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        #[serde(default)]
        instructions: Vec<String>,
        #[serde(default)]
        injections: Vec<RuntimeHookInjectionEntry>,
    },
    AutoResume {
        title: String,
        summary: String,
        reason: String,
        prompt: String,
    },
    CompactionSummary {
        title: String,
        summary: String,
        tokens_before: u64,
        messages_compacted: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compaction_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projection_version: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strategy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trigger: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_start_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_end_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first_kept_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compacted_until_ref: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    /// 用户级偏好（来自 settings）。作为系统级指引随 `system_guidelines` 帧投递。
    UserPreferences {
        title: String,
        summary: String,
        #[serde(default)]
        items: Vec<String>,
    },
    /// 项目级指引（来自 VFS 发现的 AGENTS.md 等）。
    ProjectGuidelines {
        title: String,
        summary: String,
        #[serde(default)]
        entries: Vec<ProjectGuidelineEntry>,
    },
}

/// `ProjectGuidelines` section 中的单条指引条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ProjectGuidelineEntry {
    /// 相对于 mount 根的路径（如 `AGENTS.md` 或 `packages/foo/AGENTS.md`）。
    pub path: String,
    /// 文件全文内容。
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeToolSchemaEntry {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeHookInjectionEntry {
    pub slot: String,
    pub source: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeContextFragmentEntry {
    pub slot: String,
    pub label: String,
    pub source: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSkillEntry {
    pub name: String,
    #[serde(default)]
    pub capability_key: String,
    #[serde(default)]
    pub provider_key: String,
    #[serde(default)]
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub exposure: crate::platform::skill_discovery::SkillContextExposure,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCompanionAgentEntry {
    pub agent_key: String,
    pub executor: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookPendingActionStatus {
    #[default]
    Pending,
    Resolved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookPendingActionResolutionKind {
    Adopted,
    Dismissed,
}

impl HookPendingAction {
    pub fn is_unresolved(&self) -> bool {
        matches!(self.status, HookPendingActionStatus::Pending)
    }

    /// Returns `true` if this action must be resolved before the session can stop.
    pub fn is_blocking(&self) -> bool {
        self.action_type == action_type::BLOCKING_REVIEW
    }

    /// Returns `true` if this action should be delivered as a follow-up prompt
    /// rather than an inline steering message.
    pub fn is_follow_up(&self) -> bool {
        self.action_type == action_type::FOLLOW_UP_REQUIRED
    }
}

/// Hook 运行时的接口 — 用于 executor/connector 层通过 trait object 访问。
/// 具体实现（`AgentFrameHookRuntime`）位于 application 层。
#[async_trait]
pub trait HookRuntimeAccess: Send + Sync + std::fmt::Debug {
    fn session_id(&self) -> &str;
    fn control_target(&self) -> HookControlTarget;
    fn snapshot(&self) -> AgentFrameHookSnapshot;
    fn diagnostics(&self) -> Vec<HookDiagnosticEntry>;
    fn revision(&self) -> u64;
    fn trace(&self) -> Vec<HookTraceEntry>;
    fn pending_actions(&self) -> Vec<HookPendingAction>;
    fn runtime_snapshot(&self) -> AgentFrameRuntimeSnapshot;

    async fn refresh_from_provenance(
        &self,
        query: HookRuntimeRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError>;
    async fn evaluate_from_provenance(
        &self,
        query: HookRuntimeEvaluationQuery,
    ) -> Result<HookResolution, HookError>;

    fn replace_snapshot(&self, snapshot: AgentFrameHookSnapshot);
    fn append_diagnostics_vec(&self, entries: Vec<HookDiagnosticEntry>);
    fn append_trace(&self, trace: HookTraceEntry);
    fn next_trace_sequence(&self) -> u64;
    fn enqueue_pending_action(&self, action: HookPendingAction);
    fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction>;
    fn enqueue_turn_start_notice(&self, notice: HookTurnStartNotice);
    fn collect_turn_start_notices_for_injection(&self) -> Vec<HookTurnStartNotice>;
    fn unresolved_pending_actions(&self) -> Vec<HookPendingAction>;
    fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction>;
    fn resolve_pending_action(
        &self,
        action_id: &str,
        resolution_kind: HookPendingActionResolutionKind,
        note: Option<String>,
        turn_id: Option<String>,
    ) -> Option<HookPendingAction>;

    /// 更新实时 token 统计。
    fn update_token_stats(&self, stats: ContextTokenStats);
    /// 读取当前 token 统计。
    fn token_stats(&self) -> ContextTokenStats;

    /// 记录一次结构性压缩失败，返回当前连续失败次数。
    fn record_compaction_failure(&self, _error: &str) -> u32 {
        0
    }

    /// 清空结构性压缩连续失败计数。
    fn reset_compaction_failures(&self) {}

    /// 读取结构性压缩连续失败次数。
    fn compaction_failure_count(&self) -> u32 {
        0
    }

    /// 读取当前生效的能力 key 集合。
    fn current_capabilities(&self) -> std::collections::BTreeSet<String> {
        Default::default()
    }

    /// 更新当前能力集并返回 delta（若有变更）。
    fn update_capabilities(
        &self,
        _new_caps: std::collections::BTreeSet<String>,
    ) -> Option<SetDelta> {
        None
    }

    /// 订阅实时 trace 事件流。返回 None 表示此实现不支持 trace 广播。
    fn subscribe_traces(&self) -> Option<broadcast::Receiver<HookTraceEntry>> {
        None
    }
}

pub type SharedHookRuntime = Arc<dyn HookRuntimeAccess>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookControlTarget {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeAdapterProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub source: String,
}

impl RuntimeAdapterProvenance {
    pub fn runtime_session(
        runtime_session_id: impl Into<String>,
        turn_id: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            runtime_session_id: Some(runtime_session_id.into()),
            turn_id,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameHookSnapshotQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameHookRefreshQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameHookEvaluationQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
    pub trigger: HookTrigger,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub snapshot: Option<AgentFrameHookSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_stats: Option<ContextTokenStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookRuntimeRefreshQuery {
    pub provenance: RuntimeAdapterProvenance,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookRuntimeEvaluationQuery {
    pub provenance: RuntimeAdapterProvenance,
    pub trigger: HookTrigger,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub snapshot: Option<AgentFrameHookSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_stats: Option<ContextTokenStats>,
}

/// Hook trace 触发点：只用于 Agent 核心生命周期的可见追踪。
pub use agentdash_agent_protocol::HookTraceTrigger;

/// Hook 规则评估入口。
///
/// 与 [`HookTraceTrigger`] 不同，这里可以包含运行期事件（例如 companion 结果回流）。
/// 这类事件可驱动规则产生 pending action / turn-start notice，但不应写入 HookTrace。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HookEvaluationTrigger {
    SessionStart,
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    CompanionResult,
    BeforeCompact,
    AfterCompact,
    BeforeProviderRequest,
}

impl HookEvaluationTrigger {
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
            Self::CompanionResult => "companion_result",
            Self::BeforeCompact => "before_compact",
            Self::AfterCompact => "after_compact",
            Self::BeforeProviderRequest => "before_provider_request",
        }
    }

    #[must_use]
    pub const fn trace_trigger(self) -> Option<HookTraceTrigger> {
        match self {
            Self::SessionStart => Some(HookTraceTrigger::SessionStart),
            Self::UserPromptSubmit => Some(HookTraceTrigger::UserPromptSubmit),
            Self::BeforeTool => Some(HookTraceTrigger::BeforeTool),
            Self::AfterTool => Some(HookTraceTrigger::AfterTool),
            Self::AfterTurn => Some(HookTraceTrigger::AfterTurn),
            Self::BeforeStop => Some(HookTraceTrigger::BeforeStop),
            Self::SessionTerminal => Some(HookTraceTrigger::SessionTerminal),
            Self::BeforeSubagentDispatch => Some(HookTraceTrigger::BeforeSubagentDispatch),
            Self::AfterSubagentDispatch => Some(HookTraceTrigger::AfterSubagentDispatch),
            Self::CompanionResult => None,
            Self::BeforeCompact => Some(HookTraceTrigger::BeforeCompact),
            Self::AfterCompact => Some(HookTraceTrigger::AfterCompact),
            Self::BeforeProviderRequest => Some(HookTraceTrigger::BeforeProviderRequest),
        }
    }
}

impl From<HookTraceTrigger> for HookEvaluationTrigger {
    fn from(value: HookTraceTrigger) -> Self {
        match value {
            HookTraceTrigger::SessionStart => Self::SessionStart,
            HookTraceTrigger::UserPromptSubmit => Self::UserPromptSubmit,
            HookTraceTrigger::BeforeTool => Self::BeforeTool,
            HookTraceTrigger::AfterTool => Self::AfterTool,
            HookTraceTrigger::AfterTurn => Self::AfterTurn,
            HookTraceTrigger::BeforeStop => Self::BeforeStop,
            HookTraceTrigger::SessionTerminal => Self::SessionTerminal,
            HookTraceTrigger::BeforeSubagentDispatch => Self::BeforeSubagentDispatch,
            HookTraceTrigger::AfterSubagentDispatch => Self::AfterSubagentDispatch,
            HookTraceTrigger::BeforeCompact => Self::BeforeCompact,
            HookTraceTrigger::AfterCompact => Self::AfterCompact,
            HookTraceTrigger::BeforeProviderRequest => Self::BeforeProviderRequest,
        }
    }
}

/// Hook 评估入口的本地别名。新代码如果需要强调“不是 trace”，优先使用
/// [`HookEvaluationTrigger`] 全名。
pub use HookEvaluationTrigger as HookTrigger;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookEvaluationQuery {
    pub session_id: String,
    pub trigger: HookTrigger,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub snapshot: Option<AgentFrameHookSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// 实时 token 统计（由 runtime 自动注入）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_stats: Option<ContextTokenStats>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookResolution {
    #[serde(default)]
    pub refresh_snapshot: bool,
    /// 统一注入列表（合并了原 context_fragments + constraints + policies）
    #[serde(default)]
    pub injections: Vec<HookInjection>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default)]
    pub matched_rule_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<HookCompletionStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewritten_tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_request: Option<HookApprovalRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    /// Step advancement signal. When set, `AgentFrameHookRuntime::evaluate`
    /// delegates to `provider.advance_workflow_step()` in a post-evaluate
    /// step and updates `completion.advanced` accordingly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_advance: Option<HookStepAdvanceRequest>,
    /// Execution log entries collected during this evaluation cycle.
    /// Flushed to `LifecycleRun.execution_log` by `AgentFrameHookRuntime`
    /// post-evaluate, via `provider.append_execution_log()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_execution_log: Vec<PendingExecutionLogEntry>,
    /// 通用副作用声明列表。
    ///
    /// Hook 规则（Rhai 脚本 / preset）通过返回 `effects` 数组声明需要在 pipeline 中
    /// 执行的领域级副作用。Pipeline 将 effects 转交给注册的 `HookEffectExecutor` 分派执行。
    ///
    /// kind 约定格式 `domain:action`，如 `"record:note"`。
    /// 载荷由 kind 消费方定义，SPI 层不对 payload 做类型约束。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<HookEffect>,
    /// 压缩决策。由 BeforeCompact hook 设置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<HookCompactionDecision>,
    /// 改写后的用户消息。由 UserPromptSubmit hook 设置。
    /// 当存在时，agent loop 使用此消息替换用户原始输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transformed_message: Option<String>,
}

/// Hook 评估产出的通用副作用声明。
///
/// 领域无关的通用结构：`kind` 标识副作用类型，`payload` 携带类型特定数据。
/// Pipeline 中注册的 effect executor 按 `kind` 前缀分派执行。
///
/// `kind` 约定格式 `domain:action`，示例：`"record:note"`。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookEffect {
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Agent loop 的实时 token 统计。
/// 由 AgentFrameHookRuntime 维护，自动注入到每次 hook 评估的 query 中。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextTokenStats {
    /// 最近一次 LLM 调用的 input token 数（来自 usage data）
    #[serde(default)]
    pub last_input_tokens: u64,
    /// 当前模型可见上下文压力，用于 UI 与压缩判断。
    #[serde(default)]
    pub current_context_tokens: u64,
    /// 最近 provider usage 后新增内容的本地估算。
    #[serde(default)]
    pub pending_estimate_tokens: u64,
    /// 模型 context window 上限
    #[serde(default)]
    pub context_window: u64,
    /// 扣除策略保留空间后的实际判断窗口。
    #[serde(default)]
    pub effective_context_window: u64,
    /// 输出、工具调用或摘要预留的 token 空间。
    #[serde(default)]
    pub reserve_tokens: u64,
}

/// Hook 层的压缩决策
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookCompactionDecision {
    /// 取消压缩
    #[serde(default)]
    pub cancel: bool,
    /// 覆盖 reserve_tokens 参数
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve_tokens: Option<u64>,
    /// 覆盖 keep_last_n 参数
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_last_n: Option<u32>,
    /// 提供自定义摘要（跳过 LLM 调用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_summary: Option<String>,
    /// 覆盖默认摘要 prompt
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookApprovalRequest {
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookCompletionStatus {
    pub mode: String,
    pub satisfied: bool,
    pub advanced: bool,
    pub reason: String,
}

/// Request payload for the post-evaluate step advancement bridge.
/// Produced by `evaluate_hook` when completion conditions are met, consumed by
/// `AgentFrameHookRuntime::evaluate` which delegates to
/// `ExecutionHookProvider::advance_workflow_step`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookStepAdvanceRequest {
    pub run_id: String,
    pub activity_key: String,
    pub completion_mode: String,
    pub summary: Option<String>,
    pub record_artifacts: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookTraceEntry {
    pub sequence: u64,
    pub timestamp_ms: i64,
    pub revision: u64,
    pub trigger: HookTraceTrigger,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub matched_rule_keys: Vec<String>,
    #[serde(default)]
    pub refresh_snapshot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<HookCompletionStatus>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<HookInjection>,
}

/// A lifecycle execution log entry collected during hook evaluation.
/// Carries the same shape as `LifecycleExecutionEntry` from the domain
/// layer, but without requiring a domain dependency in the connector crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PendingExecutionLogEntry {
    pub run_id: String,
    pub activity_key: String,
    pub event_kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

#[derive(Debug, Error)]
pub enum HookError {
    #[error("{0}")]
    Runtime(String),
}

#[async_trait]
pub trait ExecutionHookProvider: Send + Sync {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError>;

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError>;

    async fn evaluate_frame_hook(
        &self,
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, HookError>;

    /// Execute the actual step advancement. Called by `AgentFrameHookRuntime`
    /// post-evaluate when the resolution carries a `pending_advance` signal.
    async fn advance_workflow_step(
        &self,
        request: HookStepAdvanceRequest,
    ) -> Result<(), HookError> {
        let _ = request;
        Ok(())
    }

    /// Batch-flush execution log entries to `LifecycleRun.execution_log`.
    /// Called by `AgentFrameHookRuntime` post-evaluate when the resolution
    /// carries non-empty `pending_execution_log`.
    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        let _ = entries;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct NoopExecutionHookProvider;

#[async_trait]
impl ExecutionHookProvider for NoopExecutionHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        Ok(AgentFrameHookSnapshot {
            runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
            ..AgentFrameHookSnapshot::default()
        })
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
            target: query.target,
            provenance: query.provenance,
        })
        .await
    }

    async fn evaluate_frame_hook(
        &self,
        _query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        Ok(HookResolution::default())
    }
}

#[cfg(test)]
mod run_context_tests {
    use super::*;

    #[test]
    fn session_run_context_serde_roundtrip() {
        let ctx = SubjectRunContext {
            project_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            story_id: Some(Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap()),
            task_id: Some(Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap()),
            story_title: Some("Test Story".to_string()),
            task_title: Some("Test Task".to_string()),
            scope: CapabilityScope::Task,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let decoded: SubjectRunContext = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ctx);
    }

    #[test]
    fn session_run_context_scope_serializes_as_snake_case() {
        let ctx = SubjectRunContext {
            project_id: Uuid::nil(),
            scope: CapabilityScope::Story,
            ..Default::default()
        };
        let value = serde_json::to_value(&ctx).unwrap();
        assert_eq!(value["scope"], serde_json::Value::String("story".into()));
    }

    #[test]
    fn session_hook_snapshot_default_has_no_run_context() {
        let snapshot = AgentFrameHookSnapshot::default();
        assert!(snapshot.run_context.is_none());
    }
}
