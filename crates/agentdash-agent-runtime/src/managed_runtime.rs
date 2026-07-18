use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangeDelta,
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandEnvelope, ManagedRuntimeCommandKind,
    ManagedRuntimeOperation, ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus,
    ManagedRuntimePlatformChange, ManagedRuntimeSnapshot, ManagedRuntimeUnavailabilityReason,
    RuntimeChangeSequence, RuntimeIdempotencyKey, RuntimeOperationId, RuntimeProjectionRevision,
    RuntimeThreadId,
};
use agentdash_agent_service_api::AgentEffectIdentity;
use async_trait::async_trait;
use thiserror::Error;

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
    for (operation_id, operation) in &candidate.operations {
        if operation_id != &operation.receipt.operation_id
            || operation_id != &operation.command.operation_id
            || operation.receipt.thread_id != operation.command.thread_id
        {
            return invariant("operation identity or thread coordinates are inconsistent");
        }
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
        if !candidate.operations.contains_key(operation_id) {
            return invariant("pending command references an unknown operation");
        }
    }

    if candidate.outbox.len() != candidate.changes.len() {
        return invariant("every Runtime change must have exactly one outbox entry");
    }
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
        if let ManagedRuntimeChangeDelta::OperationUpserted { operation } = &entry.change.delta
            && entry.operation_id.as_ref() != Some(&operation.id)
        {
            return invariant("operation change outbox has mismatched operation identity");
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
            || !operation_status_can_advance(operation.receipt.status, next.receipt.status)
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
            && facts.idempotency.is_empty()
            && facts.pending_commands.is_empty()
            && facts.changes.is_empty()
            && facts.outbox.is_empty()
        {
            return Ok(());
        }
        return invariant("managed Runtime facts require one authoritative projection");
    };
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
    if projection.operations.len() != facts.operations.len()
        || projection.operations.iter().any(|operation| {
            facts.operations.get(&operation.id).is_none_or(|record| {
                record.receipt.thread_id != projection.thread_id
                    || record.receipt.status != operation.status
            })
        })
    {
        return invariant("Runtime projection operation state does not match operation authority");
    }
    let mut previous_sequence = None;
    for change in &facts.changes {
        if change.thread_id != projection.thread_id
            || change.sequence.0 == 0
            || previous_sequence.is_some_and(|previous| change.sequence.0 != previous + 1)
            || change.revision.0 > projection.revision.0
        {
            return invariant("Runtime change coordinates are invalid");
        }
        previous_sequence = Some(change.sequence.0);
    }
    if previous_sequence.unwrap_or_default() != projection.latest_change_sequence.0 {
        return invariant("Runtime projection head does not match change history");
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
                ManagedRuntimePendingCommandState::Settled
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

pub struct ManagedRuntimeCoordinator<R> {
    repository: Arc<R>,
}

impl<R> ManagedRuntimeCoordinator<R>
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
        let snapshot = self.repository.load(&command.thread_id).await?;
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
        let projection = snapshot.facts.projection.as_ref().ok_or_else(|| {
            ManagedRuntimeStateStoreError::Invariant {
                reason: "managed Runtime thread has no admitted projection".to_owned(),
            }
        })?;
        if projection.thread_id != command.thread_id {
            return invariant("command thread does not match the admitted projection");
        }
        if command.expected_revision != Some(projection.revision) {
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
            duplicate: false,
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
        let operation = ManagedRuntimeOperation {
            id: receipt.operation_id.clone(),
            turn_id: None,
            status: receipt.status,
        };
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
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability,
        }
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
        assert_eq!(snapshot.facts.changes.len(), 8);
        assert_eq!(snapshot.facts.outbox.len(), 8);
        let projection = snapshot.facts.projection.expect("projection");
        assert_eq!(projection.revision, RuntimeProjectionRevision(8));
        assert_eq!(projection.latest_change_sequence, RuntimeChangeSequence(8));
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
        let command = command();
        let receipt = ManagedRuntimeOperationReceipt {
            operation_id: command.operation_id.clone(),
            thread_id: command.thread_id.clone(),
            accepted_revision: RuntimeProjectionRevision(7),
            status: ManagedRuntimeOperationStatus::Accepted,
            duplicate: false,
        };
        let operation = ManagedRuntimeOperation {
            id: command.operation_id.clone(),
            turn_id: None,
            status: receipt.status,
        };
        let change = ManagedRuntimePlatformChange {
            thread_id: command.thread_id.clone(),
            sequence: RuntimeChangeSequence(1),
            revision: RuntimeProjectionRevision(7),
            delta: ManagedRuntimeChangeDelta::OperationUpserted {
                operation: operation.clone(),
            },
        };
        let current = ManagedRuntimeFacts {
            projection: Some({
                let mut projection = projection();
                projection.operations.push(operation);
                projection.latest_change_sequence = RuntimeChangeSequence(1);
                projection
            }),
            operations: BTreeMap::from([(
                command.operation_id.clone(),
                ManagedRuntimeOperationRecord {
                    receipt,
                    command: command.clone(),
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
                operation_id: Some(command.operation_id.clone()),
                change,
            }],
        };
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
}
