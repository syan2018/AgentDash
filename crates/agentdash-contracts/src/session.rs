use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_spi::session_persistence::PersistedSessionEvent;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct SessionEventResponse {
    pub session_id: String,
    #[ts(type = "number")]
    pub event_seq: u64,
    #[ts(type = "number")]
    pub occurred_at_ms: i64,
    #[ts(type = "number")]
    pub committed_at_ms: i64,
    pub session_update_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_call_id: Option<String>,
    #[ts(type = "BackboneEnvelope")]
    pub notification: BackboneEnvelope,
}

impl From<PersistedSessionEvent> for SessionEventResponse {
    fn from(event: PersistedSessionEvent) -> Self {
        Self {
            session_id: event.session_id,
            event_seq: event.event_seq,
            occurred_at_ms: event.occurred_at_ms,
            committed_at_ms: event.committed_at_ms,
            session_update_type: event.session_update_type,
            turn_id: event.turn_id,
            entry_index: event.entry_index,
            tool_call_id: event.tool_call_id,
            notification: event.notification,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct SessionEventsPageResponse {
    #[ts(type = "number")]
    pub snapshot_seq: u64,
    pub events: Vec<SessionEventResponse>,
    pub has_more: bool,
    #[ts(type = "number")]
    pub next_after_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum SessionNdjsonEnvelope {
    Connected {
        #[ts(type = "number")]
        last_event_id: u64,
    },
    Event {
        #[serde(flatten)]
        event: SessionEventResponse,
    },
    Heartbeat {
        #[ts(type = "number")]
        timestamp: i64,
    },
}

impl SessionNdjsonEnvelope {
    pub fn connected(last_event_id: u64) -> Self {
        Self::Connected { last_event_id }
    }

    pub fn event(event: PersistedSessionEvent) -> Self {
        Self::Event {
            event: event.into(),
        }
    }

    pub fn heartbeat_now() -> Self {
        Self::Heartbeat {
            timestamp: Utc::now().timestamp_millis(),
        }
    }
}
