use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    ManagedRuntimeChangePage, ManagedRuntimeContentBlock, ManagedRuntimeOperationStatus,
    ManagedRuntimeSnapshot, RuntimeChangeSequence, RuntimeContextContributionId,
    RuntimeContextPackageId, RuntimeContextSourceRef, RuntimeContextSourceRevision,
    RuntimeIdempotencyKey, RuntimeInteractionId, RuntimeOperationId, RuntimePayloadDigest,
    RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeInitialContextMode {
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeContextAuthority {
    AgentHistory,
    AgentSnapshot,
    Workflow,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeContextProvenance {
    pub authority: ManagedRuntimeContextAuthority,
    pub source: RuntimeContextSourceRef,
    pub revision: RuntimeContextSourceRevision,
    pub digest: RuntimePayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeInitialContextContributionContent {
    CompactSummary {
        summary: String,
        provenance: ManagedRuntimeContextProvenance,
    },
    WorkflowContext {
        schema: String,
        value: serde_json::Value,
        provenance: ManagedRuntimeContextProvenance,
    },
    ConstraintSet {
        schema: String,
        value: serde_json::Value,
        provenance: ManagedRuntimeContextProvenance,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeInitialContextContribution {
    pub contribution_id: RuntimeContextContributionId,
    pub digest: RuntimePayloadDigest,
    pub content: ManagedRuntimeInitialContextContributionContent,
}

impl ManagedRuntimeInitialContextContribution {
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
pub struct ManagedRuntimeInitialContextPackage {
    pub package_id: RuntimeContextPackageId,
    pub schema_version: u32,
    pub mode: ManagedRuntimeInitialContextMode,
    pub contributions: Vec<ManagedRuntimeInitialContextContribution>,
    pub digest: RuntimePayloadDigest,
}

impl ManagedRuntimeInitialContextPackage {
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
pub enum ManagedRuntimeInteractionResponse {
    Approved,
    Denied {
        reason: Option<String>,
    },
    UserInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Structured {
        schema: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeCommand {
    Create {
        initial_context: Option<ManagedRuntimeInitialContextPackage>,
    },
    Resume,
    Activate,
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Steer {
        expected_turn_id: RuntimeTurnId,
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt {
        expected_turn_id: RuntimeTurnId,
    },
    RequestCompaction,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
    Close,
    Fork {
        child_thread_id: RuntimeThreadId,
        through_completed_turn_id: Option<RuntimeTurnId>,
    },
}

impl ManagedRuntimeCommand {
    pub fn kind(&self) -> crate::ManagedRuntimeCommandKind {
        match self {
            Self::Create { .. } => crate::ManagedRuntimeCommandKind::Create,
            Self::Resume => crate::ManagedRuntimeCommandKind::Resume,
            Self::Activate => crate::ManagedRuntimeCommandKind::Activate,
            Self::SubmitInput { .. } => crate::ManagedRuntimeCommandKind::SubmitInput,
            Self::Steer { .. } => crate::ManagedRuntimeCommandKind::Steer,
            Self::Interrupt { .. } => crate::ManagedRuntimeCommandKind::Interrupt,
            Self::RequestCompaction => crate::ManagedRuntimeCommandKind::RequestCompaction,
            Self::ResolveInteraction { .. } => crate::ManagedRuntimeCommandKind::ResolveInteraction,
            Self::Close => crate::ManagedRuntimeCommandKind::Close,
            Self::Fork { .. } => crate::ManagedRuntimeCommandKind::Fork,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeCommandEnvelope {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: RuntimeIdempotencyKey,
    pub thread_id: RuntimeThreadId,
    pub expected_revision: Option<RuntimeProjectionRevision>,
    pub command: ManagedRuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeOperationReceipt {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub accepted_revision: RuntimeProjectionRevision,
    pub status: ManagedRuntimeOperationStatus,
    pub evidence: Option<crate::ManagedRuntimeOperationEvidence>,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeReadRequest {
    pub thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeChangesRequest {
    pub thread_id: RuntimeThreadId,
    pub after: Option<RuntimeChangeSequence>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeGatewayError {
    #[error("managed Runtime command conflicts with revision {actual:?}")]
    Conflict { actual: RuntimeProjectionRevision },
    #[error("managed Runtime thread was not found")]
    NotFound,
    #[error("managed Runtime command is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("managed Runtime request is invalid: {reason}")]
    Invalid { reason: String },
    #[error("managed Runtime persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait ManagedAgentRuntimeGateway: Send + Sync {
    async fn execute(
        &self,
        command: ManagedRuntimeCommandEnvelope,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError>;

    async fn read(
        &self,
        request: ManagedRuntimeReadRequest,
    ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError>;

    async fn changes(
        &self,
        request: ManagedRuntimeChangesRequest,
    ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeContractSchema {
    pub command: ManagedRuntimeCommandEnvelope,
    pub operation_receipt: ManagedRuntimeOperationReceipt,
    pub read: ManagedRuntimeReadRequest,
    pub changes: ManagedRuntimeChangesRequest,
    pub error: ManagedRuntimeGatewayError,
    pub snapshot: ManagedRuntimeSnapshot,
    pub change_page: ManagedRuntimeChangePage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_schema_contains_command_read_and_change_families() {
        let schema = schemars::schema_for!(ManagedRuntimeContractSchema);
        let schema = serde_json::to_string(&schema).expect("serialize Runtime schema");
        for family in [
            "ManagedRuntimeCommandEnvelope",
            "ManagedRuntimeOperationReceipt",
            "ManagedRuntimeReadRequest",
            "ManagedRuntimeChangesRequest",
            "ManagedRuntimeSnapshot",
            "ManagedRuntimeChangePage",
        ] {
            assert!(schema.contains(family), "missing {family}");
        }
        for lifecycle in ["create", "resume", "activate", "fork"] {
            assert!(schema.contains(lifecycle), "missing {lifecycle}");
        }
        assert!(schema.contains("ManagedRuntimeOperationEvidence"));
        assert!(!schema.contains("binding_generation"));
        assert!(!schema.contains("AgentSourceCoordinate"));
    }

    #[test]
    fn initial_context_package_validates_typed_provenance_and_nested_digests() {
        let provenance = ManagedRuntimeContextProvenance {
            authority: ManagedRuntimeContextAuthority::Workflow,
            source: RuntimeContextSourceRef::new("workflow:primary").expect("source"),
            revision: RuntimeContextSourceRevision::new("workflow-revision:7").expect("revision"),
            digest: RuntimePayloadDigest::new("sha256:workflow").expect("digest"),
        };
        let mut contribution = ManagedRuntimeInitialContextContribution {
            contribution_id: RuntimeContextContributionId::new("workflow-context")
                .expect("contribution"),
            digest: RuntimePayloadDigest::new("pending").expect("digest"),
            content: ManagedRuntimeInitialContextContributionContent::WorkflowContext {
                schema: "agentdash.workflow/v1".to_owned(),
                value: serde_json::json!({"step": "implement"}),
                provenance,
            },
        };
        contribution.digest = contribution.calculated_digest();
        assert!(contribution.validate());
        let mut package = ManagedRuntimeInitialContextPackage {
            package_id: RuntimeContextPackageId::new("initial-package").expect("package"),
            schema_version: 1,
            mode: ManagedRuntimeInitialContextMode::WorkflowOnly,
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
