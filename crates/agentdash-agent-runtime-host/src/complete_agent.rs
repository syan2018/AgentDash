use std::sync::Arc;

use crate::{
    CompleteAgentCallbackRoute, CompleteAgentHostCommit, CompleteAgentHostFacts,
    CompleteAgentHostRepository, CompleteAgentHostSnapshot, CompleteAgentHostStoreError,
    SharedCompleteAgentHostRepository, SharedCompleteAgentServiceRegistry,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCommandEnvelope, AgentCommandId, AgentCommandReceipt,
    AgentEffectIdentity, AgentEffectInspection, AgentEffectInspectionState, AgentPayloadDigest,
    AgentProfileDigest, AgentReadQuery, AgentReceiptState, AgentRuntimeOffer,
    AgentServiceDescriptor, AgentServiceError, AgentServiceInstanceId, AgentSourceCoordinate,
    AgentSurfaceProfile, AgentSurfaceSemanticFacet, AppliedAgentSurface,
    AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface, BoundAgentSurface, CompleteAgentService,
    RevokeBoundAgentSurface, SemanticFidelity,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentBinding {
    pub id: CompleteAgentBindingId,
    pub service_instance_id: AgentServiceInstanceId,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface: BoundAgentSurface,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub state: CompleteAgentBindingState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl CompleteAgentBinding {
    pub fn dispatch_admitted(&self) -> bool {
        self.state == CompleteAgentBindingState::Available
            && self
                .applied_surface
                .as_ref()
                .is_some_and(|applied| self.bound_surface.accepts_applied(applied))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompleteAgentBindingState {
    PendingSurface,
    Available,
    Desynchronized,
    Lost,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentBindingLease {
    pub binding_id: CompleteAgentBindingId,
    pub generation: AgentBindingGeneration,
    pub owner: String,
    pub token: String,
    pub epoch: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompleteAgentEffectState {
    Dispatching,
    Accepted,
    Applied,
    Rejected,
    NotApplied,
    Unknown,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentEffectAttemptEvidence {
    pub dispatch_attempt: u64,
    pub delivery_epoch: u64,
    pub state: CompleteAgentEffectState,
    pub receipt: Option<AgentCommandReceipt>,
    pub surface_receipt: Option<AppliedAgentSurfaceReceipt>,
    pub inspection: Option<AgentEffectInspection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentEffectRecord {
    pub effect_id: AgentEffectIdentity,
    pub command_id: AgentCommandId,
    pub binding_id: CompleteAgentBindingId,
    pub service_instance_id: AgentServiceInstanceId,
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
    #[error("Complete Agent service instance was not found: {instance_id}")]
    UnknownService { instance_id: AgentServiceInstanceId },
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
    #[error("Complete Agent host invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("Complete Agent payload cannot be encoded: {reason}")]
    Encoding { reason: String },
    #[error(transparent)]
    Store(#[from] CompleteAgentHostStoreError),
    #[error(transparent)]
    Service(#[from] AgentServiceError),
}

/// Target Complete-Agent lane for service registration, binding/generation fencing, and stable
/// effect reconciliation. It is additive until the production registry cutover.
pub struct CompleteAgentHost {
    repository: SharedCompleteAgentHostRepository,
    services: SharedCompleteAgentServiceRegistry,
}

impl CompleteAgentHost {
    pub fn new(
        repository: Arc<dyn CompleteAgentHostRepository>,
        services: SharedCompleteAgentServiceRegistry,
    ) -> Self {
        Self {
            repository,
            services,
        }
    }

    pub async fn register_service(
        &self,
        instance_id: AgentServiceInstanceId,
        placement: CompleteAgentPlacement,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<AgentServiceDescriptor, CompleteAgentHostError> {
        if !placement.is_valid() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Complete Agent placement coordinates must not be empty".to_owned(),
            });
        }
        let descriptor = service.describe().await?;
        validate_service_descriptor(&descriptor)?;
        let offer = runtime_offer_from_descriptor(&descriptor)?;
        let snapshot = self.repository.load().await?;
        let mut facts = snapshot.facts;
        if let Some(existing) = facts.service_instances.get(&instance_id) {
            if existing != &descriptor || facts.placements.get(&instance_id) != Some(&placement) {
                return Err(CompleteAgentHostError::Invariant {
                    reason:
                        "service instance id is already registered with another descriptor or placement"
                            .to_owned(),
                });
            }
        } else {
            facts
                .service_instances
                .insert(instance_id.clone(), descriptor.clone());
            facts.offers.insert(instance_id.clone(), offer);
            facts.placements.insert(instance_id.clone(), placement);
            self.commit(snapshot.revision, facts).await?;
        }
        self.services.attach(instance_id, service).await;
        Ok(descriptor)
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
        let descriptor = facts
            .service_instances
            .get(&binding.service_instance_id)
            .ok_or_else(|| CompleteAgentHostError::UnknownService {
                instance_id: binding.service_instance_id.clone(),
            })?;
        if binding.profile_digest != descriptor.profile_digest
            || binding.bound_surface.offer_profile_digest != binding.profile_digest
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "binding profile does not match the registered service descriptor"
                    .to_owned(),
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
        if facts
            .source_coordinates
            .values()
            .any(|source| source == &binding.source)
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "source coordinate is already assigned to another binding".to_owned(),
            });
        }
        facts
            .source_coordinates
            .insert(binding.id.clone(), binding.source.clone());
        facts.bindings.insert(binding.id.clone(), binding);
        self.commit(snapshot.revision, facts).await?;
        Ok(())
    }

    pub async fn runtime_offer(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Result<AgentRuntimeOffer, CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        snapshot
            .facts
            .offers
            .get(instance_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::UnknownService {
                instance_id: instance_id.clone(),
            })
    }

    pub async fn placement(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Result<CompleteAgentPlacement, CompleteAgentHostError> {
        let snapshot = self.repository.load().await?;
        snapshot
            .facts
            .placements
            .get(instance_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::UnknownService {
                instance_id: instance_id.clone(),
            })
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

    pub async fn dispatch_execute(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, CompleteAgentHostError> {
        let digest = payload_digest(&command)?;
        let (service_instance_id, source, should_dispatch) = {
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
                service_instance_id: binding.service_instance_id.clone(),
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
                        binding.service_instance_id.clone(),
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
                    let result = (
                        binding.service_instance_id.clone(),
                        binding.source.clone(),
                        true,
                    );
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };

        let service = self.service(&service_instance_id).await?;
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
        let service_instance_id = {
            let state = self.repository.load().await?.facts;
            let record = state.effects.get(effect_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                }
            })?;
            validate_effect_fence(&state, lease, record, current_time_ms())?;
            record.service_instance_id.clone()
        };
        let service = self.service(&service_instance_id).await?;
        let inspection = service.inspect(effect_id.clone()).await?;

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
        let (service_instance_id, should_dispatch) = {
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
                service_instance_id: binding.service_instance_id.clone(),
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
                    let result = (binding.service_instance_id, confirmed_not_applied);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
                None => {
                    state.effects.insert(command.effect_id.clone(), candidate);
                    let result = (binding.service_instance_id, true);
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };
        let service = self.service(&service_instance_id).await?;
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
                self.record_surface_receipt(
                    lease,
                    binding_id,
                    &command.effect_id,
                    receipt.clone(),
                    callback_route.as_ref(),
                )
                .await?;
                Ok(receipt)
            }
            Err(error) => {
                self.mark_unknown(lease, &command.effect_id).await?;
                Err(error.into())
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
        let (service_instance_id, source, plan) = {
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
                service_instance_id: binding.service_instance_id.clone(),
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
                        binding.service_instance_id,
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
                    let result = (
                        binding.service_instance_id,
                        binding.source,
                        RevokeDispatchPlan::Dispatch,
                    );
                    self.commit(snapshot.revision, state).await?;
                    result
                }
            }
        };
        let receipt = match plan {
            RevokeDispatchPlan::Dispatch => {
                let service = self.service(&service_instance_id).await?;
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
        instance_id: &AgentServiceInstanceId,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentHostError> {
        self.services.resolve(instance_id).await.ok_or_else(|| {
            CompleteAgentHostError::UnknownService {
                instance_id: instance_id.clone(),
            }
        })
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
        match inspection.state {
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
            AgentEffectInspectionState::Applied { .. } => {}
        }
        let (service_instance_id, source, command_id) = {
            let state = self.repository.load().await?.facts;
            let record = state.effects.get(effect_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                }
            })?;
            (
                record.service_instance_id.clone(),
                record.source.clone(),
                record.command_id.clone(),
            )
        };
        let snapshot = self
            .service(&service_instance_id)
            .await?
            .read(AgentReadQuery {
                source: source.clone(),
                at_revision: None,
            })
            .await?;
        let applied =
            snapshot
                .applied_surface
                .ok_or_else(|| CompleteAgentHostError::Invariant {
                    reason: "applied surface effect is missing from authoritative Agent snapshot"
                        .to_owned(),
                })?;
        let receipt = AppliedAgentSurfaceReceipt {
            command_id,
            effect_id: effect_id.clone(),
            source,
            applied,
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

fn validate_service_descriptor(
    descriptor: &AgentServiceDescriptor,
) -> Result<(), CompleteAgentHostError> {
    validate_surface_profile(&descriptor.profile.surface)
}

fn runtime_offer_from_descriptor(
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
        || existing.service_instance_id != candidate.service_instance_id
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
            terminal,
            initial_context,
            ..
        } => {
            return AgentCommandReceipt {
                command_id: expected_command_id,
                effect_id: inspection.effect_id,
                source: expected_source,
                state: AgentReceiptState::AlreadyApplied { terminal },
                snapshot_revision: None,
                initial_context,
            };
        }
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
        AgentEffectInspectionState::Accepted { source }
        | AgentEffectInspectionState::Applied { source, .. } => Some(source),
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
    use tokio::sync::{Mutex, Notify, RwLock};

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

    #[derive(Default)]
    struct FixtureServiceRegistry {
        handles: RwLock<BTreeMap<AgentServiceInstanceId, Arc<dyn CompleteAgentService>>>,
    }

    #[async_trait]
    impl crate::CompleteAgentServiceRegistry for FixtureServiceRegistry {
        async fn attach(
            &self,
            instance_id: AgentServiceInstanceId,
            service: Arc<dyn CompleteAgentService>,
        ) {
            self.handles.write().await.insert(instance_id, service);
        }

        async fn resolve(
            &self,
            instance_id: &AgentServiceInstanceId,
        ) -> Option<Arc<dyn CompleteAgentService>> {
            self.handles.read().await.get(instance_id).cloned()
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

    #[test]
    fn stale_generation_is_fenced() {
        let binding = CompleteAgentBinding {
            id: CompleteAgentBindingId::new("binding").expect("binding"),
            service_instance_id: AgentServiceInstanceId::new("service").expect("service"),
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
        host.register_service(instance_id.clone(), fixture_placement(), service)
            .await
            .expect("register service");

        let offer = host.runtime_offer(&instance_id).await.expect("offer");

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
            Arc::new(FixtureServiceRegistry::default()),
        );
        host.register_service(instance_id.clone(), fixture_placement(), service.clone())
            .await
            .expect("register service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let bound_surface = empty_bound_surface();
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            service_instance_id: instance_id,
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
            CompleteAgentHost::new(repository, Arc::new(FixtureServiceRegistry::default()));
        restarted
            .register_service(
                AgentServiceInstanceId::new("service").expect("service"),
                fixture_placement(),
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
            service_instance_id: AgentServiceInstanceId::new("service").expect("service"),
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
            service_instance_id: binding.service_instance_id.clone(),
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
            CompleteAgentHost::new(durable, Arc::new(FixtureServiceRegistry::default()));
        restarted
            .register_service(
                AgentServiceInstanceId::new("service").expect("service"),
                fixture_placement(),
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
        host.register_service(instance_id.clone(), fixture_placement(), service)
            .await
            .expect("register service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            service_instance_id: instance_id,
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
            service_instance_id: AgentServiceInstanceId::new("service").expect("service"),
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
            service_instance_id: AgentServiceInstanceId::new("service").expect("service"),
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
            Arc::new(FixtureServiceRegistry::default()),
        )
    }

    fn fixture_placement() -> CompleteAgentPlacement {
        CompleteAgentPlacement::InProcess {
            host_incarnation_id: "fixture-host".to_owned(),
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
                AgentEffectInspectionState::Applied {
                    source: source.clone(),
                    terminal: None,
                    initial_context: None,
                    child_source: None,
                },
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
            Arc::new(FixtureServiceRegistry::default()),
        ));
        restarted
            .register_service(
                AgentServiceInstanceId::new("service").expect("service"),
                fixture_placement(),
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
            CompleteAgentHost::new(repository, Arc::new(FixtureServiceRegistry::default()));
        replayed
            .register_service(
                AgentServiceInstanceId::new("service").expect("service"),
                fixture_placement(),
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
            AgentEffectInspectionState::Applied {
                source: source.clone(),
                terminal: None,
                initial_context: None,
                child_source: None,
            }
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
            CompleteAgentHost::new(repository, Arc::new(FixtureServiceRegistry::default()));
        restarted
            .register_service(
                AgentServiceInstanceId::new("service").expect("service"),
                fixture_placement(),
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
            Arc::new(FixtureServiceRegistry::default()),
        ));
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        host.register_service(instance_id.clone(), fixture_placement(), service)
            .await
            .expect("register service");
        let binding_id = CompleteAgentBindingId::new("binding").expect("binding");
        let bound_surface = empty_bound_surface();
        host.register_binding(CompleteAgentBinding {
            id: binding_id.clone(),
            service_instance_id: instance_id,
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

    async fn seed_effect(
        host: &CompleteAgentHost,
        record: CompleteAgentEffectRecord,
        binding: Option<CompleteAgentBinding>,
    ) -> CompleteAgentBindingLease {
        let descriptor = descriptor();
        let binding = binding.unwrap_or_else(|| CompleteAgentBinding {
            id: record.binding_id.clone(),
            service_instance_id: record.service_instance_id.clone(),
            generation: record.generation,
            source: record.source.clone(),
            profile_digest: descriptor.profile_digest.clone(),
            bound_surface: empty_bound_surface(),
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        });
        let offer = runtime_offer_from_descriptor(&descriptor).expect("runtime offer");
        let mut facts = CompleteAgentHostFacts::default();
        facts
            .service_instances
            .insert(binding.service_instance_id.clone(), descriptor);
        facts
            .offers
            .insert(binding.service_instance_id.clone(), offer);
        facts
            .placements
            .insert(binding.service_instance_id.clone(), fixture_placement());
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
                .unwrap_or_else(|| AgentEffectInspectionState::Applied {
                    source: self.source.clone(),
                    terminal: None,
                    initial_context: None,
                    child_source: None,
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
