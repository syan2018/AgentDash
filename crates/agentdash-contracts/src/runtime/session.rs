use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_agent_types::MessageRef;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
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
pub enum SessionNdjsonEnvelope {
    Connected {
        #[ts(type = "number")]
        last_event_id: u64,
        #[ts(type = "number")]
        ephemeral_epoch: u64,
    },
    Event {
        #[serde(flatten)]
        event: Box<SessionEventResponse>,
    },
    EphemeralEvent {
        #[serde(flatten)]
        event: Box<SessionEventResponse>,
    },
    Heartbeat {
        #[ts(type = "number")]
        timestamp: i64,
    },
}

impl SessionNdjsonEnvelope {
    pub fn connected(last_event_id: u64, ephemeral_epoch: u64) -> Self {
        Self::Connected {
            last_event_id,
            ephemeral_epoch,
        }
    }

    pub fn event(event: SessionEventResponse) -> Self {
        Self::Event {
            event: Box::new(event),
        }
    }

    pub fn ephemeral_event(event: SessionEventResponse) -> Self {
        Self::EphemeralEvent {
            event: Box::new(event),
        }
    }

    pub fn heartbeat_now() -> Self {
        Self::Heartbeat {
            timestamp: Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionMessageRefDto {
    pub turn_id: String,
    pub entry_index: u32,
}

impl From<MessageRef> for SessionMessageRefDto {
    fn from(value: MessageRef) -> Self {
        Self {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        }
    }
}
