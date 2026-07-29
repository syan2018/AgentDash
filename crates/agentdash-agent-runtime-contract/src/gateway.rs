use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AgentContextCoordinate, AgentContextRecipe, AgentEffectIdentity, AgentInteractionResponse,
    AgentRuntimeOperationStatus, AgentRuntimeView, InitialAgentContextPackage, RuntimeThreadId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContextRequirement {
    pub at_least: AgentContextCoordinate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContextProjection {
    pub thread_id: RuntimeThreadId,
    pub recipe: AgentContextRecipe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeOperationReceipt {
    pub operation_id: AgentEffectIdentity,
    pub thread_id: RuntimeThreadId,
    pub status: AgentRuntimeOperationStatus,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContractSchema {
    pub complete_agent: crate::AgentServiceApiSchema,
    pub initial_context: InitialAgentContextPackage,
    pub interaction_response: AgentInteractionResponse,
    pub operation_receipt: AgentRuntimeOperationReceipt,
    pub view: AgentRuntimeView,
    pub context_requirement: AgentRuntimeContextRequirement,
    pub context_projection: AgentRuntimeContextProjection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_schema_contains_complete_agent_and_product_wrapper_families() {
        let schema = schemars::schema_for!(AgentRuntimeContractSchema);
        let schema = serde_json::to_string(&schema).expect("serialize Runtime schema");
        for family in [
            "AgentServiceApiSchema",
            "InitialAgentContextPackage",
            "AgentInteractionResponse",
            "AgentRuntimeOperationReceipt",
            "AgentRuntimeView",
            "AgentRuntimeContextRequirement",
            "AgentRuntimeContextProjection",
        ] {
            assert!(schema.contains(family), "missing {family}");
        }
    }
}
