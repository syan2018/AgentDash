use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    AgentContextPackageId, AgentContextSchemaVersion, AgentContextSourceCoordinate,
    AgentContextSourceRevision, AgentPayloadDigest,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextMode {
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextContributionKind {
    CompactSummary,
    WorkflowContext,
    ConstraintSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextAuthorityKind {
    AgentHistory,
    AgentSnapshot,
    Workflow,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ContextProvenance {
    pub authority: ContextAuthorityKind,
    pub source: AgentContextSourceCoordinate,
    pub revision: AgentContextSourceRevision,
    pub digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TypedContextPayload {
    pub schema: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InitialContextContribution {
    CompactSummary {
        summary: String,
        provenance: ContextProvenance,
    },
    WorkflowContext {
        payload: TypedContextPayload,
        provenance: ContextProvenance,
    },
    ConstraintSet {
        payload: TypedContextPayload,
        provenance: ContextProvenance,
    },
}

impl InitialContextContribution {
    pub fn kind(&self) -> InitialContextContributionKind {
        match self {
            Self::CompactSummary { .. } => InitialContextContributionKind::CompactSummary,
            Self::WorkflowContext { .. } => InitialContextContributionKind::WorkflowContext,
            Self::ConstraintSet { .. } => InitialContextContributionKind::ConstraintSet,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InitialAgentContextPackage {
    pub package_id: AgentContextPackageId,
    pub schema_version: AgentContextSchemaVersion,
    pub mode: InitialContextMode,
    pub contributions: Vec<InitialContextContribution>,
    pub digest: AgentPayloadDigest,
}

impl InitialAgentContextPackage {
    pub fn calculated_digest(
        package_id: &AgentContextPackageId,
        schema_version: AgentContextSchemaVersion,
        mode: InitialContextMode,
        contributions: &[InitialContextContribution],
    ) -> AgentPayloadDigest {
        let canonical = serde_json::to_vec(&(package_id, schema_version, mode, contributions))
            .expect("typed context package serialization cannot fail");
        AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
            .expect("sha256 digest is non-empty")
    }

    pub fn digest_matches(&self) -> bool {
        self.digest
            == Self::calculated_digest(
                &self.package_id,
                self.schema_version,
                self.mode,
                &self.contributions,
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AppliedInitialContextEvidence {
    pub package_id: AgentContextPackageId,
    pub package_digest: AgentPayloadDigest,
    pub fidelity: crate::InitialContextDeliveryFidelity,
    pub renderer_version: Option<String>,
    pub materialized_digest: Option<AgentPayloadDigest>,
}

impl AppliedInitialContextEvidence {
    pub fn guarantee(&self) -> crate::InitialContextAppliedEvidence {
        if self.materialized_digest.is_some() {
            crate::InitialContextAppliedEvidence::PackageAndMaterializedDigest
        } else {
            crate::InitialContextAppliedEvidence::PackageDigest
        }
    }

    pub fn satisfies(&self, required: crate::InitialContextAppliedEvidence) -> bool {
        self.guarantee().satisfies(required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<T>(
        value: &str,
        constructor: impl FnOnce(String) -> Result<T, crate::InvalidAgentServiceId>,
    ) -> T {
        constructor(value.to_owned()).expect("valid id")
    }

    #[test]
    fn package_digest_covers_typed_contributions_and_provenance() {
        let package_id = id("package-1", AgentContextPackageId::new);
        let contribution = InitialContextContribution::CompactSummary {
            summary: "summary".to_owned(),
            provenance: ContextProvenance {
                authority: ContextAuthorityKind::AgentHistory,
                source: id("parent", AgentContextSourceCoordinate::new),
                revision: id("rev-7", AgentContextSourceRevision::new),
                digest: id("sha256:parent", AgentPayloadDigest::new),
            },
        };
        let package = InitialAgentContextPackage {
            package_id: package_id.clone(),
            schema_version: AgentContextSchemaVersion(1),
            mode: InitialContextMode::Compact,
            digest: InitialAgentContextPackage::calculated_digest(
                &package_id,
                AgentContextSchemaVersion(1),
                InitialContextMode::Compact,
                std::slice::from_ref(&contribution),
            ),
            contributions: vec![contribution],
        };
        assert!(package.digest_matches());
    }

    #[test]
    fn materialized_digest_evidence_is_stronger_than_package_digest_only() {
        let evidence = AppliedInitialContextEvidence {
            package_id: id("package-1", AgentContextPackageId::new),
            package_digest: id("sha256:package", AgentPayloadDigest::new),
            fidelity: crate::InitialContextDeliveryFidelity::CanonicalRendered,
            renderer_version: Some("renderer-v1".to_owned()),
            materialized_digest: Some(id("sha256:rendered", AgentPayloadDigest::new)),
        };

        assert!(
            evidence.satisfies(crate::InitialContextAppliedEvidence::PackageAndMaterializedDigest)
        );
    }
}
