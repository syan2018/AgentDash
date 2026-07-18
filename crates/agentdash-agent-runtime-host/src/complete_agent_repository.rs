use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentCallbackRouteId, AgentEffectIdentity, AgentEffectInspectionState, AgentReceiptState,
    AgentRuntimeOffer, AgentServiceDescriptor, AgentServiceInstanceId, AgentSourceCoordinate,
    AgentSurfaceRoute, CompleteAgentService,
};
use async_trait::async_trait;
use thiserror::Error;

use crate::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
    CompleteAgentBindingState, CompleteAgentCallbackRoute, CompleteAgentEffectAttemptEvidence,
    CompleteAgentEffectRecord, CompleteAgentEffectState, CompleteAgentLifecycleEffectRecord,
    CompleteAgentPlacement, CompleteAgentRuntimeTarget,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompleteAgentHostRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentHostFacts {
    pub service_instances: BTreeMap<AgentServiceInstanceId, AgentServiceDescriptor>,
    pub offers: BTreeMap<AgentServiceInstanceId, AgentRuntimeOffer>,
    pub placements: BTreeMap<AgentServiceInstanceId, CompleteAgentPlacement>,
    pub bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    pub source_coordinates: BTreeMap<CompleteAgentBindingId, AgentSourceCoordinate>,
    pub callback_routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    pub revoked_callback_routes: BTreeSet<AgentCallbackRouteId>,
    pub effects: BTreeMap<AgentEffectIdentity, CompleteAgentEffectRecord>,
    pub leases: BTreeMap<CompleteAgentBindingId, CompleteAgentBindingLease>,
    pub lease_epochs: BTreeMap<CompleteAgentBindingId, u64>,
    pub runtime_targets: BTreeMap<RuntimeThreadId, CompleteAgentRuntimeTarget>,
    pub lifecycle_effects: BTreeMap<AgentEffectIdentity, CompleteAgentLifecycleEffectRecord>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentHostSnapshot {
    pub revision: CompleteAgentHostRevision,
    pub facts: CompleteAgentHostFacts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentHostCommit {
    pub expected_revision: CompleteAgentHostRevision,
    pub facts: CompleteAgentHostFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentHostStoreError {
    #[error("Complete Agent Host revision conflict: expected {expected:?}, actual {actual:?}")]
    Conflict {
        expected: CompleteAgentHostRevision,
        actual: CompleteAgentHostRevision,
    },
    #[error("Complete Agent Host persistence invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("Complete Agent Host persistence failed: {reason}")]
    Persistence { reason: String },
}

/// Durable authority for Complete Agent service, offer, binding, source, effect, lease, and
/// generation facts.
///
/// A commit is one Host transaction. Implementations must compare `expected_revision`, validate
/// the complete fact graph, and atomically persist every changed fact before advancing revision.
/// Replaying the exact already-committed fact graph is idempotent even when the expected revision
/// is stale; a different graph returns `Conflict`.
#[async_trait]
pub trait CompleteAgentHostRepository: Send + Sync {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;
}

/// Process-local resolver for live Complete Agent service handles.
///
/// Service handles are deliberately outside durable Host facts. Production composition attaches
/// handles from the final Complete Agent registrations; a reconstructed Host resolves the same
/// durable service instance through this port.
#[async_trait]
pub trait CompleteAgentServiceRegistry: Send + Sync {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    );

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>>;
}

pub type SharedCompleteAgentHostRepository = Arc<dyn CompleteAgentHostRepository>;
pub type SharedCompleteAgentServiceRegistry = Arc<dyn CompleteAgentServiceRegistry>;

pub fn apply_complete_agent_host_commit(
    current: &mut CompleteAgentHostSnapshot,
    commit: CompleteAgentHostCommit,
) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
    if current.revision != commit.expected_revision {
        if current.facts == commit.facts {
            return Ok(current.clone());
        }
        return Err(CompleteAgentHostStoreError::Conflict {
            expected: commit.expected_revision,
            actual: current.revision,
        });
    }
    validate_complete_agent_host_facts(&current.facts, &commit.facts)?;
    current.revision =
        CompleteAgentHostRevision(current.revision.0.checked_add(1).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "Host repository revision is exhausted".to_owned(),
            }
        })?);
    current.facts = commit.facts;
    Ok(current.clone())
}

pub fn validate_complete_agent_host_facts(
    current: &CompleteAgentHostFacts,
    candidate: &CompleteAgentHostFacts,
) -> Result<(), CompleteAgentHostStoreError> {
    if candidate.service_instances.len() != candidate.offers.len()
        || candidate.service_instances.len() != candidate.placements.len()
    {
        return invariant(
            "every service instance must have exactly one Runtime offer and placement",
        );
    }
    for (instance_id, descriptor) in &candidate.service_instances {
        let offer = candidate.offers.get(instance_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "service instance has no Runtime offer".to_owned(),
            }
        })?;
        let placement = candidate.placements.get(instance_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "service instance has no placement".to_owned(),
            }
        })?;
        if !placement.is_valid() {
            return invariant("service placement coordinates must not be empty");
        }
        if offer.profile_digest != descriptor.profile_digest
            || offer.contributions != descriptor.profile.surface.facets
        {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "Runtime offer does not exactly match its service descriptor".to_owned(),
            });
        }
    }
    for (instance_id, descriptor) in &current.service_instances {
        if candidate.service_instances.get(instance_id) != Some(descriptor) {
            return invariant("service descriptor history is immutable");
        }
        if candidate.offers.get(instance_id) != current.offers.get(instance_id) {
            return invariant("published Runtime offer history is immutable");
        }
        if candidate.placements.get(instance_id) != current.placements.get(instance_id) {
            return invariant("service placement history is immutable");
        }
    }
    if candidate
        .placements
        .keys()
        .any(|instance_id| !candidate.service_instances.contains_key(instance_id))
    {
        return invariant("service placement has no owning service instance");
    }

    for (binding_id, binding) in &candidate.bindings {
        if binding_id != &binding.id {
            return invariant("binding map key does not match binding identity");
        }
        if binding.generation.0 == 0 {
            return invariant("binding generation must be positive");
        }
        let descriptor = candidate
            .service_instances
            .get(&binding.service_instance_id)
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "binding has no owning service instance".to_owned(),
            })?;
        let offer = candidate
            .offers
            .get(&binding.service_instance_id)
            .expect("service-to-offer completeness was validated");
        if binding.profile_digest != descriptor.profile_digest
            || binding.profile_digest != offer.profile_digest
            || binding.bound_surface.offer_profile_digest != offer.profile_digest
        {
            return invariant("binding profile or bound surface does not match its Runtime offer");
        }
        let applied_matches = binding
            .applied_surface
            .as_ref()
            .is_some_and(|applied| binding.bound_surface.accepts_applied(applied));
        if (binding.state == CompleteAgentBindingState::Available && !applied_matches)
            || (binding.state == CompleteAgentBindingState::PendingSurface
                && binding.applied_surface.is_some())
        {
            return invariant("binding state does not match applied surface evidence");
        }
        if candidate.source_coordinates.get(binding_id) != Some(&binding.source) {
            return invariant("binding source coordinate is missing or inconsistent");
        }
    }
    for (binding_id, binding) in &current.bindings {
        let next = candidate.bindings.get(binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "binding history cannot be removed".to_owned(),
            }
        })?;
        if binding.id != next.id
            || binding.service_instance_id != next.service_instance_id
            || binding.generation != next.generation
            || binding.source != next.source
            || binding.profile_digest != next.profile_digest
            || binding.bound_surface != next.bound_surface
        {
            return invariant("binding immutable coordinates or surface were rewritten");
        }
        if !binding_state_can_advance(binding.state, next.state) {
            return invariant("binding lifecycle moved backwards");
        }
    }
    for (binding_id, source) in &candidate.source_coordinates {
        if !candidate.bindings.contains_key(binding_id) {
            return invariant("source coordinate has no owning binding");
        }
        if candidate
            .source_coordinates
            .iter()
            .any(|(other_id, other_source)| other_id != binding_id && other_source == source)
        {
            return invariant("source coordinate is assigned to multiple bindings");
        }
    }
    for (binding_id, source) in &current.source_coordinates {
        if candidate.source_coordinates.get(binding_id) != Some(source) {
            return invariant("source coordinate history is immutable");
        }
    }
    for (route_id, route) in &candidate.callback_routes {
        if route_id != &route.route_id
            || route.generation.0 == 0
            || route.delivery != AgentSurfaceRoute::AgentNativeCallback
            || route.default_deadline_ms == 0
        {
            return invariant("callback route coordinates are invalid");
        }
        let binding = candidate.bindings.get(&route.binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "callback route has no owning binding".to_owned(),
            }
        })?;
        if route.generation != binding.generation
            || route.source != binding.source
            || route.bound_surface != binding.bound_surface
        {
            return invariant("callback route fence does not exactly match its binding");
        }
    }
    for (route_id, route) in &current.callback_routes {
        if candidate.callback_routes.get(route_id) != Some(route) {
            return invariant("callback route tombstone history is immutable");
        }
    }
    if !candidate
        .revoked_callback_routes
        .is_superset(&current.revoked_callback_routes)
        || candidate
            .revoked_callback_routes
            .iter()
            .any(|route_id| !candidate.callback_routes.contains_key(route_id))
    {
        return invariant("callback route revocation history is invalid");
    }
    for (binding_id, binding) in &candidate.bindings {
        let active_routes = candidate
            .callback_routes
            .values()
            .filter(|route| {
                &route.binding_id == binding_id
                    && !candidate.revoked_callback_routes.contains(&route.route_id)
            })
            .count();
        let requires_active_route = binding.state == CompleteAgentBindingState::Available
            && binding.applied_surface.is_some()
            && binding_requires_callback_route(binding);
        if (requires_active_route && active_routes != 1)
            || (!requires_active_route && active_routes != 0)
        {
            return invariant(
                "binding applied surface and active callback route are not atomically aligned",
            );
        }
    }
    for (route_id, route) in &candidate.callback_routes {
        if current.callback_routes.contains_key(route_id) {
            continue;
        }
        let previous = current.bindings.get(&route.binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "new callback route has no preceding pending binding".to_owned(),
            }
        })?;
        let next = candidate
            .bindings
            .get(&route.binding_id)
            .expect("route owning binding was validated");
        if previous.state != CompleteAgentBindingState::PendingSurface
            || next.state != CompleteAgentBindingState::Available
            || candidate.revoked_callback_routes.contains(route_id)
        {
            return invariant("callback route must be created with its applied binding transition");
        }
    }
    for route_id in candidate
        .revoked_callback_routes
        .difference(&current.revoked_callback_routes)
    {
        let route = current.callback_routes.get(route_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "callback route revocation has no active route tombstone".to_owned(),
            }
        })?;
        let previous = current
            .bindings
            .get(&route.binding_id)
            .expect("route owning binding was validated");
        let next = candidate
            .bindings
            .get(&route.binding_id)
            .expect("route owning binding was validated");
        if previous.state != CompleteAgentBindingState::Available
            || (next.state == CompleteAgentBindingState::Available
                && next.applied_surface.is_some())
        {
            return invariant("callback route must be revoked with its binding surface transition");
        }
    }

    let mut command_effects = BTreeMap::new();
    for (effect_id, effect) in &candidate.effects {
        if effect_id != &effect.effect_id {
            return invariant("effect map key does not match effect identity");
        }
        if command_effects
            .insert(effect.command_id.clone(), effect_id)
            .is_some()
        {
            return invariant("command identity is assigned to multiple effects");
        }
        let binding = candidate.bindings.get(&effect.binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "effect has no owning binding".to_owned(),
            }
        })?;
        if effect.generation != binding.generation
            || effect.service_instance_id != binding.service_instance_id
            || effect.source != binding.source
        {
            return invariant("effect coordinates do not match its binding generation");
        }
        if effect.delivery_epoch == 0
            || candidate
                .lease_epochs
                .get(&effect.binding_id)
                .is_none_or(|epoch| effect.delivery_epoch > *epoch)
        {
            return invariant("effect delivery epoch is outside its binding lease history");
        }
        if effect.dispatch_attempt == 0 {
            return invariant("effect dispatch attempt must be positive");
        }
        if !current.effects.contains_key(effect_id)
            && (effect.dispatch_attempt != 1 || !effect.attempt_history.is_empty())
        {
            return invariant("a new effect must begin at dispatch attempt one without history");
        }
        validate_effect_evidence(effect, binding)?;
        validate_effect_attempt_history(
            effect,
            binding,
            candidate
                .lease_epochs
                .get(&effect.binding_id)
                .copied()
                .expect("effect lease history was validated"),
        )?;
    }
    for (effect_id, effect) in &current.effects {
        let next = candidate.effects.get(effect_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "effect history cannot be removed".to_owned(),
            }
        })?;
        if effect.command_id != next.command_id
            || effect.binding_id != next.binding_id
            || effect.service_instance_id != next.service_instance_id
            || effect.generation != next.generation
            || effect.source != next.source
            || effect.payload_digest != next.payload_digest
        {
            return invariant("effect command, payload, or coordinates were rewritten");
        }
        if next.delivery_epoch < effect.delivery_epoch {
            return invariant("effect delivery epoch moved backwards");
        }
        if next.dispatch_attempt < effect.dispatch_attempt
            || next.dispatch_attempt > effect.dispatch_attempt.saturating_add(1)
        {
            return invariant("effect dispatch attempt moved backwards or skipped an attempt");
        }
        if !effect_state_can_advance(effect.state, next.state) {
            return invariant("effect observation moved backwards");
        }
        validate_attempt_transition(effect, next)?;
    }

    for (binding_id, epoch) in &current.lease_epochs {
        if candidate
            .lease_epochs
            .get(binding_id)
            .is_none_or(|next| next < epoch)
        {
            return invariant("lease epoch history moved backwards or was removed");
        }
    }
    for (binding_id, lease) in &candidate.leases {
        let binding = candidate.bindings.get(binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "lease has no owning binding".to_owned(),
            }
        })?;
        if lease.binding_id != *binding_id
            || lease.generation != binding.generation
            || lease.owner.trim().is_empty()
            || lease.token.trim().is_empty()
            || lease.epoch == 0
            || lease.expires_at_ms == 0
        {
            return invariant("lease identity, owner, token, epoch, or expiry is invalid");
        }
        if candidate.lease_epochs.get(binding_id).copied() != Some(lease.epoch) {
            return invariant("lease epoch does not match the generation fence");
        }
        let previous_epoch = current.lease_epochs.get(binding_id).copied().unwrap_or(0);
        if lease.epoch == previous_epoch {
            let previous = current.leases.get(binding_id).ok_or_else(|| {
                CompleteAgentHostStoreError::Invariant {
                    reason: "a released lease epoch cannot be reactivated".to_owned(),
                }
            })?;
            if lease.owner != previous.owner
                || lease.token != previous.token
                || lease.generation != previous.generation
                || lease.expires_at_ms < previous.expires_at_ms
            {
                return invariant("active lease identity changed or expiry moved backwards");
            }
        } else if lease.epoch != previous_epoch.saturating_add(1)
            || current
                .leases
                .get(binding_id)
                .is_some_and(|previous| previous.token == lease.token)
        {
            return invariant("lease takeover must advance one epoch with a fresh token");
        }
        if matches!(
            binding.state,
            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
        ) {
            return invariant("lost or closed binding cannot retain an active lease");
        }
    }
    if candidate
        .lease_epochs
        .keys()
        .any(|binding_id| !candidate.bindings.contains_key(binding_id))
    {
        return invariant("lease epoch has no owning binding");
    }
    for (thread_id, target) in &candidate.runtime_targets {
        if thread_id != &target.runtime_thread_id
            || target.generation.0 == 0
            || target.callbacks.binding_generation != target.generation
        {
            return invariant("Runtime target identity or generation is invalid");
        }
        let descriptor = candidate
            .service_instances
            .get(&target.service_instance_id)
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "Runtime target has no registered service instance".to_owned(),
            })?;
        if target.profile_digest != descriptor.profile_digest
            || target.bound_surface.offer_profile_digest != target.profile_digest
        {
            return invariant("Runtime target does not match its service profile");
        }
    }
    for (thread_id, target) in &current.runtime_targets {
        if candidate.runtime_targets.get(thread_id) != Some(target) {
            return invariant("Runtime target history cannot be removed or rewritten");
        }
    }
    for (effect_id, effect) in &candidate.lifecycle_effects {
        if effect_id != &effect.effect_id {
            return invariant("lifecycle effect map key does not match effect identity");
        }
        let target = candidate
            .runtime_targets
            .get(&effect.runtime_thread_id)
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "lifecycle effect has no Runtime target".to_owned(),
            })?;
        if effect.service_instance_id != target.service_instance_id
            || effect.generation != target.generation
        {
            return invariant("lifecycle effect does not match its Runtime target");
        }
    }
    for (effect_id, effect) in &current.lifecycle_effects {
        let next = candidate.lifecycle_effects.get(effect_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "lifecycle effect history cannot be removed".to_owned(),
            }
        })?;
        if effect.effect_id != next.effect_id
            || effect.runtime_thread_id != next.runtime_thread_id
            || effect.child_thread_id != next.child_thread_id
            || effect.kind != next.kind
            || effect.service_instance_id != next.service_instance_id
            || effect.generation != next.generation
            || effect.initial_context != next.initial_context
            || effect.fork_cutoff != next.fork_cutoff
            || (effect.outcome.is_some() && effect.outcome != next.outcome)
        {
            return invariant("lifecycle effect coordinates or outcome were rewritten");
        }
    }
    Ok(())
}

fn binding_requires_callback_route(binding: &CompleteAgentBinding) -> bool {
    binding
        .bound_surface
        .contributions
        .iter()
        .any(|contribution| contribution.route == AgentSurfaceRoute::AgentNativeCallback)
}

fn binding_state_can_advance(
    current: CompleteAgentBindingState,
    next: CompleteAgentBindingState,
) -> bool {
    current == next
        || match current {
            CompleteAgentBindingState::PendingSurface => matches!(
                next,
                CompleteAgentBindingState::Available
                    | CompleteAgentBindingState::Desynchronized
                    | CompleteAgentBindingState::Lost
                    | CompleteAgentBindingState::Closed
            ),
            CompleteAgentBindingState::Available => matches!(
                next,
                CompleteAgentBindingState::PendingSurface
                    | CompleteAgentBindingState::Desynchronized
                    | CompleteAgentBindingState::Lost
                    | CompleteAgentBindingState::Closed
            ),
            CompleteAgentBindingState::Desynchronized => matches!(
                next,
                CompleteAgentBindingState::Available
                    | CompleteAgentBindingState::Lost
                    | CompleteAgentBindingState::Closed
            ),
            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed => false,
        }
}

fn effect_state_can_advance(
    current: CompleteAgentEffectState,
    next: CompleteAgentEffectState,
) -> bool {
    current == next
        || match current {
            CompleteAgentEffectState::Dispatching => true,
            CompleteAgentEffectState::Unknown => matches!(
                next,
                CompleteAgentEffectState::Accepted
                    | CompleteAgentEffectState::Applied
                    | CompleteAgentEffectState::Rejected
                    | CompleteAgentEffectState::NotApplied
                    | CompleteAgentEffectState::Lost
            ),
            CompleteAgentEffectState::Accepted => matches!(
                next,
                CompleteAgentEffectState::Applied
                    | CompleteAgentEffectState::Rejected
                    | CompleteAgentEffectState::Lost
            ),
            CompleteAgentEffectState::NotApplied => next == CompleteAgentEffectState::Dispatching,
            CompleteAgentEffectState::Applied
            | CompleteAgentEffectState::Rejected
            | CompleteAgentEffectState::Lost => false,
        }
}

fn validate_effect_evidence(
    effect: &CompleteAgentEffectRecord,
    binding: &CompleteAgentBinding,
) -> Result<(), CompleteAgentHostStoreError> {
    if let Some(receipt) = &effect.receipt
        && (receipt.effect_id != effect.effect_id
            || receipt.command_id != effect.command_id
            || receipt.source != effect.source
            || !effect_state_can_advance(receipt_state(&receipt.state), effect.state))
    {
        return invariant("effect receipt coordinates or observation state are inconsistent");
    }
    if let Some(receipt) = &effect.surface_receipt
        && (receipt.effect_id != effect.effect_id
            || receipt.command_id != effect.command_id
            || receipt.source != effect.source
            || !binding.bound_surface.accepts_applied(&receipt.applied)
            || !effect_state_can_advance(CompleteAgentEffectState::Applied, effect.state))
    {
        return invariant("surface receipt coordinates or applied evidence are inconsistent");
    }
    if let Some(inspection) = &effect.inspection
        && (!inspection_coordinates_match(effect, inspection)
            || !effect_state_can_advance(inspection_state(&inspection.state), effect.state))
    {
        return invariant("effect inspection coordinates or observation state are inconsistent");
    }
    if effect.state == CompleteAgentEffectState::NotApplied
        && effect.inspection.as_ref().is_none_or(|inspection| {
            inspection_state(&inspection.state) != CompleteAgentEffectState::NotApplied
        })
    {
        return invariant("NotApplied effect state requires current-attempt inspection evidence");
    }
    Ok(())
}

fn validate_effect_attempt_history(
    effect: &CompleteAgentEffectRecord,
    binding: &CompleteAgentBinding,
    max_delivery_epoch: u64,
) -> Result<(), CompleteAgentHostStoreError> {
    let expected_history_len = usize::try_from(effect.dispatch_attempt - 1).map_err(|_| {
        CompleteAgentHostStoreError::Invariant {
            reason: "effect dispatch attempt cannot be represented as history length".to_owned(),
        }
    })?;
    if effect.attempt_history.len() != expected_history_len {
        return invariant("effect attempt history must contain every prior attempt exactly once");
    }

    for (index, archived) in effect.attempt_history.iter().enumerate() {
        let expected_attempt = u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "effect attempt history index is exhausted".to_owned(),
            })?;
        if archived.dispatch_attempt != expected_attempt
            || archived.delivery_epoch == 0
            || archived.delivery_epoch > max_delivery_epoch
            || archived.state != CompleteAgentEffectState::NotApplied
        {
            return invariant(
                "archived attempt number, delivery epoch, or terminal state is invalid",
            );
        }
        validate_attempt_evidence(effect, binding, archived)?;
    }
    Ok(())
}

fn validate_attempt_evidence(
    effect: &CompleteAgentEffectRecord,
    binding: &CompleteAgentBinding,
    archived: &CompleteAgentEffectAttemptEvidence,
) -> Result<(), CompleteAgentHostStoreError> {
    if let Some(receipt) = &archived.receipt
        && (receipt.effect_id != effect.effect_id
            || receipt.command_id != effect.command_id
            || receipt.source != effect.source
            || !effect_state_can_advance(receipt_state(&receipt.state), archived.state))
    {
        return invariant("archived effect receipt coordinates or observation state are invalid");
    }
    if let Some(receipt) = &archived.surface_receipt
        && (receipt.effect_id != effect.effect_id
            || receipt.command_id != effect.command_id
            || receipt.source != effect.source
            || !binding.bound_surface.accepts_applied(&receipt.applied)
            || !effect_state_can_advance(CompleteAgentEffectState::Applied, archived.state))
    {
        return invariant("archived surface receipt coordinates or applied evidence are invalid");
    }
    let inspection =
        archived
            .inspection
            .as_ref()
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "archived redispatch attempt requires NotApplied inspection evidence"
                    .to_owned(),
            })?;
    if inspection_state(&inspection.state) != CompleteAgentEffectState::NotApplied
        || !inspection_coordinates_match(effect, inspection)
        || !effect_state_can_advance(inspection_state(&inspection.state), archived.state)
    {
        return invariant("archived inspection coordinates or observation state are invalid");
    }
    Ok(())
}

fn inspection_coordinates_match(
    effect: &CompleteAgentEffectRecord,
    inspection: &agentdash_agent_service_api::AgentEffectInspection,
) -> bool {
    inspection.effect_id == effect.effect_id
        && inspection
            .command_id
            .as_ref()
            .is_none_or(|command_id| command_id == &effect.command_id)
        && inspection_source(&inspection.state).is_none_or(|source| source == &effect.source)
}

fn immutable_optional_evidence<T: PartialEq>(
    current: &Option<T>,
    candidate: &Option<T>,
    label: &str,
) -> Result<(), CompleteAgentHostStoreError> {
    if current
        .as_ref()
        .is_some_and(|current| candidate.as_ref() != Some(current))
    {
        return invariant(&format!("{label} was removed or rewritten"));
    }
    Ok(())
}

fn validate_attempt_transition(
    current: &CompleteAgentEffectRecord,
    candidate: &CompleteAgentEffectRecord,
) -> Result<(), CompleteAgentHostStoreError> {
    if candidate.dispatch_attempt == current.dispatch_attempt {
        if current.state == CompleteAgentEffectState::NotApplied
            && candidate.state == CompleteAgentEffectState::Dispatching
        {
            return invariant(
                "NotApplied redispatch must advance attempt and archive inspection evidence",
            );
        }
        if candidate.attempt_history != current.attempt_history {
            return invariant("attempt history changed without a new dispatch attempt");
        }
        immutable_optional_evidence(&current.receipt, &candidate.receipt, "effect receipt")?;
        immutable_optional_evidence(
            &current.surface_receipt,
            &candidate.surface_receipt,
            "surface receipt",
        )?;
        return validate_latest_inspection_evidence(current, candidate);
    }

    let current_inspection =
        current
            .inspection
            .as_ref()
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "effect redispatch requires current NotApplied inspection evidence"
                    .to_owned(),
            })?;
    let expected_archive = CompleteAgentEffectAttemptEvidence {
        dispatch_attempt: current.dispatch_attempt,
        delivery_epoch: current.delivery_epoch,
        state: current.state,
        receipt: current.receipt.clone(),
        surface_receipt: current.surface_receipt.clone(),
        inspection: Some(current_inspection.clone()),
    };
    if current.state != CompleteAgentEffectState::NotApplied
        || candidate.state != CompleteAgentEffectState::Dispatching
        || inspection_state(&current_inspection.state) != CompleteAgentEffectState::NotApplied
        || candidate.receipt.is_some()
        || candidate.surface_receipt.is_some()
        || candidate.inspection.is_some()
        || candidate.attempt_history.len() != current.attempt_history.len() + 1
        || !candidate
            .attempt_history
            .starts_with(&current.attempt_history)
        || candidate.attempt_history.last() != Some(&expected_archive)
    {
        return invariant(
            "new dispatch attempt must atomically archive all prior evidence and clear current evidence",
        );
    }
    Ok(())
}

fn validate_latest_inspection_evidence(
    current: &CompleteAgentEffectRecord,
    candidate: &CompleteAgentEffectRecord,
) -> Result<(), CompleteAgentHostStoreError> {
    let Some(current_inspection) = &current.inspection else {
        return Ok(());
    };
    let Some(candidate_inspection) = &candidate.inspection else {
        return invariant("effect inspection was removed");
    };
    if current_inspection == candidate_inspection {
        return Ok(());
    }
    if current_inspection.effect_id != candidate_inspection.effect_id
        || current_inspection
            .command_id
            .as_ref()
            .is_some_and(|command_id| candidate_inspection.command_id.as_ref() != Some(command_id))
    {
        return invariant("effect inspection identity evidence was removed or rewritten");
    }

    let current_state = inspection_state(&current_inspection.state);
    let candidate_state = inspection_state(&candidate_inspection.state);
    let advances = match current_state {
        CompleteAgentEffectState::Unknown => matches!(
            candidate_state,
            CompleteAgentEffectState::Accepted
                | CompleteAgentEffectState::NotApplied
                | CompleteAgentEffectState::Applied
        ),
        CompleteAgentEffectState::Accepted => candidate_state == CompleteAgentEffectState::Applied,
        CompleteAgentEffectState::NotApplied => false,
        CompleteAgentEffectState::Dispatching
        | CompleteAgentEffectState::Applied
        | CompleteAgentEffectState::Rejected
        | CompleteAgentEffectState::Lost => false,
    };
    if !advances {
        return invariant("effect inspection state or evidence moved backwards or was rewritten");
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

fn inspection_source(state: &AgentEffectInspectionState) -> Option<&AgentSourceCoordinate> {
    match state {
        AgentEffectInspectionState::Accepted { source }
        | AgentEffectInspectionState::Applied { source, .. } => Some(source),
        AgentEffectInspectionState::NotApplied | AgentEffectInspectionState::Unknown => None,
    }
}

fn invariant<T>(reason: &str) -> Result<T, CompleteAgentHostStoreError> {
    Err(CompleteAgentHostStoreError::Invariant {
        reason: reason.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentCallbackRouteId, AgentCapabilityProfile,
        AgentCommandCapability, AgentCommandId, AgentCommandReceipt, AgentCompactionMode,
        AgentConfigurationBoundary, AgentEffectIdentity, AgentEffectInspection,
        AgentForkCapability, AgentForkCutoffKind, AgentLifecycleCapability, AgentPayloadDigest,
        AgentProfileDigest, AgentReceiptState, AgentRuntimeOffer, AgentServiceDefinitionId,
        AgentSourceChangeLevel, AgentSurfaceContributionPayload, AgentSurfaceDigest,
        AgentSurfaceProfile, AgentSurfaceRevision, AgentSurfaceSemanticFacet, AgentTerminalOutcome,
        AgentToolDelivery, AgentToolName, AgentToolSemanticFacet, AgentToolUpdateSemantics,
        AppliedAgentSurface, AppliedAgentSurfaceContribution, AppliedContributionStatus,
        BoundAgentSurface, BoundAgentSurfaceContribution, InitialContextAppliedEvidence,
        InitialContextProfile, SemanticFidelity,
    };
    use serde_json::json;
    use tokio::sync::Mutex;

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
            let mut current = self.snapshot.lock().await;
            apply_complete_agent_host_commit(&mut current, commit)
        }
    }

    #[tokio::test]
    async fn exact_stale_commit_replay_is_idempotent() {
        let repository = FixtureHostRepository::default();
        let facts = CompleteAgentHostFacts::default();
        let first = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(0),
                facts: facts.clone(),
            })
            .await
            .expect("first commit");
        let replay = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(0),
                facts,
            })
            .await
            .expect("exact replay");

        assert_eq!(first, replay);
        assert_eq!(replay.revision, CompleteAgentHostRevision(1));
    }

    #[tokio::test]
    async fn invalid_fact_graph_is_rejected_atomically() {
        let repository = FixtureHostRepository::default();
        let mut facts = CompleteAgentHostFacts::default();
        facts.lease_epochs.insert(
            CompleteAgentBindingId::new("missing-binding").expect("binding"),
            1,
        );

        assert!(matches!(
            repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: CompleteAgentHostRevision(0),
                    facts,
                })
                .await,
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
        assert_eq!(
            repository.load().await.expect("snapshot"),
            CompleteAgentHostSnapshot::default()
        );
    }

    #[test]
    fn descriptor_offer_and_binding_contracts_reject_cross_revision_rewrites() {
        let current = valid_facts();

        let mut descriptor_rewrite = current.clone();
        descriptor_rewrite
            .service_instances
            .get_mut(&service_id())
            .expect("descriptor")
            .title = "rewritten".to_owned();
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &descriptor_rewrite,
        ));

        let mut missing_offer = current.clone();
        missing_offer.offers.clear();
        assert_invariant(validate_complete_agent_host_facts(&current, &missing_offer));

        let mut placement_rewrite = current.clone();
        placement_rewrite.placements.insert(
            service_id(),
            CompleteAgentPlacement::Remote {
                host_id: "host-2".to_owned(),
                transport_id: "transport-2".to_owned(),
                host_incarnation_id: "incarnation-2".to_owned(),
            },
        );
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &placement_rewrite,
        ));

        let mut missing_placement = current.clone();
        missing_placement.placements.clear();
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &missing_placement,
        ));

        let mut invalid_placement = current.clone();
        invalid_placement.placements.insert(
            service_id(),
            CompleteAgentPlacement::Remote {
                host_id: String::new(),
                transport_id: "transport".to_owned(),
                host_incarnation_id: "incarnation".to_owned(),
            },
        );
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &invalid_placement,
        ));

        let mut zero_generation = current.clone();
        zero_generation
            .bindings
            .get_mut(&binding_id())
            .expect("binding")
            .generation = AgentBindingGeneration(0);
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &zero_generation,
        ));

        let mut profile_mismatch = current.clone();
        profile_mismatch
            .bindings
            .get_mut(&binding_id())
            .expect("binding")
            .profile_digest = AgentProfileDigest::new("other-profile").expect("profile");
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &profile_mismatch,
        ));
    }

    #[test]
    fn callback_route_is_atomic_with_apply_and_revoke_binding_transitions() {
        let no_callback_current = valid_facts();
        let mut no_callback_applied = no_callback_current.clone();
        let binding = no_callback_applied
            .bindings
            .get_mut(&binding_id())
            .expect("binding");
        binding.applied_surface = Some(AppliedAgentSurface {
            revision: binding.bound_surface.revision,
            digest: binding.bound_surface.digest.clone(),
            contributions: Vec::new(),
        });
        binding.state = CompleteAgentBindingState::Available;
        validate_complete_agent_host_facts(&no_callback_current, &no_callback_applied)
            .expect("a surface without callbacks has no route");
        assert!(no_callback_applied.callback_routes.is_empty());

        let mut current = valid_facts();
        let binding_id = binding_id();
        let surface = callback_surface();
        let binding = current.bindings.get_mut(&binding_id).expect("binding");
        binding.bound_surface = surface.clone();
        binding.applied_surface = None;
        binding.state = CompleteAgentBindingState::PendingSurface;

        let route = callback_route(binding_id.clone(), surface.clone());
        let mut applied = current.clone();
        let binding = applied.bindings.get_mut(&binding_id).expect("binding");
        binding.applied_surface = Some(applied_surface(&surface));
        binding.state = CompleteAgentBindingState::Available;

        let mut snapshot = CompleteAgentHostSnapshot {
            revision: CompleteAgentHostRevision(7),
            facts: current.clone(),
        };
        let before_failed_apply = snapshot.clone();
        assert!(matches!(
            apply_complete_agent_host_commit(
                &mut snapshot,
                CompleteAgentHostCommit {
                    expected_revision: CompleteAgentHostRevision(7),
                    facts: applied.clone(),
                },
            ),
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
        assert_eq!(
            snapshot, before_failed_apply,
            "a rejected apply commit cannot expose the applied surface without its route"
        );

        let mut extra = applied.clone();
        extra
            .callback_routes
            .insert(route.route_id.clone(), route.clone());
        let mut second = route.clone();
        second.route_id = AgentCallbackRouteId::new("route-extra").expect("route");
        extra
            .callback_routes
            .insert(second.route_id.clone(), second);
        assert_invariant(validate_complete_agent_host_facts(&current, &extra));

        let mut stale = applied.clone();
        let mut stale_route = route.clone();
        stale_route.generation = AgentBindingGeneration(2);
        stale
            .callback_routes
            .insert(stale_route.route_id.clone(), stale_route);
        assert_invariant(validate_complete_agent_host_facts(&current, &stale));

        let mut wrong_digest = applied.clone();
        let mut wrong_route = route.clone();
        wrong_route.bound_surface.digest =
            AgentSurfaceDigest::new("wrong-surface").expect("surface");
        wrong_digest
            .callback_routes
            .insert(wrong_route.route_id.clone(), wrong_route);
        assert_invariant(validate_complete_agent_host_facts(&current, &wrong_digest));

        applied
            .callback_routes
            .insert(route.route_id.clone(), route.clone());
        let applied_snapshot = apply_complete_agent_host_commit(
            &mut snapshot,
            CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(7),
                facts: applied.clone(),
            },
        )
        .expect("atomic apply route");

        let mut missing_tombstone = applied.clone();
        let binding = missing_tombstone
            .bindings
            .get_mut(&binding_id)
            .expect("binding");
        binding.applied_surface = None;
        binding.state = CompleteAgentBindingState::PendingSurface;
        let before_failed_revoke = snapshot.clone();
        assert!(matches!(
            apply_complete_agent_host_commit(
                &mut snapshot,
                CompleteAgentHostCommit {
                    expected_revision: applied_snapshot.revision,
                    facts: missing_tombstone.clone(),
                },
            ),
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
        assert_eq!(
            snapshot, before_failed_revoke,
            "a rejected revoke commit cannot hide the applied surface while leaving its route active"
        );

        let mut revoked = missing_tombstone;
        revoked
            .revoked_callback_routes
            .insert(route.route_id.clone());
        apply_complete_agent_host_commit(
            &mut snapshot,
            CompleteAgentHostCommit {
                expected_revision: applied_snapshot.revision,
                facts: revoked.clone(),
            },
        )
        .expect("atomic revoke tombstone");
        assert_eq!(
            revoked.callback_routes.get(&route.route_id),
            Some(&route),
            "revocation retains the immutable generation/digest fence"
        );
    }

    #[test]
    fn effect_identity_payload_and_observation_evidence_are_monotonic() {
        let current = valid_facts();

        let mut wrong_map_key = current.clone();
        let effect = wrong_map_key.effects.remove(&effect_id()).expect("effect");
        wrong_map_key.effects.insert(
            AgentEffectIdentity::new("other-effect").expect("effect"),
            effect,
        );
        assert_invariant(validate_complete_agent_host_facts(&current, &wrong_map_key));

        let mut payload_rewrite = current.clone();
        payload_rewrite
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .payload_digest = AgentPayloadDigest::new("sha256:other").expect("digest");
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &payload_rewrite,
        ));

        let mut duplicate_command = current.clone();
        let mut duplicate = duplicate_command
            .effects
            .get(&effect_id())
            .expect("effect")
            .clone();
        duplicate.effect_id = AgentEffectIdentity::new("other-effect").expect("effect");
        duplicate_command
            .effects
            .insert(duplicate.effect_id.clone(), duplicate);
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &duplicate_command,
        ));

        let mut applied = current.clone();
        let applied_effect = applied.effects.get_mut(&effect_id()).expect("effect");
        applied_effect.state = CompleteAgentEffectState::Applied;
        applied_effect.receipt = Some(AgentCommandReceipt {
            command_id: applied_effect.command_id.clone(),
            effect_id: applied_effect.effect_id.clone(),
            source: applied_effect.source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        });
        let mut downgraded = applied.clone();
        downgraded
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .state = CompleteAgentEffectState::Unknown;
        assert_invariant(validate_complete_agent_host_facts(&applied, &downgraded));

        let mut rewritten_evidence = applied.clone();
        rewritten_evidence
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .receipt
            .as_mut()
            .expect("receipt")
            .state = AgentReceiptState::AlreadyApplied {
            terminal: Some(AgentTerminalOutcome::Succeeded),
        };
        assert_invariant(validate_complete_agent_host_facts(
            &applied,
            &rewritten_evidence,
        ));
    }

    #[test]
    fn latest_inspection_evidence_advances_without_downgrade_or_terminal_rewrite() {
        let unknown = facts_with_inspection(
            CompleteAgentEffectState::Unknown,
            AgentEffectInspectionState::Unknown,
        );
        let applied = facts_with_inspection(
            CompleteAgentEffectState::Applied,
            AgentEffectInspectionState::Applied {
                source: AgentSourceCoordinate::new("source").expect("source"),
                terminal: None,
                initial_context: None,
                child_source: None,
            },
        );
        validate_complete_agent_host_facts(&unknown, &applied)
            .expect("Unknown inspection advances to Applied");
        validate_complete_agent_host_facts(&applied, &applied)
            .expect("exact terminal inspection replay");

        let accepted = facts_with_inspection(
            CompleteAgentEffectState::Accepted,
            AgentEffectInspectionState::Accepted {
                source: AgentSourceCoordinate::new("source").expect("source"),
            },
        );
        validate_complete_agent_host_facts(&accepted, &applied)
            .expect("Accepted inspection advances to Applied");

        assert_invariant(validate_complete_agent_host_facts(&applied, &unknown));
        assert_invariant(validate_complete_agent_host_facts(&applied, &accepted));

        let mut same_state_different_evidence = unknown.clone();
        same_state_different_evidence
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .inspection
            .as_mut()
            .expect("inspection")
            .command_id = None;
        assert_invariant(validate_complete_agent_host_facts(
            &unknown,
            &same_state_different_evidence,
        ));

        let mut rewritten_terminal = applied.clone();
        rewritten_terminal
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .inspection
            .as_mut()
            .expect("inspection")
            .state = AgentEffectInspectionState::Applied {
            source: AgentSourceCoordinate::new("source").expect("source"),
            terminal: Some(AgentTerminalOutcome::Succeeded),
            initial_context: None,
            child_source: None,
        };
        assert_invariant(validate_complete_agent_host_facts(
            &applied,
            &rewritten_terminal,
        ));
    }

    #[test]
    fn redispatch_attempt_requires_complete_append_only_attempt_history() {
        let mut not_applied = facts_with_inspection(
            CompleteAgentEffectState::NotApplied,
            AgentEffectInspectionState::NotApplied,
        );
        let effect = not_applied.effects.get_mut(&effect_id()).expect("effect");
        effect.receipt = Some(AgentCommandReceipt {
            command_id: effect.command_id.clone(),
            effect_id: effect.effect_id.clone(),
            source: effect.source.clone(),
            state: AgentReceiptState::Unknown,
            snapshot_revision: None,
            initial_context: None,
        });

        let mut redispatched = not_applied.clone();
        let effect = redispatched.effects.get_mut(&effect_id()).expect("effect");
        let inspection = effect.inspection.take().expect("NotApplied inspection");
        effect
            .attempt_history
            .push(CompleteAgentEffectAttemptEvidence {
                dispatch_attempt: effect.dispatch_attempt,
                delivery_epoch: effect.delivery_epoch,
                state: effect.state,
                receipt: effect.receipt.take(),
                surface_receipt: effect.surface_receipt.take(),
                inspection: Some(inspection),
            });
        effect.dispatch_attempt = 2;
        effect.state = CompleteAgentEffectState::Dispatching;
        validate_complete_agent_host_facts(&not_applied, &redispatched)
            .expect("explicit redispatch archives the complete prior attempt");

        let mut unarchived_redispatch = not_applied.clone();
        unarchived_redispatch
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .state = CompleteAgentEffectState::Dispatching;
        assert_invariant(validate_complete_agent_host_facts(
            &not_applied,
            &unarchived_redispatch,
        ));

        let mut applied = redispatched.clone();
        let applied_effect = applied.effects.get_mut(&effect_id()).expect("effect");
        applied_effect.state = CompleteAgentEffectState::Applied;
        applied_effect.receipt = Some(AgentCommandReceipt {
            command_id: applied_effect.command_id.clone(),
            effect_id: applied_effect.effect_id.clone(),
            source: applied_effect.source.clone(),
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        });
        validate_complete_agent_host_facts(&redispatched, &applied)
            .expect("new attempt Applied receipt");

        let mut unknown = redispatched.clone();
        unknown.effects.get_mut(&effect_id()).expect("effect").state =
            CompleteAgentEffectState::Unknown;
        validate_complete_agent_host_facts(&redispatched, &unknown)
            .expect("new attempt Unknown observation");

        let mut removed_history = redispatched.clone();
        removed_history
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .attempt_history
            .clear();
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &removed_history,
        ));

        let mut duplicate_history = redispatched.clone();
        let duplicate = duplicate_history
            .effects
            .get(&effect_id())
            .expect("effect")
            .attempt_history[0]
            .clone();
        duplicate_history
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .attempt_history
            .push(duplicate);
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &duplicate_history,
        ));

        let mut rewritten_receipt = redispatched.clone();
        rewritten_receipt
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .attempt_history[0]
            .receipt
            .as_mut()
            .expect("receipt")
            .state = AgentReceiptState::Rejected {
            code: "tampered".to_owned(),
            message: "tampered".to_owned(),
        };
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &rewritten_receipt,
        ));

        let mut rewritten_inspection = redispatched.clone();
        rewritten_inspection
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .attempt_history[0]
            .inspection
            .as_mut()
            .expect("inspection")
            .state = AgentEffectInspectionState::Unknown;
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &rewritten_inspection,
        ));

        let mut skipped_attempt = redispatched.clone();
        skipped_attempt
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .attempt_history[0]
            .dispatch_attempt = 2;
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &skipped_attempt,
        ));

        let mut attempt_rollback = redispatched.clone();
        attempt_rollback
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .dispatch_attempt = 1;
        assert_invariant(validate_complete_agent_host_facts(
            &redispatched,
            &attempt_rollback,
        ));

        assert_invariant(validate_complete_agent_host_facts(
            &CompleteAgentHostFacts::default(),
            &redispatched,
        ));
    }

    #[test]
    fn lease_identity_epoch_and_terminal_binding_rules_are_enforced() {
        let current = valid_facts();

        let mut invalid_owner = current.clone();
        invalid_owner
            .leases
            .get_mut(&binding_id())
            .expect("lease")
            .owner = " ".to_owned();
        assert_invariant(validate_complete_agent_host_facts(&current, &invalid_owner));

        let mut expiry_regression = current.clone();
        expiry_regression
            .leases
            .get_mut(&binding_id())
            .expect("lease")
            .expires_at_ms = 99;
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &expiry_regression,
        ));

        let mut stale_epoch = current.clone();
        stale_epoch.lease_epochs.insert(binding_id(), 0);
        stale_epoch
            .leases
            .get_mut(&binding_id())
            .expect("lease")
            .epoch = 0;
        assert_invariant(validate_complete_agent_host_facts(&current, &stale_epoch));

        let mut lost_with_lease = current.clone();
        lost_with_lease
            .bindings
            .get_mut(&binding_id())
            .expect("binding")
            .state = CompleteAgentBindingState::Lost;
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &lost_with_lease,
        ));
    }

    #[test]
    fn stale_divergent_compare_and_swap_is_rejected_without_mutation() {
        let mut snapshot = CompleteAgentHostSnapshot::default();
        apply_complete_agent_host_commit(
            &mut snapshot,
            CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(0),
                facts: valid_facts(),
            },
        )
        .expect("initial commit");
        let committed = snapshot.clone();
        let mut divergent = committed.facts.clone();
        divergent
            .effects
            .get_mut(&effect_id())
            .expect("effect")
            .state = CompleteAgentEffectState::Unknown;

        assert!(matches!(
            apply_complete_agent_host_commit(
                &mut snapshot,
                CompleteAgentHostCommit {
                    expected_revision: CompleteAgentHostRevision(0),
                    facts: divergent,
                },
            ),
            Err(CompleteAgentHostStoreError::Conflict { .. })
        ));
        assert_eq!(snapshot, committed);
    }

    fn assert_invariant(result: Result<(), CompleteAgentHostStoreError>) {
        assert!(matches!(
            result,
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
    }

    fn service_id() -> AgentServiceInstanceId {
        AgentServiceInstanceId::new("service").expect("service")
    }

    fn binding_id() -> CompleteAgentBindingId {
        CompleteAgentBindingId::new("binding").expect("binding")
    }

    fn effect_id() -> AgentEffectIdentity {
        AgentEffectIdentity::new("effect").expect("effect")
    }

    fn callback_surface() -> BoundAgentSurface {
        BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("callback-surface").expect("surface"),
            offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            contributions: vec![BoundAgentSurfaceContribution {
                key: "tool:test".to_owned(),
                required: true,
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::BindingOnly,
                }),
                payload: AgentSurfaceContributionPayload::Tool {
                    name: AgentToolName::new("test").expect("tool"),
                    description: "test".to_owned(),
                    input_schema: json!({"type": "object"}),
                    output_schema: None,
                },
                payload_digest: AgentPayloadDigest::new("callback-payload").expect("payload"),
            }],
        }
    }

    fn callback_route(
        binding_id: CompleteAgentBindingId,
        bound_surface: BoundAgentSurface,
    ) -> CompleteAgentCallbackRoute {
        CompleteAgentCallbackRoute {
            route_id: AgentCallbackRouteId::new("callback-route").expect("route"),
            binding_id,
            generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source").expect("source"),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 10,
            bound_surface,
        }
    }

    fn applied_surface(surface: &BoundAgentSurface) -> AppliedAgentSurface {
        AppliedAgentSurface {
            revision: surface.revision,
            digest: surface.digest.clone(),
            contributions: surface
                .contributions
                .iter()
                .map(|bound| AppliedAgentSurfaceContribution {
                    key: bound.key.clone(),
                    route: bound.route,
                    fidelity: bound.fidelity,
                    semantics: bound.semantics.clone(),
                    payload_digest: bound.payload_digest.clone(),
                    status: AppliedContributionStatus::Applied,
                    evidence: Some("fixture applied".to_owned()),
                })
                .collect(),
        }
    }

    fn facts_with_inspection(
        state: CompleteAgentEffectState,
        inspection_state: AgentEffectInspectionState,
    ) -> CompleteAgentHostFacts {
        let mut facts = valid_facts();
        let effect = facts.effects.get_mut(&effect_id()).expect("effect");
        effect.state = state;
        effect.inspection = Some(AgentEffectInspection {
            effect_id: effect.effect_id.clone(),
            command_id: Some(effect.command_id.clone()),
            state: inspection_state,
        });
        facts
    }

    fn valid_facts() -> CompleteAgentHostFacts {
        let service_id = service_id();
        let binding_id = binding_id();
        let effect_id = effect_id();
        let source = AgentSourceCoordinate::new("source").expect("source");
        let profile_digest = AgentProfileDigest::new("profile").expect("profile");
        let descriptor = AgentServiceDescriptor {
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            title: "service".to_owned(),
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
            profile_digest: profile_digest.clone(),
            configuration_boundary: AgentConfigurationBoundary::Binding,
        };
        let offer = AgentRuntimeOffer {
            profile_digest: profile_digest.clone(),
            contributions: Vec::new(),
        };
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            service_instance_id: service_id.clone(),
            generation: AgentBindingGeneration(1),
            source: source.clone(),
            profile_digest: profile_digest.clone(),
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("surface").expect("surface"),
                offer_profile_digest: profile_digest,
                contributions: Vec::new(),
            },
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        };
        let lease = CompleteAgentBindingLease {
            binding_id: binding_id.clone(),
            generation: AgentBindingGeneration(1),
            owner: "worker".to_owned(),
            token: "token".to_owned(),
            epoch: 1,
            expires_at_ms: 100,
        };
        let effect = CompleteAgentEffectRecord {
            effect_id: effect_id.clone(),
            command_id: AgentCommandId::new("command").expect("command"),
            binding_id: binding_id.clone(),
            service_instance_id: service_id.clone(),
            generation: AgentBindingGeneration(1),
            source: source.clone(),
            payload_digest: AgentPayloadDigest::new("sha256:payload").expect("digest"),
            delivery_epoch: 1,
            dispatch_attempt: 1,
            state: CompleteAgentEffectState::Dispatching,
            receipt: None,
            surface_receipt: None,
            inspection: None,
            attempt_history: Vec::new(),
        };
        CompleteAgentHostFacts {
            service_instances: BTreeMap::from([(service_id.clone(), descriptor)]),
            offers: BTreeMap::from([(service_id.clone(), offer)]),
            placements: BTreeMap::from([(
                service_id.clone(),
                CompleteAgentPlacement::InProcess {
                    host_incarnation_id: "fixture-host".to_owned(),
                },
            )]),
            bindings: BTreeMap::from([(binding_id.clone(), binding)]),
            source_coordinates: BTreeMap::from([(binding_id.clone(), source)]),
            callback_routes: BTreeMap::new(),
            revoked_callback_routes: BTreeSet::new(),
            effects: BTreeMap::from([(effect_id, effect)]),
            leases: BTreeMap::from([(binding_id.clone(), lease)]),
            lease_epochs: BTreeMap::from([(binding_id, 1)]),
            runtime_targets: BTreeMap::new(),
            lifecycle_effects: BTreeMap::new(),
        }
    }
}
