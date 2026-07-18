use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentEffectInspectionState, AgentReceiptState, AgentRuntimeOffer,
    AgentServiceDescriptor, AgentServiceInstanceId, AgentSourceCoordinate, CompleteAgentService,
};
use async_trait::async_trait;
use thiserror::Error;

use crate::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
    CompleteAgentBindingState, CompleteAgentEffectRecord, CompleteAgentEffectState,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompleteAgentHostRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentHostFacts {
    pub service_instances: BTreeMap<AgentServiceInstanceId, AgentServiceDescriptor>,
    pub offers: BTreeMap<AgentServiceInstanceId, AgentRuntimeOffer>,
    pub bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    pub source_coordinates: BTreeMap<CompleteAgentBindingId, AgentSourceCoordinate>,
    pub effects: BTreeMap<AgentEffectIdentity, CompleteAgentEffectRecord>,
    pub leases: BTreeMap<CompleteAgentBindingId, CompleteAgentBindingLease>,
    pub lease_epochs: BTreeMap<CompleteAgentBindingId, u64>,
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
    if candidate.service_instances.len() != candidate.offers.len() {
        return invariant("every service instance must have exactly one Runtime offer");
    }
    for (instance_id, descriptor) in &candidate.service_instances {
        let offer = candidate.offers.get(instance_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "service instance has no Runtime offer".to_owned(),
            }
        })?;
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
        validate_effect_evidence(effect, binding)?;
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
        if !effect_state_can_advance(effect.state, next.state) {
            return invariant("effect observation moved backwards");
        }
        immutable_optional_evidence(&effect.receipt, &next.receipt, "effect receipt")?;
        immutable_optional_evidence(
            &effect.surface_receipt,
            &next.surface_receipt,
            "surface receipt",
        )?;
        immutable_optional_evidence(&effect.inspection, &next.inspection, "effect inspection")?;
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
    Ok(())
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
        && (inspection.effect_id != effect.effect_id
            || inspection
                .command_id
                .as_ref()
                .is_some_and(|command_id| command_id != &effect.command_id)
            || inspection_source(&inspection.state).is_some_and(|source| source != &effect.source)
            || !effect_state_can_advance(inspection_state(&inspection.state), effect.state))
    {
        return invariant("effect inspection coordinates or observation state are inconsistent");
    }
    Ok(())
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
        AgentBindingGeneration, AgentCapabilityProfile, AgentCommandCapability, AgentCommandId,
        AgentCommandReceipt, AgentCompactionMode, AgentConfigurationBoundary, AgentEffectIdentity,
        AgentForkCapability, AgentForkCutoffKind, AgentLifecycleCapability, AgentPayloadDigest,
        AgentProfileDigest, AgentReceiptState, AgentRuntimeOffer, AgentServiceDefinitionId,
        AgentSourceChangeLevel, AgentSurfaceDigest, AgentSurfaceProfile, AgentSurfaceRevision,
        AgentTerminalOutcome, BoundAgentSurface, InitialContextAppliedEvidence,
        InitialContextProfile, SemanticFidelity,
    };
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
            state: CompleteAgentEffectState::Dispatching,
            receipt: None,
            surface_receipt: None,
            inspection: None,
        };
        CompleteAgentHostFacts {
            service_instances: BTreeMap::from([(service_id.clone(), descriptor)]),
            offers: BTreeMap::from([(service_id, offer)]),
            bindings: BTreeMap::from([(binding_id.clone(), binding)]),
            source_coordinates: BTreeMap::from([(binding_id.clone(), source)]),
            effects: BTreeMap::from([(effect_id, effect)]),
            leases: BTreeMap::from([(binding_id.clone(), lease)]),
            lease_epochs: BTreeMap::from([(binding_id, 1)]),
        }
    }
}
