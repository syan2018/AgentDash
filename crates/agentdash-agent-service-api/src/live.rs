use agentdash_agent_protocol::CanonicalConversationRecord;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AgentServiceError, AgentServiceU64, AgentSourceCoordinate};

/// Process-local, source-scoped observation of an in-flight Agent execution.
///
/// This event is presentation data, not Agent history. `sequence` is only ordered within the
/// current Complete Agent service process and may reset after restart. Consumers recover any gap
/// by reading the authoritative Agent snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentLiveEvent {
    pub source: AgentSourceCoordinate,
    pub sequence: AgentServiceU64,
    pub record: CanonicalConversationRecord,
}

#[async_trait]
pub trait AgentLiveEventStream: Send {
    async fn next(&mut self) -> Result<Option<AgentLiveEvent>, AgentServiceError>;
}
