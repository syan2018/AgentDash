use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentCallbackRouteId, AgentEffectIdentity, AgentEffectInspectionState, AgentPayloadDigest,
    AgentProfileDigest, AgentReceiptState, AgentServiceInstanceId, AgentSourceCoordinate,
    AgentSurfaceRoute,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
    CompleteAgentBindingState, CompleteAgentCallbackRoute, CompleteAgentEffectAttemptEvidence,
    CompleteAgentEffectRecord, CompleteAgentEffectState, CompleteAgentLifecycleAppliedReceipt,
    CompleteAgentLifecycleEffectRecord, CompleteAgentLifecycleOperationKind,
    CompleteAgentLifecycleOutcome, CompleteAgentRuntimeTarget,
    CompleteAgentRuntimeTargetProvisioning, CompleteAgentRuntimeTargetRecovery,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompleteAgentHostRevision(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentVerifiedBuildEvidence {
    pub claimed_build_digest: AgentPayloadDigest,
    pub evidence_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentVerificationMethod {
    PinnedBuiltin,
    RemoteTransportAttestation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerificationRequest {
    pub service_instance_id: AgentServiceInstanceId,
    pub publisher_integration: String,
    pub service_version: String,
    pub claimed_build_digest: AgentPayloadDigest,
    pub profile_digest: AgentProfileDigest,
    pub claimed_conformance_suite_revision: String,
}

/// Host-owned trust root supplied independently from an Integration contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerificationRecord {
    pub service_instance_id: AgentServiceInstanceId,
    pub expected_publisher_integration: String,
    pub expected_service_version: String,
    pub expected_build_digest: AgentPayloadDigest,
    pub expected_profile_digest: AgentProfileDigest,
    pub expected_conformance_suite_revision: String,
    pub method: CompleteAgentVerificationMethod,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub evidence_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentServiceVerification {
    pub service_instance_id: AgentServiceInstanceId,
    pub publisher_integration: String,
    pub service_version: String,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub method: CompleteAgentVerificationMethod,
    pub verified_profile_digest: AgentProfileDigest,
    pub claimed_conformance_suite_revision: String,
    pub verified_build: CompleteAgentVerifiedBuildEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentVerificationError {
    #[error("no trusted Complete Agent verification record for {service_instance_id}")]
    MissingRecord {
        service_instance_id: AgentServiceInstanceId,
    },
    #[error("Complete Agent verification claim drifted at {coordinate}")]
    ClaimDrift { coordinate: &'static str },
    #[error("Complete Agent verification record is invalid: {reason}")]
    InvalidRecord { reason: String },
}

#[async_trait]
pub trait CompleteAgentRegistrationVerifier: Send + Sync {
    async fn verify(
        &self,
        request: CompleteAgentVerificationRequest,
    ) -> Result<CompleteAgentServiceVerification, CompleteAgentVerificationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentRemoteBindingFact {
    pub local_service_instance_id: AgentServiceInstanceId,
    pub remote_service_instance_id: AgentServiceInstanceId,
    pub remote_binding_generation: agentdash_agent_service_api::AgentBindingGeneration,
    pub host_incarnation_id: String,
    pub transport_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentHostFacts {
    pub bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    pub source_coordinates: BTreeMap<CompleteAgentBindingId, AgentSourceCoordinate>,
    pub callback_routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    pub revoked_callback_routes: BTreeSet<AgentCallbackRouteId>,
    pub effects: BTreeMap<AgentEffectIdentity, CompleteAgentEffectRecord>,
    pub leases: BTreeMap<CompleteAgentBindingId, CompleteAgentBindingLease>,
    pub lease_epochs: BTreeMap<CompleteAgentBindingId, u64>,
    pub runtime_targets: BTreeMap<RuntimeThreadId, CompleteAgentRuntimeTarget>,
    pub runtime_target_provisionings: BTreeMap<
        agentdash_agent_service_api::AgentIdempotencyKey,
        CompleteAgentRuntimeTargetProvisioning,
    >,
    pub runtime_target_recoveries: BTreeMap<
        agentdash_agent_service_api::AgentIdempotencyKey,
        CompleteAgentRuntimeTargetRecovery,
    >,
    pub lifecycle_effects: BTreeMap<AgentEffectIdentity, CompleteAgentLifecycleEffectRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

/// Durable authority for Complete Agent exact target snapshots, bindings, sources, effects,
/// leases, recovery, and generation facts.
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

/// Current-process Host routing graph.
///
/// Attachments, bindings, leases and callback routes are incarnation-scoped facts. Dropping this
/// value on restart is the fencing rule: old routes become unknown and a new surface application
/// establishes the next route.
#[derive(Default)]
pub struct ProcessCompleteAgentHostRepository {
    state: tokio::sync::RwLock<CompleteAgentHostSnapshot>,
}

impl ProcessCompleteAgentHostRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompleteAgentHostRepository for ProcessCompleteAgentHostRepository {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        Ok(self.state.read().await.clone())
    }

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        let mut state = self.state.write().await;
        apply_complete_agent_host_commit(&mut state, commit)
    }
}

pub fn encode_complete_agent_host_snapshot(
    snapshot: &CompleteAgentHostSnapshot,
) -> Result<serde_json::Value, CompleteAgentHostStoreError> {
    serde_json::to_value(snapshot).map_err(|error| CompleteAgentHostStoreError::Persistence {
        reason: format!("failed to encode Complete Agent Host snapshot: {error}"),
    })
}

pub fn decode_complete_agent_host_snapshot(
    value: serde_json::Value,
) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
    let snapshot: CompleteAgentHostSnapshot =
        serde_json::from_value(value).map_err(|error| CompleteAgentHostStoreError::Invariant {
            reason: format!("failed to decode Complete Agent Host snapshot: {error}"),
        })?;
    validate_complete_agent_host_facts(&snapshot.facts, &snapshot.facts)?;
    Ok(snapshot)
}

pub type SharedCompleteAgentHostRepository = Arc<dyn CompleteAgentHostRepository>;

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
    for (binding_id, binding) in &candidate.bindings {
        if binding_id.as_str().trim().is_empty()
            || !binding.target.is_valid()
            || binding.source.as_str().trim().is_empty()
            || binding.profile_digest.as_str().trim().is_empty()
            || binding.bound_surface.digest.as_str().trim().is_empty()
            || binding
                .bound_surface
                .offer_profile_digest
                .as_str()
                .trim()
                .is_empty()
            || binding_id != &binding.id
            || binding.generation.0 == 0
        {
            return invariant("binding coordinates or generation are invalid");
        }
        if binding.profile_digest != binding.target.offer_profile_digest
            || binding.bound_surface.offer_profile_digest != binding.target.offer_profile_digest
        {
            return invariant(
                "binding profile or surface does not match its exact target snapshot",
            );
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
            || binding.target != next.target
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
        if binding_id.as_str().trim().is_empty()
            || source.as_str().trim().is_empty()
            || !candidate.bindings.contains_key(binding_id)
        {
            return invariant("source coordinate has no owning binding");
        }
        if !matches!(
            candidate.bindings[binding_id].state,
            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
        ) && candidate
            .source_coordinates
            .iter()
            .any(|(other_id, other_source)| {
                other_id != binding_id
                    && other_source == source
                    && candidate.bindings.get(other_id).is_some_and(|other| {
                        !matches!(
                            other.state,
                            CompleteAgentBindingState::Lost | CompleteAgentBindingState::Closed
                        )
                    })
            })
        {
            return invariant("source coordinate has multiple nonterminal bindings");
        }
    }
    for (binding_id, source) in &current.source_coordinates {
        if candidate.source_coordinates.get(binding_id) != Some(source) {
            return invariant("source coordinate history is immutable");
        }
    }
    for (route_id, route) in &candidate.callback_routes {
        if route_id.as_str().trim().is_empty()
            || route.binding_id.as_str().trim().is_empty()
            || route.source.as_str().trim().is_empty()
            || route_id != &route.route_id
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
    if candidate
        .revoked_callback_routes
        .iter()
        .any(|route_id| route_id.as_str().trim().is_empty())
    {
        return invariant("callback route tombstone identity is invalid");
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
        if effect_id.as_str().trim().is_empty()
            || effect.command_id.as_str().trim().is_empty()
            || effect.binding_id.as_str().trim().is_empty()
            || effect.source.as_str().trim().is_empty()
            || effect.payload_digest.as_str().trim().is_empty()
            || effect_id != &effect.effect_id
        {
            return invariant("effect coordinates or payload digest are invalid");
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
        if effect.generation != binding.generation || effect.source != binding.source {
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
    if candidate
        .lease_epochs
        .iter()
        .any(|(binding_id, epoch)| binding_id.as_str().trim().is_empty() || *epoch == 0)
    {
        return invariant("lease epoch history contains invalid coordinates");
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
        validate_runtime_target(candidate, thread_id, target)?;
    }
    for (thread_id, target) in &current.runtime_targets {
        let Some(next) = candidate.runtime_targets.get(thread_id) else {
            return invariant("Runtime target history cannot be removed");
        };
        if next != target {
            let transitions = candidate
                .runtime_target_recoveries
                .values()
                .filter(|recovery| {
                    &recovery.previous_target == target && &recovery.recovered_target == next
                })
                .count();
            if transitions != 1 {
                return invariant(
                    "Runtime target may advance only through one explicit recovery record",
                );
            }
        }
    }
    for (idempotency_key, provisioning) in &candidate.runtime_target_provisionings {
        if idempotency_key != &provisioning.idempotency_key
            || provisioning.request_digest.as_str().trim().is_empty()
            || !runtime_target_in_lineage(candidate, &provisioning.target)
        {
            return invariant(
                "Runtime target provisioning must identify one immutable registered target",
            );
        }
    }
    for (idempotency_key, provisioning) in &current.runtime_target_provisionings {
        if candidate.runtime_target_provisionings.get(idempotency_key) != Some(provisioning) {
            return invariant("Runtime target provisioning history cannot be removed or rewritten");
        }
    }
    for (idempotency_key, recovery) in &candidate.runtime_target_recoveries {
        if idempotency_key != &recovery.idempotency_key
            || recovery.request_digest.as_str().trim().is_empty()
            || recovery.previous_target.runtime_thread_id
                != recovery.recovered_target.runtime_thread_id
            || recovery.recovered_target.generation.0
                != recovery.previous_target.generation.0.saturating_add(1)
        {
            return invariant("Runtime target recovery coordinates are invalid");
        }
        validate_runtime_target(
            candidate,
            &recovery.previous_target.runtime_thread_id,
            &recovery.previous_target,
        )?;
        validate_runtime_target(
            candidate,
            &recovery.recovered_target.runtime_thread_id,
            &recovery.recovered_target,
        )?;
        let previous_binding_id = CompleteAgentBindingId::new(format!(
            "runtime-binding:{}:{}",
            recovery.previous_target.runtime_thread_id, recovery.previous_target.generation.0
        ))
        .map_err(|error| CompleteAgentHostStoreError::Invariant {
            reason: error.to_string(),
        })?;
        let previous_binding = candidate
            .bindings
            .get(&previous_binding_id)
            .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "Runtime target recovery has no previous binding".to_owned(),
            })?;
        if previous_binding.state != CompleteAgentBindingState::Lost
            || previous_binding.generation != recovery.previous_target.generation
            || previous_binding.target != recovery.previous_target.target
        {
            return invariant("Runtime target recovery previous binding is not exactly lost");
        }
        if !runtime_target_in_lineage(candidate, &recovery.previous_target)
            || !runtime_target_in_lineage(candidate, &recovery.recovered_target)
        {
            return invariant("Runtime target recovery is detached from target lineage");
        }
    }
    for (idempotency_key, recovery) in &current.runtime_target_recoveries {
        if candidate.runtime_target_recoveries.get(idempotency_key) != Some(recovery) {
            return invariant("Runtime target recovery history cannot be removed or rewritten");
        }
    }
    for (effect_id, effect) in &candidate.lifecycle_effects {
        if effect_id.as_str().trim().is_empty()
            || effect.runtime_thread_id.as_str().trim().is_empty()
            || effect
                .child_thread_id
                .as_ref()
                .is_some_and(|thread_id| thread_id.as_str().trim().is_empty())
            || !effect.target.is_valid()
            || effect_id != &effect.effect_id
        {
            return invariant("lifecycle effect coordinates are invalid");
        }
        let target =
            runtime_target_at_generation(candidate, &effect.runtime_thread_id, effect.generation)
                .ok_or_else(|| CompleteAgentHostStoreError::Invariant {
                reason: "lifecycle effect has no matching Runtime target generation".to_owned(),
            })?;
        if effect.target != target.target || effect.generation != target.generation {
            return invariant("lifecycle effect does not match its Runtime target");
        }
        validate_lifecycle_effect(effect)?;
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
            || effect.target != next.target
            || effect.generation != next.generation
            || effect.initial_context != next.initial_context
            || effect.fork_cutoff != next.fork_cutoff
            || (effect.applied_receipt.is_some() && effect.applied_receipt != next.applied_receipt)
            || (effect.outcome.is_some() && effect.outcome != next.outcome)
        {
            return invariant("lifecycle effect coordinates or applied evidence were rewritten");
        }
    }
    Ok(())
}

fn validate_runtime_target(
    _facts: &CompleteAgentHostFacts,
    thread_id: &RuntimeThreadId,
    target: &CompleteAgentRuntimeTarget,
) -> Result<(), CompleteAgentHostStoreError> {
    if thread_id.as_str().trim().is_empty()
        || !target.target.is_valid()
        || target.profile_digest.as_str().trim().is_empty()
        || target.callbacks.route_id.as_str().trim().is_empty()
        || thread_id != &target.runtime_thread_id
        || target.generation.0 == 0
        || target.callbacks.binding_generation != target.generation
        || target.callbacks.delivery != AgentSurfaceRoute::AgentNativeCallback
        || target.callbacks.default_deadline_ms == 0
    {
        return invariant("Runtime target identity or generation is invalid");
    }
    if target.profile_digest != target.target.offer_profile_digest
        || target.bound_surface.offer_profile_digest != target.target.offer_profile_digest
    {
        return invariant("Runtime target does not match its exact target snapshot");
    }
    Ok(())
}

fn runtime_target_in_lineage(
    facts: &CompleteAgentHostFacts,
    target: &CompleteAgentRuntimeTarget,
) -> bool {
    facts
        .runtime_targets
        .get(&target.runtime_thread_id)
        .is_some_and(|current| current == target)
        || facts.runtime_target_recoveries.values().any(|recovery| {
            &recovery.previous_target == target || &recovery.recovered_target == target
        })
}

fn runtime_target_at_generation<'a>(
    facts: &'a CompleteAgentHostFacts,
    thread_id: &RuntimeThreadId,
    generation: agentdash_agent_service_api::AgentBindingGeneration,
) -> Option<&'a CompleteAgentRuntimeTarget> {
    facts
        .runtime_targets
        .get(thread_id)
        .filter(|target| target.generation == generation)
        .or_else(|| {
            facts
                .runtime_target_recoveries
                .values()
                .find_map(|recovery| {
                    [&recovery.previous_target, &recovery.recovered_target]
                        .into_iter()
                        .find(|target| {
                            &target.runtime_thread_id == thread_id
                                && target.generation == generation
                        })
                })
        })
}

fn validate_lifecycle_effect(
    effect: &CompleteAgentLifecycleEffectRecord,
) -> Result<(), CompleteAgentHostStoreError> {
    let is_fork = effect.kind == CompleteAgentLifecycleOperationKind::Fork;
    if effect.child_thread_id.is_some() != is_fork || effect.fork_cutoff.is_some() != is_fork {
        return invariant("lifecycle effect child and cutoff coordinates do not match its kind");
    }
    if effect.initial_context.is_some()
        && effect.kind != CompleteAgentLifecycleOperationKind::Create
    {
        return invariant("only Create lifecycle effects may carry initial context");
    }
    if let Some(applied_receipt) = &effect.applied_receipt {
        match (effect.kind, applied_receipt) {
            (
                CompleteAgentLifecycleOperationKind::Create
                | CompleteAgentLifecycleOperationKind::Resume
                | CompleteAgentLifecycleOperationKind::Rebind
                | CompleteAgentLifecycleOperationKind::Execute,
                CompleteAgentLifecycleAppliedReceipt::Agent(receipt),
            ) if receipt.effect_id == effect.effect_id => {}
            (
                CompleteAgentLifecycleOperationKind::Fork,
                CompleteAgentLifecycleAppliedReceipt::Fork(receipt),
            ) if receipt.effect_id == effect.effect_id
                && Some(&receipt.cutoff) == effect.fork_cutoff.as_ref() => {}
            _ => {
                return invariant(
                    "lifecycle applied receipt does not match its durable effect identity and kind",
                );
            }
        }
    }
    let Some(outcome) = &effect.outcome else {
        return Ok(());
    };
    let valid_outcome = match (effect.kind, outcome) {
        (
            CompleteAgentLifecycleOperationKind::Create
            | CompleteAgentLifecycleOperationKind::Resume
            | CompleteAgentLifecycleOperationKind::Rebind
            | CompleteAgentLifecycleOperationKind::Execute,
            CompleteAgentLifecycleOutcome::Agent { receipt, .. },
        ) if receipt.effect_id == effect.effect_id => true,
        (
            CompleteAgentLifecycleOperationKind::Fork,
            CompleteAgentLifecycleOutcome::Fork { receipt, .. },
        ) if receipt.effect_id == effect.effect_id
            && Some(&receipt.cutoff) == effect.fork_cutoff.as_ref() =>
        {
            true
        }
        _ => false,
    };
    if !valid_outcome {
        return invariant("lifecycle outcome does not match its durable effect identity and kind");
    }
    match (&effect.applied_receipt, outcome) {
        (
            Some(CompleteAgentLifecycleAppliedReceipt::Agent(applied)),
            CompleteAgentLifecycleOutcome::Agent { receipt, .. },
        ) if agent_outcome_matches_applied(applied, receipt) => Ok(()),
        (
            Some(CompleteAgentLifecycleAppliedReceipt::Fork(applied)),
            CompleteAgentLifecycleOutcome::Fork { receipt, .. },
        ) if fork_outcome_matches_applied(applied, receipt) => Ok(()),
        (None, _) => invariant("settled lifecycle outcome has no durable applied receipt"),
        _ => invariant("settled lifecycle outcome rewrites its durable applied receipt"),
    }
}

fn agent_outcome_matches_applied(
    applied: &agentdash_agent_service_api::AppliedAgentCommandReceipt,
    receipt: &agentdash_agent_service_api::AgentCommandReceipt,
) -> bool {
    let terminal = match &receipt.state {
        AgentReceiptState::AlreadyApplied { terminal } => *terminal,
        AgentReceiptState::Terminal { outcome } => Some(*outcome),
        _ => return false,
    };
    applied.command_id == receipt.command_id
        && applied.effect_id == receipt.effect_id
        && applied.source == receipt.source
        && applied.terminal == terminal
        && applied.snapshot_revision == receipt.snapshot_revision
        && applied.initial_context == receipt.initial_context
}

fn fork_outcome_matches_applied(
    applied: &agentdash_agent_service_api::AppliedForkAgentReceipt,
    receipt: &agentdash_agent_service_api::ForkAgentReceipt,
) -> bool {
    let terminal = match &receipt.state {
        AgentReceiptState::AlreadyApplied { terminal } => *terminal,
        AgentReceiptState::Terminal { outcome } => Some(*outcome),
        _ => return false,
    };
    applied.command_id == receipt.command_id
        && applied.effect_id == receipt.effect_id
        && applied.parent_source == receipt.parent_source
        && receipt.child_source.as_ref() == Some(&applied.child_source)
        && applied.cutoff == receipt.cutoff
        && receipt.child_history_digest.as_ref() == Some(&applied.child_history_digest)
        && applied.terminal == terminal
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
        AgentEffectInspectionState::Accepted { source } => Some(source),
        AgentEffectInspectionState::Applied { outcome } => Some(outcome.source()),
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
        AgentAppliedEffectOutcome, AgentBindingGeneration, AgentCallbackRouteId, AgentCommandId,
        AgentCommandReceipt, AgentEffectIdentity, AgentEffectInspection, AgentForkPoint,
        AgentHostCallbackBinding, AgentPayloadDigest, AgentProfileDigest, AgentReceiptState,
        AgentServiceDefinitionId, AgentSurfaceContributionPayload, AgentSurfaceDigest,
        AgentSurfaceRevision, AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentToolDelivery,
        AgentToolName, AgentToolSemanticFacet, AgentToolUpdateSemantics,
        AppliedAgentCommandReceipt, AppliedAgentSurface, AppliedAgentSurfaceContribution,
        AppliedContributionStatus, BoundAgentSurface, BoundAgentSurfaceContribution,
        CompleteAgentLiveAttachmentId, ForkAgentReceipt, SemanticFidelity,
    };
    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::{CompleteAgentBindingTarget, CompleteAgentPlacement};

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

    #[test]
    fn persisted_host_state_round_trips_only_durable_binding_effect_and_lease_facts() {
        let snapshot = CompleteAgentHostSnapshot {
            revision: CompleteAgentHostRevision(8),
            facts: valid_facts(),
        };

        let encoded = encode_complete_agent_host_snapshot(&snapshot).expect("encode Host snapshot");
        let decoded = decode_complete_agent_host_snapshot(encoded).expect("decode Host snapshot");

        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.facts.bindings.len(), 1);
        assert_eq!(decoded.facts.source_coordinates.len(), 1);
        assert_eq!(decoded.facts.effects.len(), 1);
        assert_eq!(decoded.facts.leases.len(), 1);
        assert_eq!(decoded.facts.lease_epochs.len(), 1);
    }

    #[test]
    fn persisted_host_state_rejects_an_incomplete_fact_graph() {
        let snapshot = CompleteAgentHostSnapshot {
            revision: CompleteAgentHostRevision(8),
            facts: valid_facts(),
        };
        let mut encoded =
            encode_complete_agent_host_snapshot(&snapshot).expect("encode Host snapshot");
        encoded["facts"]["source_coordinates"] = json!({});

        assert!(matches!(
            decode_complete_agent_host_snapshot(encoded),
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
    }

    #[test]
    fn persisted_host_state_rejects_invalid_runtime_target_callback_coordinates() {
        let mut facts = valid_facts();
        let runtime_thread_id = RuntimeThreadId::new("runtime-thread").expect("Runtime thread");
        let binding = facts.bindings.get(&binding_id()).expect("binding");
        let profile_digest = binding.profile_digest.clone();
        let bound_surface = binding.bound_surface.clone();
        let target = binding.target.clone();
        facts.runtime_targets.insert(
            runtime_thread_id.clone(),
            CompleteAgentRuntimeTarget {
                runtime_thread_id,
                target,
                generation: AgentBindingGeneration(1),
                profile_digest,
                bound_surface,
                callbacks: AgentHostCallbackBinding {
                    route_id: AgentCallbackRouteId::new("target-route").expect("callback route"),
                    binding_generation: AgentBindingGeneration(1),
                    delivery: AgentSurfaceRoute::AgentNativeCallback,
                    default_deadline_ms: 10,
                },
            },
        );
        let snapshot = CompleteAgentHostSnapshot {
            revision: CompleteAgentHostRevision(8),
            facts,
        };
        let encoded = encode_complete_agent_host_snapshot(&snapshot).expect("encode Host snapshot");

        let mut zero_deadline = encoded.clone();
        zero_deadline["facts"]["runtime_targets"]["runtime-thread"]["callbacks"]["default_deadline_ms"] =
            json!(0);
        assert!(matches!(
            decode_complete_agent_host_snapshot(zero_deadline),
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));

        let mut wrong_delivery = encoded;
        wrong_delivery["facts"]["runtime_targets"]["runtime-thread"]["callbacks"]["delivery"] =
            json!("runtime_native");
        assert!(matches!(
            decode_complete_agent_host_snapshot(wrong_delivery),
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
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
    fn lifecycle_effect_kind_and_outcome_coordinates_are_durable_invariants() {
        let effect_id = AgentEffectIdentity::new("lifecycle-effect").expect("effect");
        let thread_id = RuntimeThreadId::new("runtime-thread").expect("thread");
        let service_id = service_id();
        let invalid_create_coordinates = CompleteAgentLifecycleEffectRecord {
            effect_id: effect_id.clone(),
            runtime_thread_id: thread_id.clone(),
            child_thread_id: Some(RuntimeThreadId::new("child-thread").expect("child")),
            kind: CompleteAgentLifecycleOperationKind::Create,
            target: binding_target(service_id.clone()),
            generation: AgentBindingGeneration(1),
            initial_context: None,
            fork_cutoff: None,
            applied_receipt: None,
            outcome: None,
        };
        assert_invariant(validate_lifecycle_effect(&invalid_create_coordinates));

        let mismatched_outcome = CompleteAgentLifecycleEffectRecord {
            effect_id: effect_id.clone(),
            runtime_thread_id: thread_id,
            child_thread_id: None,
            kind: CompleteAgentLifecycleOperationKind::Create,
            target: binding_target(service_id),
            generation: AgentBindingGeneration(1),
            initial_context: None,
            fork_cutoff: None,
            applied_receipt: None,
            outcome: Some(CompleteAgentLifecycleOutcome::Fork {
                receipt: ForkAgentReceipt {
                    command_id: AgentCommandId::new("fork-command").expect("command"),
                    effect_id,
                    parent_source: AgentSourceCoordinate::new("parent").expect("source"),
                    child_source: Some(AgentSourceCoordinate::new("child").expect("child source")),
                    cutoff: AgentForkPoint::Head,
                    child_history_digest: Some(
                        AgentPayloadDigest::new("sha256:history").expect("digest"),
                    ),
                    state: AgentReceiptState::Terminal {
                        outcome: AgentTerminalOutcome::Succeeded,
                    },
                },
                child_applied_surface: None,
            }),
        };
        assert_invariant(validate_lifecycle_effect(&mismatched_outcome));
    }

    #[test]
    fn exact_binding_target_and_surface_contracts_reject_rewrites() {
        let current = valid_facts();

        let mut attachment_rewrite = current.clone();
        attachment_rewrite
            .bindings
            .get_mut(&binding_id())
            .expect("binding")
            .target
            .live_attachment_id =
            agentdash_agent_service_api::CompleteAgentLiveAttachmentId::new("rewritten-attachment")
                .expect("attachment");
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &attachment_rewrite,
        ));

        let mut placement_rewrite = current.clone();
        placement_rewrite
            .bindings
            .get_mut(&binding_id())
            .expect("binding")
            .target
            .placement = crate::CompleteAgentPlacement::InProcess {
            host_incarnation_id: "other-incarnation".to_owned(),
        };
        assert_invariant(validate_complete_agent_host_facts(
            &current,
            &placement_rewrite,
        ));

        let mut invalid_target = CompleteAgentHostFacts::default();
        let mut invalid_binding = current.bindings[&binding_id()].clone();
        invalid_binding.target.verified_profile_digest =
            AgentProfileDigest::new("other-profile").expect("profile");
        invalid_target
            .source_coordinates
            .insert(binding_id(), invalid_binding.source.clone());
        invalid_target
            .bindings
            .insert(binding_id(), invalid_binding);
        assert_invariant(validate_complete_agent_host_facts(
            &CompleteAgentHostFacts::default(),
            &invalid_target,
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
            applied_inspection_state(None),
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
            outcome: AgentAppliedEffectOutcome::Command {
                receipt: AppliedAgentCommandReceipt {
                    terminal: Some(AgentTerminalOutcome::Succeeded),
                    ..applied_command_inspection_receipt()
                },
            },
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

    fn binding_target(instance_id: AgentServiceInstanceId) -> CompleteAgentBindingTarget {
        let profile_digest = AgentProfileDigest::new("profile").expect("profile");
        CompleteAgentBindingTarget {
            logical_instance_id: instance_id,
            live_attachment_id: CompleteAgentLiveAttachmentId::new("attachment")
                .expect("attachment"),
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            verified_build_digest: AgentPayloadDigest::new("sha256:build").expect("build"),
            verified_profile_digest: profile_digest.clone(),
            offer_profile_digest: profile_digest,
            placement: CompleteAgentPlacement::InProcess {
                host_incarnation_id: "fixture-host".to_owned(),
            },
            remote_binding: None,
        }
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

    fn applied_inspection_state(
        terminal: Option<AgentTerminalOutcome>,
    ) -> AgentEffectInspectionState {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command {
                receipt: AppliedAgentCommandReceipt {
                    terminal,
                    ..applied_command_inspection_receipt()
                },
            },
        }
    }

    fn applied_command_inspection_receipt() -> AppliedAgentCommandReceipt {
        AppliedAgentCommandReceipt {
            command_id: AgentCommandId::new("command").expect("command"),
            effect_id: effect_id(),
            source: AgentSourceCoordinate::new("source").expect("source"),
            terminal: None,
            snapshot_revision: None,
            initial_context: None,
        }
    }

    fn valid_facts() -> CompleteAgentHostFacts {
        let service_id = service_id();
        let binding_id = binding_id();
        let effect_id = effect_id();
        let source = AgentSourceCoordinate::new("source").expect("source");
        let profile_digest = AgentProfileDigest::new("profile").expect("profile");
        let target = CompleteAgentBindingTarget {
            logical_instance_id: service_id.clone(),
            live_attachment_id: agentdash_agent_service_api::CompleteAgentLiveAttachmentId::new(
                "fixture-attachment",
            )
            .expect("attachment"),
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            verified_build_digest: AgentPayloadDigest::new("fixture-build").expect("build digest"),
            verified_profile_digest: profile_digest.clone(),
            offer_profile_digest: profile_digest.clone(),
            placement: crate::CompleteAgentPlacement::InProcess {
                host_incarnation_id: "fixture-host".to_owned(),
            },
            remote_binding: None,
        };
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target,
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
            bindings: BTreeMap::from([(binding_id.clone(), binding)]),
            source_coordinates: BTreeMap::from([(binding_id.clone(), source)]),
            callback_routes: BTreeMap::new(),
            revoked_callback_routes: BTreeSet::new(),
            effects: BTreeMap::from([(effect_id, effect)]),
            leases: BTreeMap::from([(binding_id.clone(), lease)]),
            lease_epochs: BTreeMap::from([(binding_id, 1)]),
            runtime_targets: BTreeMap::new(),
            runtime_target_provisionings: BTreeMap::new(),
            runtime_target_recoveries: BTreeMap::new(),
            lifecycle_effects: BTreeMap::new(),
        }
    }
}
