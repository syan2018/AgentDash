use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{AgentItemId, AgentServiceError, AgentServiceU64, AgentSourceCoordinate, AgentTurnId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentLiveFinishReason {
    Stop,
    ToolCalls,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentLiveEventPayload {
    ProviderRoundStarted {
        round: u32,
    },
    TextDelta {
        round: u32,
        delta: String,
    },
    ReasoningDelta {
        round: u32,
        delta: String,
    },
    ToolCallRequested {
        round: u32,
        call_id: String,
        name: String,
        arguments: Value,
    },
    ToolCallCompleted {
        round: u32,
        call_id: String,
        content: String,
        is_error: bool,
    },
    ProviderRoundCompleted {
        round: u32,
        finish_reason: AgentLiveFinishReason,
    },
}

/// Process-local, source-scoped observation of an in-flight Agent execution.
///
/// This event is presentation data, not Agent history. `sequence` is only ordered within the
/// current Complete Agent service process and may reset after restart. Consumers recover any gap
/// by reading the authoritative Agent snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentLiveEvent {
    pub source: AgentSourceCoordinate,
    pub turn_id: AgentTurnId,
    pub item_id: AgentItemId,
    pub sequence: AgentServiceU64,
    pub payload: AgentLiveEventPayload,
}

#[async_trait]
pub trait AgentLiveEventStream: Send {
    async fn next(&mut self) -> Result<Option<AgentLiveEvent>, AgentServiceError>;
}
