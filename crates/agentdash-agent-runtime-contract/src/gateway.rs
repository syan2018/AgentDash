use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    AgentRuntimeContentBlock, AgentRuntimeOperationStatus, AgentRuntimeView,
    RuntimeContextContributionId, RuntimeContextPackageId, RuntimeContextSourceRef,
    RuntimeContextSourceRevision, RuntimeOperationId, RuntimePayloadDigest, RuntimeThreadId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeInitialContextMode {
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeContextAuthority {
    AgentHistory,
    AgentSnapshot,
    Workflow,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContextProvenance {
    pub authority: AgentRuntimeContextAuthority,
    pub source: RuntimeContextSourceRef,
    pub revision: RuntimeContextSourceRevision,
    pub digest: RuntimePayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeInitialContextContributionContent {
    CompactSummary {
        summary: String,
        provenance: AgentRuntimeContextProvenance,
    },
    WorkflowContext {
        schema: String,
        value: serde_json::Value,
        provenance: AgentRuntimeContextProvenance,
    },
    ConstraintSet {
        schema: String,
        value: serde_json::Value,
        provenance: AgentRuntimeContextProvenance,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeInitialContextContribution {
    pub contribution_id: RuntimeContextContributionId,
    pub digest: RuntimePayloadDigest,
    pub content: AgentRuntimeInitialContextContributionContent,
}

impl AgentRuntimeInitialContextContribution {
    pub fn calculated_digest(&self) -> RuntimePayloadDigest {
        let canonical = serde_json::to_vec(&(&self.contribution_id, &self.content))
            .expect("Runtime initial context contribution is serializable");
        RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
            .expect("SHA-256 digest is non-empty")
    }

    pub fn validate(&self) -> bool {
        self.digest == self.calculated_digest()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeInitialContextPackage {
    pub package_id: RuntimeContextPackageId,
    pub schema_version: u32,
    pub mode: AgentRuntimeInitialContextMode,
    pub contributions: Vec<AgentRuntimeInitialContextContribution>,
    pub digest: RuntimePayloadDigest,
}

impl AgentRuntimeInitialContextPackage {
    pub fn calculated_digest(&self) -> RuntimePayloadDigest {
        let contents = self
            .contributions
            .iter()
            .map(|contribution| &contribution.content)
            .collect::<Vec<_>>();
        let canonical = serde_json::to_vec(&(
            &self.package_id,
            u64::from(self.schema_version),
            self.mode,
            contents,
        ))
        .expect("Runtime initial context package is serializable");
        RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
            .expect("SHA-256 digest is non-empty")
    }

    pub fn validate(&self) -> bool {
        let mut ids = std::collections::BTreeSet::new();
        self.schema_version > 0
            && self.digest == self.calculated_digest()
            && self.contributions.iter().all(|contribution| {
                ids.insert(contribution.contribution_id.clone()) && contribution.validate()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeInteractionResponse {
    Approved,
    Denied {
        reason: Option<String>,
    },
    UserInput {
        content: Vec<AgentRuntimeContentBlock>,
    },
    Structured {
        schema: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeOperationReceipt {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub status: AgentRuntimeOperationStatus,
    pub evidence: Option<crate::AgentRuntimeOperationEvidence>,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContractSchema {
    pub initial_context: AgentRuntimeInitialContextPackage,
    pub interaction_response: AgentRuntimeInteractionResponse,
    pub operation_receipt: AgentRuntimeOperationReceipt,
    pub view: AgentRuntimeView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_schema_contains_product_handoff_and_view_families() {
        let schema = schemars::schema_for!(AgentRuntimeContractSchema);
        let schema = serde_json::to_string(&schema).expect("serialize Runtime schema");
        for family in [
            "AgentRuntimeInitialContextPackage",
            "AgentRuntimeInteractionResponse",
            "AgentRuntimeOperationReceipt",
            "AgentRuntimeView",
        ] {
            assert!(schema.contains(family), "missing {family}");
        }
        assert!(schema.contains("AgentRuntimeOperationEvidence"));
        assert!(!schema.contains("binding_generation"));
        assert!(!schema.contains("AgentSourceCoordinate"));
        assert!(!schema.contains("AgentRuntimeGatewayError"));
        assert!(!schema.contains("AgentRuntimeCommandEnvelope"));
    }

    #[test]
    fn initial_context_package_validates_typed_provenance_and_nested_digests() {
        let provenance = AgentRuntimeContextProvenance {
            authority: AgentRuntimeContextAuthority::Workflow,
            source: RuntimeContextSourceRef::new("workflow:primary").expect("source"),
            revision: RuntimeContextSourceRevision::new("workflow-revision:7").expect("revision"),
            digest: RuntimePayloadDigest::new("sha256:workflow").expect("digest"),
        };
        let mut contribution = AgentRuntimeInitialContextContribution {
            contribution_id: RuntimeContextContributionId::new("workflow-context")
                .expect("contribution"),
            digest: RuntimePayloadDigest::new("pending").expect("digest"),
            content: AgentRuntimeInitialContextContributionContent::WorkflowContext {
                schema: "agentdash.workflow/v1".to_owned(),
                value: serde_json::json!({"step": "implement"}),
                provenance,
            },
        };
        contribution.digest = contribution.calculated_digest();
        assert!(contribution.validate());
        let mut package = AgentRuntimeInitialContextPackage {
            package_id: RuntimeContextPackageId::new("initial-package").expect("package"),
            schema_version: 1,
            mode: AgentRuntimeInitialContextMode::WorkflowOnly,
            contributions: vec![contribution],
            digest: RuntimePayloadDigest::new("pending").expect("digest"),
        };
        package.digest = package.calculated_digest();
        assert!(package.validate());

        package.contributions[0].digest =
            RuntimePayloadDigest::new("sha256:tampered").expect("digest");
        assert!(!package.validate());
    }
}
