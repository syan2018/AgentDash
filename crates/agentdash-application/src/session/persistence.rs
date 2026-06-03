use std::sync::Arc;

use agentdash_agent_protocol::BackboneEnvelope;
use async_trait::async_trait;
use uuid::Uuid;

pub use agentdash_spi::session_persistence::{
    AgentFrameTransitionRecord, CompactionProjectionCommitResult, NewCompactionProjectionCommit,
    NewTerminalEffectRecord, PersistedSessionEvent, RuntimeCommandRecord, RuntimeCommandStatus,
    RuntimeDeliveryCommand, SessionCompactionRecord, SessionCompactionStatus,
    SessionCompactionStore, SessionEventBacklog, SessionEventPage, SessionEventStore,
    SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus, SessionLineageStore,
    SessionMeta, SessionMetaStore, SessionPersistence, SessionProjectionHeadRecord,
    SessionProjectionSegmentRecord, SessionProjectionStore, SessionRuntimeCommandStore,
    SessionStoreError, SessionStoreResult, SessionTerminalEffectStore, TerminalEffectRecord,
    TerminalEffectStatus,
};

#[derive(Clone)]
pub struct SessionStoreSet {
    pub meta: Arc<dyn SessionMetaStore>,
    pub events: Arc<dyn SessionEventStore>,
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
    pub compactions: Arc<dyn SessionCompactionStore>,
    pub projections: Arc<dyn SessionProjectionStore>,
    pub lineage: Arc<dyn SessionLineageStore>,
}

impl SessionStoreSet {
    pub fn from_persistence(persistence: Arc<dyn SessionPersistence>) -> Self {
        let adapter = SessionPersistenceStoreAdapter { persistence };
        Self {
            meta: Arc::new(adapter.clone()),
            events: Arc::new(adapter.clone()),
            terminal_effects: Arc::new(adapter.clone()),
            runtime_commands: Arc::new(adapter.clone()),
            compactions: Arc::new(adapter.clone()),
            projections: Arc::new(adapter.clone()),
            lineage: Arc::new(adapter),
        }
    }
}

#[derive(Clone)]
struct SessionPersistenceStoreAdapter {
    persistence: Arc<dyn SessionPersistence>,
}

#[async_trait]
impl SessionMetaStore for SessionPersistenceStoreAdapter {
    async fn create_session(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
        self.persistence.create_session(meta).await
    }

    async fn get_session_meta(&self, session_id: &str) -> SessionStoreResult<Option<SessionMeta>> {
        self.persistence.get_session_meta(session_id).await
    }

    async fn list_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
        self.persistence.list_sessions().await
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
        self.persistence.save_session_meta(meta).await
    }

    async fn delete_session(&self, session_id: &str) -> SessionStoreResult<()> {
        self.persistence.delete_session(session_id).await
    }
}

#[async_trait]
impl SessionEventStore for SessionPersistenceStoreAdapter {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> SessionStoreResult<PersistedSessionEvent> {
        self.persistence.append_event(session_id, envelope).await
    }

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> SessionStoreResult<SessionEventBacklog> {
        self.persistence.read_backlog(session_id, after_seq).await
    }

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> SessionStoreResult<SessionEventPage> {
        self.persistence
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    async fn list_all_events(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
        self.persistence.list_all_events(session_id).await
    }
}

#[async_trait]
impl SessionTerminalEffectStore for SessionPersistenceStoreAdapter {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> SessionStoreResult<TerminalEffectRecord> {
        self.persistence.insert_terminal_effect(effect).await
    }

    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> SessionStoreResult<()> {
        self.persistence
            .mark_terminal_effect_running(effect_id)
            .await
    }

    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> SessionStoreResult<()> {
        self.persistence
            .mark_terminal_effect_succeeded(effect_id)
            .await
    }

    async fn mark_terminal_effect_failed(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> SessionStoreResult<()> {
        self.persistence
            .mark_terminal_effect_failed(effect_id, error)
            .await
    }

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> SessionStoreResult<()> {
        self.persistence
            .mark_terminal_effect_dead_letter(effect_id, error)
            .await
    }

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> SessionStoreResult<Vec<TerminalEffectRecord>> {
        self.persistence
            .list_terminal_effects_by_status(statuses, limit)
            .await
    }
}

#[async_trait]
impl SessionRuntimeCommandStore for SessionPersistenceStoreAdapter {
    async fn upsert_runtime_delivery_command(
        &self,
        delivery_runtime_session_id: &str,
        delivery: RuntimeDeliveryCommand,
        frame_transition: AgentFrameTransitionRecord,
    ) -> SessionStoreResult<RuntimeCommandRecord> {
        self.persistence
            .upsert_runtime_delivery_command(
                delivery_runtime_session_id,
                delivery,
                frame_transition,
            )
            .await
    }

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
        self.persistence
            .list_requested_runtime_commands(session_id)
            .await
    }

    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> SessionStoreResult<()> {
        self.persistence
            .mark_runtime_commands_applied(command_ids)
            .await
    }

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> SessionStoreResult<()> {
        self.persistence
            .mark_runtime_commands_failed(command_ids, error)
            .await
    }

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
        self.persistence
            .list_runtime_commands_by_status(statuses, limit)
            .await
    }
}

#[async_trait]
impl SessionCompactionStore for SessionPersistenceStoreAdapter {
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> SessionStoreResult<Option<SessionCompactionRecord>> {
        self.persistence
            .get_compaction(session_id, compaction_id)
            .await
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> SessionStoreResult<Vec<SessionCompactionRecord>> {
        self.persistence
            .list_compactions(session_id, projection_kind)
            .await
    }
}

#[async_trait]
impl SessionProjectionStore for SessionPersistenceStoreAdapter {
    async fn list_projection_segments(
        &self,
        session_id: &str,
        projection_kind: &str,
        projection_version: u64,
    ) -> SessionStoreResult<Vec<SessionProjectionSegmentRecord>> {
        self.persistence
            .list_projection_segments(session_id, projection_kind, projection_version)
            .await
    }

    async fn read_projection_head(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> SessionStoreResult<Option<SessionProjectionHeadRecord>> {
        self.persistence
            .read_projection_head(session_id, projection_kind)
            .await
    }

    async fn upsert_projection_head(
        &self,
        head: SessionProjectionHeadRecord,
    ) -> SessionStoreResult<()> {
        self.persistence.upsert_projection_head(head).await
    }

    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> SessionStoreResult<CompactionProjectionCommitResult> {
        self.persistence
            .commit_compaction_projection(session_id, commit)
            .await
    }
}

#[async_trait]
impl SessionLineageStore for SessionPersistenceStoreAdapter {
    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> SessionStoreResult<()> {
        self.persistence.upsert_session_lineage(record).await
    }

    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> SessionStoreResult<Option<SessionLineageRecord>> {
        self.persistence.get_session_lineage(child_session_id).await
    }

    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        self.persistence
            .list_session_children(parent_session_id, relation_kind, status)
            .await
    }

    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        self.persistence
            .list_session_ancestors(child_session_id)
            .await
    }

    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        self.persistence
            .list_session_descendants(root_session_id, relation_kind, status)
            .await
    }

    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> SessionStoreResult<()> {
        self.persistence
            .set_session_lineage_status(child_session_id, status, updated_at_ms)
            .await
    }
}
