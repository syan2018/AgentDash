use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{
    AgentInteractionId, AgentItemId, AgentPayloadDigest, AgentSnapshotRevision,
    AgentSourceCoordinate, AgentSourceCursor, AgentSourceRevision, AgentTurnId, SemanticFidelity,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentSnapshotAuthority {
    AgentAuthoritative,
    AgentObserved,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSnapshotSource {
    pub authority: AgentSnapshotAuthority,
    pub source_revision: Option<AgentSourceRevision>,
    pub fidelity: SemanticFidelity,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentThreadNameSnapshot {
    pub thread_name: Option<String>,
    pub source_info: AgentSnapshotSource,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleStatus {
    Creating,
    Active,
    Suspended,
    Closed,
    Lost,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentEntityStatus {
    Accepted,
    Running,
    Completed,
    Failed,
    Interrupted,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentItemContent {
    UserInput {
        input: crate::AgentInput,
    },
    AgentOutput {
        content: Vec<crate::AgentInputContent>,
    },
    ToolCall {
        name: crate::AgentToolName,
        arguments: Value,
    },
    ToolResult {
        name: crate::AgentToolName,
        result: Value,
    },
    ContextCompaction,
    Error {
        code: String,
        message: String,
    },
    Extension {
        namespace: String,
        schema: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentItemSnapshot {
    pub id: AgentItemId,
    pub status: AgentEntityStatus,
    pub content: AgentItemContent,
    pub content_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentTurnSnapshot {
    pub id: AgentTurnId,
    pub status: AgentEntityStatus,
    pub items: Vec<AgentItemSnapshot>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentInteractionKind {
    Approval,
    UserInput,
    McpElicitation,
    DynamicTool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentInteractionSnapshot {
    pub id: AgentInteractionId,
    pub turn_id: AgentTurnId,
    pub item_id: Option<AgentItemId>,
    pub kind: AgentInteractionKind,
    pub prompt: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSnapshot {
    pub source: AgentSourceCoordinate,
    pub revision: AgentSnapshotRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub active_turn_id: Option<AgentTurnId>,
    pub turns: Vec<AgentTurnSnapshot>,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub source_info: AgentSnapshotSource,
    pub applied_surface: Option<crate::AppliedAgentSurface>,
    pub initial_context: Option<crate::AppliedInitialContextEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentReadQuery {
    pub source: AgentSourceCoordinate,
    pub at_revision: Option<AgentSnapshotRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChangesQuery {
    pub source: AgentSourceCoordinate,
    pub after: Option<AgentSourceCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentChangePayload {
    ThreadNameChanged {
        thread_name: Option<String>,
        source_info: AgentSnapshotSource,
    },
    LifecycleChanged {
        status: AgentLifecycleStatus,
    },
    TurnChanged {
        turn: AgentTurnSnapshot,
    },
    ActiveTurnChanged {
        active_turn_id: Option<AgentTurnId>,
    },
    ItemChanged {
        turn_id: AgentTurnId,
        item: AgentItemSnapshot,
    },
    InteractionChanged {
        interaction: AgentInteractionSnapshot,
    },
    SurfaceApplied {
        applied: crate::AppliedAgentSurface,
    },
    SnapshotInvalidated {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChange {
    pub cursor: AgentSourceCursor,
    pub source_revision: Option<AgentSourceRevision>,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
    pub occurred_at_ms: u64,
    pub payload: AgentChangePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChangePage {
    pub source: AgentSourceCoordinate,
    pub changes: Vec<AgentChange>,
    pub next: Option<AgentSourceCursor>,
    pub gap: bool,
}
