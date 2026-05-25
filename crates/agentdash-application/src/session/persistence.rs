use std::{io, sync::Arc};

use agentdash_agent_protocol::BackboneEnvelope;
use async_trait::async_trait;
use uuid::Uuid;

pub use agentdash_spi::session_persistence::{
    NewTerminalEffectRecord, PendingCapabilityStateTransition, PersistedSessionEvent,
    RuntimeCommandRecord, RuntimeCommandStatus, SessionEventBacklog, SessionEventPage,
    SessionEventStore, SessionMeta, SessionMetaStore, SessionPersistence,
    SessionRuntimeCommandStore, SessionTerminalEffectStore, TerminalEffectRecord,
    TerminalEffectStatus,
};

#[derive(Clone)]
pub struct SessionStoreSet {
    pub meta: Arc<dyn SessionMetaStore>,
    pub events: Arc<dyn SessionEventStore>,
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
}

impl SessionStoreSet {
    pub fn from_persistence(persistence: Arc<dyn SessionPersistence>) -> Self {
        let adapter = SessionPersistenceStoreAdapter { persistence };
        Self {
            meta: Arc::new(adapter.clone()),
            events: Arc::new(adapter.clone()),
            terminal_effects: Arc::new(adapter.clone()),
            runtime_commands: Arc::new(adapter),
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
