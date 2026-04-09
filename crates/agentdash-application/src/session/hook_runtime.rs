use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use agentdash_spi::hooks::{
    ContextTokenStats, ExecutionHookProvider, HookDiagnosticEntry, HookError, HookEvaluationQuery,
    HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
    HookSessionRuntimeAccess, HookSessionRuntimeSnapshot, HookTraceEntry, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionSnapshotMetadata,
};
use async_trait::async_trait;
use tokio::sync::broadcast;

const TRACE_BROADCAST_CAPACITY: usize = 128;

pub struct HookSessionRuntime {
    session_id: String,
    provider: Arc<dyn ExecutionHookProvider>,
    snapshot: RwLock<SessionHookSnapshot>,
    diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
    trace: RwLock<Vec<HookTraceEntry>>,
    pending_actions: RwLock<Vec<HookPendingAction>>,
    token_stats: RwLock<ContextTokenStats>,
    revision: AtomicU64,
    trace_sequence: AtomicU64,
    trace_broadcast: broadcast::Sender<HookTraceEntry>,
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
        let (trace_broadcast, _) = broadcast::channel(TRACE_BROADCAST_CAPACITY);
        Self {
            session_id,
            provider,
            snapshot: RwLock::new(snapshot),
            diagnostics: RwLock::new(diagnostics),
            trace: RwLock::new(Vec::new()),
            pending_actions: RwLock::new(Vec::new()),
            token_stats: RwLock::new(ContextTokenStats::default()),
            revision: AtomicU64::new(1),
            trace_sequence: AtomicU64::new(0),
            trace_broadcast,
        }
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
impl HookSessionRuntimeAccess for HookSessionRuntime {
    fn session_id(&self) -> &str {
        &self.session_id
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
            session_id: self.session_id.clone(),
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
        let mut snapshot = self.provider.refresh_session_snapshot(query).await?;
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

    async fn evaluate(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        // 注入 runtime 状态到 query
        let mut query = query;
        query.token_stats = Some(self.token_stats());

        let mut resolution = self.provider.evaluate_hook(query).await?;

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
    use agentdash_spi::hooks::NoopExecutionHookProvider;

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

    #[test]
    fn preserve_session_level_metadata_does_not_overwrite_existing() {
        let mut snapshot = SessionHookSnapshot {
            session_id: "s1".into(),
            metadata: Some(SessionSnapshotMetadata {
                permission_policy: Some("AUTONOMOUS".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let previous = SessionSnapshotMetadata {
            permission_policy: Some("SUPERVISED".into()),
            ..Default::default()
        };

        preserve_session_level_metadata(&mut snapshot, Some(&previous));

        let meta = snapshot.metadata.as_ref().unwrap();
        assert_eq!(meta.permission_policy.as_deref(), Some("AUTONOMOUS"));
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
        let runtime = HookSessionRuntime::new("sess-1".into(), provider, initial_snapshot);

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
}
