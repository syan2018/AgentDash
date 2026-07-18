use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeContextAuthority, ManagedRuntimeInitialContextContribution,
    ManagedRuntimeInitialContextContributionContent, ManagedRuntimeInitialContextContributionKind,
    ManagedRuntimeInitialContextMode, ManagedRuntimeInitialContextPackage, RuntimePayloadDigest,
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
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeAgentBinding {
    pub source: AgentSourceCoordinate,
    pub generation: AgentBindingGeneration,
    pub applied_surface: AppliedAgentSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRuntimeDispatchContext {
    pub runtime_thread_id: RuntimeThreadId,
    pub effect_id: AgentEffectIdentity,
    pub dispatch_owner: String,
    pub now_ms: u64,
    pub lease_duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeCreateOutcome {
    pub receipt: AgentCommandReceipt,
    pub binding: ManagedRuntimeAgentBinding,
    pub initial_context: Option<AppliedInitialContextEvidence>,
    pub contribution_fidelity:
        BTreeMap<InitialContextContributionKind, InitialContextDeliveryFidelity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeResumeOutcome {
    pub receipt: AgentCommandReceipt,
    pub binding: ManagedRuntimeAgentBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeForkOutcome {
    pub receipt: ForkAgentReceipt,
    pub child_binding: ManagedRuntimeAgentBinding,
    pub child_history_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagedRuntimeLifecycleInspection {
    NotApplied,
    Accepted,
    CreateApplied(ManagedRuntimeCreateOutcome),
    ResumeApplied(ManagedRuntimeResumeOutcome),
    ForkApplied(ManagedRuntimeForkOutcome),
    CommandApplied(AgentCommandReceipt),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagedRuntimeLifecycleError {
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
    #[error("managed Runtime lifecycle persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait ManagedRuntimeLifecyclePort: Send + Sync {
    async fn create(
        &self,
        context: ManagedRuntimeDispatchContext,
        initial_context: Option<InitialAgentContextPackage>,
    ) -> Result<ManagedRuntimeCreateOutcome, ManagedRuntimeLifecycleError>;

    async fn resume(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError>;

    async fn fork(
        &self,
        context: ManagedRuntimeDispatchContext,
        parent: ManagedRuntimeAgentBinding,
        child_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
    ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError>;

    async fn execute(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError>;

    async fn inspect(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: Option<ManagedRuntimeAgentBinding>,
    ) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError>;

    async fn read(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentReadQuery,
    ) -> Result<AgentSnapshot, ManagedRuntimeLifecycleError>;

    async fn changes(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, ManagedRuntimeLifecycleError>;

    async fn is_ready(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<bool, ManagedRuntimeLifecycleError>;
}

pub fn map_initial_context_package(
    package: ManagedRuntimeInitialContextPackage,
) -> Result<InitialAgentContextPackage, ManagedRuntimeLifecycleError> {
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
        ManagedRuntimeInitialContextMode::Compact => InitialContextMode::Compact,
        ManagedRuntimeInitialContextMode::WorkflowOnly => InitialContextMode::WorkflowOnly,
        ManagedRuntimeInitialContextMode::ConstraintsOnly => InitialContextMode::ConstraintsOnly,
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
    contribution: &ManagedRuntimeInitialContextContribution,
) -> ManagedRuntimeInitialContextContributionKind {
    match contribution.content {
        ManagedRuntimeInitialContextContributionContent::CompactSummary { .. } => {
            ManagedRuntimeInitialContextContributionKind::CompactSummary
        }
        ManagedRuntimeInitialContextContributionContent::WorkflowContext { .. } => {
            ManagedRuntimeInitialContextContributionKind::WorkflowContext
        }
        ManagedRuntimeInitialContextContributionContent::ConstraintSet { .. } => {
            ManagedRuntimeInitialContextContributionKind::ConstraintSet
        }
    }
}

pub fn runtime_payload_digest(
    digest: &AgentPayloadDigest,
) -> Result<RuntimePayloadDigest, ManagedRuntimeLifecycleError> {
    RuntimePayloadDigest::new(digest.as_str().to_owned())
        .map_err(|error| invalid(error.to_string()))
}

fn map_initial_context_contribution(
    contribution: ManagedRuntimeInitialContextContribution,
) -> Result<InitialContextContribution, ManagedRuntimeLifecycleError> {
    Ok(match contribution.content {
        ManagedRuntimeInitialContextContributionContent::CompactSummary {
            summary,
            provenance,
        } => InitialContextContribution::CompactSummary {
            summary,
            provenance: map_context_provenance(provenance)?,
        },
        ManagedRuntimeInitialContextContributionContent::WorkflowContext {
            schema,
            value,
            provenance,
        } => InitialContextContribution::WorkflowContext {
            payload: TypedContextPayload { schema, value },
            provenance: map_context_provenance(provenance)?,
        },
        ManagedRuntimeInitialContextContributionContent::ConstraintSet {
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
    provenance: agentdash_agent_runtime_contract::ManagedRuntimeContextProvenance,
) -> Result<ContextProvenance, ManagedRuntimeLifecycleError> {
    Ok(ContextProvenance {
        authority: match provenance.authority {
            ManagedRuntimeContextAuthority::AgentHistory => ContextAuthorityKind::AgentHistory,
            ManagedRuntimeContextAuthority::AgentSnapshot => ContextAuthorityKind::AgentSnapshot,
            ManagedRuntimeContextAuthority::Workflow => ContextAuthorityKind::Workflow,
            ManagedRuntimeContextAuthority::Constraint => ContextAuthorityKind::Constraint,
        },
        source: AgentContextSourceCoordinate::new(provenance.source.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
        revision: AgentContextSourceRevision::new(provenance.revision.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
        digest: AgentPayloadDigest::new(provenance.digest.into_inner())
            .map_err(|error| invalid(error.to_string()))?,
    })
}

fn invalid(reason: impl Into<String>) -> ManagedRuntimeLifecycleError {
    ManagedRuntimeLifecycleError::Invalid {
        reason: reason.into(),
    }
}
