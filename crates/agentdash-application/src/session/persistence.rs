use std::io;

use agent_client_protocol::SessionNotification;
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
    pub notification: SessionNotification,
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

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()>;

    async fn delete_session(&self, session_id: &str) -> io::Result<()>;

    async fn append_event(
        &self,
        session_id: &str,
        notification: &SessionNotification,
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
