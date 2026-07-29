//! Complete Agent callable boundary inside the managed Agent Runtime contract.
//!
//! Concrete Agents own source history, context, compaction, fork and public effects. These
//! modules define that source-facing seam while reusing the same contract crate as Product-facing
//! Runtime wrappers.

pub mod command;
pub mod context;
pub mod ids;
pub mod live;
pub mod presentation;
pub mod profile;
pub mod service;
pub mod snapshot;
pub mod surface;

pub use command::*;
pub use context::*;
pub use ids::*;
pub use live::*;
pub use presentation::*;
pub use profile::*;
pub use service::*;
pub use snapshot::*;
pub use surface::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Schema root covering every public Complete Agent contract family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceApiSchema {
    pub descriptor: AgentServiceDescriptor,
    pub create: CreateAgentCommand,
    pub resume: ResumeAgentCommand,
    pub fork: ForkAgentCommand,
    pub execute: AgentCommandEnvelope,
    pub receipt: AgentCommandReceipt,
    pub fork_receipt: ForkAgentReceipt,
    pub create_evidence: AgentCreateEvidence,
    pub read: AgentReadQuery,
    pub snapshot: AgentSnapshot,
    pub context_query: AgentContextQuery,
    pub context_snapshot: AgentContextSnapshot,
    pub execution_snapshot: AgentExecutionSnapshot,
    pub observe: AgentObservationQuery,
    pub observation: AgentSourceState,
    pub changes: AgentChangesQuery,
    pub change_page: AgentChangePage,
    pub live_event: AgentLiveEvent,
    pub inspection: AgentEffectInspection,
    pub applied_effect_outcome: AgentAppliedEffectOutcome,
    pub desired_surface: AgentSurfaceSnapshot,
    pub surface_contribution_kind: AgentSurfaceContributionKind,
    pub offer: AgentRuntimeOffer,
    pub bound_surface: BoundAgentSurface,
    pub applied_surface: AppliedAgentSurface,
    pub apply_surface: ApplyBoundAgentSurface,
    pub revoke_surface: RevokeBoundAgentSurface,
    pub tool_invocation: AgentToolInvocation,
    pub tool_result: AgentToolResult,
    pub hook_invocation: AgentHookInvocation,
    pub hook_decision: AgentHookDecision,
    pub error: AgentServiceError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_covers_complete_agent_boundary() {
        let schema = schemars::schema_for!(AgentServiceApiSchema);
        let value = serde_json::to_value(schema).expect("serialize service API schema");
        let properties = value
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("schema root properties");
        for family in [
            "descriptor",
            "create",
            "resume",
            "fork",
            "execute",
            "receipt",
            "fork_receipt",
            "create_evidence",
            "read",
            "snapshot",
            "context_query",
            "context_snapshot",
            "observe",
            "observation",
            "changes",
            "change_page",
            "live_event",
            "inspection",
            "applied_effect_outcome",
            "desired_surface",
            "surface_contribution_kind",
            "offer",
            "bound_surface",
            "applied_surface",
            "apply_surface",
            "revoke_surface",
            "tool_invocation",
            "tool_result",
            "hook_invocation",
            "hook_decision",
            "error",
        ] {
            assert!(properties.contains_key(family), "missing {family}");
        }
    }
}
