use std::{io, sync::Arc};

use agentdash_agent_protocol::BackboneEnvelope;
use async_trait::async_trait;
use uuid::Uuid;

pub use agentdash_spi::session_persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, NewTerminalEffectRecord,
    PendingCapabilityStateTransition, PersistedSessionEvent, RuntimeCommandRecord,
    RuntimeCommandStatus, SessionCompactionRecord, SessionCompactionStatus, SessionCompactionStore,
    SessionEventBacklog, SessionEventPage, SessionEventStore, SessionMeta, SessionMetaStore,
    SessionPersistence, SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
    SessionProjectionStore, SessionRuntimeCommandStore, SessionTerminalEffectStore,
    TerminalEffectRecord, TerminalEffectStatus,
};

#[derive(Clone)]
pub struct SessionStoreSet {
    pub meta: Arc<dyn SessionMetaStore>,
    pub events: Arc<dyn SessionEventStore>,
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
    pub compactions: Arc<dyn SessionCompactionStore>,
    pub projections: Arc<dyn SessionProjectionStore>,
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
            projections: Arc::new(adapter),
        }
    }
}

#[derive(Clone)]
struct SessionPersistenceStoreAdapter {
    persistence: Arc<dyn SessionPersistence>,
}

#[async_trait]
impl SessionMetaStore for SessionPersistenceStoreAdapter {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()> {
        self.persistence.create_session(meta).await
    }

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>> {
        self.persistence.get_session_meta(session_id).await
    }

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>> {
        self.persistence.list_sessions().await
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        self.persistence.save_session_meta(meta).await
    }

    async fn delete_session(&self, session_id: &str) -> io::Result<()> {
        self.persistence.delete_session(session_id).await
    }
}

#[async_trait]
impl SessionEventStore for SessionPersistenceStoreAdapter {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        self.persistence.append_event(session_id, envelope).await
    }

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog> {
        self.persistence.read_backlog(session_id, after_seq).await
    }

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        self.persistence
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>> {
        self.persistence.list_all_events(session_id).await
    }
}

#[async_trait]
impl SessionTerminalEffectStore for SessionPersistenceStoreAdapter {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord> {
        self.persistence.insert_terminal_effect(effect).await
    }

    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> io::Result<()> {
        self.persistence
            .mark_terminal_effect_running(effect_id)
            .await
    }

    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> io::Result<()> {
        self.persistence
            .mark_terminal_effect_succeeded(effect_id)
            .await
    }

    async fn mark_terminal_effect_failed(&self, effect_id: Uuid, error: String) -> io::Result<()> {
        self.persistence
            .mark_terminal_effect_failed(effect_id, error)
            .await
    }

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> io::Result<()> {
        self.persistence
            .mark_terminal_effect_dead_letter(effect_id, error)
            .await
    }

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>> {
        self.persistence
            .list_terminal_effects_by_status(statuses, limit)
            .await
    }
}

#[async_trait]
impl SessionRuntimeCommandStore for SessionPersistenceStoreAdapter {
    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord> {
        self.persistence
            .upsert_runtime_command_request(session_id, transition)
            .await
    }

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        self.persistence
            .list_requested_runtime_commands(session_id)
            .await
    }

    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> io::Result<()> {
        self.persistence
            .mark_runtime_commands_applied(command_ids)
            .await
    }

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> io::Result<()> {
        self.persistence
            .mark_runtime_commands_failed(command_ids, error)
            .await
    }

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
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
    ) -> io::Result<Option<SessionCompactionRecord>> {
        self.persistence
            .get_compaction(session_id, compaction_id)
            .await
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Vec<SessionCompactionRecord>> {
        self.persistence
            .list_compactions(session_id, branch_id, projection_kind)
            .await
    }
}

#[async_trait]
impl SessionProjectionStore for SessionPersistenceStoreAdapter {
    async fn list_projection_segments(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
        projection_version: u64,
    ) -> io::Result<Vec<SessionProjectionSegmentRecord>> {
        self.persistence
            .list_projection_segments(session_id, branch_id, projection_kind, projection_version)
            .await
    }

    async fn read_projection_head(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Option<SessionProjectionHeadRecord>> {
        self.persistence
            .read_projection_head(session_id, branch_id, projection_kind)
            .await
    }

    async fn upsert_projection_head(&self, head: SessionProjectionHeadRecord) -> io::Result<()> {
        self.persistence.upsert_projection_head(head).await
    }

    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> io::Result<CompactionProjectionCommitResult> {
        self.persistence
            .commit_compaction_projection(session_id, commit)
            .await
    }
}
