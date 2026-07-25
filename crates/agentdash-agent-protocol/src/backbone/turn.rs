use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AgentDashThreadItem;
use crate::codex_app_server_protocol as codex;

/// AgentDash canonical turn. A turn is only an ordered container around the
/// canonical AgentDash thread-item stream; it must not narrow native items back
/// to one connector's protocol vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub items: Vec<AgentDashThreadItem>,
    pub items_view: codex::TurnItemsView,
    pub status: codex::TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub duration_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "TurnError | null")]
    pub error: Option<codex::TurnError>,
}

impl From<codex::Turn> for Turn {
    fn from(value: codex::Turn) -> Self {
        Self {
            id: value.id,
            items: value.items.into_iter().map(Into::into).collect(),
            items_view: value.items_view,
            status: value.status,
            started_at: value.started_at.flatten(),
            completed_at: value.completed_at.flatten(),
            duration_ms: value.duration_ms.flatten(),
            error: value.error.flatten(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartedNotification {
    pub thread_id: String,
    pub turn: Turn,
}

impl From<codex::TurnStartedNotification> for TurnStartedNotification {
    fn from(value: codex::TurnStartedNotification) -> Self {
        Self {
            thread_id: value.thread_id,
            turn: value.turn.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompletedNotification {
    pub thread_id: String,
    pub turn: Turn,
}

impl From<codex::TurnCompletedNotification> for TurnCompletedNotification {
    fn from(value: codex::TurnCompletedNotification) -> Self {
        Self {
            thread_id: value.thread_id,
            turn: value.turn.into(),
        }
    }
}
