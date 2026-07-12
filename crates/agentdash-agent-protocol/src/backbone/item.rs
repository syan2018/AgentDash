use crate::AgentDashThreadItem;
use crate::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemStartedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    #[ts(type = "number")]
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemUpdatedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    #[ts(type = "number")]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemCompletedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    #[ts(type = "number")]
    pub completed_at_ms: i64,
}

impl ItemStartedNotification {
    pub fn new(
        item: impl Into<AgentDashThreadItem>,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            item: item.into(),
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            started_at_ms: now_ms(),
        }
    }

    pub fn from_codex(value: codex::ItemStartedNotification) -> Self {
        Self {
            item: value.item.into(),
            thread_id: value.thread_id,
            turn_id: value.turn_id,
            started_at_ms: value.started_at_ms,
        }
    }
}

impl ItemUpdatedNotification {
    pub fn new(
        item: impl Into<AgentDashThreadItem>,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            item: item.into(),
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            updated_at_ms: now_ms(),
        }
    }
}

impl ItemCompletedNotification {
    pub fn new(
        item: impl Into<AgentDashThreadItem>,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            item: item.into(),
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            completed_at_ms: now_ms(),
        }
    }

    pub fn from_codex(value: codex::ItemCompletedNotification) -> Self {
        Self {
            item: value.item.into(),
            thread_id: value.thread_id,
            turn_id: value.turn_id,
            completed_at_ms: value.completed_at_ms,
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
