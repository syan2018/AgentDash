use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    AgentRuntimeContextAuthority, AgentRuntimeInitialContextContribution,
    AgentRuntimeInitialContextContributionContent, AgentRuntimeInitialContextContributionKind,
    AgentRuntimeInitialContextMode, AgentRuntimeInitialContextPackage, RuntimePayloadDigest,
    RuntimeThreadId,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentChangePage, AgentChangesQuery, AgentCommandEnvelope,
    AgentCommandReceipt, AgentContextPackageId, AgentContextSchemaVersion,
    AgentContextSourceCoordinate, AgentContextSourceRevision, AgentEffectIdentity, AgentForkPoint,
    AgentPayloadDigest, AgentReadQuery, AgentSnapshot, AgentSourceCoordinate, AppliedAgentSurface,
    AppliedInitialContextEvidence, ContextAuthorityKind, ContextProvenance, ForkAgentReceipt,
    InitialAgentContextPackage, InitialContextContribution, InitialContextContributionKind,
    InitialContextDeliveryFidelity, InitialContextMode, TypedContextPayload,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRuntimeAgentBinding {
    pub source: AgentSourceCoordinate,
    pub generation: AgentBindingGeneration,
    pub applied_surface: AppliedAgentSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRuntimeDispatchContext {
    pub runtime_thread_id: RuntimeThreadId,
    pub effect_id: AgentEffectIdentity,
    pub dispatch_owner: String,
    pub now_ms: u64,
    pub lease_duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeCreateOutcome {
    pub receipt: AgentCommandReceipt,
    pub binding: AgentRuntimeAgentBinding,
    pub initial_context: Option<AppliedInitialContextEvidence>,
    pub contribution_fidelity:
        BTreeMap<InitialContextContributionKind, InitialContextDeliveryFidelity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeResumeOutcome {
    pub receipt: AgentCommandReceipt,
    pub binding: AgentRuntimeAgentBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeRebindOutcome {
    pub receipt: AgentCommandReceipt,
    pub previous_binding: AgentRuntimeAgentBinding,
    pub binding: AgentRuntimeAgentBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeForkOutcome {
    pub receipt: ForkAgentReceipt,
    pub child_binding: AgentRuntimeAgentBinding,
    pub child_history_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRuntimeLifecycleInspection {
    NotApplied,
    Accepted,
    CreateApplied(AgentRuntimeCreateOutcome),
    ResumeApplied(AgentRuntimeResumeOutcome),
    RebindApplied(AgentRuntimeRebindOutcome),
    ForkApplied(AgentRuntimeForkOutcome),
    CommandApplied(AgentCommandReceipt),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRuntimeLifecycleError {
    #[error("managed Runtime lifecycle target was not found")]
    NotFound,
    #[error("managed Runtime lifecycle target generation is stale")]
    StaleGeneration,
    #[error("managed Runtime lifecycle request is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("managed Runtime lifecycle request is invalid: {reason}")]
    Invalid { reason: String },
    #[error("managed Runtime lifecycle outcome requires inspection: {reason}")]
    InspectionRequired { reason: String },
    #[error("managed Runtime Fork child is known but provisioning is incomplete: {reason}")]
    ForkChildKnown {
        child_source: AgentSourceCoordinate,
        child_history_digest: Option<AgentPayloadDigest>,
        reason: String,
    },
    #[error("managed Runtime Fork child is known but its outcome requires inspection: {reason}")]
    ForkInspectionRequired {
        child_source: AgentSourceCoordinate,
        child_history_digest: Option<AgentPayloadDigest>,
        reason: String,
    },
    #[error("managed Runtime lifecycle persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait AgentRuntimeLifecyclePort: Send + Sync {
    async fn create(
        &self,
        context: AgentRuntimeDispatchContext,
        initial_context: Option<InitialAgentContextPackage>,
    ) -> Result<AgentRuntimeCreateOutcome, AgentRuntimeLifecycleError>;

    async fn resume(
        &self,
        context: AgentRuntimeDispatchContext,
        binding: AgentRuntimeAgentBinding,
    ) -> Result<AgentRuntimeResumeOutcome, AgentRuntimeLifecycleError>;

    async fn rebind(
        &self,
        context: AgentRuntimeDispatchContext,
        previous_binding: AgentRuntimeAgentBinding,
    ) -> Result<AgentRuntimeRebindOutcome, AgentRuntimeLifecycleError>;

    async fn fork(
        &self,
        context: AgentRuntimeDispatchContext,
        parent: AgentRuntimeAgentBinding,
        child_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
    ) -> Result<AgentRuntimeForkOutcome, AgentRuntimeLifecycleError>;

    async fn execute(
        &self,
        context: AgentRuntimeDispatchContext,
        binding: AgentRuntimeAgentBinding,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentRuntimeLifecycleError>;

    async fn inspect(
        &self,
        context: AgentRuntimeDispatchContext,
        binding: Option<AgentRuntimeAgentBinding>,
    ) -> Result<AgentRuntimeLifecycleInspection, AgentRuntimeLifecycleError>;

    async fn read(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: AgentRuntimeAgentBinding,
        query: AgentReadQuery,
    ) -> Result<AgentSnapshot, AgentRuntimeLifecycleError>;

    async fn changes(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: AgentRuntimeAgentBinding,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentRuntimeLifecycleError>;

    async fn is_ready(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: AgentRuntimeAgentBinding,
    ) -> Result<bool, AgentRuntimeLifecycleError>;
}

pub fn map_initial_context_package(
    package: AgentRuntimeInitialContextPackage,
) -> Result<InitialAgentContextPackage, AgentRuntimeLifecycleError> {
    if !package.validate() {
        return Err(invalid(
            "initial context package or contribution digest is invalid",
        ));
    }
    let package_id = AgentContextPackageId::new(package.package_id.into_inner())
        .map_err(|error| invalid(error.to_string()))?;
    if package.schema_version == 0 {
        return Err(invalid("initial context schema version must be positive"));
    }
    let schema_version = AgentContextSchemaVersion(package.schema_version.into());
    let mode = match package.mode {
        AgentRuntimeInitialContextMode::Compact => InitialContextMode::Compact,
        AgentRuntimeInitialContextMode::WorkflowOnly => InitialContextMode::WorkflowOnly,
        AgentRuntimeInitialContextMode::ConstraintsOnly => InitialContextMode::ConstraintsOnly,
    };
    let contributions = package
        .contributions
        .into_iter()
        .map(map_initial_context_contribution)
        .collect::<Result<Vec<_>, _>>()?;
    let digest = AgentPayloadDigest::new(package.digest.into_inner())
        .map_err(|error| invalid(error.to_string()))?;
    let mapped = InitialAgentContextPackage {
        package_id,
        schema_version,
        mode,
        contributions,
        digest,
    };
    if !mapped.digest_matches() {
        return Err(invalid(
            "initial context package digest does not match its payload",
        ));
    }
    Ok(mapped)
}

pub fn context_contribution_kind(
    contribution: &AgentRuntimeInitialContextContribution,
) -> AgentRuntimeInitialContextContributionKind {
    match contribution.content {
        AgentRuntimeInitialContextContributionContent::CompactSummary { .. } => {
            AgentRuntimeInitialContextContributionKind::CompactSummary
        }
        AgentRuntimeInitialContextContributionContent::WorkflowContext { .. } => {
            AgentRuntimeInitialContextContributionKind::WorkflowContext
        }
        AgentRuntimeInitialContextContributionContent::ConstraintSet { .. } => {
            AgentRuntimeInitialContextContributionKind::ConstraintSet
        }
    }
}

pub fn runtime_payload_digest(
    digest: &AgentPayloadDigest,
) -> Result<RuntimePayloadDigest, AgentRuntimeLifecycleError> {
    RuntimePayloadDigest::new(digest.as_str().to_owned())
        .map_err(|error| invalid(error.to_string()))
}

fn map_initial_context_contribution(
    contribution: AgentRuntimeInitialContextContribution,
) -> Result<InitialContextContribution, AgentRuntimeLifecycleError> {
    Ok(match contribution.content {
        AgentRuntimeInitialContextContributionContent::CompactSummary {
            summary,
            provenance,
        } => InitialContextContribution::CompactSummary {
            summary,
            provenance: map_context_provenance(provenance)?,
        },
        AgentRuntimeInitialContextContributionContent::WorkflowContext {
            schema,
            value,
            provenance,
        } => InitialContextContribution::WorkflowContext {
            payload: TypedContextPayload { schema, value },
            provenance: map_context_provenance(provenance)?,
        },
        AgentRuntimeInitialContextContributionContent::ConstraintSet {
            schema,
            value,
            provenance,
        } => InitialContextContribution::ConstraintSet {
            payload: TypedContextPayload { schema, value },
            provenance: map_context_provenance(provenance)?,
        },
    })
}

fn map_context_provenance(
    provenance: agentdash_agent_runtime_contract::AgentRuntimeContextProvenance,
) -> Result<ContextProvenance, AgentRuntimeLifecycleError> {
    Ok(ContextProvenance {
        authority: match provenance.authority {
            AgentRuntimeContextAuthority::AgentHistory => ContextAuthorityKind::AgentHistory,
            AgentRuntimeContextAuthority::AgentSnapshot => ContextAuthorityKind::AgentSnapshot,
            AgentRuntimeContextAuthority::Workflow => ContextAuthorityKind::Workflow,
            AgentRuntimeContextAuthority::Constraint => ContextAuthorityKind::Constraint,
        },
        source: AgentContextSourceCoordinate::new(provenance.source.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
        revision: AgentContextSourceRevision::new(provenance.revision.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
        digest: AgentPayloadDigest::new(provenance.digest.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
    })
}

fn invalid(reason: impl Into<String>) -> AgentRuntimeLifecycleError {
    AgentRuntimeLifecycleError::Invalid {
        reason: reason.into(),
    }
}
