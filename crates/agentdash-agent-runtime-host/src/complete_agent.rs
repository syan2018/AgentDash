use std::{collections::BTreeMap, sync::Arc};

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
use tokio::sync::{Mutex, RwLock};

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
pub struct CompleteAgentEffectRecord {
    pub effect_id: AgentEffectIdentity,
    pub command_id: AgentCommandId,
    pub binding_id: CompleteAgentBindingId,
    pub service_instance_id: AgentServiceInstanceId,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub payload_digest: AgentPayloadDigest,
    pub delivery_epoch: u64,
    pub state: CompleteAgentEffectState,
    pub receipt: Option<AgentCommandReceipt>,
    pub surface_receipt: Option<AppliedAgentSurfaceReceipt>,
    pub inspection: Option<AgentEffectInspection>,
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
    Service(#[from] AgentServiceError),
}

#[derive(Clone)]
struct RegisteredCompleteAgentService {
    service: Arc<dyn CompleteAgentService>,
    descriptor: AgentServiceDescriptor,
}

#[derive(Default)]
struct CompleteAgentHostState {
    bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    effects: BTreeMap<AgentEffectIdentity, CompleteAgentEffectRecord>,
    leases: BTreeMap<CompleteAgentBindingId, CompleteAgentBindingLease>,
    lease_epochs: BTreeMap<CompleteAgentBindingId, u64>,
}

/// Target Complete-Agent lane for service registration, binding/generation fencing, and stable
/// effect reconciliation. It is additive until the production registry cutover.
#[derive(Default)]
pub struct CompleteAgentHost {
    services: RwLock<BTreeMap<AgentServiceInstanceId, RegisteredCompleteAgentService>>,
    state: Mutex<CompleteAgentHostState>,
}

impl CompleteAgentHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_service(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<AgentServiceDescriptor, CompleteAgentHostError> {
        let descriptor = service.describe().await?;
        validate_service_descriptor(&descriptor)?;
        let mut services = self.services.write().await;
        if let Some(existing) = services.get(&instance_id) {
            if existing.descriptor != descriptor {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "service instance id is already registered with another descriptor"
                        .to_owned(),
                });
            }
            return Ok(existing.descriptor.clone());
        }
        services.insert(
            instance_id,
            RegisteredCompleteAgentService {
                service,
                descriptor: descriptor.clone(),
            },
        );
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
        let services = self.services.read().await;
        let registered = services.get(&binding.service_instance_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownService {
                instance_id: binding.service_instance_id.clone(),
            }
        })?;
        if binding.profile_digest != registered.descriptor.profile_digest
            || binding.bound_surface.offer_profile_digest != binding.profile_digest
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "binding profile does not match the registered service descriptor"
                    .to_owned(),
            });
        }
        drop(services);

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
        let mut state = self.state.lock().await;
        if let Some(existing) = state.bindings.get(&binding.id) {
            if existing == &binding {
                return Ok(());
            }
            return Err(CompleteAgentHostError::Invariant {
                reason: "binding id is already reserved with different coordinates".to_owned(),
            });
        }
        state.bindings.insert(binding.id.clone(), binding);
        Ok(())
    }

    pub async fn runtime_offer(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Result<AgentRuntimeOffer, CompleteAgentHostError> {
        let services = self.services.read().await;
        let descriptor = &services
            .get(instance_id)
            .ok_or_else(|| CompleteAgentHostError::UnknownService {
                instance_id: instance_id.clone(),
            })?
            .descriptor;
        let surface = &descriptor.profile.surface;
        validate_surface_profile(surface)?;
        Ok(AgentRuntimeOffer {
            profile_digest: descriptor.profile_digest.clone(),
            contributions: surface.facets.clone(),
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
        let mut state = self.state.lock().await;
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
        let mut state = self.state.lock().await;
        validate_lease_state(&state, lease, now_ms)?;
        let current = state
            .leases
            .get_mut(&lease.binding_id)
            .expect("validated lease exists in the same state");
        current.expires_at_ms = expires_at_ms;
        Ok(current.clone())
    }

    pub async fn release_binding_lease(
        &self,
        lease: &CompleteAgentBindingLease,
        now_ms: u64,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.lock().await;
        validate_lease_state(&state, lease, now_ms)?;
        state.leases.remove(&lease.binding_id);
        Ok(())
    }

    pub async fn mark_binding_lost(
        &self,
        binding_id: &CompleteAgentBindingId,
        generation: AgentBindingGeneration,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.lock().await;
        let binding = state.bindings.get_mut(binding_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        ensure_generation(binding, generation)?;
        binding.state = CompleteAgentBindingState::Lost;
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
            let mut state = self.state.lock().await;
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
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
            };
            match state.effects.get(&command.meta.effect_id) {
                Some(existing) => {
                    ensure_same_effect(existing, &candidate)?;
                    if matches!(
                        existing.state,
                        CompleteAgentEffectState::Applied | CompleteAgentEffectState::Rejected
                    ) && let Some(receipt) = &existing.receipt
                    {
                        return Ok(receipt.clone());
                    }
                    let confirmed_not_applied =
                        existing.state == CompleteAgentEffectState::NotApplied;
                    if confirmed_not_applied {
                        let effect = state
                            .effects
                            .get_mut(&command.meta.effect_id)
                            .expect("effect was read from the same state");
                        effect.state = CompleteAgentEffectState::Dispatching;
                        effect.delivery_epoch = lease.epoch;
                    } else {
                        state
                            .effects
                            .get_mut(&command.meta.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    (
                        binding.service_instance_id.clone(),
                        binding.source.clone(),
                        confirmed_not_applied,
                    )
                }
                None => {
                    state
                        .effects
                        .insert(command.meta.effect_id.clone(), candidate);
                    (
                        binding.service_instance_id.clone(),
                        binding.source.clone(),
                        true,
                    )
                }
            }
        };

        let service = self.service(&service_instance_id).await?;
        if !should_dispatch {
            return self
                .inspect_effect(&command.meta.effect_id, lease.epoch)
                .await
                .map(|inspection| {
                    inspection_receipt(inspection, command.meta.command_id.clone(), source)
                });
        }

        match service.execute(command.clone()).await {
            Ok(receipt) => {
                self.record_receipt(&command.meta.effect_id, lease.epoch, receipt.clone())
                    .await?;
                Ok(receipt)
            }
            Err(error) => {
                self.mark_unknown(&command.meta.effect_id, lease.epoch)
                    .await?;
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
            let mut state = self.state.lock().await;
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
        }
        self.inspect_effect(effect_id, lease.epoch).await
    }

    async fn inspect_effect(
        &self,
        effect_id: &AgentEffectIdentity,
        delivery_epoch: u64,
    ) -> Result<AgentEffectInspection, CompleteAgentHostError> {
        let service_instance_id = {
            let state = self.state.lock().await;
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?
                .service_instance_id
                .clone()
        };
        let service = self.service(&service_instance_id).await?;
        let inspection = service.inspect(effect_id.clone()).await?;

        let mut state = self.state.lock().await;
        let record = state.effects.get_mut(effect_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownEffect {
                effect_id: effect_id.clone(),
            }
        })?;
        ensure_effect_epoch(record, delivery_epoch)?;
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
        Ok(inspection)
    }

    pub async fn apply_bound_surface(
        &self,
        lease: &CompleteAgentBindingLease,
        binding_id: &CompleteAgentBindingId,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, CompleteAgentHostError> {
        let digest = payload_digest(&command)?;
        let (service_instance_id, should_dispatch) = {
            let mut state = self.state.lock().await;
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
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
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
                        effect.state = CompleteAgentEffectState::Dispatching;
                        effect.delivery_epoch = lease.epoch;
                    } else {
                        state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    (binding.service_instance_id, confirmed_not_applied)
                }
                None => {
                    state.effects.insert(command.effect_id.clone(), candidate);
                    (binding.service_instance_id, true)
                }
            }
        };
        let service = self.service(&service_instance_id).await?;
        if !should_dispatch {
            return self
                .recover_applied_surface(binding_id, &command.effect_id, lease.epoch)
                .await;
        }

        match service.apply_surface(command.clone()).await {
            Ok(receipt) => {
                self.record_surface_receipt(
                    binding_id,
                    &command.effect_id,
                    lease.epoch,
                    receipt.clone(),
                )
                .await?;
                Ok(receipt)
            }
            Err(error) => {
                self.mark_unknown(&command.effect_id, lease.epoch).await?;
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
        let (service_instance_id, source, should_dispatch) = {
            let mut state = self.state.lock().await;
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
                state: CompleteAgentEffectState::Dispatching,
                receipt: None,
                surface_receipt: None,
                inspection: None,
            };
            match state.effects.get(&command.effect_id) {
                Some(existing) => {
                    ensure_same_effect(existing, &candidate)?;
                    if matches!(
                        existing.state,
                        CompleteAgentEffectState::Applied | CompleteAgentEffectState::Rejected
                    ) && let Some(receipt) = &existing.receipt
                    {
                        return Ok(receipt.clone());
                    }
                    let confirmed_not_applied =
                        existing.state == CompleteAgentEffectState::NotApplied;
                    if confirmed_not_applied {
                        let effect = state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state");
                        effect.state = CompleteAgentEffectState::Dispatching;
                        effect.delivery_epoch = lease.epoch;
                    } else {
                        state
                            .effects
                            .get_mut(&command.effect_id)
                            .expect("effect was read from the same state")
                            .delivery_epoch = lease.epoch;
                    }
                    (
                        binding.service_instance_id,
                        binding.source,
                        confirmed_not_applied,
                    )
                }
                None => {
                    state.effects.insert(command.effect_id.clone(), candidate);
                    (binding.service_instance_id, binding.source, true)
                }
            }
        };
        let service = self.service(&service_instance_id).await?;
        let receipt = if should_dispatch {
            match service.revoke_surface(command.clone()).await {
                Ok(receipt) => {
                    self.record_receipt(&command.effect_id, lease.epoch, receipt.clone())
                        .await?;
                    receipt
                }
                Err(error) => {
                    self.mark_unknown(&command.effect_id, lease.epoch).await?;
                    return Err(error.into());
                }
            }
        } else {
            let inspection = self.inspect_effect(&command.effect_id, lease.epoch).await?;
            inspection_receipt(inspection, command.command_id.clone(), source)
        };
        if matches!(
            receipt.state,
            AgentReceiptState::AlreadyApplied { .. } | AgentReceiptState::Terminal { .. }
        ) {
            let mut state = self.state.lock().await;
            let binding = state.bindings.get_mut(binding_id).ok_or_else(|| {
                CompleteAgentHostError::UnknownBinding {
                    binding_id: binding_id.as_str().to_owned(),
                }
            })?;
            binding.applied_surface = None;
            binding.state = CompleteAgentBindingState::PendingSurface;
        }
        Ok(receipt)
    }

    pub async fn effect(
        &self,
        effect_id: &AgentEffectIdentity,
    ) -> Option<CompleteAgentEffectRecord> {
        self.state.lock().await.effects.get(effect_id).cloned()
    }

    pub async fn binding(
        &self,
        binding_id: &CompleteAgentBindingId,
    ) -> Option<CompleteAgentBinding> {
        self.state.lock().await.bindings.get(binding_id).cloned()
    }

    async fn service(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentHostError> {
        self.services
            .read()
            .await
            .get(instance_id)
            .map(|registered| registered.service.clone())
            .ok_or_else(|| CompleteAgentHostError::UnknownService {
                instance_id: instance_id.clone(),
            })
    }

    async fn record_receipt(
        &self,
        effect_id: &AgentEffectIdentity,
        delivery_epoch: u64,
        receipt: AgentCommandReceipt,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.lock().await;
        let record = state.effects.get_mut(effect_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownEffect {
                effect_id: effect_id.clone(),
            }
        })?;
        ensure_effect_epoch(record, delivery_epoch)?;
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
        Ok(())
    }

    async fn record_surface_receipt(
        &self,
        binding_id: &CompleteAgentBindingId,
        effect_id: &AgentEffectIdentity,
        delivery_epoch: u64,
        receipt: AppliedAgentSurfaceReceipt,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.lock().await;
        let record =
            state
                .effects
                .get(effect_id)
                .ok_or_else(|| CompleteAgentHostError::UnknownEffect {
                    effect_id: effect_id.clone(),
                })?;
        ensure_effect_epoch(record, delivery_epoch)?;
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
        Ok(())
    }

    async fn recover_applied_surface(
        &self,
        binding_id: &CompleteAgentBindingId,
        effect_id: &AgentEffectIdentity,
        delivery_epoch: u64,
    ) -> Result<AppliedAgentSurfaceReceipt, CompleteAgentHostError> {
        let inspection = self.inspect_effect(effect_id, delivery_epoch).await?;
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
            let state = self.state.lock().await;
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
        self.record_surface_receipt(binding_id, effect_id, delivery_epoch, receipt.clone())
            .await?;
        Ok(receipt)
    }

    async fn mark_unknown(
        &self,
        effect_id: &AgentEffectIdentity,
        delivery_epoch: u64,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.lock().await;
        let record = state.effects.get_mut(effect_id).ok_or_else(|| {
            CompleteAgentHostError::UnknownEffect {
                effect_id: effect_id.clone(),
            }
        })?;
        ensure_effect_epoch(record, delivery_epoch)?;
        observe_effect_state(record, CompleteAgentEffectState::Unknown)?;
        Ok(())
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
    state: &CompleteAgentHostState,
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

fn ensure_effect_epoch(
    record: &CompleteAgentEffectRecord,
    delivery_epoch: u64,
) -> Result<(), CompleteAgentHostError> {
    if record.delivery_epoch != delivery_epoch {
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
        collections::{BTreeMap, BTreeSet},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
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

    use super::*;

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
        });
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let host = CompleteAgentHost::new();
        host.register_service(instance_id.clone(), service)
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
    async fn unknown_dispatch_is_inspected_without_second_execution() {
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let source = AgentSourceCoordinate::new("source").expect("source");
        let command_id = AgentCommandId::new("command").expect("command");
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: source.clone(),
            command_id: command_id.clone(),
            execute_calls: AtomicUsize::new(0),
        });
        let host = CompleteAgentHost::new();
        host.register_service(instance_id.clone(), service.clone())
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
        let recovered = host
            .dispatch_execute(&lease, &binding_id, command)
            .await
            .expect("inspect same effect");

        assert!(matches!(
            recovered.state,
            AgentReceiptState::AlreadyApplied { .. }
        ));
        assert_eq!(service.execute_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            host.effect(&effect_id).await.expect("effect").state,
            CompleteAgentEffectState::Applied
        );
    }

    #[tokio::test]
    async fn lease_reclaim_fences_old_owner_and_binding_loss_converges_state() {
        let host = CompleteAgentHost::new();
        let instance_id = AgentServiceInstanceId::new("service").expect("service");
        let service = Arc::new(UnknownThenAppliedService {
            descriptor: descriptor(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            command_id: AgentCommandId::new("command").expect("command"),
            execute_calls: AtomicUsize::new(0),
        });
        host.register_service(instance_id.clone(), service)
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
            host.binding(&binding_id).await.expect("binding").state,
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
        let host = CompleteAgentHost::new();
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");
        let mut record = effect_record("sha256:payload");
        record.delivery_epoch = 2;
        host.state
            .lock()
            .await
            .effects
            .insert(effect_id.clone(), record.clone());
        let receipt = AgentCommandReceipt {
            command_id: record.command_id.clone(),
            effect_id: effect_id.clone(),
            source: record.source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        };

        assert_eq!(
            host.record_receipt(&effect_id, 1, receipt).await,
            Err(CompleteAgentHostError::StaleLeaseOutcome)
        );
        assert_eq!(
            host.effect(&effect_id).await.expect("effect").state,
            CompleteAgentEffectState::Dispatching
        );
    }

    #[tokio::test]
    async fn terminal_effect_observations_are_idempotent_and_never_downgrade() {
        let host = CompleteAgentHost::new();
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
        host.state
            .lock()
            .await
            .effects
            .insert(effect_id.clone(), record);

        host.record_receipt(&effect_id, 1, applied.clone())
            .await
            .expect("first terminal observation");
        host.record_receipt(&effect_id, 1, applied)
            .await
            .expect("duplicate terminal observation");
        assert_eq!(
            host.mark_unknown(&effect_id, 1).await,
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
            host.record_receipt(&effect_id, 1, conflicting_applied)
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
            host.record_receipt(&effect_id, 1, rejected).await,
            Err(CompleteAgentHostError::EffectObservationConflict {
                current: CompleteAgentEffectState::Applied,
                observed: CompleteAgentEffectState::Rejected,
            })
        );
        assert_eq!(
            host.effect(&effect_id).await.expect("effect").state,
            CompleteAgentEffectState::Applied
        );
    }

    #[tokio::test]
    async fn mismatched_surface_receipt_never_makes_binding_available() {
        let host = CompleteAgentHost::new();
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
        let mut state = host.state.lock().await;
        state.bindings.insert(binding_id.clone(), binding);
        state.effects.insert(effect_id.clone(), record.clone());
        drop(state);
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
            host.record_surface_receipt(&binding_id, &effect_id, 1, receipt)
                .await,
            Err(CompleteAgentHostError::DispatchRejected { .. })
        ));
        assert_eq!(
            host.binding(&binding_id).await.expect("binding").state,
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
            state: CompleteAgentEffectState::Dispatching,
            receipt: None,
            surface_receipt: None,
            inspection: None,
        }
    }

    struct UnknownThenAppliedService {
        descriptor: AgentServiceDescriptor,
        source: AgentSourceCoordinate,
        command_id: AgentCommandId,
        execute_calls: AtomicUsize,
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
            Ok(AgentEffectInspection {
                effect_id: identity,
                command_id: Some(self.command_id.clone()),
                state: AgentEffectInspectionState::Applied {
                    source: self.source.clone(),
                    terminal: None,
                    initial_context: None,
                    child_source: None,
                },
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
            Err(unsupported())
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
