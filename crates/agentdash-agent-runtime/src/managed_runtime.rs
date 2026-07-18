use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangeDelta, ManagedRuntimeCommand,
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandEnvelope, ManagedRuntimeCommandKind,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperation, ManagedRuntimeOperationEvidence,
    ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus, ManagedRuntimePlatformChange,
    ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
    ManagedRuntimeSourceBindingEvidence, ManagedRuntimeUnavailabilityReason, RuntimeChangeSequence,
    RuntimeIdempotencyKey, RuntimeOperationId, RuntimeProjectionRevision, RuntimeSourceRef,
    RuntimeThreadId, SurfaceRevision,
};
use agentdash_agent_service_api::AgentEffectIdentity;
use async_trait::async_trait;
use thiserror::Error;

use crate::{
    CompleteAgentRuntimeIdentityMap, ManagedRuntimeAgentBinding, NormalizedAgentPlatformChange,
    NormalizedAgentProjection, validate_complete_agent_source_facts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedRuntimePendingCommandState {
    Pending,
    Claimed,
    Delivered,
    InspectionRequired,
    Settled,
    Lost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimePendingCommand {
    pub operation_id: RuntimeOperationId,
    pub effect_id: AgentEffectIdentity,
    pub command: ManagedRuntimeCommandEnvelope,
    pub state: ManagedRuntimePendingCommandState,
    pub claim_owner: Option<String>,
    pub claim_epoch: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeOperationRecord {
    pub receipt: ManagedRuntimeOperationReceipt,
    pub command: ManagedRuntimeCommandEnvelope,
    pub operation: ManagedRuntimeOperation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeBindingFact {
    pub source_ref: RuntimeSourceRef,
    pub binding: ManagedRuntimeAgentBinding,
    pub committed_at_revision: RuntimeProjectionRevision,
    pub activated_at_revision: Option<RuntimeProjectionRevision>,
}

impl ManagedRuntimeBindingFact {
    pub fn evidence(&self) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: self.source_ref.clone(),
            committed_at_revision: self.committed_at_revision,
            applied_surface_revision: SurfaceRevision(self.binding.applied_surface.revision.0),
            activated_at_revision: self.activated_at_revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeSettlement {
    pub status: ManagedRuntimeOperationStatus,
    pub evidence: Option<ManagedRuntimeOperationEvidence>,
    pub binding: Option<ManagedRuntimeBindingFact>,
    pub lifecycle: Option<ManagedRuntimeLifecycleStatus>,
    pub pending_state: ManagedRuntimePendingCommandState,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeOutboxEntry {
    pub sequence: RuntimeChangeSequence,
    pub operation_id: Option<RuntimeOperationId>,
    pub change: ManagedRuntimePlatformChange,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManagedRuntimeFacts {
    pub projection: Option<ManagedRuntimeSnapshot>,
    pub binding: Option<ManagedRuntimeBindingFact>,
    pub source_projection: Option<NormalizedAgentProjection>,
    pub source_identities: Option<CompleteAgentRuntimeIdentityMap>,
    pub source_changes: Vec<NormalizedAgentPlatformChange>,
    pub operations: BTreeMap<RuntimeOperationId, ManagedRuntimeOperationRecord>,
    pub idempotency: BTreeMap<RuntimeIdempotencyKey, RuntimeOperationId>,
    pub pending_commands: BTreeMap<RuntimeOperationId, ManagedRuntimePendingCommand>,
    pub changes: Vec<ManagedRuntimePlatformChange>,
    pub outbox: Vec<ManagedRuntimeOutboxEntry>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManagedRuntimeStateRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManagedRuntimeStateSnapshot {
    pub revision: ManagedRuntimeStateRevision,
    pub facts: ManagedRuntimeFacts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRuntimeStateCommit {
    pub thread_id: RuntimeThreadId,
    pub expected_revision: ManagedRuntimeStateRevision,
    pub facts: ManagedRuntimeFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagedRuntimeStateStoreError {
    #[error("managed Runtime state revision conflict")]
    Conflict,
    #[error("managed Runtime state invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("managed Runtime state persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait ManagedRuntimeStateRepository: Send + Sync {
    async fn load(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError>;

    async fn commit(
        &self,
        commit: ManagedRuntimeStateCommit,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError>;
}

pub fn apply_managed_runtime_state_commit(
    current: &mut ManagedRuntimeStateSnapshot,
    commit: ManagedRuntimeStateCommit,
) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
    if commit
        .facts
        .projection
        .as_ref()
        .is_some_and(|projection| projection.thread_id != commit.thread_id)
    {
        return invariant("Runtime commit thread does not match its projection");
    }
    if current.revision != commit.expected_revision {
        if current.facts == commit.facts {
            return Ok(current.clone());
        }
        return Err(ManagedRuntimeStateStoreError::Conflict);
    }
    validate_managed_runtime_facts(&current.facts, &commit.facts)?;
    current.revision =
        ManagedRuntimeStateRevision(current.revision.0.checked_add(1).ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime state revision is exhausted".to_owned(),
            }
        })?);
    current.facts = commit.facts;
    Ok(current.clone())
}

pub fn validate_managed_runtime_facts(
    current: &ManagedRuntimeFacts,
    candidate: &ManagedRuntimeFacts,
) -> Result<(), ManagedRuntimeStateStoreError> {
    validate_projection(candidate)?;
    validate_complete_agent_source_facts(current, candidate)?;
    for (operation_id, operation) in &candidate.operations {
        if operation_id != &operation.receipt.operation_id
            || operation_id != &operation.command.operation_id
            || operation_id != &operation.operation.id
            || operation.receipt.thread_id != operation.command.thread_id
            || operation.receipt.status != operation.operation.status
            || operation.receipt.evidence != operation.operation.evidence
        {
            return invariant("operation identity or thread coordinates are inconsistent");
        }
        validate_operation_evidence(operation)?;
        if candidate
            .idempotency
            .get(&operation.command.idempotency_key)
            != Some(operation_id)
        {
            return invariant("operation idempotency index is missing or inconsistent");
        }
    }
    if candidate
        .idempotency
        .values()
        .any(|operation_id| !candidate.operations.contains_key(operation_id))
    {
        return invariant("idempotency index references an unknown operation");
    }
    for (operation_id, pending) in &candidate.pending_commands {
        if operation_id != &pending.operation_id
            || operation_id != &pending.command.operation_id
            || pending.effect_id.as_str().trim().is_empty()
            || (pending.state == ManagedRuntimePendingCommandState::Claimed
                && pending
                    .claim_owner
                    .as_ref()
                    .is_none_or(|owner| owner.trim().is_empty()))
            || (pending.state != ManagedRuntimePendingCommandState::Claimed
                && pending.claim_owner.is_some())
        {
            return invariant("pending command identity, effect, state, or claim is invalid");
        }
        let Some(operation) = candidate.operations.get(operation_id) else {
            return invariant("pending command references an unknown operation");
        };
        if pending.command != operation.command {
            return invariant("pending command does not match its authoritative operation command");
        }
    }

    if candidate.outbox.len() != candidate.changes.len() {
        return invariant("every Runtime change must have exactly one outbox entry");
    }
    let operation_change_revisions = candidate
        .changes
        .iter()
        .filter_map(|change| match &change.delta {
            ManagedRuntimeChangeDelta::OperationUpserted { operation } => {
                Some((operation.id.clone(), change.revision))
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut last_changed_operations = BTreeMap::new();
    for (entry, change) in candidate.outbox.iter().zip(&candidate.changes) {
        if entry.sequence.0 == 0
            || entry.sequence != entry.change.sequence
            || &entry.change != change
            || entry
                .operation_id
                .as_ref()
                .is_some_and(|operation_id| !candidate.operations.contains_key(operation_id))
        {
            return invariant("Runtime outbox sequence or operation coordinates are invalid");
        }
        match &entry.change.delta {
            ManagedRuntimeChangeDelta::OperationUpserted { operation } => {
                if entry.operation_id.as_ref() != Some(&operation.id)
                    || candidate
                        .operations
                        .get(&operation.id)
                        .is_none_or(|record| {
                            record.operation.turn_id != operation.turn_id
                                || !operation_status_can_reach(
                                    operation.status,
                                    record.operation.status,
                                )
                        })
                {
                    return invariant("operation change outbox does not match operation authority");
                }
                if let Some(previous) =
                    last_changed_operations.insert(operation.id.clone(), operation.clone())
                    && (previous.turn_id != operation.turn_id
                        || !operation_status_can_advance(previous.status, operation.status))
                {
                    return invariant(
                        "operation change history moved backwards or changed payload",
                    );
                }
            }
            _ if entry.operation_id.as_ref().is_some_and(|operation_id| {
                !operation_change_revisions.contains(&(operation_id.clone(), change.revision))
            }) =>
            {
                return invariant(
                    "Runtime change operation association has no same-revision operation fact",
                );
            }
            _ => {}
        }
    }
    for (operation_id, record) in &candidate.operations {
        if last_changed_operations.get(operation_id) != Some(&record.operation) {
            return invariant("operation authority has no exact latest typed Runtime change");
        }
    }

    for (operation_id, operation) in &current.operations {
        let next = candidate.operations.get(operation_id).ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "operation history cannot be removed".to_owned(),
            }
        })?;
        if operation.command != next.command
            || operation.receipt.operation_id != next.receipt.operation_id
            || operation.receipt.thread_id != next.receipt.thread_id
            || operation.receipt.accepted_revision != next.receipt.accepted_revision
            || operation.receipt.duplicate != next.receipt.duplicate
            || !operation_evidence_can_advance(
                operation.receipt.evidence.as_ref(),
                next.receipt.evidence.as_ref(),
            )
            || operation.operation.id != next.operation.id
            || operation.operation.turn_id != next.operation.turn_id
            || !operation_status_can_advance(operation.receipt.status, next.receipt.status)
            || !operation_status_can_advance(operation.operation.status, next.operation.status)
            || !operation_evidence_can_advance(
                operation.operation.evidence.as_ref(),
                next.operation.evidence.as_ref(),
            )
        {
            return invariant("operation command, receipt, or status was rewritten");
        }
    }
    for (key, operation_id) in &current.idempotency {
        if candidate.idempotency.get(key) != Some(operation_id) {
            return invariant("idempotency history cannot be removed or rewritten");
        }
    }
    for (operation_id, pending) in &current.pending_commands {
        let next = candidate
            .pending_commands
            .get(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "pending command history cannot be removed".to_owned(),
            })?;
        if pending.operation_id != next.operation_id
            || pending.effect_id != next.effect_id
            || pending.command != next.command
            || next.claim_epoch < pending.claim_epoch
            || !pending_state_can_advance(pending.state, next.state)
        {
            return invariant("pending command coordinates or delivery state moved backwards");
        }
    }
    if !candidate.outbox.starts_with(&current.outbox) {
        return invariant("Runtime outbox is append-only");
    }
    if !candidate.changes.starts_with(&current.changes) {
        return invariant("Runtime change history is append-only");
    }
    let appended_changes = &candidate.changes[current.changes.len()..];
    if !appended_changes.is_empty() {
        let current_revision = current
            .projection
            .as_ref()
            .map_or(0, |projection| projection.revision.0);
        let candidate_revision = candidate
            .projection
            .as_ref()
            .map_or(0, |projection| projection.revision.0);
        if candidate_revision <= current_revision
            || appended_changes
                .iter()
                .any(|change| change.revision.0 != candidate_revision)
        {
            return invariant(
                "new Runtime changes must share one strictly advanced projection revision",
            );
        }
    }
    match (&current.projection, &candidate.projection) {
        (Some(current), Some(candidate))
            if current.thread_id != candidate.thread_id
                || current.revision.0 > candidate.revision.0
                || current.latest_change_sequence.0 > candidate.latest_change_sequence.0 =>
        {
            return invariant("Runtime projection coordinates moved backwards");
        }
        (Some(_), None) => return invariant("Runtime projection cannot be removed"),
        _ => {}
    }
    Ok(())
}

fn validate_projection(facts: &ManagedRuntimeFacts) -> Result<(), ManagedRuntimeStateStoreError> {
    let Some(projection) = &facts.projection else {
        if facts.operations.is_empty()
            && facts.source_projection.is_none()
            && facts.binding.is_none()
            && facts.source_identities.is_none()
            && facts.source_changes.is_empty()
            && facts.idempotency.is_empty()
            && facts.pending_commands.is_empty()
            && facts.changes.is_empty()
            && facts.outbox.is_empty()
        {
            return Ok(());
        }
        return invariant("managed Runtime facts require one authoritative projection");
    };
    if projection.source_binding.as_ref()
        != facts
            .binding
            .as_ref()
            .map(|binding| binding.evidence())
            .as_ref()
    {
        return invariant("Runtime projection binding evidence does not match binding authority");
    }
    for command in ManagedRuntimeCommandKind::ALL {
        let availability = projection
            .command_availability
            .get(&command)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: format!("Runtime projection is missing {command:?} availability"),
            })?;
        if availability.evidence().decided_at_revision != projection.revision {
            return invariant("Runtime availability evidence does not match projection revision");
        }
    }
    let mut projected_operations = BTreeMap::new();
    for operation in &projection.operations {
        if projected_operations
            .insert(operation.id.clone(), operation)
            .is_some()
        {
            return invariant("Runtime projection contains a duplicate operation identity");
        }
    }
    if projected_operations.len() != facts.operations.len()
        || facts.operations.iter().any(|(operation_id, record)| {
            record.receipt.thread_id != projection.thread_id
                || projected_operations.get(operation_id).copied() != Some(&record.operation)
        })
    {
        return invariant("Runtime projection operation state does not match operation authority");
    }
    let mut previous_sequence = None;
    let mut previous_revision = None;
    for change in &facts.changes {
        if change.thread_id != projection.thread_id
            || change.sequence.0 == 0
            || previous_sequence.is_some_and(|previous| change.sequence.0 != previous + 1)
            || previous_revision.is_some_and(|previous| change.revision.0 < previous)
            || change.revision.0 > projection.revision.0
        {
            return invariant("Runtime change coordinates are invalid");
        }
        previous_sequence = Some(change.sequence.0);
        previous_revision = Some(change.revision.0);
    }
    if previous_sequence.unwrap_or_default() != projection.latest_change_sequence.0 {
        return invariant("Runtime projection head does not match change history");
    }
    if previous_revision.is_some_and(|revision| revision != projection.revision.0) {
        return invariant("latest Runtime change revision does not match projection revision");
    }
    Ok(())
}

fn operation_status_can_advance(
    current: ManagedRuntimeOperationStatus,
    next: ManagedRuntimeOperationStatus,
) -> bool {
    current == next
        || match current {
            ManagedRuntimeOperationStatus::Accepted => {
                matches!(
                    next,
                    ManagedRuntimeOperationStatus::Running
                        | ManagedRuntimeOperationStatus::Failed
                        | ManagedRuntimeOperationStatus::Lost
                )
            }
            ManagedRuntimeOperationStatus::Running => matches!(
                next,
                ManagedRuntimeOperationStatus::Succeeded
                    | ManagedRuntimeOperationStatus::Failed
                    | ManagedRuntimeOperationStatus::Interrupted
                    | ManagedRuntimeOperationStatus::Lost
            ),
            ManagedRuntimeOperationStatus::Succeeded
            | ManagedRuntimeOperationStatus::Failed
            | ManagedRuntimeOperationStatus::Interrupted
            | ManagedRuntimeOperationStatus::Lost => false,
        }
}

fn operation_status_can_reach(
    current: ManagedRuntimeOperationStatus,
    next: ManagedRuntimeOperationStatus,
) -> bool {
    operation_status_can_advance(current, next)
        || current == ManagedRuntimeOperationStatus::Accepted
            && matches!(
                next,
                ManagedRuntimeOperationStatus::Succeeded
                    | ManagedRuntimeOperationStatus::Interrupted
            )
}

fn operation_evidence_can_advance(
    current: Option<&ManagedRuntimeOperationEvidence>,
    next: Option<&ManagedRuntimeOperationEvidence>,
) -> bool {
    if current == next || current.is_none() && next.is_some() {
        return true;
    }
    matches!(
        (current, next),
        (
            Some(ManagedRuntimeOperationEvidence::Fork {
                parent_binding,
                progress:
                    agentdash_agent_runtime_contract::ManagedRuntimeForkProgressEvidence::ChildKnown {
                        child_thread_id,
                        child_source_ref,
                        cutoff,
                        child_history_digest,
                    },
            }),
            Some(ManagedRuntimeOperationEvidence::Fork {
                parent_binding: next_parent_binding,
                progress:
                    agentdash_agent_runtime_contract::ManagedRuntimeForkProgressEvidence::Provisioned {
                        child_thread_id: next_child_thread_id,
                        child_binding,
                        cutoff: next_cutoff,
                        child_history_digest: next_child_history_digest,
                    },
            }),
        ) if parent_binding == next_parent_binding
            && child_thread_id == next_child_thread_id
            && child_source_ref == &child_binding.source_ref
            && cutoff == next_cutoff
            && child_history_digest
                .as_ref()
                .is_none_or(|digest| digest == next_child_history_digest)
    )
}

fn validate_operation_evidence(
    record: &ManagedRuntimeOperationRecord,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let Some(evidence) = &record.operation.evidence else {
        if record.operation.status == ManagedRuntimeOperationStatus::Succeeded
            && matches!(
                record.command.command,
                ManagedRuntimeCommand::Create { .. }
                    | ManagedRuntimeCommand::Resume
                    | ManagedRuntimeCommand::Activate
                    | ManagedRuntimeCommand::Fork { .. }
            )
        {
            return invariant("successful Runtime lifecycle operation requires typed evidence");
        }
        return Ok(());
    };
    match (&record.command.command, record.operation.status, evidence) {
        (
            ManagedRuntimeCommand::Create { initial_context },
            ManagedRuntimeOperationStatus::Succeeded,
            ManagedRuntimeOperationEvidence::Create {
                binding,
                initial_context: applied,
            },
        ) => {
            validate_binding_evidence(binding, false)?;
            match (initial_context, applied) {
                (None, None) => {}
                (Some(package), Some(applied)) => {
                    if !package.validate()
                        || package.package_id != applied.package_id
                        || package.digest != applied.package_digest
                        || package.contributions.len() != applied.contributions.len()
                    {
                        return invariant(
                            "Create context evidence does not match its admitted package",
                        );
                    }
                    for contribution in &package.contributions {
                        let applied = applied
                            .contributions
                            .iter()
                            .find(|applied| applied.contribution_id == contribution.contribution_id)
                            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                                reason: "Create context contribution has no exact applied evidence"
                                    .to_owned(),
                            })?;
                        if applied.contribution_digest != contribution.digest {
                            return invariant(
                                "Create context contribution digest evidence drifted",
                            );
                        }
                        let (kind, provenance) = match &contribution.content {
                            agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::CompactSummary {
                                provenance,
                                ..
                            } => (
                                agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionKind::CompactSummary,
                                provenance,
                            ),
                            agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::WorkflowContext {
                                provenance,
                                ..
                            } => (
                                agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionKind::WorkflowContext,
                                provenance,
                            ),
                            agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::ConstraintSet {
                                provenance,
                                ..
                            } => (
                                agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionKind::ConstraintSet,
                                provenance,
                            ),
                        };
                        if applied.kind != kind
                            || applied.provenance.authority != provenance.authority
                            || applied.provenance.source != provenance.source
                            || applied.provenance.revision != provenance.revision
                            || applied.provenance.digest != provenance.digest
                        {
                            return invariant(
                                "Create context contribution provenance evidence drifted",
                            );
                        }
                        if let agentdash_agent_runtime_contract::ManagedRuntimeInitialContextAppliedFidelity::CanonicalRendered {
                            renderer_version,
                            ..
                        } = &applied.fidelity
                            && renderer_version.trim().is_empty()
                        {
                            return invariant(
                                "canonical rendered context evidence requires a renderer version",
                            );
                        }
                    }
                }
                _ => return invariant("Create context applied evidence is incomplete"),
            }
        }
        (
            ManagedRuntimeCommand::Resume,
            ManagedRuntimeOperationStatus::Succeeded,
            ManagedRuntimeOperationEvidence::Resume { binding },
        ) => validate_binding_evidence(binding, false)?,
        (
            ManagedRuntimeCommand::Activate,
            ManagedRuntimeOperationStatus::Succeeded,
            ManagedRuntimeOperationEvidence::Activate { binding },
        ) => validate_binding_evidence(binding, true)?,
        (
            ManagedRuntimeCommand::Fork {
                child_thread_id,
                through_completed_turn_id,
            },
            status,
            ManagedRuntimeOperationEvidence::Fork {
                parent_binding,
                progress,
            },
        ) => {
            validate_binding_evidence(parent_binding, true)?;
            let expected_cutoff = through_completed_turn_id.as_ref().map_or(
                agentdash_agent_runtime_contract::ManagedRuntimeForkCutoff::Head,
                |turn_id| {
                    agentdash_agent_runtime_contract::ManagedRuntimeForkCutoff::CompletedTurn {
                        turn_id: turn_id.clone(),
                    }
                },
            );
            match progress {
                agentdash_agent_runtime_contract::ManagedRuntimeForkProgressEvidence::ChildKnown {
                    child_thread_id: evidence_child,
                    cutoff,
                    ..
                } => {
                    if !matches!(
                        status,
                        ManagedRuntimeOperationStatus::Running
                            | ManagedRuntimeOperationStatus::Lost
                    )
                        || evidence_child != child_thread_id
                        || cutoff != &expected_cutoff
                    {
                        return invariant("Fork child-known evidence has invalid coordinates");
                    }
                }
                agentdash_agent_runtime_contract::ManagedRuntimeForkProgressEvidence::Provisioned {
                    child_thread_id: evidence_child,
                    child_binding,
                    cutoff,
                    ..
                } => {
                    if status != ManagedRuntimeOperationStatus::Succeeded
                        || evidence_child != child_thread_id
                        || cutoff != &expected_cutoff
                    {
                        return invariant("Fork provisioned evidence has invalid coordinates");
                    }
                    validate_binding_evidence(child_binding, false)?;
                }
            }
        }
        _ => return invariant("operation evidence does not match its command or terminal status"),
    }
    Ok(())
}

fn validate_binding_evidence(
    binding: &ManagedRuntimeSourceBindingEvidence,
    requires_active: bool,
) -> Result<(), ManagedRuntimeStateStoreError> {
    if binding.committed_at_revision.0 == 0
        || binding.applied_surface_revision.0 == 0
        || requires_active && binding.activated_at_revision.is_none()
        || binding
            .activated_at_revision
            .is_some_and(|revision| revision < binding.committed_at_revision)
    {
        return invariant("Runtime source binding evidence has invalid revisions");
    }
    Ok(())
}

fn pending_state_can_advance(
    current: ManagedRuntimePendingCommandState,
    next: ManagedRuntimePendingCommandState,
) -> bool {
    current == next
        || match current {
            ManagedRuntimePendingCommandState::Pending => matches!(
                next,
                ManagedRuntimePendingCommandState::Claimed
                    | ManagedRuntimePendingCommandState::Lost
            ),
            ManagedRuntimePendingCommandState::Claimed => matches!(
                next,
                ManagedRuntimePendingCommandState::Pending
                    | ManagedRuntimePendingCommandState::Delivered
                    | ManagedRuntimePendingCommandState::InspectionRequired
                    | ManagedRuntimePendingCommandState::Settled
                    | ManagedRuntimePendingCommandState::Lost
            ),
            ManagedRuntimePendingCommandState::Delivered => matches!(
                next,
                ManagedRuntimePendingCommandState::InspectionRequired
                    | ManagedRuntimePendingCommandState::Settled
                    | ManagedRuntimePendingCommandState::Lost
            ),
            ManagedRuntimePendingCommandState::InspectionRequired => matches!(
                next,
                ManagedRuntimePendingCommandState::Pending
                    | ManagedRuntimePendingCommandState::Settled
                    | ManagedRuntimePendingCommandState::Lost
            ),
            ManagedRuntimePendingCommandState::Settled
            | ManagedRuntimePendingCommandState::Lost => false,
        }
}

fn invariant<T>(reason: &str) -> Result<T, ManagedRuntimeStateStoreError> {
    Err(ManagedRuntimeStateStoreError::Invariant {
        reason: reason.to_owned(),
    })
}

pub struct ManagedRuntimeCoordinator<R: ?Sized> {
    repository: Arc<R>,
}

impl<R: ?Sized> ManagedRuntimeCoordinator<R>
where
    R: ManagedRuntimeStateRepository,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn accept(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        effect_id: AgentEffectIdentity,
        captured_at_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeStateStoreError> {
        let mut snapshot = self.repository.load(&command.thread_id).await?;
        if let Some(operation_id) = snapshot.facts.idempotency.get(&command.idempotency_key) {
            let operation = snapshot
                .facts
                .operations
                .get(operation_id)
                .expect("validated idempotency index");
            if operation.command != command {
                return invariant("idempotency key was reused for another command");
            }
            let mut receipt = operation.receipt.clone();
            receipt.duplicate = true;
            return Ok(receipt);
        }
        if snapshot.facts.projection.is_none()
            && matches!(command.command, ManagedRuntimeCommand::Create { .. })
        {
            if command.expected_revision.is_some()
                || snapshot.facts != ManagedRuntimeFacts::default()
            {
                return invariant(
                    "Create requires one empty Runtime thread and no expected revision",
                );
            }
            snapshot.facts.projection = Some(initial_projection(
                command.thread_id.clone(),
                captured_at_ms,
            ));
        }
        let projection = snapshot.facts.projection.as_ref().ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime thread has no admitted projection".to_owned(),
            }
        })?;
        if projection.thread_id != command.thread_id {
            return invariant("command thread does not match the admitted projection");
        }
        let expected_revision = if matches!(command.command, ManagedRuntimeCommand::Create { .. }) {
            command.expected_revision.unwrap_or_default()
        } else {
            command
                .expected_revision
                .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                    reason: "command requires an expected Runtime revision".to_owned(),
                })?
        };
        if expected_revision != projection.revision {
            return invariant("command expected revision does not match the admitted projection");
        }
        if !matches!(
            projection.command_availability.get(&command.command.kind()),
            Some(ManagedRuntimeCommandAvailability::Available { .. })
        ) {
            return invariant("command is unavailable in the admitted projection");
        }
        if snapshot
            .facts
            .operations
            .contains_key(&command.operation_id)
            || snapshot
                .facts
                .pending_commands
                .contains_key(&command.operation_id)
        {
            return invariant("operation identity was reused");
        }

        let accepted_revision =
            RuntimeProjectionRevision(projection.revision.0.checked_add(1).ok_or_else(|| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: "managed Runtime projection revision is exhausted".to_owned(),
                }
            })?);
        let receipt = ManagedRuntimeOperationReceipt {
            operation_id: command.operation_id.clone(),
            thread_id: command.thread_id.clone(),
            accepted_revision,
            status: ManagedRuntimeOperationStatus::Accepted,
            evidence: None,
            duplicate: false,
        };
        let operation = ManagedRuntimeOperation {
            id: receipt.operation_id.clone(),
            turn_id: None,
            status: receipt.status,
            evidence: None,
        };
        let mut facts = snapshot.facts;
        facts.idempotency.insert(
            command.idempotency_key.clone(),
            command.operation_id.clone(),
        );
        facts.operations.insert(
            command.operation_id.clone(),
            ManagedRuntimeOperationRecord {
                receipt: receipt.clone(),
                command: command.clone(),
                operation: operation.clone(),
            },
        );
        facts.pending_commands.insert(
            command.operation_id.clone(),
            ManagedRuntimePendingCommand {
                operation_id: command.operation_id.clone(),
                effect_id,
                command,
                state: ManagedRuntimePendingCommandState::Pending,
                claim_owner: None,
                claim_epoch: 0,
            },
        );
        let projection = facts
            .projection
            .as_mut()
            .expect("projection was validated before mutation");
        projection.revision = accepted_revision;
        projection.captured_at_ms = captured_at_ms;
        projection.operations.push(operation.clone());

        let mut deltas = vec![ManagedRuntimeChangeDelta::OperationUpserted { operation }];
        for command_kind in ManagedRuntimeCommandKind::ALL {
            let previous = projection
                .command_availability
                .get(&command_kind)
                .expect("availability was validated");
            let availability = ManagedRuntimeCommandAvailability::Unavailable {
                reason: ManagedRuntimeUnavailabilityReason::OperationInFlight,
                evidence: ManagedRuntimeAvailabilityEvidence {
                    decided_at_revision: accepted_revision,
                    blocking_operation_id: Some(receipt.operation_id.clone()),
                    bound_surface_revision: previous.evidence().bound_surface_revision,
                    applied_surface_revision: previous.evidence().applied_surface_revision,
                },
            };
            projection
                .command_availability
                .insert(command_kind, availability.clone());
            deltas.push(ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
                command: command_kind,
                availability,
            });
        }
        let mut next_sequence = projection.latest_change_sequence.0;
        for delta in deltas {
            next_sequence = next_sequence.checked_add(1).ok_or_else(|| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: "managed Runtime change sequence is exhausted".to_owned(),
                }
            })?;
            let change = ManagedRuntimePlatformChange {
                thread_id: receipt.thread_id.clone(),
                sequence: RuntimeChangeSequence(next_sequence),
                revision: accepted_revision,
                delta,
            };
            facts.changes.push(change.clone());
            facts.outbox.push(ManagedRuntimeOutboxEntry {
                sequence: change.sequence,
                operation_id: Some(receipt.operation_id.clone()),
                change,
            });
        }
        facts
            .projection
            .as_mut()
            .expect("projection exists")
            .latest_change_sequence = RuntimeChangeSequence(next_sequence);
        self.repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: receipt.thread_id.clone(),
                expected_revision: snapshot.revision,
                facts,
            })
            .await?;
        Ok(receipt)
    }

    pub async fn mark_running(
        &self,
        thread_id: &RuntimeThreadId,
        operation_id: &RuntimeOperationId,
        claim_owner: String,
        captured_at_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeStateStoreError> {
        if claim_owner.trim().is_empty() {
            return invariant("Runtime dispatch claim owner must not be empty");
        }
        let snapshot = self.repository.load(thread_id).await?;
        let mut facts = snapshot.facts;
        let record = facts.operations.get(operation_id).ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime operation was not found".to_owned(),
            }
        })?;
        if record.operation.status != ManagedRuntimeOperationStatus::Accepted {
            let receipt = record.receipt.clone();
            if record.operation.status == ManagedRuntimeOperationStatus::Running
                && facts
                    .pending_commands
                    .get(operation_id)
                    .is_some_and(|pending| {
                        pending.state == ManagedRuntimePendingCommandState::Pending
                    })
            {
                let pending = facts
                    .pending_commands
                    .get_mut(operation_id)
                    .expect("pending command was read from the same facts");
                pending.state = ManagedRuntimePendingCommandState::Claimed;
                pending.claim_owner = Some(claim_owner);
                pending.claim_epoch = pending.claim_epoch.checked_add(1).ok_or_else(|| {
                    ManagedRuntimeStateStoreError::Invariant {
                        reason: "managed Runtime dispatch claim epoch is exhausted".to_owned(),
                    }
                })?;
                self.repository
                    .commit(ManagedRuntimeStateCommit {
                        thread_id: thread_id.clone(),
                        expected_revision: snapshot.revision,
                        facts,
                    })
                    .await?;
            }
            return Ok(receipt);
        }
        let record = facts
            .operations
            .get_mut(operation_id)
            .expect("operation was read from the same facts");
        record.operation.status = ManagedRuntimeOperationStatus::Running;
        record.receipt.status = ManagedRuntimeOperationStatus::Running;
        let operation = record.operation.clone();
        let receipt = record.receipt.clone();
        let pending = facts
            .pending_commands
            .get_mut(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime pending operation was not found".to_owned(),
            })?;
        pending.state = ManagedRuntimePendingCommandState::Claimed;
        pending.claim_owner = Some(claim_owner);
        pending.claim_epoch = pending.claim_epoch.checked_add(1).ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime dispatch claim epoch is exhausted".to_owned(),
            }
        })?;
        let revision = next_projection_revision(&facts)?;
        let mut deltas = vec![ManagedRuntimeChangeDelta::OperationUpserted { operation }];
        let availability = facts
            .projection
            .as_ref()
            .expect("operation facts require a projection")
            .command_availability
            .clone();
        for (command, mut availability) in availability {
            availability.evidence_mut().decided_at_revision = revision;
            facts
                .projection
                .as_mut()
                .expect("operation facts require a projection")
                .command_availability
                .insert(command, availability.clone());
            deltas.push(ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
                command,
                availability,
            });
        }
        append_runtime_transition(&mut facts, operation_id, captured_at_ms, deltas)?;
        self.repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: snapshot.revision,
                facts,
            })
            .await?;
        Ok(receipt)
    }

    pub async fn settle(
        &self,
        thread_id: &RuntimeThreadId,
        operation_id: &RuntimeOperationId,
        settlement: ManagedRuntimeSettlement,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeStateStoreError> {
        let ManagedRuntimeSettlement {
            status,
            evidence,
            binding,
            lifecycle,
            pending_state,
            captured_at_ms,
        } = settlement;
        let snapshot = self.repository.load(thread_id).await?;
        let mut facts = snapshot.facts;
        let command_kind = facts
            .operations
            .get(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime operation was not found".to_owned(),
            })?
            .command
            .command
            .kind();
        {
            let record = facts
                .operations
                .get_mut(operation_id)
                .expect("operation was read from the same facts");
            if !operation_status_can_advance(record.operation.status, status) {
                if record.operation.status == status
                    && record.operation.evidence.as_ref() == evidence.as_ref()
                {
                    return Ok(record.receipt.clone());
                }
                return invariant("managed Runtime operation settlement moved backwards");
            }
            record.operation.status = status;
            record.operation.evidence = evidence.clone();
            record.receipt.status = status;
            record.receipt.evidence = evidence;
        }
        let pending = facts
            .pending_commands
            .get_mut(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime pending operation was not found".to_owned(),
            })?;
        pending.state = pending_state;
        pending.claim_owner = None;
        if let Some(binding) = binding {
            facts.binding = Some(binding);
        }
        let operation = facts
            .operations
            .get(operation_id)
            .expect("operation exists")
            .operation
            .clone();
        let receipt = facts
            .operations
            .get(operation_id)
            .expect("operation exists")
            .receipt
            .clone();
        let mut deltas = vec![ManagedRuntimeChangeDelta::OperationUpserted { operation }];
        if facts
            .binding
            .as_ref()
            .map(ManagedRuntimeBindingFact::evidence)
            != facts
                .projection
                .as_ref()
                .and_then(|projection| projection.source_binding.clone())
        {
            deltas.push(ManagedRuntimeChangeDelta::SourceBindingChanged {
                binding: facts
                    .binding
                    .as_ref()
                    .map(ManagedRuntimeBindingFact::evidence),
            });
        }
        let revision = next_projection_revision(&facts)?;
        {
            let projection = facts
                .projection
                .as_mut()
                .expect("operation facts require a projection");
            projection.source_binding = facts
                .binding
                .as_ref()
                .map(ManagedRuntimeBindingFact::evidence);
            if let Some(lifecycle) = lifecycle {
                projection.lifecycle = lifecycle;
                deltas.push(ManagedRuntimeChangeDelta::RuntimeLifecycleChanged { lifecycle });
            }
        }
        append_settled_availability(
            &mut facts,
            operation_id,
            command_kind,
            revision,
            &mut deltas,
        )?;
        append_runtime_transition(&mut facts, operation_id, captured_at_ms, deltas)?;
        self.repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: snapshot.revision,
                facts,
            })
            .await?;
        Ok(receipt)
    }

    pub async fn reset_for_redispatch(
        &self,
        thread_id: &RuntimeThreadId,
        operation_id: &RuntimeOperationId,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeStateStoreError> {
        let snapshot = self.repository.load(thread_id).await?;
        let mut facts = snapshot.facts;
        let pending = facts
            .pending_commands
            .get_mut(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime pending operation was not found".to_owned(),
            })?;
        if pending.state != ManagedRuntimePendingCommandState::InspectionRequired {
            return invariant("only a confirmed NotApplied effect can be reset for redispatch");
        }
        pending.state = ManagedRuntimePendingCommandState::Pending;
        pending.claim_owner = None;
        let receipt = facts
            .operations
            .get(operation_id)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime operation was not found".to_owned(),
            })?
            .receipt
            .clone();
        self.repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: snapshot.revision,
                facts,
            })
            .await?;
        Ok(receipt)
    }

    pub async fn provision_child(
        &self,
        child_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeBindingFact,
        captured_at_ms: u64,
    ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeStateStoreError> {
        let snapshot = self.repository.load(&child_thread_id).await?;
        if let Some(projection) = &snapshot.facts.projection {
            if snapshot.facts.binding.as_ref() == Some(&binding) {
                return Ok(projection.clone());
            }
            return invariant("fork child Runtime thread is already provisioned differently");
        }
        if snapshot.facts != ManagedRuntimeFacts::default() {
            return invariant("fork child Runtime thread contains partial facts");
        }
        let mut facts = ManagedRuntimeFacts {
            projection: Some(initial_projection(child_thread_id.clone(), captured_at_ms)),
            binding: Some(binding.clone()),
            ..ManagedRuntimeFacts::default()
        };
        let projection = facts
            .projection
            .as_mut()
            .expect("child projection was initialized");
        projection.revision = RuntimeProjectionRevision(1);
        projection.source_binding = Some(binding.evidence());
        projection.command_availability = provisioning_availability(
            RuntimeProjectionRevision(1),
            None,
            Some(binding.evidence().applied_surface_revision),
        );
        let mut deltas = vec![ManagedRuntimeChangeDelta::SourceBindingChanged {
            binding: Some(binding.evidence()),
        }];
        for (command, availability) in &projection.command_availability {
            deltas.push(ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
                command: *command,
                availability: availability.clone(),
            });
        }
        let mut sequence = 0_u64;
        for delta in deltas {
            sequence = sequence.checked_add(1).ok_or_else(|| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: "fork child change sequence is exhausted".to_owned(),
                }
            })?;
            let change = ManagedRuntimePlatformChange {
                thread_id: child_thread_id.clone(),
                sequence: RuntimeChangeSequence(sequence),
                revision: RuntimeProjectionRevision(1),
                delta,
            };
            facts.changes.push(change.clone());
            facts.outbox.push(ManagedRuntimeOutboxEntry {
                sequence: change.sequence,
                operation_id: None,
                change,
            });
        }
        facts
            .projection
            .as_mut()
            .expect("child projection exists")
            .latest_change_sequence = RuntimeChangeSequence(sequence);
        let committed = self
            .repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: child_thread_id,
                expected_revision: snapshot.revision,
                facts,
            })
            .await?;
        committed
            .facts
            .projection
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "committed fork child projection is missing".to_owned(),
            })
    }
}

fn next_projection_revision(
    facts: &ManagedRuntimeFacts,
) -> Result<RuntimeProjectionRevision, ManagedRuntimeStateStoreError> {
    let revision = facts
        .projection
        .as_ref()
        .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
            reason: "managed Runtime projection is missing".to_owned(),
        })?
        .revision
        .0
        .checked_add(1)
        .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
            reason: "managed Runtime projection revision is exhausted".to_owned(),
        })?;
    Ok(RuntimeProjectionRevision(revision))
}

fn append_runtime_transition(
    facts: &mut ManagedRuntimeFacts,
    operation_id: &RuntimeOperationId,
    captured_at_ms: u64,
    deltas: Vec<ManagedRuntimeChangeDelta>,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let revision = next_projection_revision(facts)?;
    let projection = facts
        .projection
        .as_mut()
        .expect("projection revision was read from the same facts");
    projection.revision = revision;
    projection.captured_at_ms = captured_at_ms;
    for availability in projection.command_availability.values_mut() {
        availability.evidence_mut().decided_at_revision = revision;
    }
    let mut sequence = projection.latest_change_sequence.0;
    for delta in deltas {
        sequence =
            sequence
                .checked_add(1)
                .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                    reason: "managed Runtime change sequence is exhausted".to_owned(),
                })?;
        let change = ManagedRuntimePlatformChange {
            thread_id: projection.thread_id.clone(),
            sequence: RuntimeChangeSequence(sequence),
            revision,
            delta,
        };
        facts.changes.push(change.clone());
        facts.outbox.push(ManagedRuntimeOutboxEntry {
            sequence: change.sequence,
            operation_id: Some(operation_id.clone()),
            change,
        });
    }
    projection.latest_change_sequence = RuntimeChangeSequence(sequence);
    if let Some(operation) = facts.operations.get(operation_id) {
        let projected = projection
            .operations
            .iter_mut()
            .find(|projected| projected.id == *operation_id)
            .expect("operation projection was committed during acceptance");
        *projected = operation.operation.clone();
    }
    Ok(())
}

fn append_settled_availability(
    facts: &mut ManagedRuntimeFacts,
    operation_id: &RuntimeOperationId,
    _command_kind: ManagedRuntimeCommandKind,
    revision: RuntimeProjectionRevision,
    deltas: &mut Vec<ManagedRuntimeChangeDelta>,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let projection =
        facts
            .projection
            .as_mut()
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime projection is missing".to_owned(),
            })?;
    let blocking = facts.operations.values().find_map(|record| {
        matches!(
            record.operation.status,
            ManagedRuntimeOperationStatus::Accepted | ManagedRuntimeOperationStatus::Running
        )
        .then(|| record.operation.id.clone())
    });
    let applied_surface_revision = facts
        .binding
        .as_ref()
        .map(|binding| binding.evidence().applied_surface_revision);
    let availability = if blocking.is_some() {
        ManagedRuntimeCommandKind::ALL
            .into_iter()
            .map(|command| {
                (
                    command,
                    ManagedRuntimeCommandAvailability::Unavailable {
                        reason: ManagedRuntimeUnavailabilityReason::OperationInFlight,
                        evidence: ManagedRuntimeAvailabilityEvidence {
                            decided_at_revision: revision,
                            blocking_operation_id: blocking.clone(),
                            bound_surface_revision: applied_surface_revision,
                            applied_surface_revision,
                        },
                    },
                )
            })
            .collect()
    } else if projection.lifecycle == ManagedRuntimeLifecycleStatus::Active {
        active_availability(
            revision,
            projection.active_turn_id.is_some(),
            !projection.interactions.is_empty(),
            applied_surface_revision,
        )
    } else {
        provisioning_availability(revision, None, applied_surface_revision)
    };
    for (command, availability) in availability {
        projection
            .command_availability
            .insert(command, availability.clone());
        deltas.push(ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
            command,
            availability,
        });
    }
    let _ = operation_id;
    Ok(())
}

fn provisioning_availability(
    revision: RuntimeProjectionRevision,
    blocking_operation_id: Option<RuntimeOperationId>,
    applied_surface_revision: Option<SurfaceRevision>,
) -> BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability> {
    ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            let evidence = ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: revision,
                blocking_operation_id: blocking_operation_id.clone(),
                bound_surface_revision: applied_surface_revision,
                applied_surface_revision,
            };
            let availability = if matches!(
                command,
                ManagedRuntimeCommandKind::Activate | ManagedRuntimeCommandKind::Resume
            ) {
                ManagedRuntimeCommandAvailability::Available { evidence }
            } else {
                ManagedRuntimeCommandAvailability::Unavailable {
                    reason: ManagedRuntimeUnavailabilityReason::RuntimeNotActive,
                    evidence,
                }
            };
            (command, availability)
        })
        .collect()
}

fn active_availability(
    revision: RuntimeProjectionRevision,
    has_active_turn: bool,
    has_pending_interaction: bool,
    applied_surface_revision: Option<SurfaceRevision>,
) -> BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability> {
    ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            let evidence = ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: revision,
                blocking_operation_id: None,
                bound_surface_revision: applied_surface_revision,
                applied_surface_revision,
            };
            let available = match command {
                ManagedRuntimeCommandKind::SubmitInput
                | ManagedRuntimeCommandKind::RequestCompaction
                | ManagedRuntimeCommandKind::Close
                | ManagedRuntimeCommandKind::Fork => true,
                ManagedRuntimeCommandKind::Steer | ManagedRuntimeCommandKind::Interrupt => {
                    has_active_turn
                }
                ManagedRuntimeCommandKind::ResolveInteraction => has_pending_interaction,
                ManagedRuntimeCommandKind::Create
                | ManagedRuntimeCommandKind::Resume
                | ManagedRuntimeCommandKind::Activate => false,
            };
            let availability = if available {
                ManagedRuntimeCommandAvailability::Available { evidence }
            } else {
                ManagedRuntimeCommandAvailability::Unavailable {
                    reason: ManagedRuntimeUnavailabilityReason::RuntimeNotActive,
                    evidence,
                }
            };
            (command, availability)
        })
        .collect()
}

fn initial_projection(thread_id: RuntimeThreadId, captured_at_ms: u64) -> ManagedRuntimeSnapshot {
    let revision = RuntimeProjectionRevision(0);
    let command_availability = ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            let evidence = ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: revision,
                blocking_operation_id: None,
                bound_surface_revision: None,
                applied_surface_revision: None,
            };
            let availability = if command == ManagedRuntimeCommandKind::Create {
                ManagedRuntimeCommandAvailability::Available { evidence }
            } else {
                ManagedRuntimeCommandAvailability::Unavailable {
                    reason: ManagedRuntimeUnavailabilityReason::RuntimeNotActive,
                    evidence,
                }
            };
            (command, availability)
        })
        .collect();
    ManagedRuntimeSnapshot {
        thread_id,
        revision,
        latest_change_sequence: RuntimeChangeSequence(0),
        captured_at_ms,
        lifecycle: ManagedRuntimeLifecycleStatus::Provisioning,
        active_turn_id: None,
        turns: Vec::new(),
        items: Vec::new(),
        interactions: Vec::new(),
        operations: Vec::new(),
        source_binding: None,
        authority: ManagedRuntimeProjectionAuthority::RuntimeDerived,
        fidelity: ManagedRuntimeProjectionFidelity::Observed,
        command_availability,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeCommand, ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, RuntimeIdempotencyKey,
    };
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FixtureRepository {
        snapshots: Mutex<BTreeMap<RuntimeThreadId, ManagedRuntimeStateSnapshot>>,
    }

    #[async_trait]
    impl ManagedRuntimeStateRepository for FixtureRepository {
        async fn load(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            Ok(self
                .snapshots
                .lock()
                .await
                .get(thread_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn commit(
            &self,
            commit: ManagedRuntimeStateCommit,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            let mut snapshots = self.snapshots.lock().await;
            let snapshot = snapshots.entry(commit.thread_id.clone()).or_default();
            apply_managed_runtime_state_commit(snapshot, commit)
        }
    }

    fn command() -> ManagedRuntimeCommandEnvelope {
        ManagedRuntimeCommandEnvelope {
            operation_id: RuntimeOperationId::new("operation").expect("operation"),
            idempotency_key: RuntimeIdempotencyKey::new("idempotency").expect("idempotency"),
            thread_id: RuntimeThreadId::new("thread").expect("thread"),
            expected_revision: Some(RuntimeProjectionRevision(7)),
            command: ManagedRuntimeCommand::SubmitInput {
                content: vec![
                    agentdash_agent_runtime_contract::ManagedRuntimeContentBlock::Text {
                        text: "hello".to_owned(),
                    },
                ],
            },
        }
    }

    fn projection() -> ManagedRuntimeSnapshot {
        let mut command_availability = BTreeMap::new();
        for command in ManagedRuntimeCommandKind::ALL {
            command_availability.insert(
                command,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: ManagedRuntimeAvailabilityEvidence {
                        decided_at_revision: RuntimeProjectionRevision(7),
                        blocking_operation_id: None,
                        bound_surface_revision: None,
                        applied_surface_revision: None,
                    },
                },
            );
        }
        ManagedRuntimeSnapshot {
            thread_id: RuntimeThreadId::new("thread").expect("thread"),
            revision: RuntimeProjectionRevision(7),
            latest_change_sequence: RuntimeChangeSequence(0),
            captured_at_ms: 1,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            operations: Vec::new(),
            source_binding: None,
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability,
        }
    }

    fn operation_facts() -> ManagedRuntimeFacts {
        let command = command();
        let receipt = ManagedRuntimeOperationReceipt {
            operation_id: command.operation_id.clone(),
            thread_id: command.thread_id.clone(),
            accepted_revision: RuntimeProjectionRevision(7),
            status: ManagedRuntimeOperationStatus::Accepted,
            evidence: None,
            duplicate: false,
        };
        let operation = ManagedRuntimeOperation {
            id: command.operation_id.clone(),
            turn_id: None,
            status: receipt.status,
            evidence: None,
        };
        let change = ManagedRuntimePlatformChange {
            thread_id: command.thread_id.clone(),
            sequence: RuntimeChangeSequence(1),
            revision: RuntimeProjectionRevision(7),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: operation.clone(),
            },
        };
        ManagedRuntimeFacts {
            projection: Some({
                let mut projection = projection();
                projection.operations.push(operation.clone());
                projection.latest_change_sequence = RuntimeChangeSequence(1);
                projection
            }),
            operations: BTreeMap::from([(
                command.operation_id.clone(),
                ManagedRuntimeOperationRecord {
                    receipt,
                    command: command.clone(),
                    operation,
                },
            )]),
            idempotency: BTreeMap::from([(
                command.idempotency_key.clone(),
                command.operation_id.clone(),
            )]),
            pending_commands: BTreeMap::from([(
                command.operation_id.clone(),
                ManagedRuntimePendingCommand {
                    operation_id: command.operation_id.clone(),
                    effect_id: AgentEffectIdentity::new("effect").expect("effect"),
                    command: command.clone(),
                    state: ManagedRuntimePendingCommandState::Pending,
                    claim_owner: None,
                    claim_epoch: 0,
                },
            )]),
            changes: vec![change.clone()],
            outbox: vec![ManagedRuntimeOutboxEntry {
                sequence: RuntimeChangeSequence(1),
                operation_id: Some(command.operation_id),
                change,
            }],
            ..ManagedRuntimeFacts::default()
        }
    }

    fn source_binding_evidence(
        source_ref: &str,
    ) -> agentdash_agent_runtime_contract::ManagedRuntimeSourceBindingEvidence {
        agentdash_agent_runtime_contract::ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new(source_ref).expect("source ref"),
            committed_at_revision: RuntimeProjectionRevision(7),
            applied_surface_revision: agentdash_agent_runtime_contract::SurfaceRevision(4),
            activated_at_revision: Some(RuntimeProjectionRevision(7)),
        }
    }

    fn lifecycle_succeeded_without_evidence(
        command_kind: ManagedRuntimeCommand,
    ) -> ManagedRuntimeFacts {
        let mut facts = operation_facts();
        let operation_id = command().operation_id;
        let record = facts.operations.get_mut(&operation_id).expect("operation");
        record.command.command = command_kind.clone();
        record.receipt.status = ManagedRuntimeOperationStatus::Succeeded;
        record.operation.status = ManagedRuntimeOperationStatus::Succeeded;
        facts
            .pending_commands
            .get_mut(&operation_id)
            .expect("pending")
            .command
            .command = command_kind;
        facts.projection.as_mut().expect("projection").operations[0] = record.operation.clone();
        let change = ManagedRuntimePlatformChange {
            thread_id: record.receipt.thread_id.clone(),
            sequence: RuntimeChangeSequence(1),
            revision: RuntimeProjectionRevision(7),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: record.operation.clone(),
            },
        };
        facts.changes[0] = change.clone();
        facts.outbox[0].change = change;
        facts
    }

    #[tokio::test]
    async fn command_acceptance_atomically_writes_operation_pending_intent_and_outbox() {
        let repository = Arc::new(FixtureRepository::default());
        repository.snapshots.lock().await.insert(
            RuntimeThreadId::new("thread").expect("thread"),
            ManagedRuntimeStateSnapshot {
                revision: ManagedRuntimeStateRevision(1),
                facts: ManagedRuntimeFacts {
                    projection: Some(projection()),
                    ..ManagedRuntimeFacts::default()
                },
            },
        );
        let coordinator = ManagedRuntimeCoordinator::new(repository.clone());
        let command = command();
        let effect_id = AgentEffectIdentity::new("effect").expect("effect");

        let first = coordinator
            .accept(command.clone(), effect_id.clone(), 2)
            .await
            .expect("accept command");
        let duplicate = coordinator
            .accept(command.clone(), effect_id, 3)
            .await
            .expect("duplicate");

        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        let snapshot = repository
            .load(&command.thread_id)
            .await
            .expect("load state");
        assert_eq!(snapshot.facts.operations.len(), 1);
        assert_eq!(snapshot.facts.pending_commands.len(), 1);
        assert_eq!(snapshot.facts.changes.len(), 11);
        assert_eq!(snapshot.facts.outbox.len(), 11);
        let projection = snapshot.facts.projection.expect("projection");
        assert_eq!(projection.revision, RuntimeProjectionRevision(8));
        assert_eq!(projection.latest_change_sequence, RuntimeChangeSequence(11));
        assert_eq!(projection.operations.len(), 1);
    }

    #[test]
    fn commit_rejects_projection_from_another_runtime_thread() {
        let mut current = ManagedRuntimeStateSnapshot::default();
        let error = apply_managed_runtime_state_commit(
            &mut current,
            ManagedRuntimeStateCommit {
                thread_id: RuntimeThreadId::new("other-thread").expect("thread"),
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: ManagedRuntimeFacts {
                    projection: Some(projection()),
                    ..ManagedRuntimeFacts::default()
                },
            },
        )
        .expect_err("cross-thread projection must be rejected");

        assert!(matches!(
            error,
            ManagedRuntimeStateStoreError::Invariant { .. }
        ));
        assert_eq!(current, ManagedRuntimeStateSnapshot::default());
    }

    #[test]
    fn facts_reject_removed_idempotency_pending_or_outbox_history() {
        let current = operation_facts();
        let mut removed = current.clone();
        removed.idempotency.clear();
        assert!(validate_managed_runtime_facts(&current, &removed).is_err());
        let mut removed = current.clone();
        removed.pending_commands.clear();
        assert!(validate_managed_runtime_facts(&current, &removed).is_err());
        let mut removed = current.clone();
        removed.outbox.clear();
        assert!(validate_managed_runtime_facts(&current, &removed).is_err());
    }

    #[test]
    fn facts_reject_pending_command_rebinding() {
        let current = operation_facts();
        let operation_id = command().operation_id;

        let mut changed_thread = current.clone();
        changed_thread
            .pending_commands
            .get_mut(&operation_id)
            .expect("pending")
            .command
            .thread_id = RuntimeThreadId::new("other-thread").expect("thread");
        assert!(validate_managed_runtime_facts(&current, &changed_thread).is_err());

        let mut changed_command = current.clone();
        changed_command
            .pending_commands
            .get_mut(&operation_id)
            .expect("pending")
            .command
            .command = ManagedRuntimeCommand::Close;
        assert!(validate_managed_runtime_facts(&current, &changed_command).is_err());
    }

    #[test]
    fn facts_reject_duplicate_omitted_extra_or_tampered_projected_operations() {
        let current = operation_facts();
        let operation = current.projection.as_ref().expect("projection").operations[0].clone();

        let mut duplicate = current.clone();
        duplicate
            .projection
            .as_mut()
            .expect("projection")
            .operations
            .push(operation.clone());
        assert!(validate_managed_runtime_facts(&current, &duplicate).is_err());

        let mut omitted = current.clone();
        omitted
            .projection
            .as_mut()
            .expect("projection")
            .operations
            .clear();
        assert!(validate_managed_runtime_facts(&current, &omitted).is_err());

        let mut extra = current.clone();
        extra
            .projection
            .as_mut()
            .expect("projection")
            .operations
            .push(ManagedRuntimeOperation {
                id: RuntimeOperationId::new("extra").expect("operation"),
                turn_id: None,
                status: ManagedRuntimeOperationStatus::Accepted,
                evidence: None,
            });
        assert!(validate_managed_runtime_facts(&current, &extra).is_err());

        let mut tampered = current.clone();
        tampered.projection.as_mut().expect("projection").operations[0].status =
            ManagedRuntimeOperationStatus::Running;
        assert!(validate_managed_runtime_facts(&current, &tampered).is_err());
    }

    #[test]
    fn commit_rejects_succeeded_lifecycle_operations_without_typed_evidence() {
        let lifecycle_commands = [
            ManagedRuntimeCommand::Create {
                initial_context: None,
            },
            ManagedRuntimeCommand::Resume,
            ManagedRuntimeCommand::Activate,
            ManagedRuntimeCommand::Fork {
                child_thread_id: RuntimeThreadId::new("child-thread").expect("thread"),
                through_completed_turn_id: None,
            },
        ];

        for lifecycle_command in lifecycle_commands {
            let mut current = ManagedRuntimeStateSnapshot::default();
            let error = apply_managed_runtime_state_commit(
                &mut current,
                ManagedRuntimeStateCommit {
                    thread_id: RuntimeThreadId::new("thread").expect("thread"),
                    expected_revision: ManagedRuntimeStateRevision(0),
                    facts: lifecycle_succeeded_without_evidence(lifecycle_command),
                },
            )
            .expect_err("successful lifecycle fact without evidence must be rejected");

            assert!(matches!(
                error,
                ManagedRuntimeStateStoreError::Invariant { reason }
                    if reason == "successful Runtime lifecycle operation requires typed evidence"
            ));
            assert_eq!(current, ManagedRuntimeStateSnapshot::default());
        }
    }

    #[test]
    fn facts_reject_receipt_and_projected_operation_evidence_drift() {
        let current = operation_facts();
        let mut candidate = current.clone();
        candidate
            .operations
            .get_mut(&command().operation_id)
            .expect("operation")
            .receipt
            .evidence = Some(ManagedRuntimeOperationEvidence::Resume {
            binding: source_binding_evidence("source-ref"),
        });

        assert!(validate_managed_runtime_facts(&current, &candidate).is_err());
    }

    #[test]
    fn facts_reject_rewriting_committed_operation_evidence() {
        let mut current = lifecycle_succeeded_without_evidence(ManagedRuntimeCommand::Resume);
        let operation_id = command().operation_id;
        let first_evidence = ManagedRuntimeOperationEvidence::Resume {
            binding: source_binding_evidence("source-ref-1"),
        };
        let record = current
            .operations
            .get_mut(&operation_id)
            .expect("operation");
        record.receipt.evidence = Some(first_evidence.clone());
        record.operation.evidence = Some(first_evidence);
        current.projection.as_mut().expect("projection").operations[0] = record.operation.clone();
        let first_change = ManagedRuntimePlatformChange {
            thread_id: record.receipt.thread_id.clone(),
            sequence: RuntimeChangeSequence(1),
            revision: RuntimeProjectionRevision(7),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: record.operation.clone(),
            },
        };
        current.changes[0] = first_change.clone();
        current.outbox[0].change = first_change;

        let mut candidate = current.clone();
        let next_evidence = ManagedRuntimeOperationEvidence::Resume {
            binding: source_binding_evidence("source-ref-2"),
        };
        let record = candidate
            .operations
            .get_mut(&operation_id)
            .expect("operation");
        record.receipt.evidence = Some(next_evidence.clone());
        record.operation.evidence = Some(next_evidence);
        let projection = candidate.projection.as_mut().expect("projection");
        projection.revision = RuntimeProjectionRevision(8);
        projection.operations[0] = record.operation.clone();
        for availability in projection.command_availability.values_mut() {
            availability.evidence_mut().decided_at_revision = RuntimeProjectionRevision(8);
        }
        projection.latest_change_sequence = RuntimeChangeSequence(2);
        let change = ManagedRuntimePlatformChange {
            thread_id: projection.thread_id.clone(),
            sequence: RuntimeChangeSequence(2),
            revision: RuntimeProjectionRevision(8),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: record.operation.clone(),
            },
        };
        candidate.changes.push(change.clone());
        candidate.outbox.push(ManagedRuntimeOutboxEntry {
            sequence: change.sequence,
            operation_id: Some(operation_id),
            change,
        });

        validate_managed_runtime_facts(&ManagedRuntimeFacts::default(), &current)
            .expect("initial committed evidence");
        assert!(validate_managed_runtime_facts(&current, &candidate).is_err());
    }

    #[test]
    fn facts_accept_exact_monotonic_operation_transition() {
        let current = operation_facts();
        let operation_id = command().operation_id;
        let mut candidate = current.clone();
        let record = candidate
            .operations
            .get_mut(&operation_id)
            .expect("operation");
        record.receipt.status = ManagedRuntimeOperationStatus::Running;
        record.operation.status = ManagedRuntimeOperationStatus::Running;
        let projection = candidate.projection.as_mut().expect("projection");
        projection.revision = RuntimeProjectionRevision(8);
        projection.operations[0].status = ManagedRuntimeOperationStatus::Running;
        for availability in projection.command_availability.values_mut() {
            availability.evidence_mut().decided_at_revision = RuntimeProjectionRevision(8);
        }
        projection.latest_change_sequence = RuntimeChangeSequence(2);
        let change = ManagedRuntimePlatformChange {
            thread_id: projection.thread_id.clone(),
            sequence: RuntimeChangeSequence(2),
            revision: RuntimeProjectionRevision(8),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: record.operation.clone(),
            },
        };
        candidate.changes.push(change.clone());
        candidate.outbox.push(ManagedRuntimeOutboxEntry {
            sequence: change.sequence,
            operation_id: Some(operation_id),
            change,
        });

        validate_managed_runtime_facts(&current, &candidate).expect("exact monotonic transition");
    }

    #[test]
    fn facts_reject_change_outbox_operation_misassociation() {
        let current = operation_facts();
        let operation_id = command().operation_id;
        let mut candidate = current.clone();
        let projection = candidate.projection.as_mut().expect("projection");
        projection.revision = RuntimeProjectionRevision(8);
        for availability in projection.command_availability.values_mut() {
            availability.evidence_mut().decided_at_revision = RuntimeProjectionRevision(8);
        }
        projection.latest_change_sequence = RuntimeChangeSequence(2);
        let change = ManagedRuntimePlatformChange {
            thread_id: projection.thread_id.clone(),
            sequence: RuntimeChangeSequence(2),
            revision: RuntimeProjectionRevision(8),
            delta: ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
                command: ManagedRuntimeCommandKind::Close,
                availability: projection
                    .command_availability
                    .get(&ManagedRuntimeCommandKind::Close)
                    .expect("availability")
                    .clone(),
            },
        };
        candidate.changes.push(change.clone());
        candidate.outbox.push(ManagedRuntimeOutboxEntry {
            sequence: change.sequence,
            operation_id: Some(operation_id),
            change,
        });

        assert!(validate_managed_runtime_facts(&current, &candidate).is_err());
    }
}
