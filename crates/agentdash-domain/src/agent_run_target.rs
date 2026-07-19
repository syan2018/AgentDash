use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable Product identity for one lifecycle Agent inside an AgentRun.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AgentRunTarget {
    pub run_id: Uuid,
    pub agent_id: Uuid,
}
