use crate::codex_app_server_protocol as codex;
use crate::generated::codex_v2::server_notification::ThreadItem as ServerThreadItem;
use crate::{
    AgentDashCompactionStatus, AgentDashItemNotTerminal, AgentDashItemTerminal,
    AgentDashNativeThreadItem, AgentDashThreadItem,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemStartedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    #[ts(type = "number")]
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemUpdatedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    #[ts(type = "number")]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "backbone/")]
pub struct ItemCompletedNotification {
    pub item: AgentDashThreadItem,
    pub terminal: AgentDashItemTerminal,
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
        let item: AgentDashThreadItem = match value.item {
            ServerThreadItem::ContextCompaction { id } => {
                AgentDashNativeThreadItem::ContextCompaction {
                    id,
                    status: AgentDashCompactionStatus::InProgress,
                    error: None,
                    started_at_ms: u64::try_from(value.started_at_ms).ok(),
                    completed_at_ms: None,
                    context_revision: None,
                }
                .into()
            }
            item => item.into(),
        };
        Self {
            item,
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
    ) -> Result<Self, AgentDashItemNotTerminal> {
        let item = item.into();
        let terminal = item.terminal_evidence()?;
        Ok(Self {
            item,
            terminal,
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            completed_at_ms: now_ms(),
        })
    }

    pub fn from_codex(
        value: codex::ItemCompletedNotification,
    ) -> Result<Self, AgentDashItemNotTerminal> {
        let item: AgentDashThreadItem = match value.item {
            ServerThreadItem::ContextCompaction { id } => {
                AgentDashNativeThreadItem::ContextCompaction {
                    id,
                    status: AgentDashCompactionStatus::Succeeded,
                    error: None,
                    started_at_ms: None,
                    completed_at_ms: u64::try_from(value.completed_at_ms).ok(),
                    context_revision: None,
                }
                .into()
            }
            item => item.into(),
        };
        let terminal = item.terminal_evidence()?;
        Ok(Self {
            item,
            terminal,
            thread_id: value.thread_id,
            turn_id: value.turn_id,
            completed_at_ms: value.completed_at_ms,
        })
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
