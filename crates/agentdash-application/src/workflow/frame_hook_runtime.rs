//! AgentFrameHookRuntime — 以 agent/frame 为主键的 hook runtime。
//!
//! ## 设计定位
//!
//! 替代 `HookSessionRuntime` 的 session-indexed hook runtime。
//! hook query/resolution 以 `run_id + agent_id + frame_id` 为主语：
//!
//! - 读取 context/capability/VFS/MCP surface：从 `AgentFrame` 读取
//! - advance/resolution：使用 assignment 或 graph instance refs 推进 Activity
//! - `runtime_session_id` 仅保留 trace adapter / provider query 语义

use std::collections::BTreeSet;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

use agentdash_spi::hooks::{
    AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, ContextTokenStats,
    ExecutionHookProvider, HookControlTarget, HookDiagnosticEntry, HookError, HookEvaluationQuery,
    HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
    HookRuntimeAccess, HookSessionRuntimeSnapshot, HookTraceEntry, HookTurnStartNotice,
    RuntimeAdapterProvenance, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionSnapshotMetadata, SetDelta,
};
use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;

const TRACE_BROADCAST_CAPACITY: usize = 128;

/// 以 agent/frame 为主键的 hook runtime。
///
/// hook 从此结构读取 effective surface（capability/context/VFS/MCP），
/// 不再从 session 反查 business owner。
pub struct AgentFrameHookRuntime {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    /// provider query / trace 兼容用 session_id
    runtime_session_id: String,
    provider: Arc<dyn ExecutionHookProvider>,
    snapshot: RwLock<SessionHookSnapshot>,
    diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
    trace: RwLock<Vec<HookTraceEntry>>,
    pending_actions: RwLock<Vec<HookPendingAction>>,
    turn_start_notices: RwLock<Vec<HookTurnStartNotice>>,
    token_stats: RwLock<ContextTokenStats>,
    compaction_failure_count: AtomicU32,
    capabilities: RwLock<BTreeSet<String>>,
    revision: AtomicU64,
    trace_sequence: AtomicU64,
    trace_broadcast: broadcast::Sender<HookTraceEntry>,
}

impl std::fmt::Debug for AgentFrameHookRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentFrameHookRuntime")
            .field("run_id", &self.run_id)
            .field("agent_id", &self.agent_id)
            .field("frame_id", &self.frame_id)
            .field("runtime_session_id", &self.runtime_session_id)
            .field("revision", &self.revision())
            .finish()
    }
}

/// Hook query scope — 替代 session-indexed `SessionHookSnapshotQuery`。
#[derive(Debug, Clone)]
pub struct FrameHookQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub turn_id: Option<String>,
}

impl AgentFrameHookRuntime {
    pub fn new(
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Uuid,
        frame_revision: i32,
        runtime_session_id: String,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: SessionHookSnapshot,
    ) -> Self {
        let diagnostics = snapshot.diagnostics.clone();
        let (trace_broadcast, _) = broadcast::channel(TRACE_BROADCAST_CAPACITY);
        Self {
            run_id,
            agent_id,
            frame_id,
            frame_revision,
            runtime_session_id,
            provider,
            snapshot: RwLock::new(snapshot),
            diagnostics: RwLock::new(diagnostics),
            trace: RwLock::new(Vec::new()),
            pending_actions: RwLock::new(Vec::new()),
            turn_start_notices: RwLock::new(Vec::new()),
            token_stats: RwLock::new(ContextTokenStats::default()),
            compaction_failure_count: AtomicU32::new(0),
            capabilities: RwLock::new(BTreeSet::new()),
            revision: AtomicU64::new(1),
            trace_sequence: AtomicU64::new(0),
            trace_broadcast,
        }
    }

    /// 创建测试用 trace-only hook runtime（无 lifecycle/frame 上下文）。
    ///
    /// 生产 lifecycle runtime 必须使用 [`Self::from_frame`]。
    #[cfg(test)]
    pub fn new_test_runtime(
        runtime_session_id: String,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: SessionHookSnapshot,
    ) -> Self {
        let trace_run_id = Uuid::new_v4();
        Self::new(
            trace_run_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            0,
            runtime_session_id,
            provider,
            snapshot,
        )
    }

    /// 从 frame 创建 hook runtime scope。
    pub fn from_frame(
        run_id: Uuid,
        frame: &agentdash_domain::workflow::AgentFrame,
        runtime_session_id: String,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: SessionHookSnapshot,
    ) -> Self {
        Self::new(
            run_id,
            frame.agent_id,
            frame.id,
            frame.revision,
            runtime_session_id,
            provider,
            snapshot,
        )
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    fn hook_control_target(&self) -> HookControlTarget {
        HookControlTarget {
            run_id: self.run_id,
            agent_id: self.agent_id,
            frame_id: self.frame_id,
            assignment_id: None,
        }
    }

    fn runtime_provenance(
        &self,
        turn_id: Option<String>,
        source: &str,
    ) -> RuntimeAdapterProvenance {
        RuntimeAdapterProvenance::runtime_session(self.runtime_session_id.clone(), turn_id, source)
    }

    fn append_diagnostics_inner<I>(&self, entries: I)
    where
        I: IntoIterator<Item = HookDiagnosticEntry>,
    {
        let mut guard = self
            .diagnostics
            .write()
            .expect("hook diagnostics write lock poisoned");
        for entry in entries {
            if guard
                .iter()
                .any(|existing| existing.code == entry.code && existing.message == entry.message)
            {
                continue;
            }
            guard.push(entry);
        }
    }
}

/// Merge back session-level constant metadata from the previous snapshot into a
/// freshly-loaded snapshot. These fields are set once at session start and must
/// survive snapshot refreshes triggered by runtime hooks.
fn preserve_session_level_metadata(
    snapshot: &mut SessionHookSnapshot,
    previous_metadata: Option<&SessionSnapshotMetadata>,
) {
    let Some(previous) = previous_metadata else {
        return;
    };
    let target = snapshot
        .metadata
        .get_or_insert_with(SessionSnapshotMetadata::default);

    macro_rules! preserve_field {
        ($field:ident) => {
            if target.$field.is_none() {
                target.$field = previous.$field.clone();
            }
        };
    }
    preserve_field!(permission_policy);
    preserve_field!(working_directory);
    preserve_field!(connector_id);
    preserve_field!(executor);
}

#[async_trait]
impl HookRuntimeAccess for AgentFrameHookRuntime {
    fn session_id(&self) -> &str {
        &self.runtime_session_id
    }

    fn control_target(&self) -> HookControlTarget {
        self.hook_control_target()
    }

    fn snapshot(&self) -> SessionHookSnapshot {
        self.snapshot
            .read()
            .expect("hook snapshot read lock poisoned")
            .clone()
    }

    fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
        self.diagnostics
            .read()
            .expect("hook diagnostics read lock poisoned")
            .clone()
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    fn trace(&self) -> Vec<HookTraceEntry> {
        self.trace
            .read()
            .expect("hook trace read lock poisoned")
            .clone()
    }

    fn pending_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .clone()
    }

    fn runtime_snapshot(&self) -> HookSessionRuntimeSnapshot {
        HookSessionRuntimeSnapshot {
            session_id: self.runtime_session_id.clone(),
            revision: self.revision(),
            snapshot: self.snapshot(),
            diagnostics: self.diagnostics(),
            trace: self.trace(),
            pending_actions: self.pending_actions(),
        }
    }

    async fn refresh(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        let previous_metadata = self.snapshot().metadata.clone();
        let mut snapshot = self
            .provider
            .refresh_frame_snapshot(AgentFrameHookRefreshQuery {
                target: self.hook_control_target(),
                provenance: self.runtime_provenance(query.turn_id, "hook_runtime_refresh"),
                reason: query.reason,
            })
            .await?;
        preserve_session_level_metadata(&mut snapshot, previous_metadata.as_ref());
        self.replace_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    fn update_token_stats(&self, stats: ContextTokenStats) {
        *self
            .token_stats
            .write()
            .expect("token stats write lock poisoned") = stats;
    }

    fn token_stats(&self) -> ContextTokenStats {
        self.token_stats
            .read()
            .expect("token stats read lock poisoned")
            .clone()
    }

    fn record_compaction_failure(&self, _error: &str) -> u32 {
        self.compaction_failure_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn reset_compaction_failures(&self) {
        self.compaction_failure_count.store(0, Ordering::SeqCst);
    }

    fn compaction_failure_count(&self) -> u32 {
        self.compaction_failure_count.load(Ordering::SeqCst)
    }

    async fn evaluate(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        let mut query = query;
        query.token_stats = Some(self.token_stats());
        let frame_query = AgentFrameHookEvaluationQuery {
            target: self.hook_control_target(),
            provenance: self.runtime_provenance(query.turn_id.clone(), "hook_runtime_evaluate"),
            trigger: query.trigger,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            snapshot: query.snapshot,
            payload: query.payload,
            token_stats: query.token_stats,
        };

        let mut resolution = self.provider.evaluate_frame_hook(frame_query).await?;

        if let Some(advance_request) = resolution.pending_advance.take() {
            match self.provider.advance_workflow_step(advance_request).await {
                Ok(()) => {
                    resolution.refresh_snapshot = true;
                    if let Some(completion) = resolution.completion.as_mut() {
                        completion.advanced = true;
                    }
                }
                Err(error) => {
                    resolution.diagnostics.push(HookDiagnosticEntry {
                        code: "workflow_step_advance_failed".to_string(),
                        message: format!("post-evaluate step advancement failed: {error}"),
                    });
                    if let Some(completion) = resolution.completion.as_mut() {
                        completion.advanced = false;
                        completion.reason =
                            format!("completion satisfied, but advance failed: {error}");
                    }
                }
            }
        }

        let pending_log = std::mem::take(&mut resolution.pending_execution_log);
        if !pending_log.is_empty()
            && let Err(error) = self.provider.append_execution_log(pending_log).await
        {
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "execution_log_flush_failed".to_string(),
                message: format!("failed to flush execution log entries: {error}"),
            });
        }

        self.append_diagnostics_inner(resolution.diagnostics.clone());
        Ok(resolution)
    }

    fn replace_snapshot(&self, snapshot: SessionHookSnapshot) {
        {
            let mut guard = self
                .snapshot
                .write()
                .expect("hook snapshot write lock poisoned");
            *guard = snapshot.clone();
        }
        self.append_diagnostics_inner(snapshot.diagnostics);
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    fn append_diagnostics_vec(&self, entries: Vec<HookDiagnosticEntry>) {
        self.append_diagnostics_inner(entries);
    }

    fn append_trace(&self, trace: HookTraceEntry) {
        let mut guard = self.trace.write().expect("hook trace write lock poisoned");
        guard.push(trace.clone());
        if guard.len() > 200 {
            let drain_count = guard.len() - 200;
            guard.drain(0..drain_count);
        }
        drop(guard);
        let _ = self.trace_broadcast.send(trace);
    }

    fn current_capabilities(&self) -> BTreeSet<String> {
        self.capabilities
            .read()
            .expect("capabilities read lock poisoned")
            .clone()
    }

    fn update_capabilities(&self, new_caps: BTreeSet<String>) -> Option<SetDelta> {
        let mut guard = self
            .capabilities
            .write()
            .expect("capabilities write lock poisoned");
        let delta = SetDelta::compute(&guard, &new_caps);
        if delta.is_empty() {
            return None;
        }
        *guard = new_caps;
        self.revision.fetch_add(1, Ordering::SeqCst);
        Some(delta)
    }

    fn subscribe_traces(&self) -> Option<broadcast::Receiver<HookTraceEntry>> {
        Some(self.trace_broadcast.subscribe())
    }

    fn next_trace_sequence(&self) -> u64 {
        self.trace_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn enqueue_pending_action(&self, action: HookPendingAction) {
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

    fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction> {
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
            action.last_injected_at_ms = Some(now);
            injected.push(action.clone());
        }
        if !injected.is_empty() {
            self.revision.fetch_add(1, Ordering::SeqCst);
        }
        injected
    }

    fn enqueue_turn_start_notice(&self, notice: HookTurnStartNotice) {
        if notice.content.trim().is_empty() {
            return;
        }
        let mut guard = self
            .turn_start_notices
            .write()
            .expect("hook turn-start notices write lock poisoned");
        if guard.iter().any(|existing| existing.id == notice.id) {
            return;
        }
        guard.push(notice);
        if guard.len() > 64 {
            let drain_count = guard.len() - 64;
            guard.drain(0..drain_count);
        }
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    fn collect_turn_start_notices_for_injection(&self) -> Vec<HookTurnStartNotice> {
        let mut guard = self
            .turn_start_notices
            .write()
            .expect("hook turn-start notices write lock poisoned");
        if guard.is_empty() {
            return Vec::new();
        }
        let notices = guard.clone();
        guard.clear();
        self.revision.fetch_add(1, Ordering::SeqCst);
        notices
    }

    fn unresolved_pending_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .iter()
            .filter(|action| action.is_unresolved())
            .cloned()
            .collect()
    }

    fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction> {
        self.pending_actions
            .read()
            .expect("hook pending actions read lock poisoned")
            .iter()
            .filter(|action| action.is_unresolved() && action.is_blocking())
            .cloned()
            .collect()
    }

    fn resolve_pending_action(
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

        action.status = HookPendingActionStatus::Resolved;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use agentdash_spi::hooks::NoopExecutionHookProvider;
    use agentdash_spi::hooks::{
        AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, AgentFrameHookSnapshotQuery,
    };

    #[derive(Default)]
    struct RecordingFrameProvider {
        refresh_queries: Mutex<Vec<AgentFrameHookRefreshQuery>>,
    }

    #[async_trait]
    impl ExecutionHookProvider for RecordingFrameProvider {
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(SessionHookSnapshot {
                session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                metadata: Some(SessionSnapshotMetadata {
                    turn_id: query.provenance.turn_id,
                    ..Default::default()
                }),
                ..SessionHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            self.refresh_queries
                .lock()
                .expect("refresh query lock poisoned")
                .push(query.clone());
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

        async fn load_session_snapshot(
            &self,
            _query: agentdash_spi::hooks::SessionHookSnapshotQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Err(HookError::Runtime(
                "session snapshot entry should not be used".to_string(),
            ))
        }

        async fn refresh_session_snapshot(
            &self,
            _query: SessionHookRefreshQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Err(HookError::Runtime(
                "session refresh entry should not be used".to_string(),
            ))
        }

        async fn evaluate_hook(
            &self,
            _query: HookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            Err(HookError::Runtime(
                "session evaluation entry should not be used".to_string(),
            ))
        }
    }

    #[test]
    fn from_frame_creates_runtime_scope() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = agentdash_domain::workflow::AgentFrame::new_revision(agent_id, 3, "test");

        let runtime = AgentFrameHookRuntime::from_frame(
            run_id,
            &frame,
            "sess-1".to_string(),
            Arc::new(NoopExecutionHookProvider),
            SessionHookSnapshot {
                session_id: "sess-1".to_string(),
                ..SessionHookSnapshot::default()
            },
        );

        assert_eq!(runtime.run_id, run_id);
        assert_eq!(runtime.agent_id, agent_id);
        assert_eq!(runtime.frame_id, frame.id);
        assert_eq!(runtime.frame_revision, 3);
        assert_eq!(runtime.session_id(), "sess-1");
    }

    #[test]
    fn standalone_creates_trace_only_run_id() {
        let runtime = AgentFrameHookRuntime::new_test_runtime(
            "sess-standalone".to_string(),
            Arc::new(NoopExecutionHookProvider),
            SessionHookSnapshot::default(),
        );
        assert_ne!(runtime.run_id, Uuid::from_u128(0));
        assert_eq!(runtime.session_id(), "sess-standalone");
    }

    #[test]
    fn preserve_session_level_metadata_merges_missing_keys() {
        let mut snapshot = SessionHookSnapshot {
            session_id: "s1".into(),
            metadata: Some(SessionSnapshotMetadata {
                turn_id: Some("turn-2".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let previous = SessionSnapshotMetadata {
            turn_id: Some("turn-1".into()),
            permission_policy: Some("SUPERVISED".into()),
            working_directory: Some(".".into()),
            connector_id: Some("pi_agent".into()),
            executor: Some("local".into()),
            ..Default::default()
        };

        preserve_session_level_metadata(&mut snapshot, Some(&previous));

        let meta = snapshot.metadata.as_ref().unwrap();
        assert_eq!(meta.permission_policy.as_deref(), Some("SUPERVISED"));
        assert_eq!(meta.working_directory.as_deref(), Some("."));
        assert_eq!(meta.connector_id.as_deref(), Some("pi_agent"));
        assert_eq!(meta.executor.as_deref(), Some("local"));
        assert_eq!(meta.turn_id.as_deref(), Some("turn-2"));
    }

    #[tokio::test]
    async fn refresh_preserves_session_level_metadata() {
        let initial_snapshot = SessionHookSnapshot {
            session_id: "sess-1".into(),
            metadata: Some(SessionSnapshotMetadata {
                permission_policy: Some("SUPERVISED".into()),
                working_directory: Some(".".into()),
                connector_id: Some("pi_agent".into()),
                executor: Some("local".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let provider = Arc::new(NoopExecutionHookProvider);
        let runtime =
            AgentFrameHookRuntime::new_test_runtime("sess-1".into(), provider, initial_snapshot);

        let refreshed = runtime
            .refresh(SessionHookRefreshQuery {
                session_id: "sess-1".into(),
                turn_id: Some("turn-2".into()),
                reason: Some("test".into()),
            })
            .await
            .expect("refresh should succeed");

        let meta = refreshed.metadata.unwrap();
        assert_eq!(meta.permission_policy.as_deref(), Some("SUPERVISED"));
        assert_eq!(meta.working_directory.as_deref(), Some("."));
        assert_eq!(meta.connector_id.as_deref(), Some("pi_agent"));
        assert_eq!(meta.executor.as_deref(), Some("local"));
    }

    #[tokio::test]
    async fn refresh_uses_frame_target_provider_entry() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = agentdash_domain::workflow::AgentFrame::new_revision(agent_id, 7, "test");
        let provider = Arc::new(RecordingFrameProvider::default());
        let runtime = AgentFrameHookRuntime::from_frame(
            run_id,
            &frame,
            "sess-1".into(),
            provider.clone(),
            SessionHookSnapshot {
                session_id: "sess-1".into(),
                ..Default::default()
            },
        );

        runtime
            .refresh(SessionHookRefreshQuery {
                session_id: "ignored-session-owner".into(),
                turn_id: Some("turn-2".into()),
                reason: Some("test_refresh".into()),
            })
            .await
            .expect("refresh should use frame provider entry");

        let queries = provider
            .refresh_queries
            .lock()
            .expect("refresh query lock poisoned");
        assert_eq!(queries.len(), 1);
        let query = &queries[0];
        assert_eq!(query.target.run_id, run_id);
        assert_eq!(query.target.agent_id, agent_id);
        assert_eq!(query.target.frame_id, frame.id);
        assert_eq!(
            query.provenance.runtime_session_id.as_deref(),
            Some("sess-1")
        );
        assert_eq!(query.provenance.turn_id.as_deref(), Some("turn-2"));
        assert_eq!(query.reason.as_deref(), Some("test_refresh"));
    }
}
