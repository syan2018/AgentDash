use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AgentBindingGeneration, AgentCommandId, AgentContextPackageId, AgentEffectIdentity,
    AgentIdempotencyKey, AgentInteractionId, AgentItemId, AgentPayloadDigest,
    AgentSnapshotRevision, AgentSourceCoordinate, AgentSourceCursor, AgentTurnId,
    AppliedInitialContextEvidence, InitialAgentContextPackage,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCommandMeta {
    pub command_id: AgentCommandId,
    pub effect_id: AgentEffectIdentity,
    pub idempotency_key: AgentIdempotencyKey,
    pub binding_generation: AgentBindingGeneration,
    pub expected_snapshot_revision: Option<AgentSnapshotRevision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInputContent {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        source: String,
        digest: AgentPayloadDigest,
    },
    Resource {
        uri: String,
        media_type: Option<String>,
        digest: Option<AgentPayloadDigest>,
    },
    Structured {
        schema: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentInput {
    pub content: Vec<AgentInputContent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateAgentCommand {
    pub meta: AgentCommandMeta,
    pub requested_source: Option<AgentSourceCoordinate>,
    pub initial_context: Option<InitialAgentContextPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ResumeAgentCommand {
    pub meta: AgentCommandMeta,
    pub source: AgentSourceCoordinate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentForkPoint {
    Head,
    CompletedTurn {
        turn_id: AgentTurnId,
    },
    Item {
        item_id: AgentItemId,
    },
    SourceCursor {
        cursor: AgentSourceCursor,
        digest: AgentPayloadDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ForkAgentCommand {
    pub meta: AgentCommandMeta,
    pub source: AgentSourceCoordinate,
    pub requested_child_source: Option<AgentSourceCoordinate>,
    pub cutoff: AgentForkPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInteractionResponse {
    Approved,
    Denied { reason: Option<String> },
    UserInput { input: AgentInput },
    DynamicToolResult { result: Value },
    McpElicitation { response: Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentCommand {
    SubmitInput {
        input: AgentInput,
    },
    Steer {
        expected_turn_id: AgentTurnId,
        input: AgentInput,
    },
    Interrupt {
        expected_turn_id: AgentTurnId,
    },
    RequestCompaction,
    ResolveInteraction {
        interaction_id: AgentInteractionId,
        response: AgentInteractionResponse,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCommandEnvelope {
    pub meta: AgentCommandMeta,
    pub source: AgentSourceCoordinate,
    pub command: AgentCommand,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentTerminalOutcome {
    Succeeded,
    Failed,
    Interrupted,
    Closed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentReceiptState {
    Accepted,
    Rejected {
        code: String,
        message: String,
    },
    AlreadyApplied {
        terminal: Option<AgentTerminalOutcome>,
    },
    Terminal {
        outcome: AgentTerminalOutcome,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCommandReceipt {
    pub command_id: AgentCommandId,
    pub effect_id: AgentEffectIdentity,
    pub source: AgentSourceCoordinate,
    pub state: AgentReceiptState,
    pub snapshot_revision: Option<AgentSnapshotRevision>,
    pub initial_context: Option<AppliedInitialContextEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ForkAgentReceipt {
    pub command_id: AgentCommandId,
    pub effect_id: AgentEffectIdentity,
    pub parent_source: AgentSourceCoordinate,
    pub child_source: Option<AgentSourceCoordinate>,
    pub cutoff: AgentForkPoint,
    pub child_history_digest: Option<AgentPayloadDigest>,
    pub state: AgentReceiptState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEffectInspectionState {
    NotApplied,
    Accepted {
        source: AgentSourceCoordinate,
    },
    Applied {
        source: AgentSourceCoordinate,
        terminal: Option<AgentTerminalOutcome>,
        initial_context: Option<AppliedInitialContextEvidence>,
        child_source: Option<AgentSourceCoordinate>,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentEffectInspection {
    pub effect_id: AgentEffectIdentity,
    pub command_id: Option<AgentCommandId>,
    pub state: AgentEffectInspectionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCreateEvidence {
    pub source: AgentSourceCoordinate,
    pub initial_context_package_id: Option<AgentContextPackageId>,
    pub initial_context_digest: Option<AgentPayloadDigest>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_context_is_not_a_submit_input() {
        let package_id = AgentContextPackageId::new("package").expect("package");
        let create_evidence = AgentCreateEvidence {
            source: AgentSourceCoordinate::new("source").expect("source"),
            initial_context_package_id: Some(package_id),
            initial_context_digest: Some(
                AgentPayloadDigest::new("sha256:context").expect("digest"),
            ),
        };
        let submit = AgentCommand::SubmitInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: "task".to_owned(),
                }],
            },
        };

        assert!(create_evidence.initial_context_package_id.is_some());
        assert!(matches!(submit, AgentCommand::SubmitInput { .. }));
    }
}
