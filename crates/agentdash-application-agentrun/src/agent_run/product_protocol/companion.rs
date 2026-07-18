#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
#[cfg(test)]
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    AgentRunForkParent, AgentRunForkRequestId, AgentRunForkSaga, AgentRunForkSagaRepository,
    AgentRunForkSagaRepositoryError, CompiledContextApplication, CompiledContextDeliveryFidelity,
    PreallocatedAgentRunChild, RequiredInitialContextEvidence, RuntimeAgentChildIdentity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionContextMode {
    Full,
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionAdoptionMode {
    Suggestion,
    BlockingReview,
    FollowUpRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledContextAuthority {
    AgentHistory,
    AgentSnapshot,
    Workflow,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionContextSourceDraft {
    pub authority: CompiledContextAuthority,
    pub source_coordinate: String,
    pub source_revision: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledFreshContextMode {
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

impl TryFrom<CompanionContextMode> for CompiledFreshContextMode {
    type Error = CompanionTargetPlanError;

    fn try_from(mode: CompanionContextMode) -> Result<Self, Self::Error> {
        match mode {
            CompanionContextMode::Compact => Ok(Self::Compact),
            CompanionContextMode::WorkflowOnly => Ok(Self::WorkflowOnly),
            CompanionContextMode::ConstraintsOnly => Ok(Self::ConstraintsOnly),
            CompanionContextMode::Full => Err(CompanionTargetPlanError::FullModeHasNoFreshContext),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextProvenance {
    pub authority: CompiledContextAuthority,
    pub source: String,
    pub revision: String,
    pub digest: String,
}

impl From<CompanionContextSourceDraft> for CompiledContextProvenance {
    fn from(draft: CompanionContextSourceDraft) -> Self {
        Self {
            authority: draft.authority,
            source: draft.source_coordinate,
            revision: draft.source_revision,
            digest: draft.source_digest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledTypedContextPayload {
    pub schema: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompiledInitialContextContribution {
    CompactSummary {
        summary: String,
        provenance: CompiledContextProvenance,
    },
    WorkflowContext {
        payload: CompiledTypedContextPayload,
        provenance: CompiledContextProvenance,
    },
    ConstraintSet {
        payload: CompiledTypedContextPayload,
        provenance: CompiledContextProvenance,
    },
}

impl CompiledInitialContextContribution {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::CompactSummary { .. } => "compact_summary",
            Self::WorkflowContext { .. } => "workflow_context",
            Self::ConstraintSet { .. } => "constraint_set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledInitialContextPackage {
    pub package_id: Uuid,
    pub schema_version: u64,
    pub mode: CompiledFreshContextMode,
    pub contributions: Vec<CompiledInitialContextContribution>,
    pub digest: String,
}

impl CompiledInitialContextPackage {
    fn calculate_digest(
        package_id: Uuid,
        schema_version: u64,
        mode: CompiledFreshContextMode,
        contributions: &[CompiledInitialContextContribution],
    ) -> String {
        let canonical = serde_json::to_vec(&(package_id, schema_version, mode, contributions))
            .expect("compiled initial context package must serialize");
        format!("sha256:{:x}", Sha256::digest(canonical))
    }

    pub fn digest_matches(&self) -> bool {
        self.digest
            == Self::calculate_digest(
                self.package_id,
                self.schema_version,
                self.mode,
                &self.contributions,
            )
    }

    pub fn required_application_evidence(&self) -> RequiredInitialContextEvidence {
        RequiredInitialContextEvidence {
            package_id: self.package_id,
            package_digest: self.digest.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitInput {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompanionRuntimePreparation {
    ForkParentHistory {
        parent_source_coordinate: String,
        through_turn_id: String,
    },
    FreshCreate {
        initial_context: CompiledInitialContextPackage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionContextContributionRequirement {
    pub kind: String,
    pub minimum_fidelity: CompiledContextDeliveryFidelity,
    pub canonical_rendered_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionContextApplicationRequirement {
    pub contribution_requirements: Vec<CompanionContextContributionRequirement>,
    pub materialized_digest_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionDispatchTargetPlan {
    pub preparation: CompanionRuntimePreparation,
    pub context_application_requirement: Option<CompanionContextApplicationRequirement>,
    pub adoption_mode: CompanionAdoptionMode,
    pub first_submit_input: SubmitInput,
    /// Business Surface facts remain a separate target input. They are not
    /// serialized into `CompiledInitialContextPackage`.
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionContextSources {
    pub parent_source_coordinate: String,
    pub through_turn_id: Option<String>,
    pub package_id: Uuid,
    pub compact_summary: Option<(String, CompanionContextSourceDraft)>,
    pub workflow: Option<(String, Value, CompanionContextSourceDraft)>,
    pub constraints: Option<(String, Value, CompanionContextSourceDraft)>,
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionTargetPlanError {
    #[error("Full companion context requires an exact parent turn cutoff")]
    MissingParentTurnCutoff,
    #[error("Full companion context uses exact fork and has no fresh context package")]
    FullModeHasNoFreshContext,
    #[error("{mode:?} companion context has no typed contribution")]
    MissingTypedContribution { mode: CompanionContextMode },
    #[error("initial context package digest is invalid")]
    InvalidPackageDigest,
    #[error("activation requires applied initial context evidence")]
    MissingContextEvidence,
    #[error("applied initial context evidence does not match the package")]
    ContextEvidenceMismatch,
    #[error("applied initial context evidence reports unsupported delivery fidelity")]
    UnsupportedContextFidelity,
    #[error("dispatch target does not contain the required context application policy")]
    MissingContextApplicationRequirement,
    #[error("{kind} context fidelity {actual:?} is below required {minimum:?}")]
    ContextFidelityBelowMinimum {
        kind: String,
        minimum: CompiledContextDeliveryFidelity,
        actual: CompiledContextDeliveryFidelity,
    },
    #[error("CanonicalRendered delivery is not allowed for {kind}")]
    CanonicalRenderedNotAllowed { kind: String },
    #[error("applied initial context evidence does not cover each typed contribution exactly once")]
    ContributionFidelityMismatch,
    #[error("CanonicalRendered context evidence requires a renderer version")]
    MissingRendererVersion,
    #[error("applied initial context evidence requires a materialized digest")]
    MissingMaterializedDigest,
    #[error("Full history fork evidence does not match the exact parent history request")]
    ForkHistoryEvidenceMismatch,
}

pub fn compile_companion_dispatch_target(
    mode: CompanionContextMode,
    adoption_mode: CompanionAdoptionMode,
    task: SubmitInput,
    sources: CompanionContextSources,
) -> Result<CompanionDispatchTargetPlan, CompanionTargetPlanError> {
    let context_application_requirement = match mode {
        CompanionContextMode::Full => None,
        CompanionContextMode::Compact => Some(CompanionContextApplicationRequirement {
            contribution_requirements: vec![CompanionContextContributionRequirement {
                kind: "compact_summary".to_owned(),
                minimum_fidelity: CompiledContextDeliveryFidelity::CanonicalRendered,
                canonical_rendered_allowed: true,
            }],
            materialized_digest_required: true,
        }),
        CompanionContextMode::WorkflowOnly => Some(CompanionContextApplicationRequirement {
            contribution_requirements: vec![CompanionContextContributionRequirement {
                kind: "workflow_context".to_owned(),
                minimum_fidelity: CompiledContextDeliveryFidelity::TypedNative,
                canonical_rendered_allowed: false,
            }],
            materialized_digest_required: true,
        }),
        CompanionContextMode::ConstraintsOnly => Some(CompanionContextApplicationRequirement {
            contribution_requirements: vec![CompanionContextContributionRequirement {
                kind: "constraint_set".to_owned(),
                minimum_fidelity: CompiledContextDeliveryFidelity::TypedNative,
                canonical_rendered_allowed: false,
            }],
            materialized_digest_required: true,
        }),
    };
    let preparation = match mode {
        CompanionContextMode::Full => CompanionRuntimePreparation::ForkParentHistory {
            parent_source_coordinate: sources.parent_source_coordinate,
            through_turn_id: sources
                .through_turn_id
                .ok_or(CompanionTargetPlanError::MissingParentTurnCutoff)?,
        },
        CompanionContextMode::Compact
        | CompanionContextMode::WorkflowOnly
        | CompanionContextMode::ConstraintsOnly => {
            let contributions = match mode {
                CompanionContextMode::Compact => sources
                    .compact_summary
                    .map(|(summary, provenance)| {
                        vec![CompiledInitialContextContribution::CompactSummary {
                            summary,
                            provenance: provenance.into(),
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::WorkflowOnly => sources
                    .workflow
                    .map(|(schema, value, provenance)| {
                        vec![CompiledInitialContextContribution::WorkflowContext {
                            payload: CompiledTypedContextPayload { schema, value },
                            provenance: provenance.into(),
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::ConstraintsOnly => sources
                    .constraints
                    .map(|(schema, value, provenance)| {
                        vec![CompiledInitialContextContribution::ConstraintSet {
                            payload: CompiledTypedContextPayload { schema, value },
                            provenance: provenance.into(),
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::Full => unreachable!(),
            };
            if contributions.is_empty() {
                return Err(CompanionTargetPlanError::MissingTypedContribution { mode });
            }
            let schema_version = 1;
            let compiled_mode = mode.try_into()?;
            let digest = CompiledInitialContextPackage::calculate_digest(
                sources.package_id,
                schema_version,
                compiled_mode,
                &contributions,
            );
            CompanionRuntimePreparation::FreshCreate {
                initial_context: CompiledInitialContextPackage {
                    package_id: sources.package_id,
                    schema_version,
                    mode: compiled_mode,
                    contributions,
                    digest,
                },
            }
        }
    };
    Ok(CompanionDispatchTargetPlan {
        preparation,
        context_application_requirement,
        adoption_mode,
        first_submit_input: task,
        surface_facts: sources.surface_facts,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionRuntimePreparationEvidence {
    ForkParentHistory {
        child: RuntimeAgentChildIdentity,
        parent_source_coordinate: String,
        through_turn_id: String,
    },
    FreshCreate {
        child: RuntimeAgentChildIdentity,
        context: Option<CompiledContextApplication>,
    },
}

pub fn verify_companion_activation(
    plan: &CompanionDispatchTargetPlan,
    evidence: &CompanionRuntimePreparationEvidence,
) -> Result<(), CompanionTargetPlanError> {
    match (&plan.preparation, evidence) {
        (
            CompanionRuntimePreparation::ForkParentHistory {
                parent_source_coordinate,
                through_turn_id,
            },
            CompanionRuntimePreparationEvidence::ForkParentHistory {
                parent_source_coordinate: actual_parent,
                through_turn_id: actual_turn,
                ..
            },
        ) if parent_source_coordinate == actual_parent && through_turn_id == actual_turn => Ok(()),
        (
            CompanionRuntimePreparation::FreshCreate { initial_context },
            CompanionRuntimePreparationEvidence::FreshCreate {
                context: Some(actual),
                ..
            },
        ) if actual.package_id == initial_context.package_id
            && actual.package_digest == initial_context.digest =>
        {
            if !initial_context.digest_matches() {
                return Err(CompanionTargetPlanError::InvalidPackageDigest);
            }
            if actual.fidelity == CompiledContextDeliveryFidelity::Unsupported {
                return Err(CompanionTargetPlanError::UnsupportedContextFidelity);
            }
            let requirement = plan
                .context_application_requirement
                .as_ref()
                .ok_or(CompanionTargetPlanError::MissingContextApplicationRequirement)?;
            let expected = initial_context
                .contributions
                .iter()
                .map(CompiledInitialContextContribution::kind_name)
                .collect::<Vec<_>>();
            let required = requirement
                .contribution_requirements
                .iter()
                .map(|contribution| contribution.kind.as_str())
                .collect::<Vec<_>>();
            let actual_kinds = actual
                .contribution_fidelity
                .iter()
                .map(|contribution| contribution.kind.as_str())
                .collect::<Vec<_>>();
            if expected != required
                || expected != actual_kinds
                || actual.contribution_fidelity.iter().any(|contribution| {
                    contribution.fidelity == CompiledContextDeliveryFidelity::Unsupported
                })
            {
                return Err(CompanionTargetPlanError::ContributionFidelityMismatch);
            }
            for (required, applied) in requirement
                .contribution_requirements
                .iter()
                .zip(&actual.contribution_fidelity)
            {
                if (actual.fidelity == CompiledContextDeliveryFidelity::CanonicalRendered
                    || applied.fidelity == CompiledContextDeliveryFidelity::CanonicalRendered)
                    && !required.canonical_rendered_allowed
                {
                    return Err(CompanionTargetPlanError::CanonicalRenderedNotAllowed {
                        kind: required.kind.clone(),
                    });
                }
                if !actual.fidelity.satisfies(required.minimum_fidelity) {
                    return Err(CompanionTargetPlanError::ContextFidelityBelowMinimum {
                        kind: required.kind.clone(),
                        minimum: required.minimum_fidelity,
                        actual: actual.fidelity,
                    });
                }
                if !applied.fidelity.satisfies(required.minimum_fidelity) {
                    return Err(CompanionTargetPlanError::ContextFidelityBelowMinimum {
                        kind: required.kind.clone(),
                        minimum: required.minimum_fidelity,
                        actual: applied.fidelity,
                    });
                }
            }
            let canonical_rendered = actual.fidelity
                == CompiledContextDeliveryFidelity::CanonicalRendered
                || actual.contribution_fidelity.iter().any(|contribution| {
                    contribution.fidelity == CompiledContextDeliveryFidelity::CanonicalRendered
                });
            if canonical_rendered && actual.renderer_version.as_deref().is_none_or(str::is_empty) {
                return Err(CompanionTargetPlanError::MissingRendererVersion);
            }
            if requirement.materialized_digest_required
                && actual
                    .materialized_digest
                    .as_deref()
                    .is_none_or(str::is_empty)
            {
                return Err(CompanionTargetPlanError::MissingMaterializedDigest);
            }
            Ok(())
        }
        (
            CompanionRuntimePreparation::FreshCreate { .. },
            CompanionRuntimePreparationEvidence::FreshCreate { context: None, .. },
        ) => Err(CompanionTargetPlanError::MissingContextEvidence),
        (CompanionRuntimePreparation::FreshCreate { .. }, _) => {
            Err(CompanionTargetPlanError::ContextEvidenceMismatch)
        }
        (CompanionRuntimePreparation::ForkParentHistory { .. }, _) => {
            Err(CompanionTargetPlanError::ForkHistoryEvidenceMismatch)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompanionFreshRequestId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFreshStableIdentities {
    pub request_id: CompanionFreshRequestId,
    pub create_effect_id: Uuid,
    pub activation_effect_id: Uuid,
    pub first_input_effect_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionFreshOperation {
    CreateWithContextPackage,
    Activate,
    SubmitFirstInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompanionFreshOperationIdentity {
    pub request_id: CompanionFreshRequestId,
    pub operation: CompanionFreshOperation,
    pub effect_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionFreshPhase {
    Requested,
    AgentCreated,
    Activated,
    FirstInputSubmitted,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFreshDurableDispatch {
    pub identity: CompanionFreshOperationIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFreshReceipts {
    pub create: Option<String>,
    pub activation: Option<String>,
    pub first_input: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFreshLost {
    pub identity: CompanionFreshOperationIdentity,
    pub known_child: Option<RuntimeAgentChildIdentity>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionFreshEffectEvidence {
    Created {
        child: RuntimeAgentChildIdentity,
        context: CompiledContextApplication,
        receipt: String,
    },
    Activated {
        child: RuntimeAgentChildIdentity,
        receipt: String,
    },
    FirstInputSubmitted {
        child: RuntimeAgentChildIdentity,
        receipt: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionFreshEffectOutcome {
    Applied(CompanionFreshEffectEvidence),
    Unknown,
    Lost { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionFreshStep {
    Dispatch(CompanionFreshOperationIdentity),
    Inspect(CompanionFreshOperationIdentity),
    MarkSucceeded,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionFreshSaga {
    identities: CompanionFreshStableIdentities,
    plan: CompanionDispatchTargetPlan,
    phase: CompanionFreshPhase,
    version: u64,
    durable_dispatch: Option<CompanionFreshDurableDispatch>,
    child: Option<RuntimeAgentChildIdentity>,
    context_evidence: Option<CompiledContextApplication>,
    receipts: CompanionFreshReceipts,
    lost: Option<CompanionFreshLost>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionFreshSagaError {
    #[error("fresh Companion saga requires FreshCreate preparation")]
    NotFreshCreate,
    #[error("fresh Companion saga is terminal")]
    Terminal,
    #[error("fresh Companion operation is out of order")]
    OperationOutOfOrder,
    #[error("fresh Companion effect identity drifted")]
    EffectIdentityDrift,
    #[error("fresh Companion child identity drifted")]
    ChildIdentityDrift,
    #[error(transparent)]
    Preparation(#[from] CompanionTargetPlanError),
}

impl CompanionFreshSaga {
    pub fn requested(
        identities: CompanionFreshStableIdentities,
        plan: CompanionDispatchTargetPlan,
    ) -> Result<Self, CompanionFreshSagaError> {
        if !matches!(
            plan.preparation,
            CompanionRuntimePreparation::FreshCreate { .. }
        ) {
            return Err(CompanionFreshSagaError::NotFreshCreate);
        }
        Ok(Self {
            identities,
            plan,
            phase: CompanionFreshPhase::Requested,
            version: 0,
            durable_dispatch: None,
            child: None,
            context_evidence: None,
            receipts: CompanionFreshReceipts {
                create: None,
                activation: None,
                first_input: None,
            },
            lost: None,
        })
    }

    pub fn request_id(&self) -> &CompanionFreshRequestId {
        &self.identities.request_id
    }

    pub fn plan(&self) -> &CompanionDispatchTargetPlan {
        &self.plan
    }

    pub fn phase(&self) -> CompanionFreshPhase {
        self.phase
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn child(&self) -> Option<&RuntimeAgentChildIdentity> {
        self.child.as_ref()
    }

    pub fn context_evidence(&self) -> Option<&CompiledContextApplication> {
        self.context_evidence.as_ref()
    }

    pub fn receipts(&self) -> &CompanionFreshReceipts {
        &self.receipts
    }

    pub fn next_step(&self) -> CompanionFreshStep {
        if self.lost.is_some() || self.phase == CompanionFreshPhase::Succeeded {
            return CompanionFreshStep::Terminal;
        }
        if let Some(dispatch) = &self.durable_dispatch {
            return CompanionFreshStep::Inspect(dispatch.identity.clone());
        }
        match self.phase {
            CompanionFreshPhase::Requested => CompanionFreshStep::Dispatch(
                self.identity(CompanionFreshOperation::CreateWithContextPackage),
            ),
            CompanionFreshPhase::AgentCreated => {
                CompanionFreshStep::Dispatch(self.identity(CompanionFreshOperation::Activate))
            }
            CompanionFreshPhase::Activated => CompanionFreshStep::Dispatch(
                self.identity(CompanionFreshOperation::SubmitFirstInput),
            ),
            CompanionFreshPhase::FirstInputSubmitted => CompanionFreshStep::MarkSucceeded,
            CompanionFreshPhase::Succeeded => CompanionFreshStep::Terminal,
        }
    }

    pub fn mark_dispatched(
        &mut self,
        identity: CompanionFreshOperationIdentity,
    ) -> Result<(), CompanionFreshSagaError> {
        if self.durable_dispatch.is_some() || self.expected_identity() != identity {
            return Err(CompanionFreshSagaError::EffectIdentityDrift);
        }
        self.durable_dispatch = Some(CompanionFreshDurableDispatch { identity });
        Ok(())
    }

    pub fn record_outcome(
        &mut self,
        identity: CompanionFreshOperationIdentity,
        outcome: CompanionFreshEffectOutcome,
    ) -> Result<(), CompanionFreshSagaError> {
        if self.lost.is_some() || self.phase == CompanionFreshPhase::Succeeded {
            return Err(CompanionFreshSagaError::Terminal);
        }
        if self
            .durable_dispatch
            .as_ref()
            .map(|dispatch| &dispatch.identity)
            != Some(&identity)
            || self.expected_identity() != identity
        {
            return Err(CompanionFreshSagaError::EffectIdentityDrift);
        }
        match outcome {
            CompanionFreshEffectOutcome::Unknown => {}
            CompanionFreshEffectOutcome::Lost { reason } => {
                self.lost = Some(CompanionFreshLost {
                    identity,
                    known_child: self.child.clone(),
                    reason,
                });
                self.durable_dispatch = None;
            }
            CompanionFreshEffectOutcome::Applied(evidence) => {
                self.apply_evidence(identity.operation, evidence)?;
                self.durable_dispatch = None;
            }
        }
        Ok(())
    }

    pub fn mark_succeeded(&mut self) -> Result<(), CompanionFreshSagaError> {
        if self.phase != CompanionFreshPhase::FirstInputSubmitted {
            return Err(CompanionFreshSagaError::OperationOutOfOrder);
        }
        self.phase = CompanionFreshPhase::Succeeded;
        Ok(())
    }

    fn apply_evidence(
        &mut self,
        operation: CompanionFreshOperation,
        evidence: CompanionFreshEffectEvidence,
    ) -> Result<(), CompanionFreshSagaError> {
        match (operation, evidence) {
            (
                CompanionFreshOperation::CreateWithContextPackage,
                CompanionFreshEffectEvidence::Created {
                    child,
                    context,
                    receipt,
                },
            ) if self.phase == CompanionFreshPhase::Requested => {
                verify_companion_activation(
                    &self.plan,
                    &CompanionRuntimePreparationEvidence::FreshCreate {
                        child: child.clone(),
                        context: Some(context.clone()),
                    },
                )?;
                self.pin_child(&child)?;
                self.context_evidence = Some(context);
                self.receipts.create = Some(receipt);
                self.phase = CompanionFreshPhase::AgentCreated;
                Ok(())
            }
            (
                CompanionFreshOperation::Activate,
                CompanionFreshEffectEvidence::Activated { child, receipt },
            ) if self.phase == CompanionFreshPhase::AgentCreated
                && self.context_evidence.is_some() =>
            {
                self.pin_child(&child)?;
                self.receipts.activation = Some(receipt);
                self.phase = CompanionFreshPhase::Activated;
                Ok(())
            }
            (
                CompanionFreshOperation::SubmitFirstInput,
                CompanionFreshEffectEvidence::FirstInputSubmitted { child, receipt },
            ) if self.phase == CompanionFreshPhase::Activated => {
                self.pin_child(&child)?;
                self.receipts.first_input = Some(receipt);
                self.phase = CompanionFreshPhase::FirstInputSubmitted;
                Ok(())
            }
            _ => Err(CompanionFreshSagaError::OperationOutOfOrder),
        }
    }

    fn pin_child(
        &mut self,
        child: &RuntimeAgentChildIdentity,
    ) -> Result<(), CompanionFreshSagaError> {
        if self.child.as_ref().is_some_and(|current| current != child) {
            return Err(CompanionFreshSagaError::ChildIdentityDrift);
        }
        self.child = Some(child.clone());
        Ok(())
    }

    fn expected_identity(&self) -> CompanionFreshOperationIdentity {
        match self.phase {
            CompanionFreshPhase::Requested => {
                self.identity(CompanionFreshOperation::CreateWithContextPackage)
            }
            CompanionFreshPhase::AgentCreated => self.identity(CompanionFreshOperation::Activate),
            CompanionFreshPhase::Activated => {
                self.identity(CompanionFreshOperation::SubmitFirstInput)
            }
            CompanionFreshPhase::FirstInputSubmitted | CompanionFreshPhase::Succeeded => {
                self.identity(CompanionFreshOperation::SubmitFirstInput)
            }
        }
    }

    fn identity(&self, operation: CompanionFreshOperation) -> CompanionFreshOperationIdentity {
        let effect_id = match operation {
            CompanionFreshOperation::CreateWithContextPackage => self.identities.create_effect_id,
            CompanionFreshOperation::Activate => self.identities.activation_effect_id,
            CompanionFreshOperation::SubmitFirstInput => self.identities.first_input_effect_id,
        };
        CompanionFreshOperationIdentity {
            request_id: self.identities.request_id.clone(),
            operation,
            effect_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionFreshRepositoryError {
    #[error("fresh Companion request already exists")]
    AlreadyExists,
    #[error("fresh Companion saga was not found")]
    NotFound,
    #[error("fresh Companion saga revision conflict")]
    Conflict,
    #[error("fresh Companion saga repository unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait CompanionFreshSagaRepository: Send + Sync {
    async fn create(
        &self,
        saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError>;
    async fn load(
        &self,
        request_id: &CompanionFreshRequestId,
    ) -> Result<Option<CompanionFreshSaga>, CompanionFreshRepositoryError>;
    async fn save(
        &self,
        expected_version: u64,
        saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError>;
}

/// Product owner 冻结的 Fresh Companion durable shape；W8 持有该合同唯一的 migration
/// 与 PostgreSQL adapter。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanionFreshSagaSchemaContract {
    pub table: &'static str,
    pub request_key: &'static str,
    pub optimistic_revision: &'static str,
    pub durable_dispatch_identity: &'static str,
    pub context_evidence: &'static str,
    pub first_input_receipt: &'static str,
}

pub const COMPANION_FRESH_SAGA_SCHEMA_CONTRACT: CompanionFreshSagaSchemaContract =
    CompanionFreshSagaSchemaContract {
        table: "companion_fresh_saga",
        request_key: "request_id",
        optimistic_revision: "version",
        durable_dispatch_identity: "durable_dispatch",
        context_evidence: "context_application_evidence",
        first_input_receipt: "first_input_receipt",
    };

#[cfg(test)]
#[derive(Default)]
pub(super) struct RecordingCompanionFreshSagaRepository {
    sagas: Arc<Mutex<HashMap<CompanionFreshRequestId, CompanionFreshSaga>>>,
}

#[cfg(test)]
#[async_trait]
impl CompanionFreshSagaRepository for RecordingCompanionFreshSagaRepository {
    async fn create(
        &self,
        mut saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError> {
        let mut sagas = self.sagas.lock().await;
        if sagas.contains_key(saga.request_id()) {
            return Err(CompanionFreshRepositoryError::AlreadyExists);
        }
        saga.version = 1;
        sagas.insert(saga.request_id().clone(), saga.clone());
        Ok(saga)
    }

    async fn load(
        &self,
        request_id: &CompanionFreshRequestId,
    ) -> Result<Option<CompanionFreshSaga>, CompanionFreshRepositoryError> {
        Ok(self.sagas.lock().await.get(request_id).cloned())
    }

    async fn save(
        &self,
        expected_version: u64,
        mut saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError> {
        let mut sagas = self.sagas.lock().await;
        let current = sagas
            .get(saga.request_id())
            .ok_or(CompanionFreshRepositoryError::NotFound)?;
        if current.version != expected_version {
            return Err(CompanionFreshRepositoryError::Conflict);
        }
        saga.version = expected_version + 1;
        sagas.insert(saga.request_id().clone(), saga.clone());
        Ok(saga)
    }
}

#[async_trait]
pub trait CompanionFreshRuntimePort: Send + Sync {
    async fn execute(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String>;
    async fn inspect(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionFreshWorkerError {
    #[error(transparent)]
    Repository(#[from] CompanionFreshRepositoryError),
    #[error(transparent)]
    Saga(#[from] CompanionFreshSagaError),
    #[error("fresh Companion Runtime effect failed: {0}")]
    Runtime(String),
}

pub struct CompanionFreshSagaWorker<'a> {
    repository: &'a dyn CompanionFreshSagaRepository,
    runtime: &'a dyn CompanionFreshRuntimePort,
}

impl<'a> CompanionFreshSagaWorker<'a> {
    pub fn new(
        repository: &'a dyn CompanionFreshSagaRepository,
        runtime: &'a dyn CompanionFreshRuntimePort,
    ) -> Self {
        Self {
            repository,
            runtime,
        }
    }

    pub async fn advance(
        &self,
        request_id: &CompanionFreshRequestId,
    ) -> Result<CompanionFreshSaga, CompanionFreshWorkerError> {
        let mut saga = self
            .repository
            .load(request_id)
            .await?
            .ok_or(CompanionFreshRepositoryError::NotFound)?;
        match saga.next_step() {
            CompanionFreshStep::Dispatch(identity) => {
                let expected_version = saga.version;
                saga.mark_dispatched(identity.clone())?;
                let mut dispatched = self.repository.save(expected_version, saga).await?;
                let outcome = self
                    .runtime
                    .execute(&dispatched, &identity)
                    .await
                    .map_err(CompanionFreshWorkerError::Runtime)?;
                let expected_version = dispatched.version;
                dispatched.record_outcome(identity, outcome)?;
                return Ok(self.repository.save(expected_version, dispatched).await?);
            }
            CompanionFreshStep::Inspect(identity) => {
                let outcome = self
                    .runtime
                    .inspect(&saga, &identity)
                    .await
                    .map_err(CompanionFreshWorkerError::Runtime)?;
                saga.record_outcome(identity, outcome)?;
            }
            CompanionFreshStep::MarkSucceeded => saga.mark_succeeded()?,
            CompanionFreshStep::Terminal => return Ok(saga),
        }
        let expected_version = saga.version;
        Ok(self.repository.save(expected_version, saga).await?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionFullForkRequest {
    pub request_id: AgentRunForkRequestId,
    pub parent: AgentRunForkParent,
    pub child: PreallocatedAgentRunChild,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionDispatchCoordinatorError {
    #[error("Full companion dispatch requires ForkParentHistory")]
    NotFullFork,
    #[error("Full companion parent source/cutoff does not match the plan")]
    FullForkIdentityMismatch,
    #[error("existing Full companion fork saga drifted from the stable request")]
    ExistingForkSagaDrift,
    #[error("existing fresh Companion saga drifted from the stable request")]
    ExistingFreshSagaDrift,
    #[error(transparent)]
    ForkRepository(#[from] AgentRunForkSagaRepositoryError),
    #[error(transparent)]
    FreshRepository(#[from] CompanionFreshRepositoryError),
    #[error(transparent)]
    FreshSaga(#[from] CompanionFreshSagaError),
}

pub struct CompanionDispatchCoordinator<'a> {
    fork_repository: &'a dyn AgentRunForkSagaRepository,
    fresh_repository: &'a dyn CompanionFreshSagaRepository,
}

impl<'a> CompanionDispatchCoordinator<'a> {
    pub fn new(
        fork_repository: &'a dyn AgentRunForkSagaRepository,
        fresh_repository: &'a dyn CompanionFreshSagaRepository,
    ) -> Self {
        Self {
            fork_repository,
            fresh_repository,
        }
    }

    pub async fn materialize_full_fork(
        &self,
        plan: &CompanionDispatchTargetPlan,
        request: CompanionFullForkRequest,
    ) -> Result<AgentRunForkSaga, CompanionDispatchCoordinatorError> {
        let CompanionRuntimePreparation::ForkParentHistory {
            parent_source_coordinate,
            through_turn_id,
        } = &plan.preparation
        else {
            return Err(CompanionDispatchCoordinatorError::NotFullFork);
        };
        if request.parent.source_coordinate != *parent_source_coordinate
            || request.parent.through_turn_id != *through_turn_id
        {
            return Err(CompanionDispatchCoordinatorError::FullForkIdentityMismatch);
        }
        let requested =
            AgentRunForkSaga::requested(request.request_id.clone(), request.parent, request.child);
        match self.fork_repository.create(requested.clone()).await {
            Ok(saga) => Ok(saga),
            Err(AgentRunForkSagaRepositoryError::AlreadyExists) => {
                let existing = self
                    .fork_repository
                    .load(&request.request_id)
                    .await?
                    .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
                if existing.request_id() != requested.request_id()
                    || existing.parent() != requested.parent()
                    || existing.child() != requested.child()
                {
                    return Err(CompanionDispatchCoordinatorError::ExistingForkSagaDrift);
                }
                Ok(existing)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn materialize_fresh(
        &self,
        identities: CompanionFreshStableIdentities,
        plan: CompanionDispatchTargetPlan,
    ) -> Result<CompanionFreshSaga, CompanionDispatchCoordinatorError> {
        let request_id = identities.request_id.clone();
        let requested = CompanionFreshSaga::requested(identities, plan)?;
        match self.fresh_repository.create(requested.clone()).await {
            Ok(saga) => Ok(saga),
            Err(CompanionFreshRepositoryError::AlreadyExists) => {
                let existing = self
                    .fresh_repository
                    .load(&request_id)
                    .await?
                    .ok_or(CompanionFreshRepositoryError::NotFound)?;
                if existing.identities != requested.identities || existing.plan != requested.plan {
                    return Err(CompanionDispatchCoordinatorError::ExistingFreshSagaDrift);
                }
                Ok(existing)
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::agent_run::product_protocol::{
        AgentRunForkOperationIdentity, AgentRunForkProductGraphPort, AgentRunForkRuntimePort,
        AgentRunForkSagaPhase, AgentRunForkSagaWorker, CompiledContextContributionApplication,
        PreparedAgentRunForkGraph, RuntimeForkPhaseEvidence, RuntimeOperationOutcome,
    };
    use agentdash_agent_service_api as service_api;
    use agentdash_application_ports::agent_run_fork::AgentRunForkGraph;
    use agentdash_domain::workflow::{
        AgentFrame, AgentRunLineage, AgentSource, LifecycleAgent, LifecycleRun,
    };
    use serde_json::json;

    use super::super::fork_saga::RecordingAgentRunForkSagaRepository;
    use super::*;

    fn provenance(authority: &str) -> CompanionContextSourceDraft {
        CompanionContextSourceDraft {
            authority: match authority {
                "agent_history" => CompiledContextAuthority::AgentHistory,
                "agent_snapshot" => CompiledContextAuthority::AgentSnapshot,
                "workflow" => CompiledContextAuthority::Workflow,
                "constraint" => CompiledContextAuthority::Constraint,
                other => panic!("unknown test authority: {other}"),
            },
            source_coordinate: "parent".to_owned(),
            source_revision: "rev-9".to_owned(),
            source_digest: "sha256:source".to_owned(),
        }
    }

    fn sources() -> CompanionContextSources {
        CompanionContextSources {
            parent_source_coordinate: "parent".to_owned(),
            through_turn_id: Some("turn-9".to_owned()),
            package_id: Uuid::new_v4(),
            compact_summary: Some(("summary".to_owned(), provenance("agent_history"))),
            workflow: Some((
                "agentdash.workflow.v1".to_owned(),
                json!({"step": "review"}),
                provenance("workflow"),
            )),
            constraints: Some((
                "agentdash.constraints.v1".to_owned(),
                json!({"deny": ["network"]}),
                provenance("constraint"),
            )),
            surface_facts: json!({"tools": ["read"], "working_directory": "/workspace"}),
        }
    }

    fn fresh_plan() -> CompanionDispatchTargetPlan {
        fresh_plan_for(CompanionContextMode::WorkflowOnly)
    }

    fn fresh_plan_for(mode: CompanionContextMode) -> CompanionDispatchTargetPlan {
        compile_companion_dispatch_target(
            mode,
            CompanionAdoptionMode::BlockingReview,
            SubmitInput {
                text: "perform durable review".to_owned(),
            },
            sources(),
        )
        .expect("fresh plan")
    }

    fn stable_fresh_identities() -> CompanionFreshStableIdentities {
        CompanionFreshStableIdentities {
            request_id: CompanionFreshRequestId(Uuid::new_v4()),
            create_effect_id: Uuid::new_v4(),
            activation_effect_id: Uuid::new_v4(),
            first_input_effect_id: Uuid::new_v4(),
        }
    }

    fn map_authority(authority: CompiledContextAuthority) -> service_api::ContextAuthorityKind {
        match authority {
            CompiledContextAuthority::AgentHistory => {
                service_api::ContextAuthorityKind::AgentHistory
            }
            CompiledContextAuthority::AgentSnapshot => {
                service_api::ContextAuthorityKind::AgentSnapshot
            }
            CompiledContextAuthority::Workflow => service_api::ContextAuthorityKind::Workflow,
            CompiledContextAuthority::Constraint => service_api::ContextAuthorityKind::Constraint,
        }
    }

    fn map_provenance(provenance: &CompiledContextProvenance) -> service_api::ContextProvenance {
        service_api::ContextProvenance {
            authority: map_authority(provenance.authority),
            source: service_api::AgentContextSourceCoordinate::new(&provenance.source)
                .expect("compiled source coordinate"),
            revision: service_api::AgentContextSourceRevision::new(&provenance.revision)
                .expect("compiled source revision"),
            digest: service_api::AgentPayloadDigest::new(&provenance.digest)
                .expect("compiled source digest"),
        }
    }

    fn map_contribution(
        contribution: &CompiledInitialContextContribution,
    ) -> service_api::InitialContextContribution {
        match contribution {
            CompiledInitialContextContribution::CompactSummary {
                summary,
                provenance,
            } => service_api::InitialContextContribution::CompactSummary {
                summary: summary.clone(),
                provenance: map_provenance(provenance),
            },
            CompiledInitialContextContribution::WorkflowContext {
                payload,
                provenance,
            } => service_api::InitialContextContribution::WorkflowContext {
                payload: service_api::TypedContextPayload {
                    schema: payload.schema.clone(),
                    value: payload.value.clone(),
                },
                provenance: map_provenance(provenance),
            },
            CompiledInitialContextContribution::ConstraintSet {
                payload,
                provenance,
            } => service_api::InitialContextContribution::ConstraintSet {
                payload: service_api::TypedContextPayload {
                    schema: payload.schema.clone(),
                    value: payload.value.clone(),
                },
                provenance: map_provenance(provenance),
            },
        }
    }

    fn map_initial_context(
        package: &CompiledInitialContextPackage,
    ) -> service_api::InitialAgentContextPackage {
        let mode = match package.mode {
            CompiledFreshContextMode::Compact => service_api::InitialContextMode::Compact,
            CompiledFreshContextMode::WorkflowOnly => service_api::InitialContextMode::WorkflowOnly,
            CompiledFreshContextMode::ConstraintsOnly => {
                service_api::InitialContextMode::ConstraintsOnly
            }
        };
        service_api::InitialAgentContextPackage {
            package_id: service_api::AgentContextPackageId::new(package.package_id.to_string())
                .expect("compiled package id"),
            schema_version: service_api::AgentContextSchemaVersion(package.schema_version),
            mode,
            contributions: package.contributions.iter().map(map_contribution).collect(),
            digest: service_api::AgentPayloadDigest::new(&package.digest)
                .expect("compiled package digest"),
        }
    }

    fn map_create_command(
        identity: &CompanionFreshOperationIdentity,
        package: &CompiledInitialContextPackage,
    ) -> service_api::CreateAgentCommand {
        service_api::CreateAgentCommand {
            meta: service_api::AgentCommandMeta {
                command_id: service_api::AgentCommandId::new(identity.request_id.0.to_string())
                    .expect("request command id"),
                effect_id: service_api::AgentEffectIdentity::new(identity.effect_id.to_string())
                    .expect("create effect id"),
                idempotency_key: service_api::AgentIdempotencyKey::new(format!(
                    "{}:create",
                    identity.request_id.0
                ))
                .expect("create idempotency key"),
                binding_generation: service_api::AgentBindingGeneration(0),
                expected_snapshot_revision: None,
            },
            requested_source: None,
            initial_context: Some(map_initial_context(package)),
        }
    }

    fn map_fidelity(
        fidelity: service_api::InitialContextDeliveryFidelity,
    ) -> CompiledContextDeliveryFidelity {
        match fidelity {
            service_api::InitialContextDeliveryFidelity::Unsupported => {
                CompiledContextDeliveryFidelity::Unsupported
            }
            service_api::InitialContextDeliveryFidelity::CanonicalRendered => {
                CompiledContextDeliveryFidelity::CanonicalRendered
            }
            service_api::InitialContextDeliveryFidelity::TypedNative => {
                CompiledContextDeliveryFidelity::TypedNative
            }
        }
    }

    fn map_applied_context(
        evidence: &service_api::AppliedInitialContextEvidence,
        contribution_fidelity: &[(
            service_api::InitialContextContributionKind,
            service_api::InitialContextDeliveryFidelity,
        )],
    ) -> CompiledContextApplication {
        CompiledContextApplication {
            package_id: Uuid::parse_str(evidence.package_id.as_str())
                .expect("Product package UUID"),
            package_digest: evidence.package_digest.as_str().to_owned(),
            fidelity: map_fidelity(evidence.fidelity),
            contribution_fidelity: contribution_fidelity
                .iter()
                .map(|(kind, fidelity)| CompiledContextContributionApplication {
                    kind: match kind {
                        service_api::InitialContextContributionKind::CompactSummary => {
                            "compact_summary"
                        }
                        service_api::InitialContextContributionKind::WorkflowContext => {
                            "workflow_context"
                        }
                        service_api::InitialContextContributionKind::ConstraintSet => {
                            "constraint_set"
                        }
                    }
                    .to_owned(),
                    fidelity: map_fidelity(*fidelity),
                })
                .collect(),
            renderer_version: evidence.renderer_version.clone(),
            materialized_digest: evidence
                .materialized_digest
                .as_ref()
                .map(|digest| digest.as_str().to_owned()),
        }
    }

    fn applied_context(plan: &CompanionDispatchTargetPlan) -> CompiledContextApplication {
        let CompanionRuntimePreparation::FreshCreate { initial_context } = &plan.preparation else {
            panic!("fresh");
        };
        let requirement = plan
            .context_application_requirement
            .as_ref()
            .expect("fresh context requirement");
        let fidelity = requirement.contribution_requirements[0].minimum_fidelity;
        CompiledContextApplication {
            package_id: initial_context.package_id,
            package_digest: initial_context.digest.clone(),
            fidelity,
            contribution_fidelity: requirement
                .contribution_requirements
                .iter()
                .map(|contribution| CompiledContextContributionApplication {
                    kind: contribution.kind.clone(),
                    fidelity,
                })
                .collect(),
            renderer_version: (fidelity == CompiledContextDeliveryFidelity::CanonicalRendered)
                .then(|| "context-renderer-v1".to_owned()),
            materialized_digest: Some("sha256:materialized-context".to_owned()),
        }
    }

    fn fresh_evidence(saga: &CompanionFreshSaga) -> CompanionFreshEffectEvidence {
        let child = saga.child.clone().unwrap_or(RuntimeAgentChildIdentity {
            source_coordinate: "fresh-child-source".to_owned(),
            runtime_agent_id: "fresh-runtime-child".to_owned(),
        });
        match saga.phase {
            CompanionFreshPhase::Requested => CompanionFreshEffectEvidence::Created {
                child,
                context: applied_context(&saga.plan),
                receipt: "create-receipt".to_owned(),
            },
            CompanionFreshPhase::AgentCreated => CompanionFreshEffectEvidence::Activated {
                child,
                receipt: "activation-receipt".to_owned(),
            },
            CompanionFreshPhase::Activated => CompanionFreshEffectEvidence::FirstInputSubmitted {
                child,
                receipt: "first-input-receipt".to_owned(),
            },
            CompanionFreshPhase::FirstInputSubmitted | CompanionFreshPhase::Succeeded => {
                panic!("no Runtime effect")
            }
        }
    }

    #[derive(Default)]
    struct FreshCompleteAgentTargetFixture {
        effects: Mutex<HashMap<CompanionFreshOperationIdentity, CompanionFreshEffectOutcome>>,
        lose_response_once: Mutex<HashSet<CompanionFreshOperation>>,
        executed: Mutex<Vec<CompanionFreshOperationIdentity>>,
        inspected: Mutex<Vec<CompanionFreshOperationIdentity>>,
        submitted_inputs: Mutex<Vec<(Uuid, String)>>,
    }

    impl FreshCompleteAgentTargetFixture {
        fn losing_responses(operations: impl IntoIterator<Item = CompanionFreshOperation>) -> Self {
            Self {
                lose_response_once: Mutex::new(operations.into_iter().collect()),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl CompanionFreshRuntimePort for FreshCompleteAgentTargetFixture {
        async fn execute(
            &self,
            saga: &CompanionFreshSaga,
            identity: &CompanionFreshOperationIdentity,
        ) -> Result<CompanionFreshEffectOutcome, String> {
            self.executed.lock().await.push(identity.clone());
            if identity.operation == CompanionFreshOperation::SubmitFirstInput {
                self.submitted_inputs.lock().await.push((
                    identity.effect_id,
                    saga.plan.first_submit_input.text.clone(),
                ));
            }
            let outcome = CompanionFreshEffectOutcome::Applied(fresh_evidence(saga));
            self.effects
                .lock()
                .await
                .insert(identity.clone(), outcome.clone());
            if self
                .lose_response_once
                .lock()
                .await
                .remove(&identity.operation)
            {
                return Err("effect applied but response was lost".to_owned());
            }
            Ok(outcome)
        }

        async fn inspect(
            &self,
            _saga: &CompanionFreshSaga,
            identity: &CompanionFreshOperationIdentity,
        ) -> Result<CompanionFreshEffectOutcome, String> {
            self.inspected.lock().await.push(identity.clone());
            self.effects
                .lock()
                .await
                .get(identity)
                .cloned()
                .ok_or_else(|| "stable fresh effect was not found".to_owned())
        }
    }

    #[test]
    fn compiled_fresh_context_maps_losslessly_to_create_and_reuses_effect_for_inspect() {
        let plan = fresh_plan();
        let CompanionRuntimePreparation::FreshCreate { initial_context } = &plan.preparation else {
            panic!("fresh context");
        };
        let identities = stable_fresh_identities();
        let mut saga = CompanionFreshSaga::requested(identities, plan.clone()).expect("fresh saga");
        let CompanionFreshStep::Dispatch(create_identity) = saga.next_step() else {
            panic!("create dispatch");
        };
        let command = map_create_command(&create_identity, initial_context);
        let canonical = command
            .initial_context
            .as_ref()
            .expect("CreateAgentCommand.initial_context");

        assert_eq!(
            serde_json::to_value(canonical).expect("canonical package"),
            serde_json::to_value(initial_context).expect("compiled package")
        );
        assert!(canonical.digest_matches());
        assert_eq!(
            canonical.package_id.as_str(),
            initial_context.package_id.to_string()
        );
        assert_eq!(canonical.schema_version.0, initial_context.schema_version);
        assert_eq!(
            canonical.contributions[0].kind(),
            service_api::InitialContextContributionKind::WorkflowContext
        );

        let canonical_evidence = service_api::AppliedInitialContextEvidence {
            package_id: canonical.package_id.clone(),
            package_digest: canonical.digest.clone(),
            fidelity: service_api::InitialContextDeliveryFidelity::TypedNative,
            renderer_version: None,
            materialized_digest: Some(
                service_api::AgentPayloadDigest::new("sha256:materialized")
                    .expect("materialized digest"),
            ),
        };
        assert_eq!(
            canonical_evidence.guarantee(),
            service_api::InitialContextAppliedEvidence::PackageAndMaterializedDigest
        );
        let mapped_evidence = map_applied_context(
            &canonical_evidence,
            &[(
                service_api::InitialContextContributionKind::WorkflowContext,
                service_api::InitialContextDeliveryFidelity::TypedNative,
            )],
        );
        verify_companion_activation(
            &plan,
            &CompanionRuntimePreparationEvidence::FreshCreate {
                child: RuntimeAgentChildIdentity {
                    source_coordinate: "service:child".to_owned(),
                    runtime_agent_id: "runtime-child".to_owned(),
                },
                context: Some(mapped_evidence),
            },
        )
        .expect("canonical evidence preserves Product activation contract");

        saga.mark_dispatched(create_identity.clone())
            .expect("durable create dispatch");
        let CompanionFreshStep::Inspect(inspect_identity) = saga.next_step() else {
            panic!("inspect create");
        };
        assert_eq!(inspect_identity, create_identity);
        assert_eq!(
            command.meta.effect_id.as_str(),
            inspect_identity.effect_id.to_string()
        );
        saga.record_outcome(
            inspect_identity.clone(),
            CompanionFreshEffectOutcome::Unknown,
        )
        .expect("unknown inspection");
        assert_eq!(
            saga.next_step(),
            CompanionFreshStep::Inspect(inspect_identity)
        );
    }

    #[test]
    fn full_is_an_exact_parent_history_fork() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "review this".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        assert!(matches!(
            plan.preparation,
            CompanionRuntimePreparation::ForkParentHistory {
                ref parent_source_coordinate,
                ref through_turn_id,
            } if parent_source_coordinate == "parent" && through_turn_id == "turn-9"
        ));
    }

    #[test]
    fn fresh_modes_compile_only_their_typed_package_contribution() {
        let cases = [
            (CompanionContextMode::Compact, "compact_summary"),
            (CompanionContextMode::WorkflowOnly, "workflow_context"),
            (CompanionContextMode::ConstraintsOnly, "constraint_set"),
        ];
        for (mode, expected_kind) in cases {
            let plan = compile_companion_dispatch_target(
                mode,
                CompanionAdoptionMode::BlockingReview,
                SubmitInput {
                    text: "task".to_owned(),
                },
                sources(),
            )
            .expect("plan");
            let CompanionRuntimePreparation::FreshCreate {
                ref initial_context,
            } = plan.preparation
            else {
                panic!("fresh create");
            };
            assert!(initial_context.digest_matches());
            assert_eq!(initial_context.contributions.len(), 1);
            let value =
                serde_json::to_value(&initial_context.contributions[0]).expect("contribution json");
            assert_eq!(value["kind"], expected_kind);
            assert!(value.get("surface_facts").is_none());
            let requirement = plan
                .context_application_requirement
                .as_ref()
                .expect("fresh requirement");
            assert_eq!(requirement.contribution_requirements.len(), 1);
            assert_eq!(requirement.contribution_requirements[0].kind, expected_kind);
            match mode {
                CompanionContextMode::Compact => {
                    assert_eq!(
                        requirement.contribution_requirements[0].minimum_fidelity,
                        CompiledContextDeliveryFidelity::CanonicalRendered
                    );
                    assert!(requirement.contribution_requirements[0].canonical_rendered_allowed);
                }
                CompanionContextMode::WorkflowOnly | CompanionContextMode::ConstraintsOnly => {
                    assert_eq!(
                        requirement.contribution_requirements[0].minimum_fidelity,
                        CompiledContextDeliveryFidelity::TypedNative
                    );
                    assert!(!requirement.contribution_requirements[0].canonical_rendered_allowed);
                }
                CompanionContextMode::Full => unreachable!(),
            }
        }
    }

    #[test]
    fn task_and_surface_facts_are_not_context_package_contributions() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Compact,
            CompanionAdoptionMode::FollowUpRequired,
            SubmitInput {
                text: "dispatch task".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        assert_eq!(plan.first_submit_input.text, "dispatch task");
        assert_eq!(plan.surface_facts["tools"], json!(["read"]));
        let CompanionRuntimePreparation::FreshCreate { initial_context } = plan.preparation else {
            panic!("fresh");
        };
        let package_json = serde_json::to_value(initial_context).expect("package json");
        assert!(!package_json.to_string().contains("dispatch task"));
        assert!(!package_json.to_string().contains("working_directory"));
    }

    #[test]
    fn fresh_create_activation_requires_exact_package_evidence() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::WorkflowOnly,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        let CompanionRuntimePreparation::FreshCreate { initial_context } = &plan.preparation else {
            panic!("fresh");
        };
        let child = RuntimeAgentChildIdentity {
            source_coordinate: "child".to_owned(),
            runtime_agent_id: "runtime-child".to_owned(),
        };
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: None,
                }
            ),
            Err(CompanionTargetPlanError::MissingContextEvidence)
        );
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: Some(CompiledContextApplication {
                        package_id: initial_context.package_id,
                        package_digest: initial_context.digest.clone(),
                        fidelity: CompiledContextDeliveryFidelity::Unsupported,
                        contribution_fidelity: vec![CompiledContextContributionApplication {
                            kind: "workflow_context".to_owned(),
                            fidelity: CompiledContextDeliveryFidelity::Unsupported,
                        },],
                        renderer_version: None,
                        materialized_digest: None,
                    }),
                }
            ),
            Err(CompanionTargetPlanError::UnsupportedContextFidelity)
        );
        verify_companion_activation(
            &plan,
            &CompanionRuntimePreparationEvidence::FreshCreate {
                child,
                context: Some(CompiledContextApplication {
                    package_id: initial_context.package_id,
                    package_digest: initial_context.digest.clone(),
                    fidelity: CompiledContextDeliveryFidelity::TypedNative,
                    contribution_fidelity: vec![CompiledContextContributionApplication {
                        kind: "workflow_context".to_owned(),
                        fidelity: CompiledContextDeliveryFidelity::TypedNative,
                    }],
                    renderer_version: None,
                    materialized_digest: Some("sha256:rendered".to_owned()),
                }),
            },
        )
        .expect("matching evidence");
    }

    #[test]
    fn adoption_mode_does_not_change_runtime_preparation() {
        let first = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("first");
        let second = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::BlockingReview,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("second");
        assert_eq!(first.preparation, second.preparation);
        assert_ne!(first.adoption_mode, second.adoption_mode);
    }

    #[test]
    fn fresh_persistence_contract_keeps_context_and_first_input_evidence_durable() {
        assert_eq!(
            COMPANION_FRESH_SAGA_SCHEMA_CONTRACT.table,
            "companion_fresh_saga"
        );
        assert_eq!(
            COMPANION_FRESH_SAGA_SCHEMA_CONTRACT.context_evidence,
            "context_application_evidence"
        );
        assert_eq!(
            COMPANION_FRESH_SAGA_SCHEMA_CONTRACT.first_input_receipt,
            "first_input_receipt"
        );
    }

    #[test]
    fn canonical_rendered_evidence_requires_renderer_and_exact_contributions() {
        let plan = fresh_plan_for(CompanionContextMode::Compact);
        let child = RuntimeAgentChildIdentity {
            source_coordinate: "fresh-child-source".to_owned(),
            runtime_agent_id: "fresh-runtime-child".to_owned(),
        };
        let mut evidence = applied_context(&plan);
        evidence.renderer_version = None;
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: Some(evidence),
                }
            ),
            Err(CompanionTargetPlanError::MissingRendererVersion)
        );

        let mut evidence = applied_context(&plan);
        evidence.contribution_fidelity[0].kind = "constraint_set".to_owned();
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child,
                    context: Some(evidence),
                }
            ),
            Err(CompanionTargetPlanError::ContributionFidelityMismatch)
        );
    }

    #[test]
    fn slice_policy_rejects_unapproved_or_below_minimum_rendered_fidelity() {
        let plan = fresh_plan();
        let child = RuntimeAgentChildIdentity {
            source_coordinate: "fresh-child-source".to_owned(),
            runtime_agent_id: "fresh-runtime-child".to_owned(),
        };
        let mut evidence = applied_context(&plan);
        evidence.fidelity = CompiledContextDeliveryFidelity::CanonicalRendered;
        evidence.contribution_fidelity[0].fidelity =
            CompiledContextDeliveryFidelity::CanonicalRendered;
        evidence.renderer_version = Some("context-renderer-v1".to_owned());
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: Some(evidence.clone()),
                }
            ),
            Err(CompanionTargetPlanError::CanonicalRenderedNotAllowed {
                kind: "workflow_context".to_owned(),
            })
        );

        let mut allow_but_require_typed = plan;
        allow_but_require_typed
            .context_application_requirement
            .as_mut()
            .expect("requirement")
            .contribution_requirements[0]
            .canonical_rendered_allowed = true;
        assert_eq!(
            verify_companion_activation(
                &allow_but_require_typed,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child,
                    context: Some(evidence),
                }
            ),
            Err(CompanionTargetPlanError::ContextFidelityBelowMinimum {
                kind: "workflow_context".to_owned(),
                minimum: CompiledContextDeliveryFidelity::TypedNative,
                actual: CompiledContextDeliveryFidelity::CanonicalRendered,
            })
        );
    }

    #[test]
    fn canonical_rendered_evidence_requires_materialized_digest() {
        let plan = fresh_plan_for(CompanionContextMode::Compact);
        let mut evidence = applied_context(&plan);
        evidence.materialized_digest = None;

        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: RuntimeAgentChildIdentity {
                        source_coordinate: "fresh-child-source".to_owned(),
                        runtime_agent_id: "fresh-runtime-child".to_owned(),
                    },
                    context: Some(evidence),
                }
            ),
            Err(CompanionTargetPlanError::MissingMaterializedDigest)
        );
    }

    #[tokio::test]
    async fn fresh_create_activation_and_first_input_are_durable_and_ordered() {
        let fork_repository = RecordingAgentRunForkSagaRepository::default();
        let repository = RecordingCompanionFreshSagaRepository::default();
        let coordinator = CompanionDispatchCoordinator::new(&fork_repository, &repository);
        let identities = stable_fresh_identities();
        let created = coordinator
            .materialize_fresh(identities.clone(), fresh_plan())
            .await
            .expect("materialize");
        let runtime = FreshCompleteAgentTargetFixture::default();
        let worker = CompanionFreshSagaWorker::new(&repository, &runtime);

        for expected in [
            CompanionFreshPhase::AgentCreated,
            CompanionFreshPhase::Activated,
            CompanionFreshPhase::FirstInputSubmitted,
            CompanionFreshPhase::Succeeded,
        ] {
            let advanced = worker
                .advance(&created.identities.request_id)
                .await
                .expect("advance");
            assert_eq!(advanced.phase(), expected);
            let encoded = serde_json::to_vec(&advanced).expect("serialize");
            let restarted: CompanionFreshSaga = serde_json::from_slice(&encoded).expect("restart");
            assert_eq!(restarted.phase(), expected);
        }

        let submitted = runtime.submitted_inputs.lock().await.clone();
        assert_eq!(
            submitted,
            vec![(
                identities.first_input_effect_id,
                "perform durable review".to_owned()
            )]
        );
        let succeeded = repository
            .load(&identities.request_id)
            .await
            .expect("load")
            .expect("saga");
        assert!(succeeded.context_evidence().is_some());
        assert!(succeeded.receipts().create.is_some());
        assert!(succeeded.receipts().activation.is_some());
        assert!(succeeded.receipts().first_input.is_some());
    }

    #[tokio::test]
    async fn fresh_effect_crashes_recover_by_inspection_and_submit_input_once() {
        let fork_repository = RecordingAgentRunForkSagaRepository::default();
        let repository = RecordingCompanionFreshSagaRepository::default();
        let coordinator = CompanionDispatchCoordinator::new(&fork_repository, &repository);
        let identities = stable_fresh_identities();
        coordinator
            .materialize_fresh(identities.clone(), fresh_plan())
            .await
            .expect("materialize");
        let runtime = FreshCompleteAgentTargetFixture::losing_responses([
            CompanionFreshOperation::CreateWithContextPackage,
            CompanionFreshOperation::Activate,
            CompanionFreshOperation::SubmitFirstInput,
        ]);
        let worker = CompanionFreshSagaWorker::new(&repository, &runtime);

        for _ in 0..7 {
            let _ = worker.advance(&identities.request_id).await;
        }
        let succeeded = repository
            .load(&identities.request_id)
            .await
            .expect("load")
            .expect("saga");
        assert_eq!(succeeded.phase(), CompanionFreshPhase::Succeeded);
        assert_eq!(runtime.executed.lock().await.len(), 3);
        assert_eq!(runtime.inspected.lock().await.len(), 3);
        assert_eq!(
            runtime.submitted_inputs.lock().await.as_slice(),
            &[(
                identities.first_input_effect_id,
                "perform durable review".to_owned()
            )]
        );
    }

    struct ExactForkTargetFixture;

    #[async_trait]
    impl AgentRunForkRuntimePort for ExactForkTargetFixture {
        async fn execute(
            &self,
            saga: &AgentRunForkSaga,
            _identity: &AgentRunForkOperationIdentity,
        ) -> Result<RuntimeOperationOutcome, String> {
            let child = (saga.phase() != AgentRunForkSagaPhase::Requested).then(|| {
                saga.runtime_child()
                    .cloned()
                    .unwrap_or(RuntimeAgentChildIdentity {
                        source_coordinate: "exact-child-source".to_owned(),
                        runtime_agent_id: "exact-runtime-child".to_owned(),
                    })
            });
            Ok(RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence {
                child,
                host_binding: matches!(
                    saga.phase(),
                    AgentRunForkSagaPhase::AgentForkApplied
                        | AgentRunForkSagaPhase::RuntimeProvisioned
                        | AgentRunForkSagaPhase::ProductGraphCommitted
                )
                .then(|| "exact-host-binding".to_owned()),
                child_history_digest: (saga.phase() != AgentRunForkSagaPhase::Requested)
                    .then(|| "sha256:exact-child-history".to_owned()),
                context: None,
                receipt: format!("complete-agent-{:?}", saga.phase()),
            }))
        }

        async fn inspect(
            &self,
            saga: &AgentRunForkSaga,
            identity: &AgentRunForkOperationIdentity,
        ) -> Result<RuntimeOperationOutcome, String> {
            self.execute(saga, identity).await
        }
    }

    struct ExactProductGraph;

    #[async_trait]
    impl AgentRunForkProductGraphPort for ExactProductGraph {
        async fn prepare_child_graph_commit(
            &self,
            saga: &AgentRunForkSaga,
        ) -> Result<PreparedAgentRunForkGraph, String> {
            let project_id = Uuid::new_v4();
            let mut child_run = LifecycleRun::new_plain(project_id);
            child_run.id = saga.child().run_id;
            let mut child_agent =
                LifecycleAgent::new_root(saga.child().run_id, project_id, AgentSource::Subagent);
            child_agent.id = saga.child().agent_id;
            let mut child_frame =
                AgentFrame::new_revision(saga.child().agent_id, 1, "companion_full_fork");
            child_frame.id = saga.child().frame_id;
            let lineage = AgentRunLineage::new_fork(
                saga.parent().run_id,
                saga.parent().agent_id,
                saga.child().run_id,
                saga.child().agent_id,
                None,
                None,
                "tester",
                None,
            )
            .with_frame_baseline(Uuid::new_v4(), 1, saga.child().frame_id, 1);
            PreparedAgentRunForkGraph::prepare(
                saga,
                AgentRunForkGraph {
                    child_run,
                    child_agent,
                    child_frame,
                    lineage,
                },
            )
            .map_err(|error| error.to_string())
        }
    }

    #[tokio::test]
    async fn full_materializes_and_reuses_the_exact_fork_saga_flow() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "review full history".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        let request = CompanionFullForkRequest {
            request_id: AgentRunForkRequestId(Uuid::new_v4()),
            parent: AgentRunForkParent {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                source_coordinate: "parent".to_owned(),
                through_turn_id: "turn-9".to_owned(),
            },
            child: PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                presentation_thread_id: "full-child-thread".to_owned(),
            },
        };
        let fork_repository = RecordingAgentRunForkSagaRepository::default();
        let fresh_repository = RecordingCompanionFreshSagaRepository::default();
        let coordinator = CompanionDispatchCoordinator::new(&fork_repository, &fresh_repository);
        let created = coordinator
            .materialize_full_fork(&plan, request.clone())
            .await
            .expect("create fork saga");
        let reused = coordinator
            .materialize_full_fork(&plan, request)
            .await
            .expect("reuse fork saga");
        assert_eq!(created.request_id(), reused.request_id());
        assert_eq!(created.parent().through_turn_id, "turn-9");

        let worker = AgentRunForkSagaWorker::new(
            &fork_repository,
            &ExactForkTargetFixture,
            &ExactProductGraph,
        );
        for _ in 0..6 {
            worker
                .advance(created.request_id())
                .await
                .expect("advance exact fork");
        }
        let succeeded = fork_repository
            .load(created.request_id())
            .await
            .expect("load")
            .expect("saga");
        assert_eq!(succeeded.phase(), AgentRunForkSagaPhase::Succeeded);
        assert_eq!(
            succeeded.child_history_digest(),
            Some("sha256:exact-child-history")
        );
        assert!(succeeded.receipts().agent_fork.is_some());
    }
}
