use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookSourceLayer {
    #[default]
    GlobalBuiltin,
    Workflow,
    Project,
    Story,
    Task,
    Session,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookSourceRef {
    pub layer: HookSourceLayer,
    pub key: String,
    pub label: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookContextFragment {
    pub slot: String,
    pub label: String,
    pub content: String,
    #[serde(default)]
    pub source_summary: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<HookSourceRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookConstraint {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub source_summary: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<HookSourceRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookPolicy {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub source_summary: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<HookSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookDiagnosticEntry {
    pub code: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub source_summary: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<HookSourceRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookSnapshot {
    pub session_id: String,
    #[serde(default)]
    pub owners: Vec<HookOwnerSummary>,
    #[serde(default)]
    pub sources: Vec<HookSourceRef>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
    #[serde(default)]
    pub policies: Vec<HookPolicy>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookContributionSet {
    #[serde(default)]
    pub sources: Vec<HookSourceRef>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
    #[serde(default)]
    pub policies: Vec<HookPolicy>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
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
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookPendingActionStatus {
    #[default]
    Pending,
    Injected,
    Resolved,
    Dismissed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookPendingActionResolutionKind {
    Adopted,
    Rejected,
    Completed,
    Superseded,
    UserDismissed,
}

impl HookPendingAction {
    pub fn is_unresolved(&self) -> bool {
        matches!(
            self.status,
            HookPendingActionStatus::Pending | HookPendingActionStatus::Injected
        )
    }

    pub fn is_blocking(&self) -> bool {
        self.action_type == "blocking_review"
    }
}

pub struct HookSessionRuntime {
    session_id: String,
    provider: Arc<dyn ExecutionHookProvider>,
    snapshot: RwLock<SessionHookSnapshot>,
    diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
    trace: RwLock<Vec<HookTraceEntry>>,
    pending_actions: RwLock<Vec<HookPendingAction>>,
    revision: AtomicU64,
    trace_sequence: AtomicU64,
}

impl std::fmt::Debug for HookSessionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookSessionRuntime")
            .field("session_id", &self.session_id)
            .field("revision", &self.revision())
            .field("snapshot", &self.snapshot())
            .field("diagnostics_count", &self.diagnostics().len())
            .field("trace_count", &self.trace().len())
            .finish()
    }
}

impl HookSessionRuntime {
    pub fn new(
        session_id: String,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: SessionHookSnapshot,
    ) -> Self {
        let diagnostics = snapshot.diagnostics.clone();
        Self {
            session_id,
            provider,
            snapshot: RwLock::new(snapshot),
            diagnostics: RwLock::new(diagnostics),
            trace: RwLock::new(Vec::new()),
            pending_actions: RwLock::new(Vec::new()),
            revision: AtomicU64::new(1),
            trace_sequence: AtomicU64::new(0),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn snapshot(&self) -> SessionHookSnapshot {
        self.snapshot
            .read()
            .expect("hook snapshot read lock poisoned")
            .clone()
    }

    pub fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
        self.diagnostics
            .read()
            .expect("hook diagnostics read lock poisoned")
            .clone()
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    pub fn trace(&self) -> Vec<HookTraceEntry> {
        self.trace
            .read()
            .expect("hook trace read lock poisoned")
            .clone()
    }

    pub fn pending_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .clone()
    }

    pub fn runtime_snapshot(&self) -> HookSessionRuntimeSnapshot {
        HookSessionRuntimeSnapshot {
            session_id: self.session_id.clone(),
            revision: self.revision(),
            snapshot: self.snapshot(),
            diagnostics: self.diagnostics(),
            trace: self.trace(),
            pending_actions: self.pending_actions(),
        }
    }

    pub async fn refresh(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        let snapshot = self.provider.refresh_session_snapshot(query).await?;
        self.replace_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    pub async fn evaluate(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        let resolution = self.provider.evaluate_hook(query).await?;
        self.append_diagnostics(resolution.diagnostics.clone());
        Ok(resolution)
    }

    pub fn replace_snapshot(&self, snapshot: SessionHookSnapshot) {
        {
            let mut guard = self
                .snapshot
                .write()
                .expect("hook snapshot write lock poisoned");
            *guard = snapshot.clone();
        }
        self.append_diagnostics(snapshot.diagnostics);
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn append_diagnostics<I>(&self, entries: I)
    where
        I: IntoIterator<Item = HookDiagnosticEntry>,
    {
        let mut guard = self
            .diagnostics
            .write()
            .expect("hook diagnostics write lock poisoned");
        for entry in entries {
            if guard.iter().any(|existing| {
                existing.code == entry.code
                    && existing.summary == entry.summary
                    && existing.detail == entry.detail
                    && existing.source_summary == entry.source_summary
            }) {
                continue;
            }
            guard.push(entry);
        }
    }

    pub fn append_trace(&self, trace: HookTraceEntry) {
        let mut guard = self.trace.write().expect("hook trace write lock poisoned");
        guard.push(trace);
        if guard.len() > 200 {
            let drain_count = guard.len() - 200;
            guard.drain(0..drain_count);
        }
    }

    pub fn next_trace_sequence(&self) -> u64 {
        self.trace_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn enqueue_pending_action(&self, action: HookPendingAction) {
        let mut guard = self
            .pending_actions
            .write()
            .expect("hook pending actions write lock poisoned");
        if guard.iter().any(|existing| existing.id == action.id) {
            return;
        }
        guard.push(HookPendingAction {
            status: HookPendingActionStatus::Pending,
            last_injected_at_ms: None,
            resolved_at_ms: None,
            resolution_kind: None,
            resolution_note: None,
            resolution_turn_id: None,
            ..action
        });
        if guard.len() > 64 {
            let drain_count = guard.len() - 64;
            guard.drain(0..drain_count);
        }
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction> {
        let mut guard = self
            .pending_actions
            .write()
            .expect("hook pending actions write lock poisoned");
        let now = chrono::Utc::now().timestamp_millis();
        let mut injected = Vec::new();
        for action in guard.iter_mut() {
            if action.status != HookPendingActionStatus::Pending {
                continue;
            }
            action.status = HookPendingActionStatus::Injected;
            action.last_injected_at_ms = Some(now);
            injected.push(action.clone());
        }
        if !injected.is_empty() {
            self.revision.fetch_add(1, Ordering::SeqCst);
        }
        injected
    }

    pub fn unresolved_pending_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .iter()
            .filter(|action| action.is_unresolved())
            .cloned()
            .collect()
    }

    pub fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .iter()
            .filter(|action| action.is_unresolved() && action.is_blocking())
            .cloned()
            .collect()
    }

    pub fn resolve_pending_action(
        &self,
        action_id: &str,
        resolution_kind: HookPendingActionResolutionKind,
        note: Option<String>,
        turn_id: Option<String>,
    ) -> Option<HookPendingAction> {
        let mut guard = self
            .pending_actions
            .write()
            .expect("hook pending actions write lock poisoned");
        let action = guard.iter_mut().find(|action| action.id == action_id)?;
        if !action.is_unresolved() {
            return Some(action.clone());
        }

        action.status = match resolution_kind {
            HookPendingActionResolutionKind::UserDismissed => HookPendingActionStatus::Dismissed,
            HookPendingActionResolutionKind::Adopted
            | HookPendingActionResolutionKind::Rejected
            | HookPendingActionResolutionKind::Completed
            | HookPendingActionResolutionKind::Superseded => HookPendingActionStatus::Resolved,
        };
        action.resolved_at_ms = Some(chrono::Utc::now().timestamp_millis());
        action.resolution_kind = Some(resolution_kind);
        action.resolution_note = note
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        action.resolution_turn_id = turn_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.revision.fetch_add(1, Ordering::SeqCst);
        Some(action.clone())
    }
}

pub type SharedHookSessionRuntime = Arc<HookSessionRuntime>;

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
    #[serde(default)]
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
    #[serde(default)]
    pub policies: Vec<HookPolicy>,
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
