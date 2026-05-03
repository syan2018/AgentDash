use std::io;

use agentdash_protocol::BackboneEnvelope;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::types::SessionMeta;

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
pub trait SessionPersistence: Send + Sync {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>>;

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>>;

    /// 整行回写 SessionMeta。
    ///
    /// **注意**：投影字段（`last_event_seq`/`last_execution_status`/`last_turn_id`/
    /// `last_terminal_message`）受 DB 层 `CASE WHEN` 保护，不会被旧快照回滚。
    /// 但非投影字段（`executor_session_id` / `companion_context` 等）会被直接覆盖。
    ///
    /// 优先使用 `SessionHub::update_session_meta()` 做 get-modify-save 原子操作。
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
}
