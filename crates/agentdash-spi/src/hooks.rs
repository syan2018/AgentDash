use std::sync::Arc;

use agentdash_domain::workflow::{
    EffectiveSessionContract, LifecycleRunStatus, WorkflowRecordArtifactType,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookOwnerSummary {
    pub owner_type: String,
    pub owner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
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
pub struct SessionHookSnapshot {
    pub session_id: String,
    #[serde(default)]
    pub owners: Vec<HookOwnerSummary>,
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
    pub workspace_root: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_status: Option<LifecycleRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workflow_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workflow_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workflow_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_contract: Option<EffectiveSessionContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_evidence_present: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_evidence_artifact_type: Option<WorkflowRecordArtifactType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_evidence_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_evidence_artifact_ids: Option<Vec<Uuid>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_evidence_titles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookSessionRuntimeSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub snapshot: SessionHookSnapshot,
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
    pub source_trigger: HookTrigger,
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

    pub fn is_blocking(&self) -> bool {
        self.action_type == "blocking_review"
    }
}

/// Hook Session 运行时的接口 — 用于 executor/connector 层通过 trait object 访问。
/// 具体实现（`HookSessionRuntime`）位于 application 层。
#[async_trait]
pub trait HookSessionRuntimeAccess: Send + Sync + std::fmt::Debug {
    fn session_id(&self) -> &str;
    fn snapshot(&self) -> SessionHookSnapshot;
    fn diagnostics(&self) -> Vec<HookDiagnosticEntry>;
    fn revision(&self) -> u64;
    fn trace(&self) -> Vec<HookTraceEntry>;
    fn pending_actions(&self) -> Vec<HookPendingAction>;
    fn runtime_snapshot(&self) -> HookSessionRuntimeSnapshot;

    async fn refresh(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError>;
    async fn evaluate(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError>;

    fn replace_snapshot(&self, snapshot: SessionHookSnapshot);
    fn append_diagnostics_vec(&self, entries: Vec<HookDiagnosticEntry>);
    fn append_trace(&self, trace: HookTraceEntry);
    fn next_trace_sequence(&self) -> u64;
    fn enqueue_pending_action(&self, action: HookPendingAction);
    fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction>;
    fn unresolved_pending_actions(&self) -> Vec<HookPendingAction>;
    fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction>;
    fn resolve_pending_action(
        &self,
        action_id: &str,
        resolution_kind: HookPendingActionResolutionKind,
        note: Option<String>,
        turn_id: Option<String>,
    ) -> Option<HookPendingAction>;

    /// 订阅实时 trace 事件流。返回 None 表示此实现不支持 trace 广播。
    fn subscribe_traces(&self) -> Option<broadcast::Receiver<HookTraceEntry>> {
        None
    }
}

pub type SharedHookSessionRuntime = Arc<dyn HookSessionRuntimeAccess>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookSnapshotQuery {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub permission_policy: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub workspace_root: Option<String>,
    #[serde(default)]
    pub owners: Vec<HookOwnerSummary>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookRefreshQuery {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    SessionStart,
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    SubagentResult,
}

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
    pub snapshot: Option<SessionHookSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
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
    /// Step advancement signal. When set, `HookSessionRuntime::evaluate`
    /// delegates to `provider.advance_workflow_step()` in a post-evaluate
    /// step and updates `completion.advanced` accordingly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_advance: Option<HookStepAdvanceRequest>,
    /// Execution log entries collected during this evaluation cycle.
    /// Flushed to `LifecycleRun.execution_log` by `HookSessionRuntime`
    /// post-evaluate, via `provider.append_execution_log()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_execution_log: Vec<PendingExecutionLogEntry>,
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
/// `HookSessionRuntime::evaluate` which delegates to
/// `ExecutionHookProvider::advance_workflow_step`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookStepAdvanceRequest {
    pub run_id: String,
    pub step_key: String,
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
    pub trigger: HookTrigger,
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
}

/// A lifecycle execution log entry collected during hook evaluation.
/// Carries the same shape as `LifecycleExecutionEntry` from the domain
/// layer, but without requiring a domain dependency in the connector crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PendingExecutionLogEntry {
    pub run_id: String,
    pub step_key: String,
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
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn evaluate_hook(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError>;

    /// Execute the actual step advancement. Called by `HookSessionRuntime`
    /// post-evaluate when the resolution carries a `pending_advance` signal.
    async fn advance_workflow_step(
        &self,
        request: HookStepAdvanceRequest,
    ) -> Result<(), HookError> {
        let _ = request;
        Ok(())
    }

    /// Batch-flush execution log entries to `LifecycleRun.execution_log`.
    /// Called by `HookSessionRuntime` post-evaluate when the resolution
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
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn evaluate_hook(
        &self,
        _query: HookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        Ok(HookResolution::default())
    }
}
