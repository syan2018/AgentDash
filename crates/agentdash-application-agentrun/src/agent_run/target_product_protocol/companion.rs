use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    AgentRunForkParent, AgentRunForkRequestId, AgentRunForkSaga, AgentRunForkSagaRepository,
    AgentRunForkSagaRepositoryError, InitialContextApplicationEvidence,
    InitialContextDeliveryFidelity, PreallocatedAgentRunChild, RequiredInitialContextEvidence,
    RuntimeAgentChildIdentity,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextContributionProvenance {
    pub authority: String,
    pub source_coordinate: String,
    pub source_revision: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InitialAgentContextContribution {
    CompactSummary {
        summary: String,
        provenance: ContextContributionProvenance,
    },
    WorkflowContext {
        schema: String,
        value: Value,
        provenance: ContextContributionProvenance,
    },
    ConstraintSet {
        schema: String,
        value: Value,
        provenance: ContextContributionProvenance,
    },
}

impl InitialAgentContextContribution {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::CompactSummary { .. } => "compact_summary",
            Self::WorkflowContext { .. } => "workflow_context",
            Self::ConstraintSet { .. } => "constraint_set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitialAgentContextPackage {
    pub package_id: Uuid,
    pub schema_version: u32,
    pub mode: CompanionContextMode,
    pub contributions: Vec<InitialAgentContextContribution>,
    pub digest: String,
}

impl InitialAgentContextPackage {
    fn calculate_digest(
        package_id: Uuid,
        schema_version: u32,
        mode: CompanionContextMode,
        contributions: &[InitialAgentContextContribution],
    ) -> String {
        let canonical = serde_json::to_vec(&(package_id, schema_version, mode, contributions))
            .expect("typed initial context package must serialize");
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
        initial_context: InitialAgentContextPackage,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionDispatchTargetPlan {
    pub preparation: CompanionRuntimePreparation,
    pub adoption_mode: CompanionAdoptionMode,
    pub first_submit_input: SubmitInput,
    /// Business Surface facts remain a separate target input. They are not
    /// serialized into `InitialAgentContextPackage`.
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionContextSources {
    pub parent_source_coordinate: String,
    pub through_turn_id: Option<String>,
    pub package_id: Uuid,
    pub compact_summary: Option<(String, ContextContributionProvenance)>,
    pub workflow: Option<(String, Value, ContextContributionProvenance)>,
    pub constraints: Option<(String, Value, ContextContributionProvenance)>,
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionTargetPlanError {
    #[error("Full companion context requires an exact parent turn cutoff")]
    MissingParentTurnCutoff,
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
    #[error("applied initial context evidence does not cover each typed contribution exactly once")]
    ContributionFidelityMismatch,
    #[error("CanonicalRendered context evidence requires a renderer version")]
    MissingRendererVersion,
    #[error("Full history fork evidence does not match the exact parent history request")]
    ForkHistoryEvidenceMismatch,
}

pub fn compile_companion_dispatch_target(
    mode: CompanionContextMode,
    adoption_mode: CompanionAdoptionMode,
    task: SubmitInput,
    sources: CompanionContextSources,
) -> Result<CompanionDispatchTargetPlan, CompanionTargetPlanError> {
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
                        vec![InitialAgentContextContribution::CompactSummary {
                            summary,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::WorkflowOnly => sources
                    .workflow
                    .map(|(schema, value, provenance)| {
                        vec![InitialAgentContextContribution::WorkflowContext {
                            schema,
                            value,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::ConstraintsOnly => sources
                    .constraints
                    .map(|(schema, value, provenance)| {
                        vec![InitialAgentContextContribution::ConstraintSet {
                            schema,
                            value,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::Full => unreachable!(),
            };
            if contributions.is_empty() {
                return Err(CompanionTargetPlanError::MissingTypedContribution { mode });
            }
            let schema_version = 1;
            let digest = InitialAgentContextPackage::calculate_digest(
                sources.package_id,
                schema_version,
                mode,
                &contributions,
            );
            CompanionRuntimePreparation::FreshCreate {
                initial_context: InitialAgentContextPackage {
                    package_id: sources.package_id,
                    schema_version,
                    mode,
                    contributions,
                    digest,
                },
            }
        }
    };
    Ok(CompanionDispatchTargetPlan {
        preparation,
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
        context: Option<InitialContextApplicationEvidence>,
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
            if actual.fidelity == InitialContextDeliveryFidelity::Unsupported {
                return Err(CompanionTargetPlanError::UnsupportedContextFidelity);
            }
            let expected = initial_context
                .contributions
                .iter()
                .map(InitialAgentContextContribution::kind_name)
                .collect::<Vec<_>>();
            let actual_kinds = actual
                .contribution_fidelity
                .iter()
                .map(|contribution| contribution.kind.as_str())
                .collect::<Vec<_>>();
            if expected != actual_kinds
                || actual.contribution_fidelity.iter().any(|contribution| {
                    contribution.fidelity == InitialContextDeliveryFidelity::Unsupported
                })
            {
                return Err(CompanionTargetPlanError::ContributionFidelityMismatch);
            }
            if (actual.fidelity == InitialContextDeliveryFidelity::CanonicalRendered
                || actual.contribution_fidelity.iter().any(|contribution| {
                    contribution.fidelity == InitialContextDeliveryFidelity::CanonicalRendered
                }))
                && actual.renderer_version.as_deref().is_none_or(str::is_empty)
            {
                return Err(CompanionTargetPlanError::MissingRendererVersion);
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
        context: InitialContextApplicationEvidence,
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
    context_evidence: Option<InitialContextApplicationEvidence>,
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

    pub fn context_evidence(&self) -> Option<&InitialContextApplicationEvidence> {
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

#[derive(Default)]
pub struct InMemoryCompanionFreshSagaRepository {
    sagas: Arc<Mutex<HashMap<CompanionFreshRequestId, CompanionFreshSaga>>>,
}

#[async_trait]
impl CompanionFreshSagaRepository for InMemoryCompanionFreshSagaRepository {
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

    use crate::agent_run::target_product_protocol::{
        AgentRunForkOperationIdentity, AgentRunForkProductGraphPort, AgentRunForkRuntimePort,
        AgentRunForkSagaPhase, AgentRunForkSagaWorker, InMemoryAgentRunForkSagaRepository,
        InitialContextContributionApplicationEvidence, ProductGraphCommitEvidence,
        RuntimeForkPhaseEvidence, RuntimeOperationOutcome,
    };
    use serde_json::json;

    use super::*;

    fn provenance(authority: &str) -> ContextContributionProvenance {
        ContextContributionProvenance {
            authority: authority.to_owned(),
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
        compile_companion_dispatch_target(
            CompanionContextMode::WorkflowOnly,
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

    fn applied_context(plan: &CompanionDispatchTargetPlan) -> InitialContextApplicationEvidence {
        let CompanionRuntimePreparation::FreshCreate { initial_context } = &plan.preparation else {
            panic!("fresh");
        };
        InitialContextApplicationEvidence {
            package_id: initial_context.package_id,
            package_digest: initial_context.digest.clone(),
            fidelity: InitialContextDeliveryFidelity::CanonicalRendered,
            contribution_fidelity: initial_context
                .contributions
                .iter()
                .map(
                    |contribution| InitialContextContributionApplicationEvidence {
                        kind: contribution.kind_name().to_owned(),
                        fidelity: InitialContextDeliveryFidelity::CanonicalRendered,
                    },
                )
                .collect(),
            renderer_version: Some("context-renderer-v1".to_owned()),
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
            let CompanionRuntimePreparation::FreshCreate { initial_context } = plan.preparation
            else {
                panic!("fresh create");
            };
            assert!(initial_context.digest_matches());
            assert_eq!(initial_context.contributions.len(), 1);
            let value =
                serde_json::to_value(&initial_context.contributions[0]).expect("contribution json");
            assert_eq!(value["kind"], expected_kind);
            assert!(value.get("surface_facts").is_none());
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
                    context: Some(InitialContextApplicationEvidence {
                        package_id: initial_context.package_id,
                        package_digest: initial_context.digest.clone(),
                        fidelity: InitialContextDeliveryFidelity::Unsupported,
                        contribution_fidelity: vec![
                            InitialContextContributionApplicationEvidence {
                                kind: "workflow_context".to_owned(),
                                fidelity: InitialContextDeliveryFidelity::Unsupported,
                            },
                        ],
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
                context: Some(InitialContextApplicationEvidence {
                    package_id: initial_context.package_id,
                    package_digest: initial_context.digest.clone(),
                    fidelity: InitialContextDeliveryFidelity::TypedNative,
                    contribution_fidelity: vec![InitialContextContributionApplicationEvidence {
                        kind: "workflow_context".to_owned(),
                        fidelity: InitialContextDeliveryFidelity::TypedNative,
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
    fn canonical_rendered_evidence_requires_renderer_and_exact_contributions() {
        let plan = fresh_plan();
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

    #[tokio::test]
    async fn fresh_create_activation_and_first_input_are_durable_and_ordered() {
        let fork_repository = InMemoryAgentRunForkSagaRepository::default();
        let repository = InMemoryCompanionFreshSagaRepository::default();
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
        let fork_repository = InMemoryAgentRunForkSagaRepository::default();
        let repository = InMemoryCompanionFreshSagaRepository::default();
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
        async fn commit_child_graph(
            &self,
            saga: &AgentRunForkSaga,
        ) -> Result<ProductGraphCommitEvidence, String> {
            Ok(ProductGraphCommitEvidence {
                agent_run_id: saga.child().agent_run_id,
                child_run_id: saga.child().run_id,
                child_agent_id: saga.child().agent_id,
                child_frame_id: saga.child().frame_id,
                presentation_thread_id: saga.child().presentation_thread_id.clone(),
                runtime_child: saga.runtime_child().cloned().expect("child"),
                host_binding: saga.host_binding().expect("binding").to_owned(),
                child_history_digest: saga
                    .child_history_digest()
                    .expect("history digest")
                    .to_owned(),
                commit_revision: 1,
            })
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
        let fork_repository = InMemoryAgentRunForkSagaRepository::default();
        let fresh_repository = InMemoryCompanionFreshSagaRepository::default();
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
