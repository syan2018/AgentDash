use std::sync::Arc;

use crate::{
    CompleteAgentCallbackRoute, CompleteAgentHostCommit, CompleteAgentHostFacts,
    CompleteAgentHostRepository, CompleteAgentHostSnapshot, CompleteAgentHostStoreError,
    CompleteAgentLiveCatalogError, CompleteAgentLiveSelection, CompleteAgentRemoteBindingFact,
    CompleteAgentServiceVerification, SharedCompleteAgentHostRepository,
    SharedCompleteAgentLiveCatalog,
};
use agentdash_agent_runtime::{
    ManagedRuntimeAgentBinding, ManagedRuntimeCreateOutcome, ManagedRuntimeDispatchContext,
    ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError, ManagedRuntimeLifecycleInspection,
    ManagedRuntimeLifecyclePort, ManagedRuntimeRebindOutcome, ManagedRuntimeResumeOutcome,
    ManagedRuntimeStateRepository, bind_complete_agent_surface, production_managed_runtime_gateway,
};
use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeGatewayError, RuntimeThreadId,
};
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentBindingGeneration, AgentCallbackRouteId, AgentChangePage,
    AgentChangesQuery, AgentCommandEnvelope, AgentCommandId, AgentCommandMeta, AgentCommandReceipt,
    AgentEffectIdentity, AgentEffectInspection, AgentEffectInspectionState, AgentForkPoint,
    AgentHostCallbackBinding, AgentIdempotencyKey, AgentPayloadDigest, AgentProfileDigest,
    AgentReadQuery, AgentReceiptState, AgentRuntimeOffer, AgentServiceDefinitionId,
    AgentServiceDescriptor, AgentServiceError, AgentServiceInstanceId, AgentSourceCoordinate,
    AgentSurfaceProfile, AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentSurfaceSnapshot,
    AppliedAgentCommandReceipt, AppliedAgentSurface, AppliedAgentSurfaceReceipt,
    AppliedForkAgentReceipt, ApplyBoundAgentSurface, BoundAgentSurface,
    CompleteAgentLiveAttachmentId, CompleteAgentService, CreateAgentCommand, ForkAgentCommand,
    ForkAgentReceipt, InitialAgentContextPackage, ResumeAgentCommand, RevokeBoundAgentSurface,
    SemanticFidelity,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompleteAgentBindingId(String);

impl CompleteAgentBindingId {
    pub fn new(value: impl Into<String>) -> Result<Self, CompleteAgentHostError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Complete Agent binding id must not be empty".to_owned(),
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentBindingTarget {
    pub logical_instance_id: AgentServiceInstanceId,
    pub live_attachment_id: CompleteAgentLiveAttachmentId,
    pub definition_id: AgentServiceDefinitionId,
    pub verified_build_digest: AgentPayloadDigest,
    pub verified_profile_digest: AgentProfileDigest,
    pub offer_profile_digest: AgentProfileDigest,
    pub placement: CompleteAgentPlacement,
    pub remote_binding: Option<CompleteAgentRemoteBindingFact>,
}

impl CompleteAgentBindingTarget {
    pub fn host_incarnation_id(&self) -> &str {
        self.placement.host_incarnation_id()
    }

    pub(crate) fn is_valid(&self) -> bool {
        !self.logical_instance_id.as_str().trim().is_empty()
            && !self.live_attachment_id.as_str().trim().is_empty()
            && !self.definition_id.as_str().trim().is_empty()
            && !self.verified_build_digest.as_str().trim().is_empty()
            && !self.verified_profile_digest.as_str().trim().is_empty()
            && !self.offer_profile_digest.as_str().trim().is_empty()
            && self.placement.is_valid()
            && self.verified_profile_digest == self.offer_profile_digest
            && match (&self.placement, &self.remote_binding) {
                (
                    CompleteAgentPlacement::Remote {
                        transport_id,
                        host_incarnation_id,
                        ..
                    },
                    Some(remote),
                ) => {
                    remote.local_service_instance_id == self.logical_instance_id
                        && !remote.remote_service_instance_id.as_str().trim().is_empty()
                        && remote.remote_binding_generation.0 > 0
                        && remote.host_incarnation_id == *host_incarnation_id
                        && remote.transport_id == *transport_id
                }
                (CompleteAgentPlacement::Remote { .. }, None) => false,
                (_, None) => true,
                (_, Some(_)) => false,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentBinding {
    pub id: CompleteAgentBindingId,
    pub target: CompleteAgentBindingTarget,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface: BoundAgentSurface,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub state: CompleteAgentBindingState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTarget {
    pub runtime_thread_id: RuntimeThreadId,
    pub target: CompleteAgentBindingTarget,
    pub generation: AgentBindingGeneration,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface: BoundAgentSurface,
    pub callbacks: AgentHostCallbackBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTargetProvisioning {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub target: CompleteAgentRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRuntimeTargetProvisioningRequest {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub runtime_thread_id: RuntimeThreadId,
    pub target: CompleteAgentBindingTarget,
    pub desired_surface: AgentSurfaceSnapshot,
    pub callback_deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTargetRecovery {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub previous_target: CompleteAgentRuntimeTarget,
    pub recovered_target: CompleteAgentRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRuntimeTargetRecoveryRequest {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub runtime_thread_id: RuntimeThreadId,
    pub expected_generation: AgentBindingGeneration,
    pub target: CompleteAgentBindingTarget,
    pub desired_surface: AgentSurfaceSnapshot,
    pub callback_deadline_ms: u64,
}

#[async_trait]
pub trait CompleteAgentRuntimeRecoveryPlanner: Send + Sync {
    async fn plan_recovery(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        previous_target: &CompleteAgentRuntimeTarget,
        previous_binding: &ManagedRuntimeAgentBinding,
        effect_id: &AgentEffectIdentity,
    ) -> Result<CompleteAgentRuntimeTargetRecoveryRequest, CompleteAgentHostError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerifiedServiceRegistration {
    pub instance_id: AgentServiceInstanceId,
    pub descriptor: AgentServiceDescriptor,
    pub placement: CompleteAgentPlacement,
    pub verification: CompleteAgentServiceVerification,
    pub remote_binding: Option<CompleteAgentRemoteBindingFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentLifecycleOperationKind {
    Create,
    Resume,
    Rebind,
    Fork,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompleteAgentLifecycleOutcome {
    Agent {
        receipt: AgentCommandReceipt,
        applied_surface: Option<AppliedAgentSurface>,
    },
    Fork {
        receipt: ForkAgentReceipt,
        child_applied_surface: Option<AppliedAgentSurface>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "receipt", rename_all = "snake_case")]
pub enum CompleteAgentLifecycleAppliedReceipt {
    Agent(AppliedAgentCommandReceipt),
    Fork(AppliedForkAgentReceipt),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentLifecycleEffectRecord {
    pub effect_id: AgentEffectIdentity,
    pub runtime_thread_id: RuntimeThreadId,
    pub child_thread_id: Option<RuntimeThreadId>,
    pub kind: CompleteAgentLifecycleOperationKind,
    pub target: CompleteAgentBindingTarget,
    pub generation: AgentBindingGeneration,
    pub initial_context: Option<InitialAgentContextPackage>,
    pub fork_cutoff: Option<AgentForkPoint>,
    pub applied_receipt: Option<CompleteAgentLifecycleAppliedReceipt>,
    pub outcome: Option<CompleteAgentLifecycleOutcome>,
}

enum CompleteAgentLifecycleBegin {
    Dispatch,
    InspectionRequired,
    Settled(Box<CompleteAgentLifecycleOutcome>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompleteAgentPlacement {
    InProcess {
        host_incarnation_id: String,
    },
    LocalProcess {
        host_id: String,
        host_incarnation_id: String,
    },
    Remote {
        host_id: String,
        transport_id: String,
        host_incarnation_id: String,
    },
}

impl CompleteAgentPlacement {
    pub fn host_incarnation_id(&self) -> &str {
        match self {
            Self::InProcess {
                host_incarnation_id,
            }
            | Self::LocalProcess {
                host_incarnation_id,
                ..
            }
            | Self::Remote {
                host_incarnation_id,
                ..
            } => host_incarnation_id,
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::InProcess {
                host_incarnation_id,
            } => !host_incarnation_id.trim().is_empty(),
            Self::LocalProcess {
                host_id,
                host_incarnation_id,
            } => !host_id.trim().is_empty() && !host_incarnation_id.trim().is_empty(),
            Self::Remote {
                host_id,
                transport_id,
                host_incarnation_id,
            } => {
                !host_id.trim().is_empty()
                    && !transport_id.trim().is_empty()
                    && !host_incarnation_id.trim().is_empty()
            }
        }
    }
}

fn validate_binding_target(
    target: &CompleteAgentBindingTarget,
) -> Result<(), CompleteAgentHostError> {
    if !target.is_valid() {
        return Err(CompleteAgentHostError::Invariant {
            reason: "Complete Agent binding target snapshot is invalid".to_owned(),
        });
    }
    Ok(())
}

impl CompleteAgentBinding {
    pub fn dispatch_admitted(&self) -> bool {
        self.state == CompleteAgentBindingState::Available
            && self
                .applied_surface
                .as_ref()
                .is_some_and(|applied| self.bound_surface.accepts_applied(applied))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentBindingState {
    PendingSurface,
    Available,
    Desynchronized,
    Lost,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentBindingLease {
    pub binding_id: CompleteAgentBindingId,
    pub generation: AgentBindingGeneration,
    pub owner: String,
    pub token: String,
    pub epoch: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentEffectState {
    Dispatching,
    Accepted,
    Applied,
    Rejected,
    NotApplied,
    Unknown,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentEffectAttemptEvidence {
    pub dispatch_attempt: u64,
    pub delivery_epoch: u64,
    pub state: CompleteAgentEffectState,
    pub receipt: Option<AgentCommandReceipt>,
    pub surface_receipt: Option<AppliedAgentSurfaceReceipt>,
    pub inspection: Option<AgentEffectInspection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentEffectRecord {
    pub effect_id: AgentEffectIdentity,
    pub command_id: AgentCommandId,
    pub binding_id: CompleteAgentBindingId,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub payload_digest: AgentPayloadDigest,
    pub delivery_epoch: u64,
    pub dispatch_attempt: u64,
    pub state: CompleteAgentEffectState,
    pub receipt: Option<AgentCommandReceipt>,
    pub surface_receipt: Option<AppliedAgentSurfaceReceipt>,
    pub inspection: Option<AgentEffectInspection>,
    pub attempt_history: Vec<CompleteAgentEffectAttemptEvidence>,
}

enum RevokeDispatchPlan {
    Dispatch,
    Inspect,
    Settled(Box<AgentCommandReceipt>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentHostError {
    #[error("Complete Agent live attachment is unavailable: {attachment_id}")]
    UnavailableAttachment {
        attachment_id: CompleteAgentLiveAttachmentId,
    },
    #[error("Complete Agent binding was not found: {binding_id}")]
    UnknownBinding { binding_id: String },
    #[error("Complete Agent effect was not found: {effect_id}")]
    UnknownEffect { effect_id: AgentEffectIdentity },
    #[error("Complete Agent generation is stale: expected {expected:?}, actual {actual:?}")]
    StaleGeneration {
        expected: AgentBindingGeneration,
        actual: AgentBindingGeneration,
    },
    #[error("Complete Agent effect identity was reused with different coordinates or payload")]
    EffectIdentityConflict,
    #[error(
        "Complete Agent effect observation conflicts with monotonic state: current {current:?}, observed {observed:?}"
    )]
    EffectObservationConflict {
        current: CompleteAgentEffectState,
        observed: CompleteAgentEffectState,
    },
    #[error("Complete Agent effect returned conflicting evidence for {effect_id}")]
    EffectEvidenceConflict { effect_id: AgentEffectIdentity },
    #[error("Complete Agent effect is confirmed not applied: {effect_id}")]
    EffectNotApplied { effect_id: AgentEffectIdentity },
    #[error("Complete Agent effect still requires inspection: {effect_id}")]
    EffectPending { effect_id: AgentEffectIdentity },
    #[error("Complete Agent binding lease is held by another owner or is stale")]
    LeaseConflict,
    #[error("Complete Agent binding lease has expired")]
    LeaseExpired,
    #[error("Complete Agent late outcome was fenced by a newer lease epoch")]
    StaleLeaseOutcome,
    #[error("Complete Agent binding is not dispatchable: {reason}")]
    DispatchRejected { reason: String },
    #[error("Complete Agent Runtime target provisioning conflicts with durable facts")]
    ProvisioningConflict,
    #[error("Complete Agent host invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("Complete Agent payload cannot be encoded: {reason}")]
    Encoding { reason: String },
    #[error(transparent)]
    Store(#[from] CompleteAgentHostStoreError),
    #[error(transparent)]
    LiveCatalog(#[from] CompleteAgentLiveCatalogError),
    #[error(transparent)]
    Service(#[from] AgentServiceError),
}

/// Final Complete-Agent Host boundary for verified service registration, binding/generation
/// fencing, and stable effect reconciliation.
pub struct CompleteAgentHost {
    repository: SharedCompleteAgentHostRepository,
    live_catalog: SharedCompleteAgentLiveCatalog,
    recovery_planner: RwLock<Option<Arc<dyn CompleteAgentRuntimeRecoveryPlanner>>>,
}

impl CompleteAgentHost {
    pub fn new(
        repository: Arc<dyn CompleteAgentHostRepository>,
        live_catalog: SharedCompleteAgentLiveCatalog,
    ) -> Self {
        Self {
            repository,
            live_catalog,
            recovery_planner: RwLock::new(None),
        }
    }

    pub async fn install_runtime_recovery_planner(
        &self,
        planner: Arc<dyn CompleteAgentRuntimeRecoveryPlanner>,
    ) {
        *self.recovery_planner.write().await = Some(planner);
    }

    pub async fn attach_verified_service(
        &self,
        registration: CompleteAgentVerifiedServiceRegistration,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentHostError> {
        Ok(self.live_catalog.attach(registration, service).await?)
    }

    pub async fn register_binding(
        &self,
        binding: CompleteAgentBinding,
    ) -> Result<(), CompleteAgentHostError> {
        if binding.generation.0 == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Complete Agent binding generation must be positive".to_owned(),
            });
        }
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        validate_binding_target(&binding.target)?;
        if binding.profile_digest != binding.target.offer_profile_digest
            || binding.bound_surface.offer_profile_digest != binding.target.offer_profile_digest
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "binding profile does not match its exact live target snapshot".to_owned(),
            });
        }
        let surface_matches = binding
            .applied_surface
            .as_ref()
            .is_some_and(|applied| binding.bound_surface.accepts_applied(applied));
        if (binding.state == CompleteAgentBindingState::Available && !surface_matches)
            || (binding.state == CompleteAgentBindingState::PendingSurface
                && binding.applied_surface.is_some())
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "binding state does not match its applied surface evidence".to_owned(),
            });
        }
        if let Some(existing) = facts.bindings.get(&binding.id) {
            if existing == &binding {
                return Ok(());
            }
            return Err(CompleteAgentHostError::Invariant {
                reason: "binding id is already reserved with different coordinates".to_owned(),
            });
        }
        if facts.source_coordinates.iter().any(|(binding_id, source)| {
            source == &binding.source
                && facts.bindings.get(binding_id).is_some_and(|existing| {
                    !matches!(
                        existing.state,
                        CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
                    )
                })
        }) {
            return Err(CompleteAgentHostError::Invariant {
                reason: "source coordinate already has a nonterminal binding".to_owned(),
            });
        }
        facts
            .source_coordinates
            .insert(binding.id.clone(), binding.source.clone());
        facts.bindings.insert(binding.id.clone(), binding);
        self.commit(snapshot.revision, facts).await?;
        Ok(())
    }

    pub async fn register_runtime_target(
        &self,
        target: CompleteAgentRuntimeTarget,
    ) -> Result<(), CompleteAgentHostError> {
        if target.generation.0 == 0 || target.callbacks.binding_generation != target.generation {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime target requires one positive matching generation".to_owned(),
            });
        }
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        validate_binding_target(&target.target)?;
        if target.target.offer_profile_digest != target.profile_digest
            || target.bound_surface.offer_profile_digest != target.target.offer_profile_digest
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Runtime target does not match its exact live target snapshot".to_owned(),
            });
        }
        if let Some(existing) = facts.runtime_targets.get(&target.runtime_thread_id) {
            if existing == &target {
                return Ok(());
            }
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime thread target is already registered differently".to_owned(),
            });
        }
        facts
            .runtime_targets
            .insert(target.runtime_thread_id.clone(), target);
        self.commit(snapshot.revision, facts).await?;
        Ok(())
    }

    /// Atomically compiles and registers one immutable Runtime target.
    ///
    /// Selection is supplied by the composition root, while the Host exclusively owns offer
    /// intersection, binding generation, callback route identity, and durable idempotency.
    pub async fn provision_runtime_target(
        &self,
        request: CompleteAgentRuntimeTargetProvisioningRequest,
    ) -> Result<CompleteAgentRuntimeTargetProvisioning, CompleteAgentHostError> {
        if request.callback_deadline_ms == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime target callback deadline must be positive".to_owned(),
            });
        }
        let selection = self.live_selection(&request.target).await?;
        loop {
            let snapshot = self.repository.load().await?;
            if let Some(existing) = snapshot
                .facts
                .runtime_target_provisionings
                .get(&request.idempotency_key)
            {
                if existing.request_digest == request.request_digest
                    && existing.target.runtime_thread_id == request.runtime_thread_id
                    && existing.target.target == request.target
                {
                    return Ok(existing.clone());
                }
                return Err(CompleteAgentHostError::ProvisioningConflict);
            }
            let bound_surface =
                bind_complete_agent_surface(&request.desired_surface, &selection.offer).map_err(
                    |error| CompleteAgentHostError::DispatchRejected {
                        reason: error.to_string(),
                    },
                )?;
            let generation = AgentBindingGeneration(1);
            let callback_route = callback_route_id(
                &request.runtime_thread_id,
                &request.target,
                generation,
                &bound_surface,
            )?;
            let target = CompleteAgentRuntimeTarget {
                runtime_thread_id: request.runtime_thread_id.clone(),
                target: request.target.clone(),
                generation,
                profile_digest: selection.offer.profile_digest.clone(),
                bound_surface,
                callbacks: AgentHostCallbackBinding {
                    route_id: callback_route,
                    binding_generation: generation,
                    delivery: AgentSurfaceRoute::AgentNativeCallback,
                    default_deadline_ms: request.callback_deadline_ms,
                },
            };
            if let Some(existing) = snapshot
                .facts
                .runtime_targets
                .get(&request.runtime_thread_id)
                && existing != &target
            {
                return Err(CompleteAgentHostError::ProvisioningConflict);
            }
            let provisioning = CompleteAgentRuntimeTargetProvisioning {
                idempotency_key: request.idempotency_key.clone(),
                request_digest: request.request_digest.clone(),
                target: target.clone(),
            };
            let mut facts = snapshot.facts;
            facts
                .runtime_targets
                .entry(request.runtime_thread_id.clone())
                .or_insert(target);
            facts
                .runtime_target_provisionings
                .insert(request.idempotency_key.clone(), provisioning.clone());
            match self
                .repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: snapshot.revision,
                    facts,
                })
                .await
            {
                Ok(_) => return Ok(provisioning),
                Err(CompleteAgentHostStoreError::Conflict { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Atomically prepares an explicit surface replacement for an active Runtime thread.
    ///
    /// The old generation is fenced in the same durable commit that installs the replacement
    /// target. Managed Runtime can therefore replay one stable Rebind operation after process
    /// loss without ever observing a new surface on the old generation.
    pub async fn prepare_runtime_surface_rebind(
        &self,
        request: CompleteAgentRuntimeTargetRecoveryRequest,
    ) -> Result<CompleteAgentRuntimeTargetRecovery, CompleteAgentHostError> {
        if request.callback_deadline_ms == 0 || request.expected_generation.0 == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime surface rebind requires positive generation and callback deadline"
                    .to_owned(),
            });
        }
        let selection = self.live_selection(&request.target).await?;
        loop {
            let snapshot = self.repository.load().await?;
            if let Some(existing) = snapshot
                .facts
                .runtime_target_recoveries
                .get(&request.idempotency_key)
            {
                if existing.request_digest == request.request_digest
                    && existing.previous_target.runtime_thread_id == request.runtime_thread_id
                    && existing.recovered_target.target == request.target
                {
                    return Ok(existing.clone());
                }
                return Err(CompleteAgentHostError::ProvisioningConflict);
            }
            let previous_target = snapshot
                .facts
                .runtime_targets
                .get(&request.runtime_thread_id)
                .cloned()
                .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                    reason: format!(
                        "Runtime target {} is not registered",
                        request.runtime_thread_id
                    ),
                })?;
            if previous_target.generation != request.expected_generation
                || previous_target.target != request.target
            {
                return Err(CompleteAgentHostError::StaleGeneration {
                    expected: request.expected_generation,
                    actual: previous_target.generation,
                });
            }
            let previous_binding_id =
                runtime_binding_id(&request.runtime_thread_id, request.expected_generation)?;
            let previous_binding = snapshot
                .facts
                .bindings
                .get(&previous_binding_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownBinding {
                    binding_id: previous_binding_id.as_str().to_owned(),
                })?;
            if previous_binding.target != previous_target.target
                || previous_binding.generation != previous_target.generation
                || !matches!(
                    previous_binding.state,
                    CompleteAgentBindingState::Available
                        | CompleteAgentBindingState::PendingSurface
                )
            {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "Runtime surface rebind requires the exact current Host binding"
                        .to_owned(),
                });
            }
            if snapshot.facts.lifecycle_effects.values().any(|effect| {
                effect.runtime_thread_id == request.runtime_thread_id
                    && effect.generation == request.expected_generation
                    && effect.outcome.is_none()
            }) || snapshot.facts.effects.values().any(|effect| {
                effect.binding_id == previous_binding_id
                    && !matches!(
                        effect.state,
                        CompleteAgentEffectState::Applied
                            | CompleteAgentEffectState::Rejected
                            | CompleteAgentEffectState::NotApplied
                    )
            }) {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "Runtime surface rebind requires all current effects to be settled"
                        .to_owned(),
                });
            }
            let bound_surface =
                bind_complete_agent_surface(&request.desired_surface, &selection.offer).map_err(
                    |error| CompleteAgentHostError::DispatchRejected {
                        reason: error.to_string(),
                    },
                )?;
            let generation =
                AgentBindingGeneration(request.expected_generation.0.checked_add(1).ok_or_else(
                    || CompleteAgentHostError::Invariant {
                        reason: "Runtime target binding generation is exhausted".to_owned(),
                    },
                )?);
            let callback_route = callback_route_id(
                &request.runtime_thread_id,
                &request.target,
                generation,
                &bound_surface,
            )?;
            let recovered_target = CompleteAgentRuntimeTarget {
                runtime_thread_id: request.runtime_thread_id.clone(),
                target: request.target.clone(),
                generation,
                profile_digest: selection.offer.profile_digest.clone(),
                bound_surface,
                callbacks: AgentHostCallbackBinding {
                    route_id: callback_route,
                    binding_generation: generation,
                    delivery: AgentSurfaceRoute::AgentNativeCallback,
                    default_deadline_ms: request.callback_deadline_ms,
                },
            };
            let recovery = CompleteAgentRuntimeTargetRecovery {
                idempotency_key: request.idempotency_key.clone(),
                request_digest: request.request_digest.clone(),
                previous_target,
                recovered_target: recovered_target.clone(),
            };
            let mut facts = snapshot.facts;
            facts
                .bindings
                .get_mut(&previous_binding_id)
                .expect("surface rebind binding was validated in the same state")
                .state = CompleteAgentBindingState::Lost;
            facts.leases.remove(&previous_binding_id);
            let route_ids = facts
                .callback_routes
                .values()
                .filter(|route| {
                    route.binding_id == previous_binding_id
                        && route.generation == request.expected_generation
                })
                .map(|route| route.route_id.clone())
                .collect::<Vec<_>>();
            facts.revoked_callback_routes.extend(route_ids);
            facts
                .runtime_targets
                .insert(request.runtime_thread_id.clone(), recovered_target);
            facts
                .runtime_target_recoveries
                .insert(request.idempotency_key.clone(), recovery.clone());
            match self
                .repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: snapshot.revision,
                    facts,
                })
                .await
            {
                Ok(_) => return Ok(recovery),
                Err(CompleteAgentHostStoreError::Conflict { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Explicitly advances a lost Runtime binding to a newly selected trusted service placement.
    ///
    /// Runtime targets are sticky. Recovery is the only operation that may replace one, and the
    /// generation fence advances exactly once while the previous target and binding remain durable
    /// lineage. Replaying the same request returns the same recovered target.
    pub async fn recover_runtime_target(
        &self,
        request: CompleteAgentRuntimeTargetRecoveryRequest,
    ) -> Result<CompleteAgentRuntimeTargetRecovery, CompleteAgentHostError> {
        if request.callback_deadline_ms == 0 || request.expected_generation.0 == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason:
                    "Runtime target recovery requires positive generation and callback deadline"
                        .to_owned(),
            });
        }
        let selection = self.live_selection(&request.target).await?;
        loop {
            let snapshot = self.repository.load().await?;
            if let Some(existing) = snapshot
                .facts
                .runtime_target_recoveries
                .get(&request.idempotency_key)
            {
                if existing.request_digest == request.request_digest
                    && existing.previous_target.runtime_thread_id == request.runtime_thread_id
                    && existing.previous_target.generation == request.expected_generation
                    && existing.recovered_target.target == request.target
                {
                    return Ok(existing.clone());
                }
                return Err(CompleteAgentHostError::ProvisioningConflict);
            }
            let previous_target = snapshot
                .facts
                .runtime_targets
                .get(&request.runtime_thread_id)
                .cloned()
                .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                    reason: format!(
                        "Runtime target {} is not registered",
                        request.runtime_thread_id
                    ),
                })?;
            if previous_target.generation != request.expected_generation {
                return Err(CompleteAgentHostError::StaleGeneration {
                    expected: request.expected_generation,
                    actual: previous_target.generation,
                });
            }
            if previous_target.target == request.target {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "Runtime target recovery requires a distinct live attachment snapshot"
                        .to_owned(),
                });
            }
            let previous_binding_id =
                runtime_binding_id(&request.runtime_thread_id, request.expected_generation)?;
            let previous_binding = snapshot
                .facts
                .bindings
                .get(&previous_binding_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownBinding {
                    binding_id: previous_binding_id.as_str().to_owned(),
                })?;
            if previous_binding.state != CompleteAgentBindingState::Lost
                || previous_binding.target != previous_target.target
                || previous_binding.generation != previous_target.generation
            {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason:
                        "Runtime target recovery requires the exact previous binding to be lost"
                            .to_owned(),
                });
            }
            if snapshot.facts.lifecycle_effects.values().any(|effect| {
                effect.runtime_thread_id == request.runtime_thread_id
                    && effect.generation == request.expected_generation
                    && effect.outcome.is_none()
            }) {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason:
                        "Runtime target recovery requires previous lifecycle effects to be settled"
                            .to_owned(),
                });
            }
            let bound_surface =
                bind_complete_agent_surface(&request.desired_surface, &selection.offer).map_err(
                    |error| CompleteAgentHostError::DispatchRejected {
                        reason: error.to_string(),
                    },
                )?;
            let generation =
                AgentBindingGeneration(request.expected_generation.0.checked_add(1).ok_or_else(
                    || CompleteAgentHostError::Invariant {
                        reason: "Runtime target binding generation is exhausted".to_owned(),
                    },
                )?);
            let callback_route = callback_route_id(
                &request.runtime_thread_id,
                &request.target,
                generation,
                &bound_surface,
            )?;
            let recovered_target = CompleteAgentRuntimeTarget {
                runtime_thread_id: request.runtime_thread_id.clone(),
                target: request.target.clone(),
                generation,
                profile_digest: selection.offer.profile_digest.clone(),
                bound_surface,
                callbacks: AgentHostCallbackBinding {
                    route_id: callback_route,
                    binding_generation: generation,
                    delivery: AgentSurfaceRoute::AgentNativeCallback,
                    default_deadline_ms: request.callback_deadline_ms,
                },
            };
            let recovery = CompleteAgentRuntimeTargetRecovery {
                idempotency_key: request.idempotency_key.clone(),
                request_digest: request.request_digest.clone(),
                previous_target,
                recovered_target: recovered_target.clone(),
            };
            let mut facts = snapshot.facts;
            facts
                .runtime_targets
                .insert(request.runtime_thread_id.clone(), recovered_target);
            facts
                .runtime_target_recoveries
                .insert(request.idempotency_key.clone(), recovery.clone());
            match self
                .repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: snapshot.revision,
                    facts,
                })
                .await
            {
                Ok(_) => return Ok(recovery),
                Err(CompleteAgentHostStoreError::Conflict { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub async fn runtime_target(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<CompleteAgentRuntimeTarget, CompleteAgentHostError> {
        self.repository
            .load()
            .await?
            .facts
            .runtime_targets
            .get(runtime_thread_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                reason: format!("Runtime target {runtime_thread_id} is not registered"),
            })
    }

    pub async fn lifecycle_effect(
        &self,
        effect_id: &AgentEffectIdentity,
    ) -> Result<Option<CompleteAgentLifecycleEffectRecord>, CompleteAgentHostError> {
        Ok(self
            .repository
            .load()
            .await?
            .facts
            .lifecycle_effects
            .get(effect_id)
            .cloned())
    }

    async fn begin_lifecycle_effect(
        &self,
        record: CompleteAgentLifecycleEffectRecord,
    ) -> Result<CompleteAgentLifecycleBegin, CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        if let Some(existing) = facts.lifecycle_effects.get(&record.effect_id) {
            if existing.runtime_thread_id != record.runtime_thread_id
                || existing.child_thread_id != record.child_thread_id
                || existing.kind != record.kind
                || existing.target != record.target
                || existing.generation != record.generation
                || existing.initial_context != record.initial_context
                || existing.fork_cutoff != record.fork_cutoff
            {
                return Err(CompleteAgentHostError::EffectIdentityConflict);
            }
            return Ok(existing.outcome.clone().map_or_else(
                || CompleteAgentLifecycleBegin::InspectionRequired,
                |outcome| CompleteAgentLifecycleBegin::Settled(Box::new(outcome)),
            ));
        }
        facts
            .lifecycle_effects
            .insert(record.effect_id.clone(), record);
        self.commit(snapshot.revision, facts).await?;
        Ok(CompleteAgentLifecycleBegin::Dispatch)
    }

    async fn observe_lifecycle_applied_receipt(
        &self,
        effect_id: &AgentEffectIdentity,
        receipt: CompleteAgentLifecycleAppliedReceipt,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        let record = facts.lifecycle_effects.get_mut(effect_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownEffect {
                effect_id: effect_id.clone(),
            }
        })?;
        if let Some(existing) = &record.applied_receipt {
            if existing == &receipt {
                return Ok(());
            }
            return Err(CompleteAgentHostError::EffectEvidenceConflict {
                effect_id: effect_id.clone(),
            });
        }
        record.applied_receipt = Some(receipt);
        self.commit(snapshot.revision, facts).await?;
        Ok(())
    }

    async fn settle_lifecycle_effect(
        &self,
        effect_id: &AgentEffectIdentity,
        outcome: CompleteAgentLifecycleOutcome,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        let record = facts.lifecycle_effects.get_mut(effect_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownEffect {
                effect_id: effect_id.clone(),
            }
        })?;
        if let Some(existing) = &record.outcome {
            if existing == &outcome {
                return Ok(());
            }
            return Err(CompleteAgentHostError::EffectEvidenceConflict {
                effect_id: effect_id.clone(),
            });
        }
        record.outcome = Some(outcome);
        self.commit(snapshot.revision, facts).await?;
        Ok(())
    }

    async fn runtime_binding(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        expected: &ManagedRuntimeAgentBinding,
    ) -> Result<(CompleteAgentBindingId, CompleteAgentBinding), CompleteAgentHostError> {
        let target = self.runtime_target(runtime_thread_id).await?;
        if target.generation != expected.generation {
            return Err(CompleteAgentHostError::StaleGeneration {
                expected: expected.generation,
                actual: target.generation,
            });
        }
        let binding_id = runtime_binding_id(runtime_thread_id, expected.generation)?;
        let binding = self.binding(&binding_id).await?.ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        if binding.source != expected.source || binding.generation != expected.generation {
            return Err(CompleteAgentHostError::StaleGeneration {
                expected: expected.generation,
                actual: binding.generation,
            });
        }
        if binding.target != target.target
            || binding.profile_digest != target.profile_digest
            || binding.bound_surface != target.bound_surface
            || !binding.dispatch_admitted()
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Runtime binding is not admitted by the current target".to_owned(),
            });
        }
        if binding.applied_surface.as_ref() != Some(&expected.applied_surface) {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Runtime binding applied surface evidence has drifted".to_owned(),
            });
        }
        Ok((binding_id, binding))
    }

    pub async fn acquire_binding_lease(
        &self,
        binding_id: &CompleteAgentBindingId,
        generation: AgentBindingGeneration,
        owner: impl Into<String>,
        now_ms: u64,
        expires_at_ms: u64,
    ) -> Result<CompleteAgentBindingLease, CompleteAgentHostError> {
        let owner = owner.into();
        if owner.trim().is_empty() || expires_at_ms <= now_ms {
            return Err(CompleteAgentHostError::Invariant {
                reason: "binding lease requires an owner and a future expiry".to_owned(),
            });
        }
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let binding = state.bindings.get(binding_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        ensure_generation(binding, generation)?;
        if matches!(
            binding.state,
            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
        ) {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "lost or closed binding cannot acquire a delivery lease".to_owned(),
            });
        }
        if let Some(existing) = state.leases.get(binding_id)
            && existing.expires_at_ms > now_ms
        {
            if existing.owner == owner && existing.generation == generation {
                return Ok(existing.clone());
            }
            return Err(CompleteAgentHostError::LeaseConflict);
        }
        let epoch = state
            .lease_epochs
            .get(binding_id)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| CompleteAgentHostError::Invariant {
                reason: "binding lease epoch is exhausted".to_owned(),
            })?;
        let lease = CompleteAgentBindingLease {
            binding_id: binding_id.clone(),
            generation,
            owner,
            token: uuid::Uuid::new_v4().to_string(),
            epoch,
            expires_at_ms,
        };
        state.lease_epochs.insert(binding_id.clone(), epoch);
        state.leases.insert(binding_id.clone(), lease.clone());
        self.commit(snapshot.revision, state).await?;
        Ok(lease)
    }

    pub async fn renew_binding_lease(
        &self,
        lease: &CompleteAgentBindingLease,
        now_ms: u64,
        expires_at_ms: u64,
    ) -> Result<CompleteAgentBindingLease, CompleteAgentHostError> {
        if expires_at_ms <= now_ms {
            return Err(CompleteAgentHostError::Invariant {
                reason: "binding lease renewal requires a future expiry".to_owned(),
            });
        }
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        validate_lease_state(&state, lease, now_ms)?;
        let current = state
            .leases
            .get_mut(&lease.binding_id)
            .expect("validated lease exists in the same state");
        current.expires_at_ms = expires_at_ms;
        let renewed = current.clone();
        self.commit(snapshot.revision, state).await?;
        Ok(renewed)
    }

    pub async fn release_binding_lease(
        &self,
        lease: &CompleteAgentBindingLease,
        now_ms: u64,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        validate_lease_state(&state, lease, now_ms)?;
        state.leases.remove(&lease.binding_id);
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    pub async fn mark_binding_lost(
        &self,
        binding_id: &CompleteAgentBindingId,
        generation: AgentBindingGeneration,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let binding = state.bindings.get_mut(binding_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        ensure_generation(binding, generation)?;
        binding.state = CompleteAgentBindingState::Lost;
        let route_ids = state
            .callback_routes
            .values()
            .filter(|route| {
                &route.binding_id == binding_id
                    && route.generation == generation
                    && !state.revoked_callback_routes.contains(&route.route_id)
            })
            .map(|route| route.route_id.clone())
            .collect::<Vec<_>>();
        for route_id in route_ids {
            state.revoked_callback_routes.insert(route_id);
        }
        state.leases.remove(binding_id);
        for effect in state
            .effects
            .values_mut()
            .filter(|effect| &effect.binding_id == binding_id && effect.generation == generation)
        {
            if !matches!(
                effect.state,
                CompleteAgentEffectState::Applied | CompleteAgentEffectState::Rejected
            ) {
                observe_effect_state(effect, CompleteAgentEffectState::Lost)?;
            }
        }
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    /// Atomically fences every nonterminal binding owned by one withdrawn live attachment.
    ///
    /// Remote transport withdrawal calls this before closing placement transport. Callback routes,
    /// leases and unsettled effects are fenced in the same Host commit.
    pub async fn mark_target_bindings_lost(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<Vec<RuntimeThreadId>, CompleteAgentHostError> {
        loop {
            let snapshot = self.repository.load().await?;
            let binding_ids = snapshot
                .facts
                .bindings
                .iter()
                .filter(|(_, binding)| {
                    &binding.target == target
                        && !matches!(
                            binding.state,
                            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
                        )
                })
                .map(|(binding_id, _)| binding_id.clone())
                .collect::<Vec<_>>();
            let affected_threads = snapshot
                .facts
                .runtime_targets
                .iter()
                .filter(|(_, runtime_target)| &runtime_target.target == target)
                .filter(|(_, target)| {
                    let binding_id =
                        runtime_binding_id(&target.runtime_thread_id, target.generation).ok();
                    binding_id.as_ref().is_some_and(|binding_id| {
                        binding_ids.iter().any(|candidate| candidate == binding_id)
                            || snapshot
                                .facts
                                .bindings
                                .get(binding_id)
                                .is_some_and(|binding| {
                                    binding.state == CompleteAgentBindingState::Lost
                                })
                    })
                })
                .map(|(thread_id, _)| thread_id.clone())
                .collect::<Vec<_>>();
            if binding_ids.is_empty() {
                return Ok(affected_threads);
            }
            let mut facts = snapshot.facts;
            for binding_id in &binding_ids {
                let generation = facts.bindings[binding_id].generation;
                facts
                    .bindings
                    .get_mut(binding_id)
                    .expect("selected binding exists")
                    .state = CompleteAgentBindingState::Lost;
                let route_ids = facts
                    .callback_routes
                    .values()
                    .filter(|route| {
                        &route.binding_id == binding_id
                            && route.generation == generation
                            && !facts.revoked_callback_routes.contains(&route.route_id)
                    })
                    .map(|route| route.route_id.clone())
                    .collect::<Vec<_>>();
                facts.revoked_callback_routes.extend(route_ids);
                facts.leases.remove(binding_id);
                for effect in facts.effects.values_mut().filter(|effect| {
                    &effect.binding_id == binding_id && effect.generation == generation
                }) {
                    if !matches!(
                        effect.state,
                        CompleteAgentEffectState::Applied | CompleteAgentEffectState::Rejected
                    ) {
                        observe_effect_state(effect, CompleteAgentEffectState::Lost)?;
                    }
                }
            }
            match self
                .repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: snapshot.revision,
                    facts,
                })
                .await
            {
                Ok(_) => return Ok(affected_threads),
                Err(CompleteAgentHostStoreError::Conflict { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub async fn lost_runtime_threads_for_profile(
        &self,
        profile_digest: &AgentProfileDigest,
    ) -> Result<Vec<RuntimeThreadId>, CompleteAgentHostError> {
        let facts = self.repository.load().await?.facts;
        Ok(facts
            .runtime_targets
            .iter()
            .filter(|(_, target)| &target.profile_digest == profile_digest)
            .filter(|(thread_id, target)| {
                runtime_binding_id(thread_id, target.generation)
                    .ok()
                    .and_then(|binding_id| facts.bindings.get(&binding_id))
                    .is_some_and(|binding| binding.state == CompleteAgentBindingState::Lost)
            })
            .map(|(thread_id, _)| thread_id.clone())
            .collect())
    }

    pub async fn dispatch_execute(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, CompleteAgentHostError> {
        let digest = payload_digest(&command)?;
        let (service_target, source, should_dispatch) = {
            let snapshot = self.repository.load().await?;
            let mut state = snapshot.facts;
            validate_lease_state(&state, lease, current_time_ms())?;
            if &lease.binding_id != binding_id {
                return Err(CompleteAgentHostError::LeaseConflict);
            }
            let binding = state.bindings.get(binding_id).cloned().ok_or_else(|| {
                CompleteAgentHostError::UnknownBinding {
                    binding_id: binding_id.as_str().to_owned(),
                }
            })?;
            ensure_generation(&binding, command.meta.binding_generation)?;
            if !binding.dispatch_admitted() {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "bound surface has not produced matching applied evidence".to_owned(),
                });
            }
            if binding.source != command.source {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "command source does not match the binding".to_owned(),
                });
            }

            let candidate = CompleteAgentEffectRecord {
                effect_id: command.meta.effect_id.clone(),
                command_id: command.meta.command_id.clone(),
                binding_id: binding_id.clone(),
                generation: binding.generation,
                source: binding.source.clone(),
                payload_digest: digest,
                delivery_epoch: lease.epoch,
                dispatch_attempt: 1,
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
                attempt_history: Vec::new(),
            };
            match state.effects.get(&command.meta.effect_id) {
                Some(existing) => {
                    ensure_same_effect(existing, &candidate)?;
                    let confirmed_not_applied =
                        existing.state == CompleteAgentEffectState::NotApplied;
                    if confirmed_not_applied {
                        let effect = state
                            .effects
                            .get_mut(&command.meta.effect_id)
                            .expect("effect was read from the same state");
                        begin_effect_redispatch(effect, lease.epoch)?;
                    } else {
                        state
                            .effects
                            .get_mut(&command.meta.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    let result = (
                        binding.target.clone(),
                        binding.source.clone(),
                        confirmed_not_applied,
                    );
                    self.commit(snapshot.revision, state).await?;
                    result
                }
                None => {
                    state
                        .effects
                        .insert(command.meta.effect_id.clone(), candidate);
                    let result = (binding.target.clone(), binding.source.clone(), true);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };

        let service = self.service(&service_target).await?;
        if !should_dispatch {
            return self
                .inspect_effect(lease, &command.meta.effect_id)
                .await
                .map(|inspection| {
                    inspection_receipt(inspection, command.meta.command_id.clone(), source)
                });
        }

        match service.execute(command.clone()).await {
            Ok(receipt) => {
                self.record_receipt(lease, &command.meta.effect_id, receipt.clone())
                    .await?;
                Ok(receipt)
            }
            Err(error) => {
                self.mark_unknown(lease, &command.meta.effect_id).await?;
                Err(error.into())
            }
        }
    }

    pub async fn reconcile_effect(
        &self,
        lease: &CompleteAgentBindingLease,
        effect_id: &AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, CompleteAgentHostError> {
        {
            let snapshot = self.repository.load().await?;
            let mut state = snapshot.facts;
            validate_lease_state(&state, lease, current_time_ms())?;
            let record = state.effects.get_mut(effect_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                }
            })?;
            if record.binding_id != lease.binding_id || record.generation != lease.generation {
                return Err(CompleteAgentHostError::LeaseConflict);
            }
            record.delivery_epoch = lease.epoch;
            self.commit(snapshot.revision, state).await?;
        }
        self.inspect_effect(lease, effect_id).await
    }

    async fn inspect_effect(
        &self,
        lease: &CompleteAgentBindingLease,
        effect_id: &AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, CompleteAgentHostError> {
        let service_target = {
            let state = self.repository.load().await?.facts;
            let record = state.effects.get(effect_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                }
            })?;
            validate_effect_fence(&state, lease, record, current_time_ms())?;
            state
                .bindings
                .get(&record.binding_id)
                .expect("effect binding was validated by the durable Host graph")
                .target
                .clone()
        };
        let service = self.service(&service_target).await?;
        let inspection = service.inspect(effect_id.clone()).await?;
        if !inspection.validate() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "effect inspection returned mismatched effect coordinates".to_owned(),
            });
        }

        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        validate_effect_fence(&state, lease, record, current_time_ms())?;
        let record = state
            .effects
            .get_mut(effect_id)
            .expect("effect was fenced in the same state");
        if inspection.effect_id != record.effect_id
            || inspection
                .command_id
                .as_ref()
                .is_some_and(|command_id| command_id != &record.command_id)
            || inspection_state_source(&inspection.state)
                .is_some_and(|source| source != &record.source)
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "effect inspection returned different coordinates".to_owned(),
            });
        }
        let observed_state = inspection_state(&inspection.state);
        ensure_same_observation_evidence(
            record.state,
            observed_state,
            record.inspection.as_ref(),
            &inspection,
            &record.effect_id,
        )?;
        observe_effect_state(record, observed_state)?;
        record.inspection = Some(inspection.clone());
        self.commit(snapshot.revision, state).await?;
        Ok(inspection)
    }

    pub async fn apply_bound_surface(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, CompleteAgentHostError> {
        let digest = payload_digest(&command)?;
        let callback_route = command
            .bound_surface
            .contributions
            .iter()
            .any(|contribution| {
                contribution.route
                    == agentdash_agent_service_api::AgentSurfaceRoute::AgentNativeCallback
            })
            .then(|| {
                CompleteAgentCallbackRoute::from_binding(
                    binding_id.clone(),
                    command.callbacks.clone(),
                    command.source.clone(),
                    command.bound_surface.clone(),
                )
            })
            .transpose()
            .map_err(|error| CompleteAgentHostError::Invariant {
                reason: error.message,
            })?;
        let (service_target, should_dispatch) = {
            let snapshot = self.repository.load().await?;
            let mut state = snapshot.facts;
            validate_lease_state(&state, lease, current_time_ms())?;
            if &lease.binding_id != binding_id {
                return Err(CompleteAgentHostError::LeaseConflict);
            }
            let binding = state.bindings.get(binding_id).cloned().ok_or_else(|| {
                CompleteAgentHostError::UnknownBinding {
                    binding_id: binding_id.as_str().to_owned(),
                }
            })?;
            ensure_generation(&binding, command.callbacks.binding_generation)?;
            if binding.source != command.source || binding.bound_surface != command.bound_surface {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "surface apply command does not match the reserved binding".to_owned(),
                });
            }
            let candidate = CompleteAgentEffectRecord {
                effect_id: command.effect_id.clone(),
                command_id: command.command_id.clone(),
                binding_id: binding_id.clone(),
                generation: binding.generation,
                source: binding.source.clone(),
                payload_digest: digest,
                delivery_epoch: lease.epoch,
                dispatch_attempt: 1,
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
                attempt_history: Vec::new(),
            };
            match state.effects.get(&command.effect_id) {
                Some(existing) => {
                    ensure_same_effect(existing, &candidate)?;
                    if let Some(receipt) = &existing.surface_receipt {
                        return Ok(receipt.clone());
                    }
                    let confirmed_not_applied =
                        existing.state == CompleteAgentEffectState::NotApplied;
                    if confirmed_not_applied {
                        let effect = state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state");
                        begin_effect_redispatch(effect, lease.epoch)?;
                    } else {
                        state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    let result = (binding.target, confirmed_not_applied);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
                None => {
                    state.effects.insert(command.effect_id.clone(), candidate);
                    let result = (binding.target, true);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };
        let service = self.service(&service_target).await?;
        if !should_dispatch {
            return self
                .recover_applied_surface(
                    lease,
                    binding_id,
                    &command.effect_id,
                    callback_route.as_ref(),
                )
                .await;
        }

        match service.apply_surface(command.clone()).await {
            Ok(receipt) => {
                if self
                    .record_surface_receipt(
                        lease,
                        binding_id,
                        &command.effect_id,
                        receipt.clone(),
                        callback_route.as_ref(),
                    )
                    .await
                    .is_err()
                {
                    return Err(CompleteAgentHostError::EffectPending {
                        effect_id: command.effect_id,
                    });
                }
                Ok(receipt)
            }
            Err(_) => {
                let effect_id = command.effect_id;
                let _ = self.mark_unknown(lease, &effect_id).await;
                Err(CompleteAgentHostError::EffectPending { effect_id })
            }
        }
    }

    pub async fn revoke_bound_surface(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, CompleteAgentHostError> {
        let digest = payload_digest(&command)?;
        let (service_target, source, plan) = {
            let snapshot = self.repository.load().await?;
            let mut state = snapshot.facts;
            validate_lease_state(&state, lease, current_time_ms())?;
            if &lease.binding_id != binding_id {
                return Err(CompleteAgentHostError::LeaseConflict);
            }
            let binding = state.bindings.get(binding_id).cloned().ok_or_else(|| {
                CompleteAgentHostError::UnknownBinding {
                    binding_id: binding_id.as_str().to_owned(),
                }
            })?;
            ensure_generation(&binding, command.binding_generation)?;
            if binding.source != command.source
                || binding.bound_surface.revision != command.expected_revision
            {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "surface revoke command does not match the active binding".to_owned(),
                });
            }
            let candidate = CompleteAgentEffectRecord {
                effect_id: command.effect_id.clone(),
                command_id: command.command_id.clone(),
                binding_id: binding_id.clone(),
                generation: binding.generation,
                source: binding.source.clone(),
                payload_digest: digest,
                delivery_epoch: lease.epoch,
                dispatch_attempt: 1,
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
                attempt_history: Vec::new(),
            };
            match state.effects.get(&command.effect_id) {
                Some(existing) => {
                    ensure_same_effect(existing, &candidate)?;
                    let settled = if matches!(
                        existing.state,
                        CompleteAgentEffectState::Applied | CompleteAgentEffectState::Rejected
                    ) {
                        existing.receipt.clone()
                    } else {
                        None
                    };
                    let confirmed_not_applied =
                        existing.state == CompleteAgentEffectState::NotApplied;
                    if confirmed_not_applied {
                        let effect = state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state");
                        begin_effect_redispatch(effect, lease.epoch)?;
                    } else {
                        state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    let result = (
                        binding.target,
                        binding.source,
                        if let Some(receipt) = settled {
                            RevokeDispatchPlan::Settled(Box::new(receipt))
                        } else if confirmed_not_applied {
                            RevokeDispatchPlan::Dispatch
                        } else {
                            RevokeDispatchPlan::Inspect
                        },
                    );
                    self.commit(snapshot.revision, state).await?;
                    result
                }
                None => {
                    state.effects.insert(command.effect_id.clone(), candidate);
                    let result = (binding.target, binding.source, RevokeDispatchPlan::Dispatch);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };
        let receipt = match plan {
            RevokeDispatchPlan::Dispatch => {
                let service = self.service(&service_target).await?;
                match service.revoke_surface(command.clone()).await {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        self.mark_unknown(lease, &command.effect_id).await?;
                        return Err(error.into());
                    }
                }
            }
            RevokeDispatchPlan::Inspect => {
                let inspection = self.inspect_effect(lease, &command.effect_id).await?;
                inspection_receipt(inspection, command.command_id.clone(), source)
            }
            RevokeDispatchPlan::Settled(receipt) => *receipt,
        };
        self.settle_revoke_receipt(lease, binding_id, &command.effect_id, receipt.clone())
            .await?;
        Ok(receipt)
    }

    pub async fn effect(
        &self,
        effect_id: &AgentEffectIdentity,
    ) -> Result<Option<CompleteAgentEffectRecord>, CompleteAgentHostError> {
        Ok(self
            .repository
            .load()
            .await?
            .facts
            .effects
            .get(effect_id)
            .cloned())
    }

    pub async fn binding(
        &self,
        binding_id: &CompleteAgentBindingId,
    ) -> Result<Option<CompleteAgentBinding>, CompleteAgentHostError> {
        Ok(self
            .repository
            .load()
            .await?
            .facts
            .bindings
            .get(binding_id)
            .cloned())
    }

    async fn service(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentHostError> {
        Ok(self.live_selection(target).await?.service())
    }

    async fn live_selection(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentHostError> {
        let selection = self
            .live_catalog
            .resolve(&target.live_attachment_id)
            .await
            .ok_or_else(|| CompleteAgentHostError::UnavailableAttachment {
                attachment_id: target.live_attachment_id.clone(),
            })?;
        if selection.target != *target {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "live attachment facts do not match the durable binding target".to_owned(),
            });
        }
        Ok(selection)
    }

    async fn record_receipt(
        &self,
        lease: &CompleteAgentBindingLease,
        effect_id: &AgentEffectIdentity,
        receipt: AgentCommandReceipt,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        validate_effect_fence(&state, lease, record, current_time_ms())?;
        let record = state
            .effects
            .get_mut(effect_id)
            .expect("effect was fenced in the same state");
        if receipt.effect_id != record.effect_id
            || receipt.command_id != record.command_id
            || receipt.source != record.source
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "command receipt returned different coordinates".to_owned(),
            });
        }
        let observed_state = receipt_state(&receipt.state);
        ensure_same_observation_evidence(
            record.state,
            observed_state,
            record.receipt.as_ref(),
            &receipt,
            &record.effect_id,
        )?;
        observe_effect_state(record, observed_state)?;
        record.receipt = Some(receipt);
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    async fn settle_revoke_receipt(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        effect_id: &AgentEffectIdentity,
        receipt: AgentCommandReceipt,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        validate_effect_fence(&state, lease, record, current_time_ms())?;
        if &record.binding_id != binding_id
            || receipt.effect_id != record.effect_id
            || receipt.command_id != record.command_id
            || receipt.source != record.source
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface revoke receipt returned different coordinates".to_owned(),
            });
        }
        let observed_state = receipt_state(&receipt.state);
        ensure_same_observation_evidence(
            record.state,
            observed_state,
            record.receipt.as_ref(),
            &receipt,
            &record.effect_id,
        )?;
        let record = state
            .effects
            .get_mut(effect_id)
            .expect("effect was fenced in the same state");
        observe_effect_state(record, observed_state)?;
        record.receipt = Some(receipt);
        if observed_state == CompleteAgentEffectState::Applied {
            let binding = state.bindings.get_mut(binding_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownBinding {
                    binding_id: binding_id.as_str().to_owned(),
                }
            })?;
            binding.applied_surface = None;
            binding.state = CompleteAgentBindingState::PendingSurface;
            let route_ids = state
                .callback_routes
                .values()
                .filter(|route| {
                    &route.binding_id == binding_id
                        && route.generation == lease.generation
                        && !state.revoked_callback_routes.contains(&route.route_id)
                })
                .map(|route| route.route_id.clone())
                .collect::<Vec<_>>();
            for route_id in route_ids {
                state.revoked_callback_routes.insert(route_id);
            }
        }
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    async fn record_surface_receipt(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        effect_id: &AgentEffectIdentity,
        receipt: AppliedAgentSurfaceReceipt,
        callback_route: Option<&CompleteAgentCallbackRoute>,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        validate_effect_fence(&state, lease, record, current_time_ms())?;
        if receipt.effect_id != record.effect_id
            || receipt.command_id != record.command_id
            || receipt.source != record.source
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface receipt returned different coordinates".to_owned(),
            });
        }
        let binding = state.bindings.get(binding_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        if !binding.bound_surface.accepts_applied(&receipt.applied) {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "surface receipt does not prove the reserved bound surface".to_owned(),
            });
        }
        let record = state
            .effects
            .get_mut(effect_id)
            .expect("effect was validated in the same state");
        ensure_same_observation_evidence(
            record.state,
            CompleteAgentEffectState::Applied,
            record.surface_receipt.as_ref(),
            &receipt,
            &record.effect_id,
        )?;
        observe_effect_state(record, CompleteAgentEffectState::Applied)?;
        record.surface_receipt = Some(receipt);
        let applied_surface = record
            .surface_receipt
            .as_ref()
            .expect("surface receipt was stored")
            .applied
            .clone();
        let binding = state
            .bindings
            .get_mut(binding_id)
            .expect("binding was validated in the same state");
        binding.applied_surface = Some(applied_surface);
        binding.state = CompleteAgentBindingState::Available;
        if let Some(route) = callback_route {
            if let Some(existing) = state.callback_routes.get(&route.route_id)
                && existing != route
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "callback route identity was reused with another binding fence"
                        .to_owned(),
                });
            }
            state
                .callback_routes
                .insert(route.route_id.clone(), route.clone());
        }
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    async fn recover_applied_surface(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        effect_id: &AgentEffectIdentity,
        callback_route: Option<&CompleteAgentCallbackRoute>,
    ) -> Result<AppliedAgentSurfaceReceipt, CompleteAgentHostError> {
        let inspection = self.inspect_effect(lease, effect_id).await?;
        let receipt = match inspection.state {
            AgentEffectInspectionState::NotApplied => {
                return Err(CompleteAgentHostError::EffectNotApplied {
                    effect_id: effect_id.clone(),
                });
            }
            AgentEffectInspectionState::Accepted { .. } | AgentEffectInspectionState::Unknown => {
                return Err(CompleteAgentHostError::EffectPending {
                    effect_id: effect_id.clone(),
                });
            }
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::SurfaceApply { receipt },
            } => receipt,
            AgentEffectInspectionState::Applied { .. } => {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "surface effect inspection returned a non-surface outcome".to_owned(),
                });
            }
        };
        self.record_surface_receipt(
            lease,
            binding_id,
            effect_id,
            receipt.clone(),
            callback_route,
        )
        .await?;
        Ok(receipt)
    }

    async fn mark_unknown(
        &self,
        lease: &CompleteAgentBindingLease,
        effect_id: &AgentEffectIdentity,
    ) -> Result<(), CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        let mut state = snapshot.facts;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        validate_effect_fence(&state, lease, record, current_time_ms())?;
        let record = state
            .effects
            .get_mut(effect_id)
            .expect("effect was fenced in the same state");
        observe_effect_state(record, CompleteAgentEffectState::Unknown)?;
        self.commit(snapshot.revision, state).await?;
        Ok(())
    }

    async fn provision_runtime_binding(
        &self,
        context: &ManagedRuntimeDispatchContext,
        target: &CompleteAgentRuntimeTarget,
        source: AgentSourceCoordinate,
    ) -> Result<ManagedRuntimeAgentBinding, CompleteAgentHostError> {
        let binding_id = runtime_binding_id(&target.runtime_thread_id, target.generation)?;
        if let Some(binding) = self.binding(&binding_id).await?
            && binding.dispatch_admitted()
        {
            if binding.target != target.target
                || binding.generation != target.generation
                || binding.source != source
                || binding.profile_digest != target.profile_digest
                || binding.bound_surface != target.bound_surface
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "dispatch-admitted Runtime binding conflicts with lifecycle intent"
                        .to_owned(),
                });
            }
            return Ok(ManagedRuntimeAgentBinding {
                source,
                generation: binding.generation,
                applied_surface: binding
                    .applied_surface
                    .expect("dispatch-admitted binding has applied surface"),
            });
        }
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target: target.target.clone(),
            generation: target.generation,
            source: source.clone(),
            profile_digest: target.profile_digest.clone(),
            bound_surface: target.bound_surface.clone(),
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        };
        self.register_binding(binding).await?;
        let now_ms = context.now_ms.max(current_time_ms());
        let expires_at_ms = now_ms
            .checked_add(context.lease_duration_ms)
            .ok_or_else(|| CompleteAgentHostError::Invariant {
                reason: "Runtime lifecycle lease expiry is exhausted".to_owned(),
            })?;
        let lease = self
            .acquire_binding_lease(
                &binding_id,
                target.generation,
                context.dispatch_owner.clone(),
                now_ms,
                expires_at_ms,
            )
            .await?;
        let command = ApplyBoundAgentSurface {
            command_id: derived_command_id(&context.effect_id, "surface")?,
            effect_id: derived_effect_id(&context.effect_id, "surface")?,
            idempotency_key: derived_idempotency_key(&context.effect_id, "surface")?,
            source: source.clone(),
            bound_surface: target.bound_surface.clone(),
            callbacks: target.callbacks.clone(),
        };
        let applied = self
            .apply_bound_surface(&lease, &binding_id, command)
            .await?
            .applied;
        self.release_binding_lease(&lease, now_ms).await?;
        Ok(ManagedRuntimeAgentBinding {
            source,
            generation: target.generation,
            applied_surface: applied,
        })
    }

    async fn commit(
        &self,
        expected_revision: crate::CompleteAgentHostRevision,
        facts: CompleteAgentHostFacts,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostError> {
        Ok(self
            .repository
            .commit(CompleteAgentHostCommit {
                expected_revision,
                facts,
            })
            .await?)
    }
}

pub fn complete_agent_managed_runtime_gateway(
    repository: Arc<dyn ManagedRuntimeStateRepository>,
    host: Arc<CompleteAgentHost>,
    dispatch_owner: impl Into<String>,
    lease_duration_ms: u64,
) -> Result<Arc<dyn ManagedAgentRuntimeGateway>, ManagedRuntimeGatewayError> {
    production_managed_runtime_gateway(repository, host, dispatch_owner, lease_duration_ms)
}

#[async_trait]
impl ManagedRuntimeLifecyclePort for CompleteAgentHost {
    async fn create(
        &self,
        context: ManagedRuntimeDispatchContext,
        initial_context: Option<InitialAgentContextPackage>,
    ) -> Result<ManagedRuntimeCreateOutcome, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        let record = CompleteAgentLifecycleEffectRecord {
            effect_id: context.effect_id.clone(),
            runtime_thread_id: context.runtime_thread_id.clone(),
            child_thread_id: None,
            kind: CompleteAgentLifecycleOperationKind::Create,
            target: target.target.clone(),
            generation: target.generation,
            initial_context: initial_context.clone(),
            fork_cutoff: None,
            applied_receipt: None,
            outcome: None,
        };
        match self
            .begin_lifecycle_effect(record)
            .await
            .map_err(map_lifecycle_host_error)?
        {
            CompleteAgentLifecycleBegin::Settled(outcome) => {
                return self
                    .stored_create_outcome(&target, *outcome)
                    .await
                    .map_err(map_applied_lifecycle_host_error);
            }
            CompleteAgentLifecycleBegin::InspectionRequired => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Create lifecycle effect requires inspection".to_owned(),
                });
            }
            CompleteAgentLifecycleBegin::Dispatch => {}
        }
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let receipt = service
            .create(CreateAgentCommand {
                meta: lifecycle_meta(&context, target.generation)?,
                requested_source: None,
                initial_context,
            })
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            })?;
        if !receipt_applied_success(&receipt.state) {
            if matches!(receipt.state, AgentReceiptState::Rejected { .. }) {
                self.settle_lifecycle_effect(
                    &context.effect_id,
                    CompleteAgentLifecycleOutcome::Agent {
                        receipt,
                        applied_surface: None,
                    },
                )
                .await
                .map_err(map_lifecycle_host_error)?;
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Complete Agent rejected Create".to_owned(),
                });
            }
            return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "Complete Agent Create is not terminal".to_owned(),
            });
        }
        self.observe_lifecycle_applied_receipt(
            &context.effect_id,
            CompleteAgentLifecycleAppliedReceipt::Agent(applied_agent_receipt_from_command(
                &receipt,
            )?),
        )
        .await
        .map_err(map_applied_receipt_observation_error)?;
        self.finish_applied_create(context, target, service, receipt)
            .await
    }

    async fn resume(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        self.runtime_binding(&context.runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        let record = CompleteAgentLifecycleEffectRecord {
            effect_id: context.effect_id.clone(),
            runtime_thread_id: context.runtime_thread_id.clone(),
            child_thread_id: None,
            kind: CompleteAgentLifecycleOperationKind::Resume,
            target: target.target.clone(),
            generation: target.generation,
            initial_context: None,
            fork_cutoff: None,
            applied_receipt: None,
            outcome: None,
        };
        match self
            .begin_lifecycle_effect(record)
            .await
            .map_err(map_lifecycle_host_error)?
        {
            CompleteAgentLifecycleBegin::Settled(outcome) => {
                return stored_resume_outcome(binding, *outcome);
            }
            CompleteAgentLifecycleBegin::InspectionRequired => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Resume lifecycle effect requires inspection".to_owned(),
                });
            }
            CompleteAgentLifecycleBegin::Dispatch => {}
        }
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let receipt = service
            .resume(ResumeAgentCommand {
                meta: lifecycle_meta(&context, target.generation)?,
                source: binding.source.clone(),
            })
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            })?;
        if !receipt_applied_success(&receipt.state) {
            return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "Complete Agent Resume is not terminal".to_owned(),
            });
        }
        self.observe_lifecycle_applied_receipt(
            &context.effect_id,
            CompleteAgentLifecycleAppliedReceipt::Agent(applied_agent_receipt_from_command(
                &receipt,
            )?),
        )
        .await
        .map_err(map_applied_receipt_observation_error)?;
        self.finish_applied_resume(context, binding, receipt).await
    }

    async fn rebind(
        &self,
        context: ManagedRuntimeDispatchContext,
        previous_binding: ManagedRuntimeAgentBinding,
    ) -> Result<ManagedRuntimeRebindOutcome, ManagedRuntimeLifecycleError> {
        let mut target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        let expected_generation =
            previous_binding
                .generation
                .0
                .checked_add(1)
                .ok_or_else(|| ManagedRuntimeLifecycleError::Invalid {
                    reason: "Runtime binding generation is exhausted".to_owned(),
                })?;
        if target.generation == previous_binding.generation {
            let planner = self.recovery_planner.read().await.clone().ok_or_else(|| {
                ManagedRuntimeLifecycleError::Unavailable {
                    reason: "Runtime Rebind has no installed recovery planner".to_owned(),
                }
            })?;
            let request = planner
                .plan_recovery(
                    &context.runtime_thread_id,
                    &target,
                    &previous_binding,
                    &context.effect_id,
                )
                .await
                .map_err(map_lifecycle_host_error)?;
            if request.runtime_thread_id != context.runtime_thread_id
                || request.expected_generation != previous_binding.generation
            {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Runtime recovery planner returned mismatched coordinates".to_owned(),
                });
            }
            target = self
                .recover_runtime_target(request)
                .await
                .map_err(map_lifecycle_host_error)?
                .recovered_target;
        }
        if target.generation != AgentBindingGeneration(expected_generation) {
            return Err(ManagedRuntimeLifecycleError::StaleGeneration);
        }
        let previous_binding_id =
            runtime_binding_id(&context.runtime_thread_id, previous_binding.generation)
                .map_err(map_lifecycle_host_error)?;
        let durable_previous = self
            .binding(&previous_binding_id)
            .await
            .map_err(map_lifecycle_host_error)?
            .ok_or(ManagedRuntimeLifecycleError::NotFound)?;
        if durable_previous.state != CompleteAgentBindingState::Lost
            || durable_previous.source != previous_binding.source
            || durable_previous.generation != previous_binding.generation
            || durable_previous.applied_surface.as_ref() != Some(&previous_binding.applied_surface)
        {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "Rebind requires the exact previous Host binding to be lost".to_owned(),
            });
        }
        let record = CompleteAgentLifecycleEffectRecord {
            effect_id: context.effect_id.clone(),
            runtime_thread_id: context.runtime_thread_id.clone(),
            child_thread_id: None,
            kind: CompleteAgentLifecycleOperationKind::Rebind,
            target: target.target.clone(),
            generation: target.generation,
            initial_context: None,
            fork_cutoff: None,
            applied_receipt: None,
            outcome: None,
        };
        match self
            .begin_lifecycle_effect(record)
            .await
            .map_err(map_lifecycle_host_error)?
        {
            CompleteAgentLifecycleBegin::Settled(outcome) => {
                return stored_rebind_outcome(previous_binding, target.generation, *outcome);
            }
            CompleteAgentLifecycleBegin::InspectionRequired => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Rebind lifecycle effect requires inspection".to_owned(),
                });
            }
            CompleteAgentLifecycleBegin::Dispatch => {}
        }
        let binding = self
            .provision_runtime_binding(&context, &target, previous_binding.source.clone())
            .await
            .map_err(map_applied_lifecycle_host_error)?;
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let receipt = service
            .resume(ResumeAgentCommand {
                meta: lifecycle_meta(&context, target.generation)?,
                source: previous_binding.source.clone(),
            })
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            })?;
        if !receipt_applied_success(&receipt.state) {
            return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "Complete Agent Rebind resume is not terminal".to_owned(),
            });
        }
        self.observe_lifecycle_applied_receipt(
            &context.effect_id,
            CompleteAgentLifecycleAppliedReceipt::Agent(applied_agent_receipt_from_command(
                &receipt,
            )?),
        )
        .await
        .map_err(map_applied_receipt_observation_error)?;
        let resumed = self
            .finish_applied_resume(context, binding.clone(), receipt)
            .await?;
        Ok(ManagedRuntimeRebindOutcome {
            receipt: resumed.receipt,
            previous_binding,
            binding,
        })
    }

    async fn fork(
        &self,
        context: ManagedRuntimeDispatchContext,
        parent: ManagedRuntimeAgentBinding,
        child_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
    ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        self.runtime_binding(&context.runtime_thread_id, &parent)
            .await
            .map_err(map_lifecycle_host_error)?;
        let record = CompleteAgentLifecycleEffectRecord {
            effect_id: context.effect_id.clone(),
            runtime_thread_id: context.runtime_thread_id.clone(),
            child_thread_id: Some(child_thread_id.clone()),
            kind: CompleteAgentLifecycleOperationKind::Fork,
            target: target.target.clone(),
            generation: target.generation,
            initial_context: None,
            fork_cutoff: Some(cutoff.clone()),
            applied_receipt: None,
            outcome: None,
        };
        match self
            .begin_lifecycle_effect(record)
            .await
            .map_err(map_lifecycle_host_error)?
        {
            CompleteAgentLifecycleBegin::Settled(outcome) => {
                return stored_fork_outcome(target.generation, *outcome);
            }
            CompleteAgentLifecycleBegin::InspectionRequired => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Fork lifecycle effect requires inspection".to_owned(),
                });
            }
            CompleteAgentLifecycleBegin::Dispatch => {}
        }
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let receipt = service
            .fork(ForkAgentCommand {
                meta: lifecycle_meta(&context, target.generation)?,
                source: parent.source.clone(),
                requested_child_source: None,
                cutoff: cutoff.clone(),
            })
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            })?;
        if !receipt_applied_success(&receipt.state) {
            return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "Complete Agent Fork is not terminal".to_owned(),
            });
        }
        let child_source = receipt.child_source.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child source".to_owned(),
            }
        })?;
        let history_digest = receipt.child_history_digest.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child history digest".to_owned(),
            }
        })?;
        self.observe_lifecycle_applied_receipt(
            &context.effect_id,
            CompleteAgentLifecycleAppliedReceipt::Fork(applied_fork_receipt_from_command(
                &receipt,
            )?),
        )
        .await
        .map_err(|error| {
            map_fork_applied_evidence_error(
                error,
                child_source.clone(),
                Some(history_digest.clone()),
            )
        })?;
        self.finish_applied_fork(context, target, parent, child_thread_id, cutoff, receipt)
            .await
    }

    async fn execute(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError> {
        let (binding_id, _) = self
            .runtime_binding(&context.runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        let now_ms = context.now_ms.max(current_time_ms());
        let lease = self
            .acquire_binding_lease(
                &binding_id,
                binding.generation,
                context.dispatch_owner,
                now_ms,
                now_ms.saturating_add(context.lease_duration_ms),
            )
            .await
            .map_err(map_lifecycle_host_error)?;
        self.dispatch_execute(&lease, &binding_id, command)
            .await
            .map_err(map_lifecycle_host_error)
    }

    async fn inspect(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: Option<ManagedRuntimeAgentBinding>,
    ) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError> {
        if let Some(record) = self
            .lifecycle_effect(&context.effect_id)
            .await
            .map_err(map_lifecycle_host_error)?
        {
            if let Some(outcome) = record.outcome {
                return match record.kind {
                    CompleteAgentLifecycleOperationKind::Create => {
                        let target = self
                            .runtime_target(&record.runtime_thread_id)
                            .await
                            .map_err(map_lifecycle_host_error)?;
                        self.stored_create_outcome(&target, outcome)
                            .await
                            .map(ManagedRuntimeLifecycleInspection::CreateApplied)
                            .map_err(map_lifecycle_host_error)
                    }
                    CompleteAgentLifecycleOperationKind::Resume => stored_resume_outcome(
                        binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?,
                        outcome,
                    )
                    .map(ManagedRuntimeLifecycleInspection::ResumeApplied),
                    CompleteAgentLifecycleOperationKind::Rebind => {
                        let target = self
                            .runtime_target(&record.runtime_thread_id)
                            .await
                            .map_err(map_lifecycle_host_error)?;
                        stored_rebind_outcome(
                            binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?,
                            target.generation,
                            outcome,
                        )
                        .map(ManagedRuntimeLifecycleInspection::RebindApplied)
                    }
                    CompleteAgentLifecycleOperationKind::Fork => {
                        { stored_fork_outcome(record.generation, outcome) }
                            .map(ManagedRuntimeLifecycleInspection::ForkApplied)
                    }
                    CompleteAgentLifecycleOperationKind::Execute => stored_agent_receipt(outcome)
                        .map(ManagedRuntimeLifecycleInspection::CommandApplied),
                };
            }
            let target = self
                .runtime_target(&record.runtime_thread_id)
                .await
                .map_err(map_lifecycle_host_error)?;
            let service = self
                .service(&target.target)
                .await
                .map_err(map_lifecycle_host_error)?;
            let inspection = service
                .inspect(context.effect_id.clone())
                .await
                .map_err(|error| ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: error.to_string(),
                })?;
            if !inspection.validate() {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "lifecycle inspection returned mismatched effect coordinates"
                        .to_owned(),
                });
            }
            return match inspection.state {
                AgentEffectInspectionState::NotApplied => {
                    Ok(ManagedRuntimeLifecycleInspection::NotApplied)
                }
                AgentEffectInspectionState::Accepted { .. } => {
                    Ok(ManagedRuntimeLifecycleInspection::Accepted)
                }
                AgentEffectInspectionState::Unknown => {
                    Ok(ManagedRuntimeLifecycleInspection::Unknown)
                }
                AgentEffectInspectionState::Applied { outcome } => {
                    self.finish_inspected_lifecycle_effect(
                        context, binding, record, service, outcome,
                    )
                    .await
                }
            };
        }
        let binding = binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?;
        let (binding_id, _) = self
            .runtime_binding(&context.runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        let now_ms = context.now_ms.max(current_time_ms());
        let lease = self
            .acquire_binding_lease(
                &binding_id,
                binding.generation,
                context.dispatch_owner,
                now_ms,
                now_ms.saturating_add(context.lease_duration_ms),
            )
            .await
            .map_err(map_lifecycle_host_error)?;
        let inspection = self
            .reconcile_effect(&lease, &context.effect_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        inspection_to_runtime(inspection, &context.effect_id, binding.source)
    }

    async fn read(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentReadQuery,
    ) -> Result<agentdash_agent_service_api::AgentSnapshot, ManagedRuntimeLifecycleError> {
        let (_, host_binding) = self
            .runtime_binding(&runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        if query.source != host_binding.source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "read source does not match Runtime binding".to_owned(),
            });
        }
        self.service(&host_binding.target)
            .await
            .map_err(map_lifecycle_host_error)?
            .read(query)
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::Unavailable {
                reason: error.to_string(),
            })
    }

    async fn changes(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, ManagedRuntimeLifecycleError> {
        let (_, host_binding) = self
            .runtime_binding(&runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        if query.source != host_binding.source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "changes source does not match Runtime binding".to_owned(),
            });
        }
        self.service(&host_binding.target)
            .await
            .map_err(map_lifecycle_host_error)?
            .changes(query)
            .await
            .map_err(|error| ManagedRuntimeLifecycleError::Unavailable {
                reason: error.to_string(),
            })
    }

    async fn is_ready(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<bool, ManagedRuntimeLifecycleError> {
        let (_, binding) = self
            .runtime_binding(&runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        Ok(binding.dispatch_admitted())
    }
}

impl CompleteAgentHost {
    async fn finish_applied_create(
        &self,
        context: ManagedRuntimeDispatchContext,
        target: CompleteAgentRuntimeTarget,
        service: Arc<dyn CompleteAgentService>,
        receipt: AgentCommandReceipt,
    ) -> Result<ManagedRuntimeCreateOutcome, ManagedRuntimeLifecycleError> {
        let binding = self
            .provision_runtime_binding(&context, &target, receipt.source.clone())
            .await
            .map_err(map_applied_lifecycle_host_error)?;
        self.settle_lifecycle_effect(
            &context.effect_id,
            CompleteAgentLifecycleOutcome::Agent {
                receipt: receipt.clone(),
                applied_surface: Some(binding.applied_surface.clone()),
            },
        )
        .await
        .map_err(map_applied_lifecycle_host_error)?;
        let descriptor = service.describe().await.map_err(|error| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            }
        })?;
        Ok(ManagedRuntimeCreateOutcome {
            initial_context: receipt.initial_context.clone(),
            receipt,
            binding,
            contribution_fidelity: descriptor.profile.initial_context.contribution_fidelity,
        })
    }

    async fn finish_applied_resume(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
        receipt: AgentCommandReceipt,
    ) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError> {
        if receipt.source != binding.source {
            return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "Resume applied receipt source does not match the durable binding"
                    .to_owned(),
            });
        }
        self.settle_lifecycle_effect(
            &context.effect_id,
            CompleteAgentLifecycleOutcome::Agent {
                receipt: receipt.clone(),
                applied_surface: Some(binding.applied_surface.clone()),
            },
        )
        .await
        .map_err(map_applied_lifecycle_host_error)?;
        Ok(ManagedRuntimeResumeOutcome { receipt, binding })
    }

    async fn finish_applied_fork(
        &self,
        context: ManagedRuntimeDispatchContext,
        target: CompleteAgentRuntimeTarget,
        parent: ManagedRuntimeAgentBinding,
        child_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
        receipt: ForkAgentReceipt,
    ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
        let child_source = receipt.child_source.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child source".to_owned(),
            }
        })?;
        let history_digest = receipt.child_history_digest.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child history digest".to_owned(),
            }
        })?;
        if receipt.parent_source != parent.source || receipt.cutoff != cutoff {
            return Err(ManagedRuntimeLifecycleError::ForkChildKnown {
                child_source,
                child_history_digest: Some(history_digest),
                reason: "Fork receipt coordinates do not match the durable intent".to_owned(),
            });
        }
        let mut child_target = target;
        child_target.runtime_thread_id = child_thread_id;
        child_target.callbacks.route_id = AgentCallbackRouteId::new(format!(
            "runtime-callback:{}",
            child_target.runtime_thread_id
        ))
        .map_err(
            |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                child_source: child_source.clone(),
                child_history_digest: Some(history_digest.clone()),
                reason: error.to_string(),
            },
        )?;
        self.register_runtime_target(child_target.clone())
            .await
            .map_err(
                |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                    child_source: child_source.clone(),
                    child_history_digest: Some(history_digest.clone()),
                    reason: error.to_string(),
                },
            )?;
        let child_binding = self
            .provision_runtime_binding(&context, &child_target, child_source.clone())
            .await
            .map_err(
                |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                    child_source: child_source.clone(),
                    child_history_digest: Some(history_digest.clone()),
                    reason: error.to_string(),
                },
            )?;
        self.settle_lifecycle_effect(
            &context.effect_id,
            CompleteAgentLifecycleOutcome::Fork {
                receipt: receipt.clone(),
                child_applied_surface: Some(child_binding.applied_surface.clone()),
            },
        )
        .await
        .map_err(
            |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                child_source: child_source.clone(),
                child_history_digest: Some(history_digest.clone()),
                reason: error.to_string(),
            },
        )?;
        Ok(ManagedRuntimeForkOutcome {
            receipt,
            child_binding,
            child_history_digest: history_digest,
        })
    }

    async fn finish_inspected_lifecycle_effect(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: Option<ManagedRuntimeAgentBinding>,
        record: CompleteAgentLifecycleEffectRecord,
        service: Arc<dyn CompleteAgentService>,
        outcome: AgentAppliedEffectOutcome,
    ) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError> {
        match (record.kind, outcome) {
            (
                CompleteAgentLifecycleOperationKind::Create,
                AgentAppliedEffectOutcome::Create { receipt },
            ) => {
                self.observe_lifecycle_applied_receipt(
                    &context.effect_id,
                    CompleteAgentLifecycleAppliedReceipt::Agent(receipt.clone()),
                )
                .await
                .map_err(map_applied_receipt_observation_error)?;
                let receipt = agent_receipt_from_inspection(receipt);
                let target = self
                    .runtime_target(&record.runtime_thread_id)
                    .await
                    .map_err(map_lifecycle_host_error)?;
                self.finish_applied_create(context, target, service, receipt)
                    .await
                    .map(ManagedRuntimeLifecycleInspection::CreateApplied)
            }
            (
                CompleteAgentLifecycleOperationKind::Resume,
                AgentAppliedEffectOutcome::Resume { receipt },
            ) => {
                self.observe_lifecycle_applied_receipt(
                    &context.effect_id,
                    CompleteAgentLifecycleAppliedReceipt::Agent(receipt.clone()),
                )
                .await
                .map_err(map_applied_receipt_observation_error)?;
                let receipt = agent_receipt_from_inspection(receipt);
                self.finish_applied_resume(
                    context,
                    binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?,
                    receipt,
                )
                .await
                .map(ManagedRuntimeLifecycleInspection::ResumeApplied)
            }
            (
                CompleteAgentLifecycleOperationKind::Rebind,
                AgentAppliedEffectOutcome::Resume { receipt },
            ) => {
                self.observe_lifecycle_applied_receipt(
                    &context.effect_id,
                    CompleteAgentLifecycleAppliedReceipt::Agent(receipt.clone()),
                )
                .await
                .map_err(map_applied_receipt_observation_error)?;
                let previous_binding = binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?;
                let target = self
                    .runtime_target(&record.runtime_thread_id)
                    .await
                    .map_err(map_lifecycle_host_error)?;
                let recovered_binding_id =
                    runtime_binding_id(&record.runtime_thread_id, record.generation)
                        .map_err(map_lifecycle_host_error)?;
                let recovered_binding = self
                    .binding(&recovered_binding_id)
                    .await
                    .map_err(map_lifecycle_host_error)?
                    .and_then(managed_binding_from_host)
                    .ok_or(ManagedRuntimeLifecycleError::NotFound)?;
                let receipt = agent_receipt_from_inspection(receipt);
                let resumed = self
                    .finish_applied_resume(context, recovered_binding.clone(), receipt)
                    .await?;
                if target.generation != recovered_binding.generation {
                    return Err(ManagedRuntimeLifecycleError::StaleGeneration);
                }
                Ok(ManagedRuntimeLifecycleInspection::RebindApplied(
                    ManagedRuntimeRebindOutcome {
                        receipt: resumed.receipt,
                        previous_binding,
                        binding: recovered_binding,
                    },
                ))
            }
            (
                CompleteAgentLifecycleOperationKind::Fork,
                AgentAppliedEffectOutcome::Fork { receipt },
            ) => {
                let child_source = receipt.child_source.clone();
                let child_history_digest = Some(receipt.child_history_digest.clone());
                self.observe_lifecycle_applied_receipt(
                    &context.effect_id,
                    CompleteAgentLifecycleAppliedReceipt::Fork(receipt.clone()),
                )
                .await
                .map_err(|error| {
                    map_fork_applied_evidence_error(
                        error,
                        child_source.clone(),
                        child_history_digest.clone(),
                    )
                })?;
                let receipt = fork_receipt_from_inspection(receipt);
                let target = self
                    .runtime_target(&record.runtime_thread_id)
                    .await
                    .map_err(
                        |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                            child_source: child_source.clone(),
                            child_history_digest: child_history_digest.clone(),
                            reason: error.to_string(),
                        },
                    )?;
                self.finish_applied_fork(
                    context,
                    target,
                    binding.ok_or_else(|| ManagedRuntimeLifecycleError::ForkChildKnown {
                        child_source: child_source.clone(),
                        child_history_digest: child_history_digest.clone(),
                        reason: "Fork inspection has no durable parent binding".to_owned(),
                    })?,
                    record.child_thread_id.ok_or_else(|| {
                        ManagedRuntimeLifecycleError::ForkChildKnown {
                            child_source: child_source.clone(),
                            child_history_digest: child_history_digest.clone(),
                            reason: "Fork inspection has no durable child thread coordinate"
                                .to_owned(),
                        }
                    })?,
                    record.fork_cutoff.ok_or_else(|| {
                        ManagedRuntimeLifecycleError::ForkChildKnown {
                            child_source: child_source.clone(),
                            child_history_digest: child_history_digest.clone(),
                            reason: "Fork inspection has no durable cutoff coordinate".to_owned(),
                        }
                    })?,
                    receipt,
                )
                .await
                .map(ManagedRuntimeLifecycleInspection::ForkApplied)
            }
            (
                CompleteAgentLifecycleOperationKind::Execute,
                AgentAppliedEffectOutcome::Command { receipt },
            ) => Ok(ManagedRuntimeLifecycleInspection::CommandApplied(
                agent_receipt_from_inspection(receipt),
            )),
            (CompleteAgentLifecycleOperationKind::Fork, outcome) => {
                Err(fork_inspection_kind_mismatch(outcome))
            }
            _ => Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "lifecycle inspection outcome does not match durable operation kind"
                    .to_owned(),
            }),
        }
    }

    async fn stored_create_outcome(
        &self,
        target: &CompleteAgentRuntimeTarget,
        outcome: CompleteAgentLifecycleOutcome,
    ) -> Result<ManagedRuntimeCreateOutcome, CompleteAgentHostError> {
        let CompleteAgentLifecycleOutcome::Agent {
            receipt,
            applied_surface: Some(applied_surface),
        } = outcome
        else {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Create lifecycle outcome has no applied binding".to_owned(),
            });
        };
        let service = self.service(&target.target).await?;
        let descriptor = service.describe().await?;
        Ok(ManagedRuntimeCreateOutcome {
            binding: ManagedRuntimeAgentBinding {
                source: receipt.source.clone(),
                generation: target.generation,
                applied_surface,
            },
            initial_context: receipt.initial_context.clone(),
            receipt,
            contribution_fidelity: descriptor.profile.initial_context.contribution_fidelity,
        })
    }
}

fn runtime_binding_id(
    runtime_thread_id: &RuntimeThreadId,
    generation: AgentBindingGeneration,
) -> Result<CompleteAgentBindingId, CompleteAgentHostError> {
    CompleteAgentBindingId::new(format!(
        "runtime-binding:{runtime_thread_id}:{}",
        generation.0
    ))
}

fn callback_route_id(
    runtime_thread_id: &RuntimeThreadId,
    target: &CompleteAgentBindingTarget,
    generation: AgentBindingGeneration,
    bound_surface: &BoundAgentSurface,
) -> Result<AgentCallbackRouteId, CompleteAgentHostError> {
    let mut digest = Sha256::new();
    digest.update(runtime_thread_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(target.live_attachment_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(target.host_incarnation_id().as_bytes());
    digest.update([0]);
    digest.update(generation.0.to_be_bytes());
    digest.update([0]);
    digest.update(bound_surface.digest.as_str().as_bytes());
    AgentCallbackRouteId::new(format!("runtime-callback:{:x}", digest.finalize())).map_err(
        |error| CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        },
    )
}

fn lifecycle_meta(
    context: &ManagedRuntimeDispatchContext,
    generation: AgentBindingGeneration,
) -> Result<AgentCommandMeta, ManagedRuntimeLifecycleError> {
    Ok(AgentCommandMeta {
        command_id: derived_command_id(&context.effect_id, "lifecycle")
            .map_err(map_lifecycle_host_error)?,
        effect_id: context.effect_id.clone(),
        idempotency_key: derived_idempotency_key(&context.effect_id, "lifecycle")
            .map_err(map_lifecycle_host_error)?,
        binding_generation: generation,
        expected_snapshot_revision: None,
    })
}

fn derived_command_id(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentCommandId, CompleteAgentHostError> {
    AgentCommandId::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

fn derived_effect_id(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentEffectIdentity, CompleteAgentHostError> {
    AgentEffectIdentity::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

fn derived_idempotency_key(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentIdempotencyKey, CompleteAgentHostError> {
    AgentIdempotencyKey::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

fn receipt_applied_success(state: &AgentReceiptState) -> bool {
    matches!(
        state,
        AgentReceiptState::AlreadyApplied {
            terminal: None
                | Some(
                    agentdash_agent_service_api::AgentTerminalOutcome::Succeeded
                        | agentdash_agent_service_api::AgentTerminalOutcome::Closed
                )
        } | AgentReceiptState::Terminal {
            outcome: agentdash_agent_service_api::AgentTerminalOutcome::Succeeded
                | agentdash_agent_service_api::AgentTerminalOutcome::Closed
        }
    )
}

fn stored_agent_receipt(
    outcome: CompleteAgentLifecycleOutcome,
) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError> {
    match outcome {
        CompleteAgentLifecycleOutcome::Agent { receipt, .. } => Ok(receipt),
        CompleteAgentLifecycleOutcome::Fork { .. } => Err(ManagedRuntimeLifecycleError::Invalid {
            reason: "expected Agent receipt, found Fork receipt".to_owned(),
        }),
    }
}

fn stored_resume_outcome(
    binding: ManagedRuntimeAgentBinding,
    outcome: CompleteAgentLifecycleOutcome,
) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError> {
    stored_agent_receipt(outcome).map(|receipt| ManagedRuntimeResumeOutcome { receipt, binding })
}

fn stored_rebind_outcome(
    previous_binding: ManagedRuntimeAgentBinding,
    generation: AgentBindingGeneration,
    outcome: CompleteAgentLifecycleOutcome,
) -> Result<ManagedRuntimeRebindOutcome, ManagedRuntimeLifecycleError> {
    let CompleteAgentLifecycleOutcome::Agent {
        receipt,
        applied_surface: Some(applied_surface),
    } = outcome
    else {
        return Err(ManagedRuntimeLifecycleError::Invalid {
            reason: "Rebind lifecycle outcome has no recovered applied binding".to_owned(),
        });
    };
    if receipt.source != previous_binding.source {
        return Err(ManagedRuntimeLifecycleError::Invalid {
            reason: "Rebind lifecycle outcome changed the Agent source coordinate".to_owned(),
        });
    }
    Ok(ManagedRuntimeRebindOutcome {
        binding: ManagedRuntimeAgentBinding {
            source: receipt.source.clone(),
            generation,
            applied_surface,
        },
        previous_binding,
        receipt,
    })
}

fn managed_binding_from_host(binding: CompleteAgentBinding) -> Option<ManagedRuntimeAgentBinding> {
    Some(ManagedRuntimeAgentBinding {
        source: binding.source,
        generation: binding.generation,
        applied_surface: binding.applied_surface?,
    })
}

fn stored_fork_outcome(
    generation: AgentBindingGeneration,
    outcome: CompleteAgentLifecycleOutcome,
) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
    let CompleteAgentLifecycleOutcome::Fork {
        receipt,
        child_applied_surface: Some(applied_surface),
    } = outcome
    else {
        return Err(ManagedRuntimeLifecycleError::Invalid {
            reason: "Fork lifecycle outcome has no applied child binding".to_owned(),
        });
    };
    let child_source =
        receipt
            .child_source
            .clone()
            .ok_or_else(|| ManagedRuntimeLifecycleError::Invalid {
                reason: "Fork lifecycle outcome has no child source".to_owned(),
            })?;
    let child_history_digest = receipt.child_history_digest.clone().ok_or_else(|| {
        ManagedRuntimeLifecycleError::Invalid {
            reason: "Fork lifecycle outcome has no child history digest".to_owned(),
        }
    })?;
    Ok(ManagedRuntimeForkOutcome {
        child_binding: ManagedRuntimeAgentBinding {
            source: child_source,
            generation,
            applied_surface,
        },
        receipt,
        child_history_digest,
    })
}

fn inspection_to_runtime(
    inspection: AgentEffectInspection,
    _effect_id: &AgentEffectIdentity,
    source: AgentSourceCoordinate,
) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError> {
    Ok(match inspection.state {
        AgentEffectInspectionState::NotApplied => ManagedRuntimeLifecycleInspection::NotApplied,
        AgentEffectInspectionState::Accepted { .. } => ManagedRuntimeLifecycleInspection::Accepted,
        AgentEffectInspectionState::Unknown => ManagedRuntimeLifecycleInspection::Unknown,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { receipt },
        } if receipt.source == source => ManagedRuntimeLifecycleInspection::CommandApplied(
            agent_receipt_from_inspection(receipt),
        ),
        AgentEffectInspectionState::Applied { .. } => {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "command inspection returned mismatched applied outcome".to_owned(),
            });
        }
    })
}

fn agent_receipt_from_inspection(receipt: AppliedAgentCommandReceipt) -> AgentCommandReceipt {
    AgentCommandReceipt {
        command_id: receipt.command_id,
        effect_id: receipt.effect_id,
        source: receipt.source,
        state: AgentReceiptState::AlreadyApplied {
            terminal: receipt.terminal,
        },
        snapshot_revision: receipt.snapshot_revision,
        initial_context: receipt.initial_context,
    }
}

fn applied_agent_receipt_from_command(
    receipt: &AgentCommandReceipt,
) -> Result<AppliedAgentCommandReceipt, ManagedRuntimeLifecycleError> {
    let terminal = match &receipt.state {
        AgentReceiptState::AlreadyApplied { terminal } => *terminal,
        AgentReceiptState::Terminal { outcome } => Some(*outcome),
        _ => {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "lifecycle receipt is not applied".to_owned(),
            });
        }
    };
    Ok(AppliedAgentCommandReceipt {
        command_id: receipt.command_id.clone(),
        effect_id: receipt.effect_id.clone(),
        source: receipt.source.clone(),
        terminal,
        snapshot_revision: receipt.snapshot_revision,
        initial_context: receipt.initial_context.clone(),
    })
}

fn fork_receipt_from_inspection(receipt: AppliedForkAgentReceipt) -> ForkAgentReceipt {
    ForkAgentReceipt {
        command_id: receipt.command_id,
        effect_id: receipt.effect_id,
        parent_source: receipt.parent_source,
        child_source: Some(receipt.child_source),
        cutoff: receipt.cutoff,
        child_history_digest: Some(receipt.child_history_digest),
        state: AgentReceiptState::AlreadyApplied {
            terminal: receipt.terminal,
        },
    }
}

fn applied_fork_receipt_from_command(
    receipt: &ForkAgentReceipt,
) -> Result<AppliedForkAgentReceipt, ManagedRuntimeLifecycleError> {
    let terminal = match &receipt.state {
        AgentReceiptState::AlreadyApplied { terminal } => *terminal,
        AgentReceiptState::Terminal { outcome } => Some(*outcome),
        _ => {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "Fork lifecycle receipt is not applied".to_owned(),
            });
        }
    };
    Ok(AppliedForkAgentReceipt {
        command_id: receipt.command_id.clone(),
        effect_id: receipt.effect_id.clone(),
        parent_source: receipt.parent_source.clone(),
        child_source: receipt.child_source.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child source".to_owned(),
            }
        })?,
        cutoff: receipt.cutoff.clone(),
        child_history_digest: receipt.child_history_digest.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "terminal Fork has no child history digest".to_owned(),
            }
        })?,
        terminal,
    })
}

fn fork_inspection_kind_mismatch(
    _outcome: AgentAppliedEffectOutcome,
) -> ManagedRuntimeLifecycleError {
    ManagedRuntimeLifecycleError::Invalid {
        reason: "Fork inspection outcome does not contain a Fork receipt".to_owned(),
    }
}

fn map_lifecycle_host_error(error: CompleteAgentHostError) -> ManagedRuntimeLifecycleError {
    match error {
        CompleteAgentHostError::UnknownBinding { .. }
        | CompleteAgentHostError::UnknownEffect { .. } => ManagedRuntimeLifecycleError::NotFound,
        CompleteAgentHostError::StaleGeneration { .. } => {
            ManagedRuntimeLifecycleError::StaleGeneration
        }
        CompleteAgentHostError::EffectPending { .. }
        | CompleteAgentHostError::LeaseConflict
        | CompleteAgentHostError::LeaseExpired
        | CompleteAgentHostError::StaleLeaseOutcome => {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: error.to_string(),
            }
        }
        CompleteAgentHostError::Store(error) => ManagedRuntimeLifecycleError::Persistence {
            reason: error.to_string(),
        },
        CompleteAgentHostError::Service(error) => ManagedRuntimeLifecycleError::Unavailable {
            reason: error.to_string(),
        },
        CompleteAgentHostError::UnavailableAttachment { .. } => {
            ManagedRuntimeLifecycleError::Unavailable {
                reason: error.to_string(),
            }
        }
        CompleteAgentHostError::LiveCatalog(error) => ManagedRuntimeLifecycleError::Invalid {
            reason: error.to_string(),
        },
        CompleteAgentHostError::DispatchRejected { reason }
        | CompleteAgentHostError::Invariant { reason }
        | CompleteAgentHostError::Encoding { reason } => {
            ManagedRuntimeLifecycleError::Invalid { reason }
        }
        CompleteAgentHostError::EffectIdentityConflict
        | CompleteAgentHostError::ProvisioningConflict
        | CompleteAgentHostError::EffectObservationConflict { .. }
        | CompleteAgentHostError::EffectEvidenceConflict { .. }
        | CompleteAgentHostError::EffectNotApplied { .. } => {
            ManagedRuntimeLifecycleError::Invalid {
                reason: error.to_string(),
            }
        }
    }
}

fn map_applied_lifecycle_host_error(error: CompleteAgentHostError) -> ManagedRuntimeLifecycleError {
    ManagedRuntimeLifecycleError::InspectionRequired {
        reason: error.to_string(),
    }
}

fn map_applied_receipt_observation_error(
    error: CompleteAgentHostError,
) -> ManagedRuntimeLifecycleError {
    let reason = error.to_string();
    match error {
        CompleteAgentHostError::Store(_) => {
            ManagedRuntimeLifecycleError::InspectionRequired { reason }
        }
        _ => ManagedRuntimeLifecycleError::Invalid { reason },
    }
}

fn map_fork_applied_evidence_error(
    error: CompleteAgentHostError,
    child_source: AgentSourceCoordinate,
    child_history_digest: Option<AgentPayloadDigest>,
) -> ManagedRuntimeLifecycleError {
    let reason = error.to_string();
    match error {
        CompleteAgentHostError::Store(_) => ManagedRuntimeLifecycleError::ForkInspectionRequired {
            child_source,
            child_history_digest,
            reason,
        },
        _ => ManagedRuntimeLifecycleError::ForkChildKnown {
            child_source,
            child_history_digest,
            reason,
        },
    }
}

fn ensure_generation(
    binding: &CompleteAgentBinding,
    actual: AgentBindingGeneration,
) -> Result<(), CompleteAgentHostError> {
    if binding.generation != actual {
        return Err(CompleteAgentHostError::StaleGeneration {
            expected: binding.generation,
            actual,
        });
    }
    Ok(())
}

pub(crate) fn validate_service_descriptor(
    descriptor: &AgentServiceDescriptor,
) -> Result<(), CompleteAgentHostError> {
    validate_surface_profile(&descriptor.profile.surface)
}

pub(crate) fn runtime_offer_from_descriptor(
    descriptor: &AgentServiceDescriptor,
) -> Result<AgentRuntimeOffer, CompleteAgentHostError> {
    let surface = &descriptor.profile.surface;
    validate_surface_profile(surface)?;
    Ok(AgentRuntimeOffer {
        profile_digest: descriptor.profile_digest.clone(),
        contributions: surface.facets.clone(),
    })
}

fn validate_surface_profile(surface: &AgentSurfaceProfile) -> Result<(), CompleteAgentHostError> {
    for facet in &surface.facets {
        if facet.routes.is_empty() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet has no materialization route".to_owned(),
            });
        }
        if facet.fidelity == SemanticFidelity::Unsupported {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet cannot declare unsupported fidelity".to_owned(),
            });
        }
        if facet
            .semantics
            .required_causal_route()
            .is_some_and(|required| !facet.routes.contains(&required))
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet omits its semantic causal route".to_owned(),
            });
        }
        match &facet.semantics {
            AgentSurfaceSemanticFacet::Tool(tool)
                if tool.invocation == SemanticFidelity::Unsupported
                    || tool.update
                        == agentdash_agent_service_api::AgentToolUpdateSemantics::Unsupported =>
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "tool capability facet declares unsupported semantics".to_owned(),
                });
            }
            AgentSurfaceSemanticFacet::Hook(hook)
                if matches!(
                    hook.blocking,
                    agentdash_agent_service_api::AgentHookBlockingSemantics::Blocking {
                        fidelity: SemanticFidelity::Unsupported
                    }
                ) || hook
                    .mutations
                    .values()
                    .chain(hook.effects.values())
                    .any(|fidelity| *fidelity == SemanticFidelity::Unsupported) =>
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "hook capability facet declares unsupported semantics".to_owned(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_lease_state(
    state: &CompleteAgentHostFacts,
    lease: &CompleteAgentBindingLease,
    now_ms: u64,
) -> Result<(), CompleteAgentHostError> {
    let binding = state.bindings.get(&lease.binding_id).ok_or_else(|| {
        CompleteAgentHostError::UnknownBinding {
            binding_id: lease.binding_id.as_str().to_owned(),
        }
    })?;
    ensure_generation(binding, lease.generation)?;
    if matches!(
        binding.state,
        CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
    ) {
        return Err(CompleteAgentHostError::DispatchRejected {
            reason: "lost or closed binding cannot deliver effects".to_owned(),
        });
    }
    let current = state
        .leases
        .get(&lease.binding_id)
        .ok_or(CompleteAgentHostError::LeaseConflict)?;
    if current != lease {
        return Err(CompleteAgentHostError::LeaseConflict);
    }
    if current.expires_at_ms <= now_ms {
        return Err(CompleteAgentHostError::LeaseExpired);
    }
    Ok(())
}

fn validate_effect_fence(
    state: &CompleteAgentHostFacts,
    lease: &CompleteAgentBindingLease,
    record: &CompleteAgentEffectRecord,
    now_ms: u64,
) -> Result<(), CompleteAgentHostError> {
    validate_lease_state(state, lease, now_ms)?;
    if record.binding_id != lease.binding_id
        || record.generation != lease.generation
        || record.delivery_epoch != lease.epoch
    {
        return Err(CompleteAgentHostError::StaleLeaseOutcome);
    }
    Ok(())
}

fn current_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn ensure_same_effect(
    existing: &CompleteAgentEffectRecord,
    candidate: &CompleteAgentEffectRecord,
) -> Result<(), CompleteAgentHostError> {
    if existing.command_id != candidate.command_id
        || existing.binding_id != candidate.binding_id
        || existing.generation != candidate.generation
        || existing.source != candidate.source
        || existing.payload_digest != candidate.payload_digest
    {
        return Err(CompleteAgentHostError::EffectIdentityConflict);
    }
    Ok(())
}

fn observe_effect_state(
    record: &mut CompleteAgentEffectRecord,
    observed: CompleteAgentEffectState,
) -> Result<(), CompleteAgentHostError> {
    let current = record.state;
    if current == observed {
        return Ok(());
    }
    let advances = match current {
        CompleteAgentEffectState::Dispatching => true,
        CompleteAgentEffectState::Unknown => matches!(
            observed,
            CompleteAgentEffectState::Accepted
                | CompleteAgentEffectState::Applied
                | CompleteAgentEffectState::Rejected
                | CompleteAgentEffectState::NotApplied
                | CompleteAgentEffectState::Lost
        ),
        CompleteAgentEffectState::Accepted => matches!(
            observed,
            CompleteAgentEffectState::Applied
                | CompleteAgentEffectState::Rejected
                | CompleteAgentEffectState::Lost
        ),
        CompleteAgentEffectState::NotApplied
        | CompleteAgentEffectState::Applied
        | CompleteAgentEffectState::Rejected
        | CompleteAgentEffectState::Lost => false,
    };
    if !advances {
        return Err(CompleteAgentHostError::EffectObservationConflict { current, observed });
    }
    record.state = observed;
    Ok(())
}

fn begin_effect_redispatch(
    record: &mut CompleteAgentEffectRecord,
    delivery_epoch: u64,
) -> Result<(), CompleteAgentHostError> {
    if record.state != CompleteAgentEffectState::NotApplied {
        return Err(CompleteAgentHostError::Invariant {
            reason: "effect redispatch requires a confirmed NotApplied observation".to_owned(),
        });
    }
    let inspection =
        record
            .inspection
            .as_ref()
            .ok_or_else(|| CompleteAgentHostError::Invariant {
                reason: "effect redispatch requires durable NotApplied inspection evidence"
                    .to_owned(),
            })?;
    if inspection_state(&inspection.state) != CompleteAgentEffectState::NotApplied {
        return Err(CompleteAgentHostError::Invariant {
            reason: "effect redispatch inspection does not prove NotApplied".to_owned(),
        });
    }
    record
        .attempt_history
        .push(CompleteAgentEffectAttemptEvidence {
            dispatch_attempt: record.dispatch_attempt,
            delivery_epoch: record.delivery_epoch,
            state: record.state,
            receipt: record.receipt.take(),
            surface_receipt: record.surface_receipt.take(),
            inspection: record.inspection.take(),
        });
    record.dispatch_attempt = record.dispatch_attempt.checked_add(1).ok_or_else(|| {
        CompleteAgentHostError::Invariant {
            reason: "effect dispatch attempt is exhausted".to_owned(),
        }
    })?;
    record.delivery_epoch = delivery_epoch;
    record.state = CompleteAgentEffectState::Dispatching;
    Ok(())
}

fn ensure_same_observation_evidence<T: PartialEq>(
    current: CompleteAgentEffectState,
    observed: CompleteAgentEffectState,
    existing: Option<&T>,
    incoming: &T,
    effect_id: &AgentEffectIdentity,
) -> Result<(), CompleteAgentHostError> {
    if current == observed && existing.is_some_and(|existing| existing != incoming) {
        return Err(CompleteAgentHostError::EffectEvidenceConflict {
            effect_id: effect_id.clone(),
        });
    }
    Ok(())
}

fn receipt_state(state: &AgentReceiptState) -> CompleteAgentEffectState {
    match state {
        AgentReceiptState::Accepted => CompleteAgentEffectState::Accepted,
        AgentReceiptState::Rejected { .. } => CompleteAgentEffectState::Rejected,
        AgentReceiptState::AlreadyApplied { .. } | AgentReceiptState::Terminal { .. } => {
            CompleteAgentEffectState::Applied
        }
        AgentReceiptState::Unknown => CompleteAgentEffectState::Unknown,
    }
}

fn inspection_state(state: &AgentEffectInspectionState) -> CompleteAgentEffectState {
    match state {
        AgentEffectInspectionState::NotApplied => CompleteAgentEffectState::NotApplied,
        AgentEffectInspectionState::Accepted { .. } => CompleteAgentEffectState::Accepted,
        AgentEffectInspectionState::Applied { .. } => CompleteAgentEffectState::Applied,
        AgentEffectInspectionState::Unknown => CompleteAgentEffectState::Unknown,
    }
}

fn inspection_receipt(
    inspection: AgentEffectInspection,
    expected_command_id: AgentCommandId,
    expected_source: AgentSourceCoordinate,
) -> AgentCommandReceipt {
    let state = match inspection.state {
        AgentEffectInspectionState::NotApplied | AgentEffectInspectionState::Unknown => {
            AgentReceiptState::Unknown
        }
        AgentEffectInspectionState::Accepted { .. } => AgentReceiptState::Accepted,
        AgentEffectInspectionState::Applied {
            outcome:
                AgentAppliedEffectOutcome::Command { receipt }
                | AgentAppliedEffectOutcome::SurfaceRevoke { receipt },
        } => return agent_receipt_from_inspection(receipt),
        AgentEffectInspectionState::Applied { .. } => AgentReceiptState::Unknown,
    };
    AgentCommandReceipt {
        command_id: expected_command_id,
        effect_id: inspection.effect_id,
        source: expected_source,
        state,
        snapshot_revision: None,
        initial_context: None,
    }
}

fn inspection_state_source(state: &AgentEffectInspectionState) -> Option<&AgentSourceCoordinate> {
    match state {
        AgentEffectInspectionState::Accepted { source } => Some(source),
        AgentEffectInspectionState::Applied { outcome } => Some(outcome.source()),
        AgentEffectInspectionState::NotApplied | AgentEffectInspectionState::Unknown => None,
    }
}

fn payload_digest(value: &impl Serialize) -> Result<AgentPayloadDigest, CompleteAgentHostError> {
    let bytes = serde_json::to_vec(value).map_err(|error| CompleteAgentHostError::Encoding {
        reason: error.to_string(),
    })?;
    AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(bytes))).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet, VecDeque},
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use agentdash_agent_service_api::{
        AgentCapabilityProfile, AgentChangesQuery, AgentCommand, AgentCommandCapability,
        AgentCommandMeta, AgentCompactionMode, AgentConfigurationBoundary, AgentForkCapability,
        AgentForkCutoffKind, AgentHookBlockingSemantics, AgentHookPoint, AgentHookSemanticFacet,
        AgentHookTiming, AgentInput, AgentLifecycleCapability, AgentReadQuery,
        AgentServiceDefinitionId, AgentServiceErrorCode, AgentSourceChangeLevel,
        AgentSurfaceCapabilityFacet, AgentSurfaceProfile, AgentSurfaceRevision, AgentSurfaceRoute,
        AgentToolDelivery, AgentToolSemanticFacet, AgentToolUpdateSemantics,
        InitialContextAppliedEvidence, InitialContextProfile, SemanticFidelity,
    };
    use async_trait::async_trait;
    use tokio::sync::{Mutex, Notify};

    use crate::ProcessCompleteAgentLiveCatalog;

    use super::*;

    #[derive(Default)]
    struct FixtureHostRepository {
        snapshot: Mutex<CompleteAgentHostSnapshot>,
    }

    #[async_trait]
    impl CompleteAgentHostRepository for FixtureHostRepository {
        async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            Ok(self.snapshot.lock().await.clone())
        }

        async fn commit(
            &self,
            commit: CompleteAgentHostCommit,
        ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            let mut snapshot = self.snapshot.lock().await;
            crate::apply_complete_agent_host_commit(&mut snapshot, commit)
        }
    }

    struct FailSettlementOnceRepository {
        inner: Arc<FixtureHostRepository>,
        armed: AtomicBool,
    }

    #[async_trait]
    impl CompleteAgentHostRepository for FailSettlementOnceRepository {
        async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            self.inner.load().await
        }

        async fn commit(
            &self,
            commit: CompleteAgentHostCommit,
        ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            let is_revoke_settlement = commit
                .facts
                .effects
                .values()
                .any(|effect| effect.receipt.is_some())
                && commit
                    .facts
                    .bindings
                    .values()
                    .any(|binding| binding.state == CompleteAgentBindingState::PendingSurface);
            if is_revoke_settlement && self.armed.swap(false, Ordering::SeqCst) {
                return Err(CompleteAgentHostStoreError::Persistence {
                    reason: "simulated crash before atomic revoke settlement commit".to_owned(),
                });
            }
            self.inner.commit(commit).await
        }
    }

    #[derive(Clone, Copy)]
    enum LifecycleFailpoint {
        AppliedReceiptObservation,
        BindingProvision,
        RuntimeTargetProvision,
        SurfaceSettlement,
        LifecycleSettlement,
    }

    struct FailLifecycleCommitOnceRepository {
        inner: Arc<FixtureHostRepository>,
        failpoint: LifecycleFailpoint,
        armed: AtomicBool,
    }

    impl FailLifecycleCommitOnceRepository {
        fn arm(&self) {
            self.armed.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl CompleteAgentHostRepository for FailLifecycleCommitOnceRepository {
        async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            self.inner.load().await
        }

        async fn commit(
            &self,
            commit: CompleteAgentHostCommit,
        ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            if self.armed.load(Ordering::SeqCst) {
                let current = self.inner.load().await?;
                let should_fail = match self.failpoint {
                    LifecycleFailpoint::AppliedReceiptObservation => commit
                        .facts
                        .lifecycle_effects
                        .iter()
                        .any(|(effect_id, effect)| {
                            effect.applied_receipt.is_some()
                                && current
                                    .facts
                                    .lifecycle_effects
                                    .get(effect_id)
                                    .is_none_or(|current| current.applied_receipt.is_none())
                        }),
                    LifecycleFailpoint::BindingProvision => {
                        commit.facts.bindings.len() > current.facts.bindings.len()
                    }
                    LifecycleFailpoint::RuntimeTargetProvision => {
                        commit.facts.runtime_targets.len() > current.facts.runtime_targets.len()
                    }
                    LifecycleFailpoint::SurfaceSettlement => {
                        commit.facts.effects.iter().any(|(effect_id, effect)| {
                            effect.surface_receipt.is_some()
                                && current
                                    .facts
                                    .effects
                                    .get(effect_id)
                                    .is_none_or(|current| current.surface_receipt.is_none())
                        })
                    }
                    LifecycleFailpoint::LifecycleSettlement => commit
                        .facts
                        .lifecycle_effects
                        .iter()
                        .any(|(effect_id, effect)| {
                            effect.outcome.is_some()
                                && current
                                    .facts
                                    .lifecycle_effects
                                    .get(effect_id)
                                    .is_none_or(|current| current.outcome.is_none())
                        }),
                };
                if should_fail && self.armed.swap(false, Ordering::SeqCst) {
                    return Err(CompleteAgentHostStoreError::Persistence {
                        reason: "simulated crash before durable Host lifecycle commit".to_owned(),
                    });
                }
            }
            self.inner.commit(commit).await
        }
    }

    struct LifecycleService {
        create_calls: AtomicUsize,
        resume_calls: AtomicUsize,
        fork_calls: AtomicUsize,
        apply_calls: AtomicUsize,
        inspect_calls: AtomicUsize,
        inspections: Mutex<BTreeMap<AgentEffectIdentity, AgentEffectInspection>>,
        applied_surface: Mutex<Option<AppliedAgentSurface>>,
    }

    impl LifecycleService {
        async fn record_applied(&self, outcome: AgentAppliedEffectOutcome) {
            let effect_id = outcome.effect_id().clone();
            let command_id = outcome.command_id().clone();
            self.inspections.lock().await.insert(
                effect_id.clone(),
                AgentEffectInspection {
                    effect_id,
                    command_id: Some(command_id),
                    state: AgentEffectInspectionState::Applied { outcome },
                },
            );
        }
    }

    #[async_trait]
    impl CompleteAgentService for LifecycleService {
        async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
            Ok(descriptor())
        }

        async fn create(
            &self,
            command: CreateAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            self.create_calls.fetch_add(1, Ordering::SeqCst);
            let source = AgentSourceCoordinate::new("lifecycle-parent").expect("source");
            let receipt = AgentCommandReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                source,
                state: AgentReceiptState::Terminal {
                    outcome: agentdash_agent_service_api::AgentTerminalOutcome::Succeeded,
                },
                snapshot_revision: None,
                initial_context: None,
            };
            self.record_applied(AgentAppliedEffectOutcome::Create {
                receipt: applied_agent_receipt(&receipt),
            })
            .await;
            Ok(receipt)
        }

        async fn resume(
            &self,
            command: ResumeAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            self.resume_calls.fetch_add(1, Ordering::SeqCst);
            let receipt = AgentCommandReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                source: command.source,
                state: AgentReceiptState::AlreadyApplied { terminal: None },
                snapshot_revision: None,
                initial_context: None,
            };
            self.record_applied(AgentAppliedEffectOutcome::Resume {
                receipt: applied_agent_receipt(&receipt),
            })
            .await;
            Ok(receipt)
        }

        async fn fork(
            &self,
            command: ForkAgentCommand,
        ) -> Result<ForkAgentReceipt, AgentServiceError> {
            self.fork_calls.fetch_add(1, Ordering::SeqCst);
            let child_source = AgentSourceCoordinate::new("lifecycle-child").expect("child source");
            let receipt = ForkAgentReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                parent_source: command.source,
                child_source: Some(child_source),
                cutoff: command.cutoff,
                child_history_digest: Some(
                    AgentPayloadDigest::new("sha256:child-history").expect("history digest"),
                ),
                state: AgentReceiptState::Terminal {
                    outcome: agentdash_agent_service_api::AgentTerminalOutcome::Succeeded,
                },
            };
            self.record_applied(AgentAppliedEffectOutcome::Fork {
                receipt: applied_fork_receipt(&receipt),
            })
            .await;
            Ok(receipt)
        }

        async fn execute(
            &self,
            command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Ok(AgentCommandReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                source: command.source,
                state: AgentReceiptState::AlreadyApplied { terminal: None },
                snapshot_revision: None,
                initial_context: None,
            })
        }

        async fn read(
            &self,
            query: AgentReadQuery,
        ) -> Result<agentdash_agent_service_api::AgentSnapshot, AgentServiceError> {
            Ok(agentdash_agent_service_api::AgentSnapshot {
                source: query.source,
                revision: agentdash_agent_service_api::AgentSnapshotRevision(1),
                lifecycle: agentdash_agent_service_api::AgentLifecycleStatus::Active,
                active_turn_id: None,
                turns: Vec::new(),
                interactions: Vec::new(),
                thread_name: None,
                source_info: agentdash_agent_service_api::AgentSnapshotSource {
                    authority:
                        agentdash_agent_service_api::AgentSnapshotAuthority::AgentAuthoritative,
                    source_revision: None,
                    fidelity: SemanticFidelity::Exact,
                    observed_at_ms: current_time_ms(),
                },
                applied_surface: self.applied_surface.lock().await.clone(),
                initial_context: None,
                conversation_history: Vec::new(),
            })
        }

        async fn changes(
            &self,
            _query: AgentChangesQuery,
        ) -> Result<AgentChangePage, AgentServiceError> {
            Err(unsupported())
        }

        async fn inspect(
            &self,
            identity: AgentEffectIdentity,
        ) -> Result<AgentEffectInspection, AgentServiceError> {
            self.inspect_calls.fetch_add(1, Ordering::SeqCst);
            let stored = self.inspections.lock().await.get(&identity).cloned();
            Ok(stored.unwrap_or(AgentEffectInspection {
                effect_id: identity,
                command_id: None,
                state: AgentEffectInspectionState::Unknown,
            }))
        }

        async fn apply_surface(
            &self,
            command: ApplyBoundAgentSurface,
        ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
            self.apply_calls.fetch_add(1, Ordering::SeqCst);
            let applied = AppliedAgentSurface {
                revision: command.bound_surface.revision,
                digest: command.bound_surface.digest,
                contributions: Vec::new(),
            };
            let receipt = AppliedAgentSurfaceReceipt {
                command_id: command.command_id,
                effect_id: command.effect_id,
                source: command.source,
                applied,
            };
            self.record_applied(AgentAppliedEffectOutcome::SurfaceApply {
                receipt: receipt.clone(),
            })
            .await;
            *self.applied_surface.lock().await = Some(receipt.applied.clone());
            Ok(receipt)
        }

        async fn revoke_surface(
            &self,
            _command: RevokeBoundAgentSurface,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }
    }

    fn lifecycle_service() -> Arc<LifecycleService> {
        Arc::new(LifecycleService {
            create_calls: AtomicUsize::new(0),
            resume_calls: AtomicUsize::new(0),
            fork_calls: AtomicUsize::new(0),
            apply_calls: AtomicUsize::new(0),
            inspect_calls: AtomicUsize::new(0),
            inspections: Mutex::new(BTreeMap::new()),
            applied_surface: Mutex::new(None),
        })
    }

    fn applied_agent_receipt(receipt: &AgentCommandReceipt) -> AppliedAgentCommandReceipt {
        let terminal = match &receipt.state {
            AgentReceiptState::AlreadyApplied { terminal } => *terminal,
            AgentReceiptState::Terminal { outcome } => Some(*outcome),
            state => panic!("fixture receipt is not applied: {state:?}"),
        };
        AppliedAgentCommandReceipt {
            command_id: receipt.command_id.clone(),
            effect_id: receipt.effect_id.clone(),
            source: receipt.source.clone(),
            terminal,
            snapshot_revision: receipt.snapshot_revision,
            initial_context: receipt.initial_context.clone(),
        }
    }

    fn applied_fork_receipt(receipt: &ForkAgentReceipt) -> AppliedForkAgentReceipt {
        let terminal = match &receipt.state {
            AgentReceiptState::AlreadyApplied { terminal } => *terminal,
            AgentReceiptState::Terminal { outcome } => Some(*outcome),
            state => panic!("fixture receipt is not applied: {state:?}"),
        };
        AppliedForkAgentReceipt {
            command_id: receipt.command_id.clone(),
            effect_id: receipt.effect_id.clone(),
            parent_source: receipt.parent_source.clone(),
            child_source: receipt.child_source.clone().expect("child source"),
            cutoff: receipt.cutoff.clone(),
            child_history_digest: receipt
                .child_history_digest
                .clone()
                .expect("child history digest"),
            terminal,
        }
    }

    async fn lifecycle_host(
        repository: Arc<dyn CompleteAgentHostRepository>,
    ) -> (CompleteAgentHost, Arc<LifecycleService>, RuntimeThreadId) {
        let service = lifecycle_service();
        let host =
            CompleteAgentHost::new(repository, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        let instance_id = AgentServiceInstanceId::new("lifecycle-service").expect("service");
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("attach lifecycle service");
        let parent_thread = RuntimeThreadId::new("runtime-parent").expect("thread");
        host.register_runtime_target(CompleteAgentRuntimeTarget {
            runtime_thread_id: parent_thread.clone(),
            target: selection.target,
            generation: AgentBindingGeneration(1),
            profile_digest: descriptor().profile_digest,
            bound_surface: empty_bound_surface(),
            callbacks: AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new("runtime-parent-callback").expect("route"),
                binding_generation: AgentBindingGeneration(1),
                delivery: agentdash_agent_service_api::AgentSurfaceRoute::AgentNativeCallback,
                default_deadline_ms: 1_000,
            },
        })
        .await
        .expect("register Runtime target");
        (host, service, parent_thread)
    }

    async fn restarted_lifecycle_host(
        repository: Arc<dyn CompleteAgentHostRepository>,
        service: Arc<LifecycleService>,
    ) -> CompleteAgentHost {
        let host =
            CompleteAgentHost::new(repository, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        let instance_id = AgentServiceInstanceId::new("lifecycle-service").expect("service");
        let _selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id,
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service,
            )
            .await
            .expect("reattach lifecycle service");
        host
    }

    fn lifecycle_context(
        runtime_thread_id: RuntimeThreadId,
        effect_id: &str,
    ) -> ManagedRuntimeDispatchContext {
        ManagedRuntimeDispatchContext {
            runtime_thread_id,
            effect_id: AgentEffectIdentity::new(effect_id).expect("effect"),
            dispatch_owner: "runtime-worker".to_owned(),
            now_ms: current_time_ms(),
            lease_duration_ms: 10_000,
        }
    }

    struct FixtureRecoveryPlanner {
        request: CompleteAgentRuntimeTargetRecoveryRequest,
    }

    #[async_trait]
    impl CompleteAgentRuntimeRecoveryPlanner for FixtureRecoveryPlanner {
        async fn plan_recovery(
            &self,
            _runtime_thread_id: &RuntimeThreadId,
            _previous_target: &CompleteAgentRuntimeTarget,
            _previous_binding: &ManagedRuntimeAgentBinding,
            _effect_id: &AgentEffectIdentity,
        ) -> Result<CompleteAgentRuntimeTargetRecoveryRequest, CompleteAgentHostError> {
            Ok(self.request.clone())
        }
    }

    #[test]
    fn different_payload_cannot_reuse_effect_identity() {
        let record = effect_record("sha256:a");
        let candidate = effect_record("sha256:b");
        assert_eq!(
            ensure_same_effect(&record, &candidate),
            Err(CompleteAgentHostError::EffectIdentityConflict)
        );
    }

    #[tokio::test]
    async fn runtime_target_provisioning_owns_surface_generation_route_and_durable_idempotency() {
        let repository = Arc::new(FixtureHostRepository::default());
        let host = CompleteAgentHost::new(
            repository.clone(),
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        );
        let service = lifecycle_service();
        let instance_id = AgentServiceInstanceId::new("lifecycle-service").expect("service");
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service,
            )
            .await
            .expect("attach lifecycle service");
        let request = CompleteAgentRuntimeTargetProvisioningRequest {
            idempotency_key: AgentIdempotencyKey::new("provision-runtime-a").expect("idempotency"),
            request_digest: AgentPayloadDigest::new("sha256:provision-a").expect("digest"),
            runtime_thread_id: RuntimeThreadId::new("runtime-provisioned").expect("thread"),
            target: selection.target,
            desired_surface: AgentSurfaceSnapshot {
                revision: AgentSurfaceRevision(7),
                digest: agentdash_agent_service_api::AgentSurfaceDigest::new("surface-7")
                    .expect("surface digest"),
                requirements: Vec::new(),
            },
            callback_deadline_ms: 30_000,
        };

        let first = host
            .provision_runtime_target(request.clone())
            .await
            .expect("provision target");
        let replay = host
            .provision_runtime_target(request.clone())
            .await
            .expect("replay provisioning");

        assert_eq!(replay, first);
        assert_eq!(first.target.generation, AgentBindingGeneration(1));
        assert_eq!(
            first.target.callbacks.binding_generation,
            first.target.generation
        );
        assert_eq!(first.target.callbacks.default_deadline_ms, 30_000);
        assert_eq!(
            first.target.bound_surface.digest,
            request.desired_surface.digest
        );
        let snapshot = repository.load().await.expect("Host facts");
        assert_eq!(snapshot.facts.runtime_targets.len(), 1);
        assert_eq!(snapshot.facts.runtime_target_provisionings.len(), 1);

        let mut conflict = request;
        conflict.request_digest =
            AgentPayloadDigest::new("sha256:another-request").expect("digest");
        assert_eq!(
            host.provision_runtime_target(conflict).await,
            Err(CompleteAgentHostError::ProvisioningConflict)
        );
    }

    #[tokio::test]
    async fn surface_rebind_preparation_survives_restart_and_rebinds_exact_new_surface() {
        let repository = Arc::new(FixtureHostRepository::default());
        let (host, service, runtime_thread_id) = lifecycle_host(repository.clone()).await;
        let created = host
            .create(
                lifecycle_context(runtime_thread_id.clone(), "surface-rebind-create"),
                None,
            )
            .await
            .expect("create source");
        let current_target = host
            .runtime_target(&runtime_thread_id)
            .await
            .expect("current target");
        let request = CompleteAgentRuntimeTargetRecoveryRequest {
            idempotency_key: AgentIdempotencyKey::new("surface-rebind-frame-2")
                .expect("idempotency"),
            request_digest: AgentPayloadDigest::new("sha256:surface-rebind-frame-2")
                .expect("digest"),
            runtime_thread_id: runtime_thread_id.clone(),
            expected_generation: AgentBindingGeneration(1),
            target: current_target.target,
            desired_surface: AgentSurfaceSnapshot {
                revision: AgentSurfaceRevision(2),
                digest: agentdash_agent_service_api::AgentSurfaceDigest::new("surface-2")
                    .expect("surface digest"),
                requirements: Vec::new(),
            },
            callback_deadline_ms: 1_000,
        };

        let prepared = host
            .prepare_runtime_surface_rebind(request.clone())
            .await
            .expect("prepare surface rebind");
        let restarted = restarted_lifecycle_host(repository.clone(), service).await;
        let replayed = restarted
            .prepare_runtime_surface_rebind(request)
            .await
            .expect("replay prepared surface rebind");
        assert_eq!(replayed, prepared);
        assert_eq!(
            restarted
                .binding(
                    &runtime_binding_id(&runtime_thread_id, AgentBindingGeneration(1))
                        .expect("old binding"),
                )
                .await
                .expect("load old binding")
                .expect("old binding")
                .state,
            CompleteAgentBindingState::Lost
        );

        let rebound = restarted
            .rebind(
                lifecycle_context(runtime_thread_id.clone(), "surface-rebind-apply"),
                created.binding,
            )
            .await
            .expect("apply prepared surface rebind");
        assert_eq!(rebound.binding.generation, AgentBindingGeneration(2));
        assert_eq!(
            rebound.binding.applied_surface.revision,
            AgentSurfaceRevision(2)
        );
        assert_eq!(
            restarted
                .runtime_target(&runtime_thread_id)
                .await
                .expect("replacement target")
                .generation,
            AgentBindingGeneration(2)
        );
    }

    #[tokio::test]
    async fn explicit_recovery_rebinds_one_lost_source_and_fences_the_old_generation() {
        let repository = Arc::new(FixtureHostRepository::default());
        let (host, service, runtime_thread_id) = lifecycle_host(repository.clone()).await;
        let created = host
            .create(
                lifecycle_context(runtime_thread_id.clone(), "rebind-create"),
                None,
            )
            .await
            .expect("create source");
        let old_binding_id =
            runtime_binding_id(&runtime_thread_id, AgentBindingGeneration(1)).expect("binding");
        let previous_target = host
            .runtime_target(&runtime_thread_id)
            .await
            .expect("runtime target")
            .target;
        let lost_threads = host
            .mark_target_bindings_lost(&previous_target)
            .await
            .expect("mark previous service bindings lost");
        assert_eq!(lost_threads, vec![runtime_thread_id.clone()]);
        assert_eq!(
            host.lost_runtime_threads_for_profile(&descriptor().profile_digest)
                .await
                .expect("query lost RuntimeThreads"),
            vec![runtime_thread_id.clone()]
        );
        let replacement_instance_id =
            AgentServiceInstanceId::new("lifecycle-service-replacement").expect("service");
        let replacement = host
            .attach_verified_service(
                fixture_verified_registration(
                    replacement_instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("attach replacement service");
        let request = CompleteAgentRuntimeTargetRecoveryRequest {
            idempotency_key: AgentIdempotencyKey::new("recover-runtime-a").expect("idempotency"),
            request_digest: AgentPayloadDigest::new("sha256:recover-runtime-a").expect("digest"),
            runtime_thread_id: runtime_thread_id.clone(),
            expected_generation: AgentBindingGeneration(1),
            target: replacement.target,
            desired_surface: AgentSurfaceSnapshot {
                revision: AgentSurfaceRevision(2),
                digest: agentdash_agent_service_api::AgentSurfaceDigest::new("surface-recovered")
                    .expect("surface"),
                requirements: Vec::new(),
            },
            callback_deadline_ms: 2_000,
        };
        host.install_runtime_recovery_planner(Arc::new(FixtureRecoveryPlanner { request }))
            .await;
        let rebind_context = lifecycle_context(runtime_thread_id.clone(), "rebind-resume");
        let rebound = host
            .rebind(rebind_context.clone(), created.binding.clone())
            .await
            .expect("rebind source");
        let replay = host
            .rebind(rebind_context, created.binding.clone())
            .await
            .expect("replay rebind");
        assert_eq!(replay, rebound);
        assert_eq!(rebound.previous_binding, created.binding);
        assert_eq!(rebound.binding.source, rebound.previous_binding.source);
        assert_eq!(rebound.binding.generation, AgentBindingGeneration(2));
        assert_eq!(service.resume_calls.load(Ordering::SeqCst), 1);

        let facts = repository.load().await.expect("Host facts").facts;
        assert_eq!(facts.runtime_target_recoveries.len(), 1);
        assert_eq!(
            facts.bindings[&old_binding_id].state,
            CompleteAgentBindingState::Lost
        );
        assert!(facts.callback_routes.values().all(|route| {
            route.binding_id != old_binding_id
                || facts.revoked_callback_routes.contains(&route.route_id)
        }));
        assert_eq!(
            facts.bindings[&runtime_binding_id(&runtime_thread_id, AgentBindingGeneration(2))
                .expect("recovered binding")]
                .state,
            CompleteAgentBindingState::Available
        );
        assert!(matches!(
            host.read(
                runtime_thread_id,
                rebound.previous_binding,
                AgentReadQuery {
                    source: rebound.binding.source,
                    at_revision: None,
                },
            )
            .await,
            Err(ManagedRuntimeLifecycleError::StaleGeneration)
        ));
    }

    #[tokio::test]
    async fn rebind_settlement_crash_recovers_by_same_effect_without_second_resume() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::LifecycleSettlement,
            armed: AtomicBool::new(false),
        });
        let (host, service, runtime_thread_id) = lifecycle_host(failing.clone()).await;
        let created = host
            .create(
                lifecycle_context(runtime_thread_id.clone(), "rebind-crash-create"),
                None,
            )
            .await
            .expect("create source");
        host.mark_binding_lost(
            &runtime_binding_id(&runtime_thread_id, AgentBindingGeneration(1)).expect("binding"),
            AgentBindingGeneration(1),
        )
        .await
        .expect("mark lost");
        let replacement_instance_id =
            AgentServiceInstanceId::new("rebind-crash-replacement").expect("service");
        let replacement = host
            .attach_verified_service(
                fixture_verified_registration(
                    replacement_instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("attach replacement");
        host.install_runtime_recovery_planner(Arc::new(FixtureRecoveryPlanner {
            request: CompleteAgentRuntimeTargetRecoveryRequest {
                idempotency_key: AgentIdempotencyKey::new("rebind-crash-target")
                    .expect("idempotency"),
                request_digest: AgentPayloadDigest::new("sha256:rebind-crash-target")
                    .expect("digest"),
                runtime_thread_id: runtime_thread_id.clone(),
                expected_generation: AgentBindingGeneration(1),
                target: replacement.target,
                desired_surface: AgentSurfaceSnapshot {
                    revision: AgentSurfaceRevision(2),
                    digest: agentdash_agent_service_api::AgentSurfaceDigest::new(
                        "rebind-crash-surface",
                    )
                    .expect("surface"),
                    requirements: Vec::new(),
                },
                callback_deadline_ms: 2_000,
            },
        }))
        .await;
        let context = lifecycle_context(runtime_thread_id, "rebind-crash-resume");
        failing.arm();

        assert!(matches!(
            host.rebind(context.clone(), created.binding.clone()).await,
            Err(ManagedRuntimeLifecycleError::InspectionRequired { .. })
        ));
        assert_eq!(service.resume_calls.load(Ordering::SeqCst), 1);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        restarted
            .attach_verified_service(
                fixture_verified_registration(
                    replacement_instance_id,
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("reattach replacement");

        let inspected = restarted
            .inspect(context, Some(created.binding))
            .await
            .expect("inspect Rebind");

        assert!(matches!(
            inspected,
            ManagedRuntimeLifecycleInspection::RebindApplied(ManagedRuntimeRebindOutcome {
                binding: ManagedRuntimeAgentBinding {
                    generation: AgentBindingGeneration(2),
                    ..
                },
                ..
            })
        ));
        assert_eq!(service.resume_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn runtime_lifecycle_adapter_persists_intent_before_create_and_fork_provisioning() {
        let repository = Arc::new(FixtureHostRepository::default());
        let service = Arc::new(LifecycleService {
            create_calls: AtomicUsize::new(0),
            resume_calls: AtomicUsize::new(0),
            fork_calls: AtomicUsize::new(0),
            apply_calls: AtomicUsize::new(0),
            inspect_calls: AtomicUsize::new(0),
            inspections: Mutex::new(BTreeMap::new()),
            applied_surface: Mutex::new(None),
        });
        let host = CompleteAgentHost::new(
            repository.clone(),
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        );
        let instance_id = AgentServiceInstanceId::new("lifecycle-service").expect("service");
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("attach lifecycle service");
        let parent_thread = RuntimeThreadId::new("runtime-parent").expect("thread");
        host.register_runtime_target(CompleteAgentRuntimeTarget {
            runtime_thread_id: parent_thread.clone(),
            target: selection.target,
            generation: AgentBindingGeneration(1),
            profile_digest: descriptor().profile_digest,
            bound_surface: empty_bound_surface(),
            callbacks: AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new("runtime-parent-callback").expect("route"),
                binding_generation: AgentBindingGeneration(1),
                delivery: agentdash_agent_service_api::AgentSurfaceRoute::AgentNativeCallback,
                default_deadline_ms: 1_000,
            },
        })
        .await
        .expect("register Runtime target");
        let now_ms = current_time_ms();
        let create_context = ManagedRuntimeDispatchContext {
            runtime_thread_id: parent_thread.clone(),
            effect_id: AgentEffectIdentity::new("runtime-create-effect").expect("effect"),
            dispatch_owner: "runtime-worker".to_owned(),
            now_ms,
            lease_duration_ms: 10_000,
        };
        let created = ManagedRuntimeLifecyclePort::create(&host, create_context.clone(), None)
            .await
            .expect("create through Host lifecycle");
        let duplicate = ManagedRuntimeLifecyclePort::create(&host, create_context.clone(), None)
            .await
            .expect("duplicate Create");
        assert_eq!(created.binding, duplicate.binding);
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        let after_create = repository.load().await.expect("Host facts");
        assert!(
            after_create
                .facts
                .lifecycle_effects
                .get(&create_context.effect_id)
                .is_some_and(|record| record.outcome.is_some())
        );
        assert!(
            after_create
                .facts
                .bindings
                .get(
                    &runtime_binding_id(&parent_thread, AgentBindingGeneration(1))
                        .expect("binding id"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );

        let child_thread = RuntimeThreadId::new("runtime-child").expect("thread");
        let fork_context = ManagedRuntimeDispatchContext {
            runtime_thread_id: parent_thread,
            effect_id: AgentEffectIdentity::new("runtime-fork-effect").expect("effect"),
            dispatch_owner: "runtime-worker".to_owned(),
            now_ms,
            lease_duration_ms: 10_000,
        };
        let forked = ManagedRuntimeLifecyclePort::fork(
            &host,
            fork_context.clone(),
            created.binding,
            child_thread.clone(),
            AgentForkPoint::Head,
        )
        .await
        .expect("fork through Host lifecycle");
        assert_eq!(forked.child_history_digest.as_str(), "sha256:child-history");
        let after_fork = repository.load().await.expect("Host facts");
        assert!(
            after_fork
                .facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.outcome.is_some())
        );
        assert!(
            after_fork
                .facts
                .bindings
                .get(
                    &runtime_binding_id(&child_thread, AgentBindingGeneration(1))
                        .expect("binding id"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );
        let encoded =
            crate::encode_complete_agent_host_snapshot(&after_fork).expect("encode Host snapshot");
        let decoded =
            crate::decode_complete_agent_host_snapshot(encoded).expect("decode Host snapshot");
        assert_eq!(decoded, after_fork);
        assert_eq!(decoded.facts.runtime_targets.len(), 2);
        assert!(
            decoded
                .facts
                .lifecycle_effects
                .values()
                .all(|record| { record.outcome.is_some() && record.applied_receipt.is_some() })
        );
    }

    #[tokio::test]
    async fn create_binding_commit_failure_remains_inspection_required() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::BindingProvision,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let context = lifecycle_context(parent_thread.clone(), "create-binding-fail");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::create(&host, context.clone(), None).await,
            Err(ManagedRuntimeLifecycleError::InspectionRequired { .. })
        ));
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        let facts = durable.load().await.expect("durable Host facts").facts;
        assert!(
            facts
                .lifecycle_effects
                .get(&context.effect_id)
                .is_some_and(|record| record.outcome.is_none())
        );
        assert!(!facts.bindings.contains_key(
            &runtime_binding_id(&parent_thread, AgentBindingGeneration(1)).expect("binding"),
        ));
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        assert!(matches!(
            ManagedRuntimeLifecyclePort::inspect(&restarted, context, None).await,
            Ok(ManagedRuntimeLifecycleInspection::CreateApplied(_))
        ));
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.apply_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn apply_settlement_commit_failure_remains_unknown_and_inspectable() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::SurfaceSettlement,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let context = lifecycle_context(parent_thread.clone(), "create-surface-fail");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::create(&host, context.clone(), None).await,
            Err(ManagedRuntimeLifecycleError::InspectionRequired { .. })
        ));
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.apply_calls.load(Ordering::SeqCst), 1);
        let surface_effect =
            derived_effect_id(&context.effect_id, "surface").expect("surface effect");
        let facts = durable.load().await.expect("durable Host facts").facts;
        assert_eq!(
            facts
                .effects
                .get(&surface_effect)
                .expect("durable Apply intent")
                .state,
            CompleteAgentEffectState::Dispatching
        );
        assert!(
            facts
                .effects
                .get(&surface_effect)
                .expect("durable Apply intent")
                .surface_receipt
                .is_none()
        );
        assert!(
            facts
                .bindings
                .get(
                    &runtime_binding_id(&parent_thread, AgentBindingGeneration(1))
                        .expect("binding"),
                )
                .is_some_and(|binding| binding.state == CompleteAgentBindingState::PendingSurface)
        );
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        assert!(matches!(
            ManagedRuntimeLifecyclePort::inspect(&restarted, context, None).await,
            Ok(ManagedRuntimeLifecycleInspection::CreateApplied(_))
        ));
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 2);
        assert_eq!(service.apply_calls.load(Ordering::SeqCst), 1);
        let recovered = durable.load().await.expect("recovered Host facts").facts;
        assert!(
            recovered
                .effects
                .get(&surface_effect)
                .is_some_and(|effect| effect.surface_receipt.is_some())
        );
        assert!(
            recovered
                .bindings
                .get(
                    &runtime_binding_id(&parent_thread, AgentBindingGeneration(1))
                        .expect("binding"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );
    }

    #[tokio::test]
    async fn create_outcome_commit_failure_recovers_the_same_effect() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::LifecycleSettlement,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let context = lifecycle_context(parent_thread.clone(), "create-settlement-fail");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::create(&host, context.clone(), None).await,
            Err(ManagedRuntimeLifecycleError::InspectionRequired { .. })
        ));
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        let facts = durable.load().await.expect("durable Host facts").facts;
        assert!(
            facts
                .bindings
                .get(
                    &runtime_binding_id(&parent_thread, AgentBindingGeneration(1))
                        .expect("binding"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );
        assert!(
            facts
                .lifecycle_effects
                .get(&context.effect_id)
                .is_some_and(|record| record.outcome.is_none())
        );

        drop(host);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        let ManagedRuntimeLifecycleInspection::CreateApplied(recovered) =
            ManagedRuntimeLifecyclePort::inspect(&restarted, context.clone(), None)
                .await
                .expect("inspect the same Create effect")
        else {
            panic!("Create inspection must recover its applied outcome");
        };
        assert_eq!(recovered.receipt.effect_id, context.effect_id);
        assert_eq!(service.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.apply_calls.load(Ordering::SeqCst), 1);
        assert!(
            durable
                .load()
                .await
                .expect("settled Host facts")
                .facts
                .lifecycle_effects
                .get(&context.effect_id)
                .is_some_and(|record| record.outcome.is_some())
        );
    }

    #[tokio::test]
    async fn resume_outcome_commit_failure_recovers_the_same_effect() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::LifecycleSettlement,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let create_context = lifecycle_context(parent_thread.clone(), "resume-parent-create");
        let created = ManagedRuntimeLifecyclePort::create(&host, create_context, None)
            .await
            .expect("create parent");
        let resume_context = lifecycle_context(parent_thread, "resume-settlement-fail");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::resume(
                &host,
                resume_context.clone(),
                created.binding.clone(),
            )
            .await,
            Err(ManagedRuntimeLifecycleError::InspectionRequired { .. })
        ));
        assert_eq!(service.resume_calls.load(Ordering::SeqCst), 1);
        assert!(
            durable
                .load()
                .await
                .expect("durable Host facts")
                .facts
                .lifecycle_effects
                .get(&resume_context.effect_id)
                .is_some_and(|record| record.outcome.is_none())
        );

        drop(host);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        let ManagedRuntimeLifecycleInspection::ResumeApplied(recovered) =
            ManagedRuntimeLifecyclePort::inspect(
                &restarted,
                resume_context.clone(),
                Some(created.binding),
            )
            .await
            .expect("inspect the same Resume effect")
        else {
            panic!("Resume inspection must recover its applied outcome");
        };
        assert_eq!(recovered.receipt.effect_id, resume_context.effect_id);
        assert_eq!(service.resume_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
        assert!(
            durable
                .load()
                .await
                .expect("settled Host facts")
                .facts
                .lifecycle_effects
                .get(&resume_context.effect_id)
                .is_some_and(|record| record.outcome.is_some())
        );
    }

    #[tokio::test]
    async fn fork_provision_commit_failure_preserves_child_known_for_inspection() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::RuntimeTargetProvision,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let created = ManagedRuntimeLifecyclePort::create(
            &host,
            lifecycle_context(parent_thread.clone(), "fork-parent-create"),
            None,
        )
        .await
        .expect("create parent");
        let fork_context = lifecycle_context(parent_thread, "fork-provision-fail");
        let child_thread = RuntimeThreadId::new("runtime-child-failpoint").expect("child");
        failing.arm();

        let error = ManagedRuntimeLifecyclePort::fork(
            &host,
            fork_context.clone(),
            created.binding.clone(),
            child_thread.clone(),
            AgentForkPoint::Head,
        )
        .await
        .expect_err("Host target provision commit fails");
        let ManagedRuntimeLifecycleError::ForkInspectionRequired {
            child_source,
            child_history_digest,
            ..
        } = error
        else {
            panic!("Fork must remain inspection-required after its Agent child exists");
        };
        assert_eq!(child_source.as_str(), "lifecycle-child");
        assert_eq!(
            child_history_digest
                .as_ref()
                .map(AgentPayloadDigest::as_str),
            Some("sha256:child-history")
        );
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        assert!(
            durable
                .load()
                .await
                .expect("durable Host facts")
                .facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.outcome.is_none())
        );

        drop(host);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        let ManagedRuntimeLifecycleInspection::ForkApplied(recovered) =
            ManagedRuntimeLifecyclePort::inspect(
                &restarted,
                fork_context.clone(),
                Some(created.binding),
            )
            .await
            .expect("inspect the same Fork effect")
        else {
            panic!("Fork inspection must recover its applied outcome");
        };
        assert_eq!(recovered.receipt.effect_id, fork_context.effect_id);
        assert_eq!(
            recovered.child_binding.source.as_str(),
            child_source.as_str()
        );
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fork_applied_receipt_commit_failure_recovers_only_by_inspection() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::AppliedReceiptObservation,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let created = ManagedRuntimeLifecyclePort::create(
            &host,
            lifecycle_context(parent_thread.clone(), "fork-receipt-parent-create"),
            None,
        )
        .await
        .expect("create parent");
        let fork_context = lifecycle_context(parent_thread, "fork-receipt-fail");
        let child_thread = RuntimeThreadId::new("runtime-child-receipt").expect("child");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::fork(
                &host,
                fork_context.clone(),
                created.binding.clone(),
                child_thread.clone(),
                AgentForkPoint::Head,
            )
            .await,
            Err(ManagedRuntimeLifecycleError::ForkInspectionRequired {
                ref child_source,
                child_history_digest: Some(ref digest),
                ..
            }) if child_source.as_str() == "lifecycle-child"
                && digest.as_str() == "sha256:child-history"
        ));
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        let facts = durable.load().await.expect("durable Host facts").facts;
        assert!(
            facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.applied_receipt.is_none() && record.outcome.is_none())
        );
        assert!(!facts.runtime_targets.contains_key(&child_thread));

        drop(host);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        assert!(matches!(
            ManagedRuntimeLifecyclePort::inspect(
                &restarted,
                fork_context.clone(),
                Some(created.binding),
            )
            .await,
            Ok(ManagedRuntimeLifecycleInspection::ForkApplied(ref outcome))
                if outcome.receipt.effect_id == fork_context.effect_id
        ));
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
        let facts = durable.load().await.expect("settled Host facts").facts;
        assert!(
            facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.applied_receipt.is_some() && record.outcome.is_some())
        );
        assert!(
            facts
                .bindings
                .get(
                    &runtime_binding_id(&child_thread, AgentBindingGeneration(1)).expect("binding"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );
    }

    #[tokio::test]
    async fn fork_inspection_evidence_mismatch_is_terminal_child_known_failure() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::RuntimeTargetProvision,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let created = ManagedRuntimeLifecyclePort::create(
            &host,
            lifecycle_context(parent_thread.clone(), "fork-mismatch-parent-create"),
            None,
        )
        .await
        .expect("create parent");
        let fork_context = lifecycle_context(parent_thread, "fork-inspection-mismatch");
        failing.arm();

        assert!(matches!(
            ManagedRuntimeLifecyclePort::fork(
                &host,
                fork_context.clone(),
                created.binding.clone(),
                RuntimeThreadId::new("runtime-child-mismatch").expect("child"),
                AgentForkPoint::Head,
            )
            .await,
            Err(ManagedRuntimeLifecycleError::ForkInspectionRequired { .. })
        ));
        let mut inspections = service.inspections.lock().await;
        let inspection = inspections
            .get_mut(&fork_context.effect_id)
            .expect("Fork inspection");
        let AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt },
        } = &mut inspection.state
        else {
            panic!("fixture must expose a Fork receipt");
        };
        receipt.child_history_digest =
            AgentPayloadDigest::new("sha256:conflicting-history").expect("digest");
        drop(inspections);
        drop(host);

        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        assert!(matches!(
            ManagedRuntimeLifecyclePort::inspect(
                &restarted,
                fork_context,
                Some(created.binding),
            )
            .await,
            Err(ManagedRuntimeLifecycleError::ForkChildKnown {
                ref child_source,
                child_history_digest: Some(ref digest),
                ..
            }) if child_source.as_str() == "lifecycle-child"
                && digest.as_str() == "sha256:conflicting-history"
        ));
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fork_outcome_commit_failure_preserves_child_known_and_recovers_by_inspection() {
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailLifecycleCommitOnceRepository {
            inner: durable.clone(),
            failpoint: LifecycleFailpoint::LifecycleSettlement,
            armed: AtomicBool::new(false),
        });
        let (host, service, parent_thread) = lifecycle_host(failing.clone()).await;
        let created = ManagedRuntimeLifecyclePort::create(
            &host,
            lifecycle_context(parent_thread.clone(), "fork-settle-parent-create"),
            None,
        )
        .await
        .expect("create parent");
        let fork_context = lifecycle_context(parent_thread, "fork-settlement-fail");
        let child_thread = RuntimeThreadId::new("runtime-child-settlement").expect("child");
        failing.arm();

        let error = ManagedRuntimeLifecyclePort::fork(
            &host,
            fork_context.clone(),
            created.binding.clone(),
            child_thread.clone(),
            AgentForkPoint::Head,
        )
        .await
        .expect_err("Host Fork outcome settlement fails");
        assert!(matches!(
            error,
            ManagedRuntimeLifecycleError::ForkInspectionRequired {
                ref child_source,
                child_history_digest: Some(ref digest),
                ..
            } if child_source.as_str() == "lifecycle-child"
                && digest.as_str() == "sha256:child-history"
        ));
        let facts = durable.load().await.expect("durable Host facts").facts;
        assert!(
            facts
                .bindings
                .get(
                    &runtime_binding_id(&child_thread, AgentBindingGeneration(1)).expect("binding"),
                )
                .is_some_and(CompleteAgentBinding::dispatch_admitted)
        );
        assert!(
            facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.outcome.is_none())
        );

        drop(host);
        let restarted = restarted_lifecycle_host(failing, service.clone()).await;
        assert!(matches!(
            ManagedRuntimeLifecyclePort::inspect(
                &restarted,
                fork_context.clone(),
                Some(created.binding),
            )
            .await,
            Ok(ManagedRuntimeLifecycleInspection::ForkApplied(ref outcome))
                if outcome.receipt.effect_id == fork_context.effect_id
        ));
        assert_eq!(service.fork_calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.inspect_calls.load(Ordering::SeqCst), 1);
        assert!(
            durable
                .load()
                .await
                .expect("settled Host facts")
                .facts
                .lifecycle_effects
                .get(&fork_context.effect_id)
                .is_some_and(|record| record.outcome.is_some())
        );
    }

    #[test]
    fn stale_generation_is_fenced() {
        let binding = CompleteAgentBinding {
            id: CompleteAgentBindingId::new("binding").expect("binding"),
            target: fixture_binding_target(
                AgentServiceInstanceId::new("service").expect("service"),
            ),
            generation: AgentBindingGeneration(2),
            source: AgentSourceCoordinate::new("source").expect("source"),
            profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            bound_surface: BoundAgentSurface {
                revision: agentdash_agent_service_api::AgentSurfaceRevision(1),
                digest: agentdash_agent_service_api::AgentSurfaceDigest::new("surface")
                    .expect("surface"),
                offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
                contributions: Vec::new(),
            },
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        };
        assert!(matches!(
            ensure_generation(&binding, AgentBindingGeneration(1)),
            Err(CompleteAgentHostError::StaleGeneration { .. })
        ));
    }

    #[test]
    fn every_terminal_effect_state_fences_late_nonterminal_observations() {
        for (current, observed) in [
            (
                CompleteAgentEffectState::Applied,
                CompleteAgentEffectState::Unknown,
            ),
            (
                CompleteAgentEffectState::Applied,
                CompleteAgentEffectState::NotApplied,
            ),
            (
                CompleteAgentEffectState::Rejected,
                CompleteAgentEffectState::Unknown,
            ),
            (
                CompleteAgentEffectState::Lost,
                CompleteAgentEffectState::NotApplied,
            ),
        ] {
            let mut record = effect_record("sha256:effect");
            record.state = current;

            assert_eq!(
                observe_effect_state(&mut record, observed),
                Err(CompleteAgentHostError::EffectObservationConflict { current, observed })
            );
            assert_eq!(record.state, current);
        }
    }

    #[tokio::test]
    async fn runtime_offer_preserves_all_five_typed_service_facets() {
        let mut service_descriptor = descriptor();
        let facets = vec![
            capability_facet(
                AgentSurfaceSemanticFacet::Instruction,
                AgentSurfaceRoute::ImmutableDelivery,
            ),
            capability_facet(
                AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::HotUpdate,
                }),
                AgentSurfaceRoute::AgentNativeCallback,
            ),
            capability_facet(
                AgentSurfaceSemanticFacet::Hook(AgentHookSemanticFacet {
                    point: AgentHookPoint::BeforeTool,
                    timing: AgentHookTiming::Before,
                    blocking: AgentHookBlockingSemantics::Blocking {
                        fidelity: SemanticFidelity::Exact,
                    },
                    mutations: BTreeMap::new(),
                    effects: BTreeMap::new(),
                }),
                AgentSurfaceRoute::AgentNativeCallback,
            ),
            capability_facet(
                AgentSurfaceSemanticFacet::Workspace,
                AgentSurfaceRoute::HostLifecycle,
            ),
            capability_facet(
                AgentSurfaceSemanticFacet::ContextRequirement,
                AgentSurfaceRoute::ImmutableDelivery,
            ),
        ];
        service_descriptor.profile.surface.facets = facets.clone();
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: service_descriptor.clone(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            command_id: AgentCommandId::new("command").expect("command"),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: None,
        });
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let host = fixture_host();
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    service_descriptor.profile_digest.clone(),
                ),
                service,
            )
            .await
            .expect("attach service");

        let offer = selection.offer;

        assert_eq!(offer.profile_digest, service_descriptor.profile_digest);
        assert_eq!(offer.contributions, facets);
    }

    #[test]
    fn invalid_surface_facets_are_rejected_without_inferred_capability() {
        let mut descriptor = descriptor();
        for facet in [
            AgentSurfaceCapabilityFacet {
                semantics: AgentSurfaceSemanticFacet::Instruction,
                routes: BTreeSet::new(),
                fidelity: SemanticFidelity::Exact,
                configuration_boundary: AgentConfigurationBoundary::Binding,
            },
            AgentSurfaceCapabilityFacet {
                semantics: AgentSurfaceSemanticFacet::Workspace,
                routes: BTreeSet::from([AgentSurfaceRoute::HostLifecycle]),
                fidelity: SemanticFidelity::Unsupported,
                configuration_boundary: AgentConfigurationBoundary::Binding,
            },
            AgentSurfaceCapabilityFacet {
                semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::BindingOnly,
                }),
                routes: BTreeSet::from([AgentSurfaceRoute::RuntimeToolBroker]),
                fidelity: SemanticFidelity::Exact,
                configuration_boundary: AgentConfigurationBoundary::Binding,
            },
        ] {
            descriptor.profile.surface.facets = vec![facet];
            assert!(matches!(
                validate_service_descriptor(&descriptor),
                Err(CompleteAgentHostError::Invariant { .. })
            ));
        }
    }

    #[tokio::test]
    async fn reconstructed_host_recovers_unknown_effect_binding_source_and_generation_fence() {
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: None,
        });
        let repository = Arc::new(FixtureHostRepository::default());
        let host = CompleteAgentHost::new(
            repository.clone(),
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        );
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("attach service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let bound_surface = empty_bound_surface();
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            target: selection.target,
            generation: AgentBindingGeneration(1),
            source: source.clone(),
            profile_digest: descriptor().profile_digest,
            bound_surface: bound_surface.clone(),
            applied_surface: Some(AppliedAgentSurface {
                revision: bound_surface.revision,
                digest: bound_surface.digest.clone(),
                contributions: Vec::new(),
            }),
            state: CompleteAgentBindingState::Available,
        })
        .await
        .expect("register binding");
        let lease = host
            .acquire_binding_lease(
                &binding_id,
                AgentBindingGeneration(1),
                "worker-1",
                0,
                u64::MAX,
            )
            .await
            .expect("lease");
        let command = AgentCommandEnvelope {
            meta: AgentCommandMeta {
                command_id,
                effect_id: effect_id.clone(),
                idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey::new("idem")
                    .expect("idempotency"),
                binding_generation: AgentBindingGeneration(1),
                expected_snapshot_revision: None,
            },
            source,
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: Vec::new(),
                },
            },
        };

        assert!(matches!(
            host.dispatch_execute(&lease, &binding_id, command.clone())
                .await,
            Err(CompleteAgentHostError::Service(_))
        ));
        drop(host);

        let restarted =
            CompleteAgentHost::new(repository, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        restarted
            .attach_verified_service(
                fixture_verified_registration(
                    AgentServiceInstanceId::new("service").expect("service"),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("reattach live service handle");
        let recovered_binding = restarted
            .binding(&binding_id)
            .await
            .expect("read durable binding")
            .expect("durable binding");
        assert_eq!(recovered_binding.source, command.source);
        assert_eq!(recovered_binding.generation, AgentBindingGeneration(1));
        assert_eq!(
            restarted
                .effect(&effect_id)
                .await
                .expect("read durable effect")
                .expect("durable effect")
                .state,
            CompleteAgentEffectState::Unknown
        );
        assert!(matches!(
            restarted
                .acquire_binding_lease(
                    &binding_id,
                    AgentBindingGeneration(2),
                    "stale-worker",
                    0,
                    u64::MAX,
                )
                .await,
            Err(CompleteAgentHostError::StaleGeneration { .. })
        ));

        let recovered = restarted
            .dispatch_execute(&lease, &binding_id, command)
            .await
            .expect("inspect same effect");

        assert!(matches!(
            recovered.state,
            AgentReceiptState::AlreadyApplied { .. }
        ));
        assert_eq!(service.execute_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            restarted
                .effect(&effect_id)
                .await
                .expect("read effect")
                .expect("effect")
                .state,
            CompleteAgentEffectState::Applied
        );
    }

    #[tokio::test]
    async fn repeated_inspect_advances_unknown_to_applied_across_restarts() {
        assert_inspection_progression_survives_restarts(AgentEffectInspectionState::Unknown).await;
    }

    #[tokio::test]
    async fn repeated_inspect_advances_accepted_to_applied_across_restarts() {
        assert_inspection_progression_survives_restarts(AgentEffectInspectionState::Accepted {
            source: AgentSourceCoordinate::new("source").expect("source"),
        })
        .await;
    }

    #[tokio::test]
    async fn not_applied_redispatch_archives_inspection_before_applied_receipt() {
        assert_not_applied_redispatch_settles_current_attempt(true).await;
    }

    #[tokio::test]
    async fn not_applied_redispatch_archives_inspection_before_unknown_error() {
        assert_not_applied_redispatch_settles_current_attempt(false).await;
    }

    #[tokio::test]
    async fn lease_takeover_rejects_receipt_returned_by_paused_old_execute() {
        let gate = Arc::new(ExternalOutcomeGate::default());
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let receipt = applied_receipt(&command_id, &effect_id, &source);
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::from([FixtureExecuteOutcome::Receipt(
                Box::new(receipt),
            )])),
            execute_gate: Some(gate.clone()),
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: None,
        });
        let (host, binding_id, lease) =
            available_host(Arc::new(FixtureHostRepository::default()), service).await;
        let command = execute_command(command_id, effect_id.clone(), source);

        let dispatch = tokio::spawn({
            let host = host.clone();
            let binding_id = binding_id.clone();
            let lease = lease.clone();
            async move { host.dispatch_execute(&lease, &binding_id, command).await }
        });
        gate.entered.notified().await;
        host.acquire_binding_lease(
            &binding_id,
            AgentBindingGeneration(1),
            "worker-2",
            lease.expires_at_ms,
            lease.expires_at_ms + 10_000,
        )
        .await
        .expect("take over lease");
        gate.release.notify_one();

        assert_eq!(
            dispatch.await.expect("dispatch task"),
            Err(CompleteAgentHostError::LeaseConflict)
        );
        let effect = host
            .effect(&effect_id)
            .await
            .expect("read effect")
            .expect("effect");
        assert_eq!(effect.state, CompleteAgentEffectState::Dispatching);
        assert!(effect.receipt.is_none());
    }

    #[tokio::test]
    async fn lease_takeover_rejects_inspection_returned_to_paused_old_owner() {
        let gate = Arc::new(ExternalOutcomeGate::default());
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: Some(gate.clone()),
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: None,
        });
        let (host, binding_id, lease) =
            available_host(Arc::new(FixtureHostRepository::default()), service).await;
        let command = execute_command(command_id, effect_id.clone(), source);
        assert!(matches!(
            host.dispatch_execute(&lease, &binding_id, command.clone())
                .await,
            Err(CompleteAgentHostError::Service(_))
        ));

        let inspection = tokio::spawn({
            let host = host.clone();
            let binding_id = binding_id.clone();
            let lease = lease.clone();
            async move { host.dispatch_execute(&lease, &binding_id, command).await }
        });
        gate.entered.notified().await;
        host.acquire_binding_lease(
            &binding_id,
            AgentBindingGeneration(1),
            "worker-2",
            lease.expires_at_ms,
            lease.expires_at_ms + 10_000,
        )
        .await
        .expect("take over lease");
        gate.release.notify_one();

        assert_eq!(
            inspection.await.expect("inspection task"),
            Err(CompleteAgentHostError::LeaseConflict)
        );
        let effect = host
            .effect(&effect_id)
            .await
            .expect("read effect")
            .expect("effect");
        assert_eq!(effect.state, CompleteAgentEffectState::Unknown);
        assert!(effect.inspection.is_none());
    }

    #[tokio::test]
    async fn terminal_revoke_replay_still_clears_applied_binding_surface() {
        let host = fixture_host();
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("revoke-command").expect("command");
        let effect_id = AgentEffectIdentity::new("revoke-effect").expect("effect");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let command = revoke_command(command_id.clone(), effect_id.clone(), source.clone());
        let receipt = applied_receipt(&command_id, &effect_id, &source);
        let bound_surface = empty_bound_surface();
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target: fixture_binding_target(
                AgentServiceInstanceId::new("service").expect("service"),
            ),
            generation: AgentBindingGeneration(1),
            source: source.clone(),
            profile_digest: descriptor().profile_digest,
            applied_surface: Some(AppliedAgentSurface {
                revision: bound_surface.revision,
                digest: bound_surface.digest.clone(),
                contributions: Vec::new(),
            }),
            bound_surface,
            state: CompleteAgentBindingState::Available,
        };
        let record = CompleteAgentEffectRecord {
            effect_id: effect_id.clone(),
            command_id,
            binding_id: binding_id.clone(),
            generation: binding.generation,
            source,
            payload_digest: payload_digest(&command).expect("command digest"),
            delivery_epoch: 1,
            dispatch_attempt: 1,
            state: CompleteAgentEffectState::Applied,
            receipt: Some(receipt.clone()),
            surface_receipt: None,
            inspection: None,
            attempt_history: Vec::new(),
        };
        let lease = seed_effect(&host, record, Some(binding)).await;

        assert_eq!(
            host.revoke_bound_surface(&lease, &binding_id, command)
                .await
                .expect("terminal replay cleanup"),
            receipt
        );
        let binding = host
            .binding(&binding_id)
            .await
            .expect("read binding")
            .expect("binding");
        assert_eq!(binding.state, CompleteAgentBindingState::PendingSurface);
        assert!(binding.applied_surface.is_none());
    }

    #[tokio::test]
    async fn failed_atomic_revoke_settlement_recovers_after_restart_without_redispatch() {
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("revoke-command").expect("command");
        let effect_id = AgentEffectIdentity::new("revoke-effect").expect("effect");
        let receipt = applied_receipt(&command_id, &effect_id, &source);
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: Some(receipt.clone()),
        });
        let durable = Arc::new(FixtureHostRepository::default());
        let failing = Arc::new(FailSettlementOnceRepository {
            inner: durable.clone(),
            armed: AtomicBool::new(true),
        });
        let (host, binding_id, lease) = available_host(failing, service.clone()).await;
        let command = revoke_command(command_id, effect_id.clone(), source);

        assert!(matches!(
            host.revoke_bound_surface(&lease, &binding_id, command.clone())
                .await,
            Err(CompleteAgentHostError::Store(
                CompleteAgentHostStoreError::Persistence { .. }
            ))
        ));
        assert_eq!(service.revoke_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            host.binding(&binding_id)
                .await
                .expect("read binding")
                .expect("binding")
                .state,
            CompleteAgentBindingState::Available
        );

        let restarted =
            CompleteAgentHost::new(durable, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        restarted
            .attach_verified_service(
                fixture_verified_registration(
                    AgentServiceInstanceId::new("service").expect("service"),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("reattach service");
        assert_eq!(
            restarted
                .revoke_bound_surface(&lease, &binding_id, command)
                .await
                .expect("recover revoke"),
            receipt
        );
        assert_eq!(service.revoke_calls.load(Ordering::SeqCst), 1);
        let binding = restarted
            .binding(&binding_id)
            .await
            .expect("read binding")
            .expect("binding");
        assert_eq!(binding.state, CompleteAgentBindingState::PendingSurface);
        assert!(binding.applied_surface.is_none());
        assert!(
            restarted
                .effect(&effect_id)
                .await
                .expect("read effect")
                .expect("effect")
                .receipt
                .is_some()
        );
    }

    #[tokio::test]
    async fn lease_reclaim_fences_old_owner_and_binding_loss_converges_state() {
        let host = fixture_host();
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            command_id: AgentCommandId::new("command").expect("command"),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::new()),
            revoke_receipt: None,
        });
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service,
            )
            .await
            .expect("attach service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            target: selection.target,
            generation: AgentBindingGeneration(3),
            source: AgentSourceCoordinate::new("source").expect("source"),
            profile_digest: descriptor().profile_digest,
            bound_surface: empty_bound_surface(),
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        })
        .await
        .expect("register binding");

        let first = host
            .acquire_binding_lease(&binding_id, AgentBindingGeneration(3), "worker-a", 0, 10)
            .await
            .expect("first lease");
        assert_eq!(
            host.acquire_binding_lease(&binding_id, AgentBindingGeneration(3), "worker-b", 5, 15,)
                .await,
            Err(CompleteAgentHostError::LeaseConflict)
        );
        let reclaimed = host
            .acquire_binding_lease(&binding_id, AgentBindingGeneration(3), "worker-b", 10, 20)
            .await
            .expect("reclaimed lease");
        assert!(reclaimed.epoch > first.epoch);
        assert_eq!(
            host.release_binding_lease(&first, 11).await,
            Err(CompleteAgentHostError::LeaseConflict)
        );

        host.mark_binding_lost(&binding_id, AgentBindingGeneration(3))
            .await
            .expect("mark lost");
        assert_eq!(
            host.binding(&binding_id)
                .await
                .expect("read binding")
                .expect("binding")
                .state,
            CompleteAgentBindingState::Lost
        );
        assert!(matches!(
            host.acquire_binding_lease(&binding_id, AgentBindingGeneration(3), "worker-c", 11, 21,)
                .await,
            Err(CompleteAgentHostError::DispatchRejected { .. })
        ));
    }

    #[tokio::test]
    async fn late_receipt_from_previous_delivery_epoch_cannot_advance_effect() {
        let host = fixture_host();
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let record = effect_record("sha256:payload");
        let stale_lease = seed_effect(&host, record.clone(), None).await;
        host.acquire_binding_lease(
            &record.binding_id,
            record.generation,
            "replacement-worker",
            stale_lease.expires_at_ms,
            stale_lease.expires_at_ms + 10_000,
        )
        .await
        .expect("take over seeded lease");
        let receipt = AgentCommandReceipt {
            command_id: record.command_id.clone(),
            effect_id: effect_id.clone(),
            source: record.source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        };

        assert_eq!(
            host.record_receipt(&stale_lease, &effect_id, receipt).await,
            Err(CompleteAgentHostError::LeaseConflict)
        );
        assert_eq!(
            host.effect(&effect_id)
                .await
                .expect("read effect")
                .expect("effect")
                .state,
            CompleteAgentEffectState::Dispatching
        );
    }

    #[tokio::test]
    async fn terminal_effect_observations_are_idempotent_and_never_downgrade() {
        let host = fixture_host();
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let record = effect_record("sha256:payload");
        let applied = AgentCommandReceipt {
            command_id: record.command_id.clone(),
            effect_id: effect_id.clone(),
            source: record.source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        };
        let lease = seed_effect(&host, record, None).await;

        host.record_receipt(&lease, &effect_id, applied.clone())
            .await
            .expect("first terminal observation");
        host.record_receipt(&lease, &effect_id, applied)
            .await
            .expect("duplicate terminal observation");
        assert_eq!(
            host.mark_unknown(&lease, &effect_id).await,
            Err(CompleteAgentHostError::EffectObservationConflict {
                current: CompleteAgentEffectState::Applied,
                observed: CompleteAgentEffectState::Unknown,
            })
        );

        let conflicting_applied = AgentCommandReceipt {
            command_id: AgentCommandId::new("command").expect("command"),
            effect_id: effect_id.clone(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            state: AgentReceiptState::AlreadyApplied {
                terminal: Some(agentdash_agent_service_api::AgentTerminalOutcome::Succeeded),
            },
            snapshot_revision: None,
            initial_context: None,
        };
        assert_eq!(
            host.record_receipt(&lease, &effect_id, conflicting_applied)
                .await,
            Err(CompleteAgentHostError::EffectEvidenceConflict {
                effect_id: effect_id.clone(),
            })
        );

        let rejected = AgentCommandReceipt {
            command_id: AgentCommandId::new("command").expect("command"),
            effect_id: effect_id.clone(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            state: AgentReceiptState::Rejected {
                code: "late_conflict".to_owned(),
                message: "late conflict".to_owned(),
            },
            snapshot_revision: None,
            initial_context: None,
        };
        assert_eq!(
            host.record_receipt(&lease, &effect_id, rejected).await,
            Err(CompleteAgentHostError::EffectObservationConflict {
                current: CompleteAgentEffectState::Applied,
                observed: CompleteAgentEffectState::Rejected,
            })
        );
        assert_eq!(
            host.effect(&effect_id)
                .await
                .expect("read effect")
                .expect("effect")
                .state,
            CompleteAgentEffectState::Applied
        );
    }

    #[tokio::test]
    async fn mismatched_surface_receipt_never_makes_binding_available() {
        let host = fixture_host();
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target: fixture_binding_target(
                AgentServiceInstanceId::new("service").expect("service"),
            ),
            generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source").expect("source"),
            profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            bound_surface: empty_bound_surface(),
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        };
        let mut record = effect_record("sha256:surface-command");
        record.binding_id = binding_id.clone();
        record.effect_id = effect_id.clone();
        let lease = seed_effect(&host, record.clone(), Some(binding)).await;
        let receipt = AppliedAgentSurfaceReceipt {
            command_id: record.command_id,
            effect_id: effect_id.clone(),
            source: record.source,
            applied: AppliedAgentSurface {
                revision: agentdash_agent_service_api::AgentSurfaceRevision(1),
                digest: agentdash_agent_service_api::AgentSurfaceDigest::new("wrong-surface")
                    .expect("surface"),
                contributions: Vec::new(),
            },
        };

        assert!(matches!(
            host.record_surface_receipt(&lease, &binding_id, &effect_id, receipt, None)
                .await,
            Err(CompleteAgentHostError::DispatchRejected { .. })
        ));
        assert_eq!(
            host.binding(&binding_id)
                .await
                .expect("read binding")
                .expect("binding")
                .state,
            CompleteAgentBindingState::PendingSurface
        );
    }

    fn effect_record(digest: &str) -> CompleteAgentEffectRecord {
        CompleteAgentEffectRecord {
            effect_id: AgentEffectIdentity::new("effect").expect("effect"),
            command_id: AgentCommandId::new("command").expect("command"),
            binding_id: CompleteAgentBindingId::new("binding").expect("binding"),
            generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source").expect("source"),
            payload_digest: AgentPayloadDigest::new(digest).expect("digest"),
            delivery_epoch: 1,
            dispatch_attempt: 1,
            state: CompleteAgentEffectState::Dispatching,
            receipt: None,
            surface_receipt: None,
            inspection: None,
            attempt_history: Vec::new(),
        }
    }

    fn fixture_host() -> CompleteAgentHost {
        CompleteAgentHost::new(
            Arc::new(FixtureHostRepository::default()),
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        )
    }

    fn fixture_placement() -> CompleteAgentPlacement {
        CompleteAgentPlacement::InProcess {
            host_incarnation_id: "fixture-host".to_owned(),
        }
    }

    fn fixture_binding_target(instance_id: AgentServiceInstanceId) -> CompleteAgentBindingTarget {
        let descriptor = descriptor();
        CompleteAgentBindingTarget {
            logical_instance_id: instance_id,
            live_attachment_id: CompleteAgentLiveAttachmentId::new("fixture-attachment")
                .expect("attachment"),
            definition_id: descriptor.definition_id,
            verified_build_digest: AgentPayloadDigest::new("fixture-build").expect("build digest"),
            verified_profile_digest: descriptor.profile_digest.clone(),
            offer_profile_digest: descriptor.profile_digest,
            placement: fixture_placement(),
            remote_binding: None,
        }
    }

    fn fixture_verified_registration(
        instance_id: AgentServiceInstanceId,
        placement: CompleteAgentPlacement,
        profile_digest: AgentProfileDigest,
    ) -> CompleteAgentVerifiedServiceRegistration {
        CompleteAgentVerifiedServiceRegistration {
            descriptor: descriptor(),
            verification: CompleteAgentServiceVerification {
                service_instance_id: instance_id.clone(),
                publisher_integration: "fixture-integration".to_owned(),
                service_version: "fixture-version".to_owned(),
                verifier_identity: "fixture-verifier".to_owned(),
                verifier_revision: "fixture-verifier-revision".to_owned(),
                method: crate::CompleteAgentVerificationMethod::PinnedBuiltin,
                verified_profile_digest: profile_digest,
                claimed_conformance_suite_revision: "fixture-conformance".to_owned(),
                verified_build: crate::CompleteAgentVerifiedBuildEvidence {
                    claimed_build_digest: AgentPayloadDigest::new("fixture-build")
                        .expect("build digest"),
                    evidence_digest: AgentPayloadDigest::new("fixture-evidence")
                        .expect("evidence digest"),
                },
            },
            instance_id,
            placement,
            remote_binding: None,
        }
    }

    async fn assert_inspection_progression_survives_restarts(
        first_inspection: AgentEffectInspectionState,
    ) {
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::new()),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::from([
                first_inspection.clone(),
                applied_command_inspection_state(&command_id, &effect_id, &source),
            ])),
            revoke_receipt: None,
        });
        let repository = Arc::new(FixtureHostRepository::default());
        let (host, binding_id, lease) = available_host(repository.clone(), service.clone()).await;
        let command = execute_command(command_id, effect_id.clone(), source);

        assert!(matches!(
            host.dispatch_execute(&lease, &binding_id, command.clone())
                .await,
            Err(CompleteAgentHostError::Service(_))
        ));
        let first_receipt = host
            .dispatch_execute(&lease, &binding_id, command.clone())
            .await
            .expect("first inspection");
        assert_eq!(
            receipt_state(&first_receipt.state),
            inspection_state(&first_inspection)
        );
        drop(host);

        let restarted = Arc::new(CompleteAgentHost::new(
            repository.clone(),
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        ));
        restarted
            .attach_verified_service(
                fixture_verified_registration(
                    AgentServiceInstanceId::new("service").expect("service"),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("reattach service");
        assert!(matches!(
            restarted
                .dispatch_execute(&lease, &binding_id, command.clone())
                .await
                .expect("advanced inspection")
                .state,
            AgentReceiptState::AlreadyApplied { .. }
        ));
        drop(restarted);

        let replayed =
            CompleteAgentHost::new(repository, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        replayed
            .attach_verified_service(
                fixture_verified_registration(
                    AgentServiceInstanceId::new("service").expect("service"),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service,
            )
            .await
            .expect("reattach service again");
        assert!(matches!(
            replayed
                .dispatch_execute(&lease, &binding_id, command)
                .await
                .expect("repeat terminal inspection")
                .state,
            AgentReceiptState::AlreadyApplied { .. }
        ));
        assert_eq!(
            replayed
                .effect(&effect_id)
                .await
                .expect("read effect")
                .expect("effect")
                .state,
            CompleteAgentEffectState::Applied
        );
    }

    async fn assert_not_applied_redispatch_settles_current_attempt(applied: bool) {
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let unknown_receipt = AgentCommandReceipt {
            command_id: command_id.clone(),
            effect_id: effect_id.clone(),
            source: source.clone(),
            state: AgentReceiptState::Unknown,
            snapshot_revision: None,
            initial_context: None,
        };
        let current_attempt_outcome = if applied {
            FixtureExecuteOutcome::Receipt(Box::new(applied_receipt(
                &command_id,
                &effect_id,
                &source,
            )))
        } else {
            FixtureExecuteOutcome::Error
        };
        let duplicate_inspection = if applied {
            applied_command_inspection_state(&command_id, &effect_id, &source)
        } else {
            AgentEffectInspectionState::Unknown
        };
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
            revoke_calls: AtomicUsize::new(0),
            execute_outcomes: Mutex::new(VecDeque::from([
                FixtureExecuteOutcome::Receipt(Box::new(unknown_receipt.clone())),
                current_attempt_outcome,
            ])),
            execute_gate: None,
            inspect_gate: None,
            inspection_states: Mutex::new(VecDeque::from([
                AgentEffectInspectionState::NotApplied,
                duplicate_inspection,
            ])),
            revoke_receipt: None,
        });
        let repository = Arc::new(FixtureHostRepository::default());
        let (host, binding_id, lease) = available_host(repository.clone(), service.clone()).await;
        let command = execute_command(command_id.clone(), effect_id.clone(), source.clone());

        assert_eq!(
            host.dispatch_execute(&lease, &binding_id, command.clone())
                .await
                .expect("initial Unknown receipt"),
            unknown_receipt
        );
        assert!(matches!(
            host.dispatch_execute(&lease, &binding_id, command.clone())
                .await
                .expect("inspect NotApplied before redispatch")
                .state,
            AgentReceiptState::Unknown
        ));
        let not_applied = host
            .effect(&effect_id)
            .await
            .expect("read NotApplied effect")
            .expect("effect");
        assert_eq!(not_applied.state, CompleteAgentEffectState::NotApplied);
        assert_eq!(not_applied.receipt, Some(unknown_receipt.clone()));
        assert!(matches!(
            not_applied
                .inspection
                .as_ref()
                .map(|inspection| &inspection.state),
            Some(AgentEffectInspectionState::NotApplied)
        ));
        drop(host);

        let restarted =
            CompleteAgentHost::new(repository, Arc::new(ProcessCompleteAgentLiveCatalog::new()));
        restarted
            .attach_verified_service(
                fixture_verified_registration(
                    AgentServiceInstanceId::new("service").expect("service"),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service.clone(),
            )
            .await
            .expect("reattach service after restart");
        let result = restarted
            .dispatch_execute(&lease, &binding_id, command.clone())
            .await;
        if applied {
            assert!(matches!(
                result.expect("applied redispatch receipt").state,
                AgentReceiptState::AlreadyApplied { .. }
            ));
        } else {
            assert!(matches!(result, Err(CompleteAgentHostError::Service(_))));
        }

        let duplicate = restarted
            .dispatch_execute(&lease, &binding_id, command)
            .await
            .expect("duplicate reconciles current attempt");
        if applied {
            assert!(matches!(
                duplicate.state,
                AgentReceiptState::AlreadyApplied { .. }
            ));
        } else {
            assert_eq!(duplicate.state, AgentReceiptState::Unknown);
        }
        assert_eq!(service.execute_calls.load(Ordering::SeqCst), 2);

        let effect = restarted
            .effect(&effect_id)
            .await
            .expect("read effect")
            .expect("effect");
        assert_eq!(effect.dispatch_attempt, 2);
        assert_eq!(
            effect.state,
            if applied {
                CompleteAgentEffectState::Applied
            } else {
                CompleteAgentEffectState::Unknown
            }
        );
        assert_eq!(effect.attempt_history.len(), 1);
        let archived = &effect.attempt_history[0];
        assert_eq!(archived.dispatch_attempt, 1);
        assert_eq!(archived.delivery_epoch, lease.epoch);
        assert_eq!(archived.state, CompleteAgentEffectState::NotApplied);
        assert_eq!(archived.receipt, Some(unknown_receipt));
        assert!(archived.surface_receipt.is_none());
        assert_eq!(
            archived
                .inspection
                .as_ref()
                .expect("archived inspection")
                .state,
            AgentEffectInspectionState::NotApplied,
        );
        if applied {
            assert!(effect.receipt.is_some());
        } else {
            assert!(effect.receipt.is_none());
        }
        assert!(effect.inspection.is_some());
    }

    async fn available_host(
        repository: Arc<dyn CompleteAgentHostRepository>,
        service: Arc<UnknownThenAppliedService>,
    ) -> (
        Arc<CompleteAgentHost>,
        CompleteAgentBindingId,
        CompleteAgentBindingLease,
    ) {
        let host = Arc::new(CompleteAgentHost::new(
            repository,
            Arc::new(ProcessCompleteAgentLiveCatalog::new()),
        ));
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let selection = host
            .attach_verified_service(
                fixture_verified_registration(
                    instance_id.clone(),
                    fixture_placement(),
                    descriptor().profile_digest,
                ),
                service,
            )
            .await
            .expect("attach service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let bound_surface = empty_bound_surface();
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            target: selection.target,
            generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source").expect("source"),
            profile_digest: descriptor().profile_digest,
            applied_surface: Some(AppliedAgentSurface {
                revision: bound_surface.revision,
                digest: bound_surface.digest.clone(),
                contributions: Vec::new(),
            }),
            bound_surface,
            state: CompleteAgentBindingState::Available,
        })
        .await
        .expect("register binding");
        let now = current_time_ms();
        let lease = host
            .acquire_binding_lease(
                &binding_id,
                AgentBindingGeneration(1),
                "worker-1",
                now,
                now + 10_000,
            )
            .await
            .expect("acquire lease");
        (host, binding_id, lease)
    }

    fn execute_command(
        command_id: AgentCommandId,
        effect_id: AgentEffectIdentity,
        source: AgentSourceCoordinate,
    ) -> AgentCommandEnvelope {
        AgentCommandEnvelope {
            meta: AgentCommandMeta {
                command_id,
                effect_id,
                idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey::new("idem")
                    .expect("idempotency"),
                binding_generation: AgentBindingGeneration(1),
                expected_snapshot_revision: None,
            },
            source,
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: Vec::new(),
                },
            },
        }
    }

    fn revoke_command(
        command_id: AgentCommandId,
        effect_id: AgentEffectIdentity,
        source: AgentSourceCoordinate,
    ) -> RevokeBoundAgentSurface {
        RevokeBoundAgentSurface {
            command_id,
            effect_id,
            idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey::new("revoke-idem")
                .expect("idempotency"),
            binding_generation: AgentBindingGeneration(1),
            source,
            expected_revision: AgentSurfaceRevision(1),
        }
    }

    fn applied_receipt(
        command_id: &AgentCommandId,
        effect_id: &AgentEffectIdentity,
        source: &AgentSourceCoordinate,
    ) -> AgentCommandReceipt {
        AgentCommandReceipt {
            command_id: command_id.clone(),
            effect_id: effect_id.clone(),
            source: source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        }
    }

    fn applied_command_inspection_state(
        command_id: &AgentCommandId,
        effect_id: &AgentEffectIdentity,
        source: &AgentSourceCoordinate,
    ) -> AgentEffectInspectionState {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command {
                receipt: applied_agent_receipt(&applied_receipt(command_id, effect_id, source)),
            },
        }
    }

    async fn seed_effect(
        host: &CompleteAgentHost,
        record: CompleteAgentEffectRecord,
        binding: Option<CompleteAgentBinding>,
    ) -> CompleteAgentBindingLease {
        let descriptor = descriptor();
        let binding = binding.unwrap_or_else(|| CompleteAgentBinding {
            id: record.binding_id.clone(),
            target: fixture_binding_target(
                AgentServiceInstanceId::new("service").expect("service"),
            ),
            generation: record.generation,
            source: record.source.clone(),
            profile_digest: descriptor.profile_digest.clone(),
            bound_surface: empty_bound_surface(),
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        });
        let mut facts = CompleteAgentHostFacts::default();
        facts
            .source_coordinates
            .insert(binding.id.clone(), binding.source.clone());
        let lease = CompleteAgentBindingLease {
            binding_id: binding.id.clone(),
            generation: binding.generation,
            owner: "fixture-worker".to_owned(),
            token: "fixture-token".to_owned(),
            epoch: record.delivery_epoch,
            expires_at_ms: current_time_ms() + 10_000,
        };
        facts.lease_epochs.insert(binding.id.clone(), lease.epoch);
        facts.leases.insert(binding.id.clone(), lease.clone());
        facts.bindings.insert(binding.id.clone(), binding);
        facts.effects.insert(record.effect_id.clone(), record);
        host.commit(crate::CompleteAgentHostRevision(0), facts)
            .await
            .expect("seed host facts");
        lease
    }

    enum FixtureExecuteOutcome {
        Receipt(Box<AgentCommandReceipt>),
        Error,
    }

    struct UnknownThenAppliedService {
        descriptor: AgentServiceDescriptor,
        source: AgentSourceCoordinate,
        command_id: AgentCommandId,
        execute_calls: AtomicUsize,
        revoke_calls: AtomicUsize,
        execute_outcomes: Mutex<VecDeque<FixtureExecuteOutcome>>,
        execute_gate: Option<Arc<ExternalOutcomeGate>>,
        inspect_gate: Option<Arc<ExternalOutcomeGate>>,
        inspection_states: Mutex<VecDeque<AgentEffectInspectionState>>,
        revoke_receipt: Option<AgentCommandReceipt>,
    }

    #[derive(Default)]
    struct ExternalOutcomeGate {
        entered: Notify,
        release: Notify,
    }

    impl ExternalOutcomeGate {
        async fn pause(&self) {
            self.entered.notify_one();
            self.release.notified().await;
        }
    }

    #[async_trait::async_trait]
    impl CompleteAgentService for UnknownThenAppliedService {
        async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
            Ok(self.descriptor.clone())
        }

        async fn create(
            &self,
            _command: agentdash_agent_service_api::CreateAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn resume(
            &self,
            _command: agentdash_agent_service_api::ResumeAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn fork(
            &self,
            _command: agentdash_agent_service_api::ForkAgentCommand,
        ) -> Result<agentdash_agent_service_api::ForkAgentReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn execute(
            &self,
            _command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.execute_gate {
                gate.pause().await;
            }
            if let Some(FixtureExecuteOutcome::Receipt(receipt)) =
                self.execute_outcomes.lock().await.pop_front()
            {
                return Ok(*receipt);
            }
            Err(AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                "receipt lost",
                true,
            ))
        }

        async fn read(
            &self,
            _query: AgentReadQuery,
        ) -> Result<agentdash_agent_service_api::AgentSnapshot, AgentServiceError> {
            Err(unsupported())
        }

        async fn changes(
            &self,
            _query: AgentChangesQuery,
        ) -> Result<agentdash_agent_service_api::AgentChangePage, AgentServiceError> {
            Err(unsupported())
        }

        async fn inspect(
            &self,
            identity: AgentEffectIdentity,
        ) -> Result<AgentEffectInspection, AgentServiceError> {
            if let Some(gate) = &self.inspect_gate {
                gate.pause().await;
            }
            let state = self
                .inspection_states
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| {
                    applied_command_inspection_state(&self.command_id, &identity, &self.source)
                });
            Ok(AgentEffectInspection {
                effect_id: identity,
                command_id: Some(self.command_id.clone()),
                state,
            })
        }

        async fn apply_surface(
            &self,
            _command: ApplyBoundAgentSurface,
        ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn revoke_surface(
            &self,
            _command: RevokeBoundAgentSurface,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            self.revoke_calls.fetch_add(1, Ordering::SeqCst);
            self.revoke_receipt.clone().ok_or_else(unsupported)
        }
    }

    fn unsupported() -> AgentServiceError {
        AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "not used by test",
            false,
        )
    }

    fn capability_facet(
        semantics: AgentSurfaceSemanticFacet,
        route: AgentSurfaceRoute,
    ) -> AgentSurfaceCapabilityFacet {
        AgentSurfaceCapabilityFacet {
            semantics,
            routes: BTreeSet::from([route]),
            fidelity: SemanticFidelity::Exact,
            configuration_boundary: AgentConfigurationBoundary::Binding,
        }
    }

    fn descriptor() -> AgentServiceDescriptor {
        AgentServiceDescriptor {
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            title: "Test".to_owned(),
            protocol_revision: 1,
            profile: AgentCapabilityProfile {
                lifecycle: BTreeSet::<AgentLifecycleCapability>::new(),
                commands: BTreeSet::<AgentCommandCapability>::new(),
                fork: AgentForkCapability {
                    cutoffs: BTreeMap::<AgentForkCutoffKind, SemanticFidelity>::new(),
                    lineage_fidelity: SemanticFidelity::Unsupported,
                    native_durability: SemanticFidelity::Unsupported,
                },
                compaction: BTreeMap::<AgentCompactionMode, SemanticFidelity>::new(),
                source_changes: AgentSourceChangeLevel::SnapshotOnly,
                initial_context: InitialContextProfile {
                    contribution_fidelity: BTreeMap::new(),
                    applied_evidence: InitialContextAppliedEvidence::Unsupported,
                    renderer_versions: BTreeSet::new(),
                },
                surface: AgentSurfaceProfile { facets: Vec::new() },
                inspect_effects: SemanticFidelity::Exact,
            },
            profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            configuration_boundary: AgentConfigurationBoundary::Binding,
        }
    }

    fn empty_bound_surface() -> BoundAgentSurface {
        BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: agentdash_agent_service_api::AgentSurfaceDigest::new("surface")
                .expect("surface"),
            offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            contributions: Vec::new(),
        }
    }
}
