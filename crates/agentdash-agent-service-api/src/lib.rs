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
    pub changes: AgentChangesQuery,
    pub change_page: AgentChangePage,
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
    use std::{fs, path::Path};

    use ts_rs::TS;

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
            "changes",
            "change_page",
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

    #[test]
    fn complete_agent_typescript_root_exports_public_contracts_without_bigint() {
        let temp = tempfile::tempdir().expect("create TypeScript export directory");
        AgentServiceApiSchema::export_all_to(temp.path())
            .expect("export Complete Agent service types");
        let typescript = read_typescript(temp.path());

        for contract in [
            "AgentServiceApiSchema",
            "AgentAppliedEffectOutcome",
            "AgentHostCallbackMeta",
            "AgentChange",
            "AgentHostCallbackBinding",
            "AgentCreateEvidence",
            "AgentSurfaceContributionKind",
        ] {
            assert!(typescript.contains(contract), "missing {contract}");
        }
        for outcome in [
            "\"create\"",
            "\"resume\"",
            "\"fork\"",
            "\"command\"",
            "\"surface_apply\"",
            "\"surface_revoke\"",
        ] {
            assert!(typescript.contains(outcome), "missing outcome {outcome}");
        }
        assert!(!typescript.contains("bigint"));
        assert!(typescript.contains("deadline_at_ms: number"));
        assert!(typescript.contains("occurred_at_ms: number"));
    }

    fn read_typescript(directory: &Path) -> String {
        let mut output = String::new();
        for entry in fs::read_dir(directory).expect("read TypeScript export directory") {
            let path = entry.expect("read TypeScript export entry").path();
            if path.is_dir() {
                output.push_str(&read_typescript(&path));
            } else if path.extension().is_some_and(|extension| extension == "ts") {
                output.push_str(&fs::read_to_string(path).expect("read TypeScript export"));
            }
        }
        output
    }
}
