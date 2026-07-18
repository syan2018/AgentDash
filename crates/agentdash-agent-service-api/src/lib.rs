//! Dependency-light contract between the Agent Runtime Host and a complete Agent.
//!
//! A complete Agent owns its history, fork, context/compaction, and native lifecycle.
//! This crate contains only finite commands, authoritative reads, capability evidence,
//! and reverse Host callbacks. It deliberately has no Product, Runtime repository,
//! transport, infrastructure, or vendor dependencies.

pub mod command;
pub mod context;
pub mod ids;
pub mod profile;
pub mod service;
pub mod snapshot;
pub mod surface;

pub use command::*;
pub use context::*;
pub use ids::*;
pub use profile::*;
pub use service::*;
pub use snapshot::*;
pub use surface::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Schema root covering every public Complete Agent contract family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceApiSchema {
    pub descriptor: AgentServiceDescriptor,
    pub create: CreateAgentCommand,
    pub resume: ResumeAgentCommand,
    pub fork: ForkAgentCommand,
    pub execute: AgentCommandEnvelope,
    pub receipt: AgentCommandReceipt,
    pub fork_receipt: ForkAgentReceipt,
    pub read: AgentReadQuery,
    pub snapshot: AgentSnapshot,
    pub changes: AgentChangesQuery,
    pub change_page: AgentChangePage,
    pub inspection: AgentEffectInspection,
    pub desired_surface: AgentSurfaceSnapshot,
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
            "read",
            "snapshot",
            "changes",
            "change_page",
            "inspection",
            "desired_surface",
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
