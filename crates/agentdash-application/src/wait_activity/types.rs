use std::collections::BTreeSet;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub(crate) const WAIT_TOOL_TIMEOUT_MS_DEFAULT: u64 = 10_000;
pub(crate) const WAIT_TOOL_TIMEOUT_MS_MAX: u64 = 120_000;
pub(crate) const WAIT_TOOL_MAX_ITEMS_DEFAULT: usize = 10;
pub(crate) const WAIT_TOOL_MAX_ITEMS_LIMIT: usize = 50;
pub(crate) const WAIT_POLL_INTERVAL_MS: u64 = 250;
pub(crate) const WAIT_PREVIEW_CHARS: usize = 280;

#[derive(Debug, Clone)]
pub struct WaitToolContext {
    pub delivery_runtime_session_id: Option<String>,
    pub turn_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedWaitScope {
    pub(crate) delivery_runtime_session_id: Option<String>,
    pub(crate) run_id: Option<Uuid>,
    pub(crate) agent_id: Option<Uuid>,
    pub(crate) frame_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct WaitActivityRequest {
    /// Specific activity/source refs to observe, for example terminal_id, gate_id, or mailbox message id.
    #[serde(default)]
    pub activity_refs: Vec<String>,
    /// Activity kinds to observe: exec, human, subagent, companion, workflow, mailbox.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Bounded wait window. Timeout returns current status and does not cancel background activity.
    pub timeout_ms: Option<u64>,
    /// Maximum items returned in the bounded summary.
    pub max_items: Option<usize>,
    /// Opaque cursor returned by a previous wait call; only newer activity summaries are returned.
    pub after_cursor: Option<String>,
}

impl WaitActivityRequest {
    pub(crate) fn normalized_activity_refs(&self) -> Vec<String> {
        self.activity_refs
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    pub(crate) fn normalized_kinds(&self) -> BTreeSet<String> {
        self.kinds
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect()
    }

    pub(crate) fn max_items(&self) -> usize {
        self.max_items
            .unwrap_or(WAIT_TOOL_MAX_ITEMS_DEFAULT)
            .clamp(1, WAIT_TOOL_MAX_ITEMS_LIMIT)
    }

    pub(crate) fn after_cursor_ms(&self) -> Option<i64> {
        self.after_cursor
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<i64>().ok())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WaitActivityResult {
    pub status: String,
    pub timed_out: bool,
    pub items: Vec<WaitActivityItem>,
    pub cursor: String,
}

impl WaitActivityResult {
    pub(crate) fn ready(items: Vec<WaitActivityItem>) -> Self {
        Self {
            status: "ready".to_string(),
            timed_out: false,
            items,
            cursor: Utc::now().timestamp_millis().to_string(),
        }
    }

    pub(crate) fn timed_out(items: Vec<WaitActivityItem>) -> Self {
        Self {
            status: "timed_out".to_string(),
            timed_out: true,
            items,
            cursor: Utc::now().timestamp_millis().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WaitActivityItem {
    pub activity_ref: String,
    pub kind: String,
    pub status: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub preview: Option<String>,
    pub result_refs: Value,
    pub cursor: Option<String>,
    pub next: Option<Value>,
    pub updated_at_ms: i64,
}

impl WaitActivityItem {
    pub(crate) fn is_ready(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed"
                | "failed"
                | "cancelled"
                | "lost"
                | "resolved"
                | "queued"
                | "ready_to_consume"
                | "dispatched"
                | "steered"
                | "blocked"
        )
    }
}
