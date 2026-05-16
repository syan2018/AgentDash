use std::{io, sync::Arc};

use agentdash_agent_protocol::BackboneEnvelope;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::runtime_commands::{RuntimeCommandRecord, RuntimeCommandStatus};
use super::terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus,
};
use super::types::{PendingCapabilityStateTransition, SessionMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSessionEvent {
    pub session_id: String,
    pub event_seq: u64,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub session_update_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub notification: BackboneEnvelope,
}

#[derive(Debug, Clone)]
pub struct SessionEventBacklog {
    pub snapshot_seq: u64,
    pub events: Vec<PersistedSessionEvent>,
}

#[derive(Debug, Clone)]
pub struct SessionEventPage {
    pub snapshot_seq: u64,
    pub events: Vec<PersistedSessionEvent>,
    pub has_more: bool,
    pub next_after_seq: u64,
}

#[async_trait]
pub trait SessionMetaStore: Send + Sync {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>>;

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>>;

    /// 整行回写 SessionMeta。
    ///
    /// **注意**：投影字段（`last_event_seq`/`last_execution_status`/`last_turn_id`/
    /// `last_terminal_message`）受 DB 层 `CASE WHEN` 保护，不会被旧快照回滚。
    /// 但非投影字段（`executor_session_id` / `companion_context` 等）会被直接覆盖。
    ///
    /// 优先使用 `SessionRuntimeInner::update_session_meta()` 做 get-modify-save 原子操作。
    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn delete_session(&self, session_id: &str) -> io::Result<()>;
}

#[async_trait]
pub trait SessionEventStore: Send + Sync {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent>;

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog>;

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;

    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>>;
}

#[async_trait]
pub trait SessionTerminalEffectStore: Send + Sync {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord>;

    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> io::Result<()>;

    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> io::Result<()>;

    async fn mark_terminal_effect_failed(&self, effect_id: Uuid, error: String) -> io::Result<()>;

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> io::Result<()>;

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>>;
}

#[async_trait]
pub trait SessionRuntimeCommandStore: Send + Sync {
    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord>;

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;

    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> io::Result<()>;

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> io::Result<()>;

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
}

#[async_trait]
pub trait SessionPersistence: Send + Sync {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>>;

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>>;

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn delete_session(&self, session_id: &str) -> io::Result<()>;

    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent>;

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog>;

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;

    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>>;

    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord>;

    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> io::Result<()>;

    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> io::Result<()>;

    async fn mark_terminal_effect_failed(&self, effect_id: Uuid, error: String) -> io::Result<()>;

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> io::Result<()>;

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>>;

    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord>;

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;

    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> io::Result<()>;

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> io::Result<()>;

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
}

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
