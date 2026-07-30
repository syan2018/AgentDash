use agentdash_agent_protocol::CanonicalConversationRecord;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{AgentObservationState, AgentSourceCoordinate, RuntimeU64};

/// Process-local, source-scoped observation of an in-flight Agent execution.
///
/// This event is presentation data, not Agent history. `sequence` is only ordered within the
/// current Complete Agent service process and may reset after restart. Consumers recover any gap
/// by reading the authoritative Agent snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentLiveBatch {
    pub source: AgentSourceCoordinate,
    pub sequence: RuntimeU64,
    pub state: Option<AgentObservationState>,
    pub presentations: Vec<CanonicalConversationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentLiveStreamError {
    #[error("Complete Agent live stream lagged by {skipped}")]
    Lagged { skipped: u64 },
    #[error("Complete Agent live stream protocol error: {message}")]
    Protocol { message: String },
    #[error("Complete Agent live stream unavailable: {message}")]
    Unavailable { message: String },
}

#[async_trait]
pub trait AgentLiveBatchStream: Send {
    async fn next(&mut self) -> Result<Option<AgentLiveBatch>, AgentLiveStreamError>;
}
