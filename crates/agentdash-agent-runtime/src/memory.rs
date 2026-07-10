use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU8, Ordering},
};

use agentdash_agent_runtime_contract::{
    EventSequence, IdempotencyKey, RuntimeEventEnvelope, RuntimeOperationId, RuntimeThreadId,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{
    QuarantinedDriverEvent, RuntimeCommit, RuntimeEventBatch, RuntimeOperationRecord,
    RuntimeRepository, RuntimeStoreError, RuntimeThreadState, RuntimeTransientEvents,
    RuntimeUnitOfWork,
};

#[derive(Clone, Default)]
struct MemoryState {
    threads: BTreeMap<RuntimeThreadId, RuntimeThreadState>,
    operations: BTreeMap<RuntimeOperationId, RuntimeOperationRecord>,
    idempotency: BTreeMap<(RuntimeThreadId, IdempotencyKey), RuntimeOperationId>,
    events: BTreeMap<RuntimeThreadId, Vec<RuntimeEventEnvelope>>,
    outbox: Vec<crate::RuntimeOutboxEntry>,
    context_activation_outbox: BTreeMap<
        agentdash_agent_runtime_contract::ContextActivationId,
        crate::ContextActivationOutboxEntry,
    >,
    context_preparation_work_items: BTreeMap<
        agentdash_agent_runtime_contract::ContextCompactionId,
        crate::ContextPreparationWorkItem,
    >,
    context_checkpoints:
        BTreeMap<agentdash_agent_runtime_contract::ContextCheckpointId, crate::ContextCheckpoint>,
    context_candidates:
        BTreeMap<agentdash_agent_runtime_contract::ContextCompactionId, crate::ContextCandidate>,
    context_activations:
        BTreeMap<agentdash_agent_runtime_contract::ContextActivationId, crate::ContextActivation>,
    context_heads: BTreeMap<RuntimeThreadId, crate::ActiveContextHead>,
    quarantine: Vec<QuarantinedDriverEvent>,
    transient: BTreeMap<RuntimeThreadId, Vec<RuntimeEventEnvelope>>,
}

/// Transactional fixture used by interface tests and future infrastructure adapters.
#[derive(Default)]
pub struct InMemoryRuntimeStore {
    state: Mutex<MemoryState>,
    fail_next_commit_at: AtomicU8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommitFailurePoint {
    BeforeWrite = 1,
    AfterProjection = 2,
    AfterOperation = 3,
    AfterEvents = 4,
    AfterOutbox = 5,
    AfterContext = 6,
}

impl InMemoryRuntimeStore {
    pub fn fail_next_commit(&self) {
        self.fail_next_commit_at(CommitFailurePoint::BeforeWrite);
    }

    pub fn fail_next_commit_at(&self, point: CommitFailurePoint) {
        self.fail_next_commit_at
            .store(point as u8, Ordering::SeqCst);
    }

    pub async fn outbox(&self) -> Vec<crate::RuntimeOutboxEntry> {
        self.state.lock().await.outbox.clone()
    }

    pub async fn quarantined(&self) -> Vec<QuarantinedDriverEvent> {
        self.state.lock().await.quarantine.clone()
    }

    pub async fn context_activation_outbox(&self) -> Vec<crate::ContextActivationOutboxEntry> {
        self.state
            .lock()
            .await
            .context_activation_outbox
            .values()
            .cloned()
            .collect()
    }

    pub async fn discard_events_through(
        &self,
        thread_id: &RuntimeThreadId,
        sequence: EventSequence,
    ) {
        if let Some(events) = self.state.lock().await.events.get_mut(thread_id) {
            events.retain(|event| event.sequence.is_none_or(|current| current > sequence));
        }
    }

    fn inject_failure(&self, point: CommitFailurePoint) -> Result<(), RuntimeStoreError> {
        if self
            .fail_next_commit_at
            .compare_exchange(point as u8, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Err(RuntimeStoreError::Unavailable(format!(
                "injected commit failure at {point:?}"
            )))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl RuntimeRepository for InMemoryRuntimeStore {
    async fn load_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError> {
        Ok(self.state.lock().await.threads.get(thread_id).cloned())
    }

    async fn find_thread_by_source(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
        source_thread_id: &agentdash_agent_runtime_contract::DriverThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .threads
            .values()
            .find(|thread| {
                thread.binding_id == *binding_id && thread.source_thread_id == *source_thread_id
            })
            .cloned())
    }

    async fn find_operation(
        &self,
        operation_id: &RuntimeOperationId,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .operations
            .get(operation_id)
            .cloned())
    }

    async fn find_idempotency(
        &self,
        thread_id: &RuntimeThreadId,
        key: &IdempotencyKey,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError> {
        let state = self.state.lock().await;
        Ok(state
            .idempotency
            .get(&(thread_id.clone(), key.clone()))
            .and_then(|id| state.operations.get(id))
            .cloned())
    }

    async fn events_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeEventBatch, RuntimeStoreError> {
        let state = self.state.lock().await;
        let retained = state.events.get(thread_id);
        let latest_available = state
            .threads
            .get(thread_id)
            .map_or(EventSequence(0), |thread| thread.next_event_sequence);
        Ok(RuntimeEventBatch {
            earliest_available: retained
                .and_then(|events| events.first())
                .and_then(|event| event.sequence)
                .unwrap_or(EventSequence(latest_available.0.saturating_add(1))),
            latest_available,
            events: retained
                .into_iter()
                .flatten()
                .filter(|event| {
                    after.is_none_or(|cursor| {
                        event.sequence.is_some_and(|sequence| sequence > cursor)
                    })
                })
                .cloned()
                .collect(),
        })
    }

    async fn load_context_head(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<crate::ActiveContextHead>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_heads
            .get(thread_id)
            .cloned())
    }

    async fn load_context_checkpoint(
        &self,
        checkpoint_id: &agentdash_agent_runtime_contract::ContextCheckpointId,
    ) -> Result<Option<crate::ContextCheckpoint>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_checkpoints
            .get(checkpoint_id)
            .cloned())
    }

    async fn load_context_candidate(
        &self,
        compaction_id: &agentdash_agent_runtime_contract::ContextCompactionId,
    ) -> Result<Option<crate::ContextCandidate>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_candidates
            .get(compaction_id)
            .cloned())
    }

    async fn load_context_activation(
        &self,
        activation_id: &agentdash_agent_runtime_contract::ContextActivationId,
    ) -> Result<Option<crate::ContextActivation>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_activations
            .get(activation_id)
            .cloned())
    }

    async fn load_context_preparation(
        &self,
        compaction_id: &agentdash_agent_runtime_contract::ContextCompactionId,
    ) -> Result<Option<crate::ContextPreparationWorkItem>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_preparation_work_items
            .get(compaction_id)
            .cloned())
    }

    async fn pending_context_preparations(
        &self,
    ) -> Result<Vec<crate::ContextPreparationWorkItem>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_preparation_work_items
            .values()
            .filter(|work| matches!(work.status, crate::ContextPreparationStatus::Pending))
            .cloned()
            .collect())
    }

    async fn pending_context_activations(
        &self,
    ) -> Result<Vec<crate::ContextActivationOutboxEntry>, RuntimeStoreError> {
        let state = self.state.lock().await;
        Ok(state
            .context_activation_outbox
            .values()
            .filter(|entry| {
                state
                    .context_activations
                    .get(&entry.activation_id)
                    .is_some_and(|activation| {
                        matches!(activation.status, crate::ContextActivationStatus::Prepared)
                    })
            })
            .cloned()
            .collect())
    }

    async fn recoverable_context_activations(
        &self,
    ) -> Result<Vec<crate::ContextActivation>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .context_activations
            .values()
            .filter(|activation| {
                !matches!(
                    activation.status,
                    crate::ContextActivationStatus::Terminal { .. }
                )
            })
            .cloned()
            .collect())
    }
}

#[async_trait]
impl RuntimeUnitOfWork for InMemoryRuntimeStore {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError> {
        self.inject_failure(CommitFailurePoint::BeforeWrite)?;
        let mut state = self.state.lock().await;
        let commit_thread_id = commit.projection.thread_id.clone();
        let current = state
            .threads
            .get(&commit.projection.thread_id)
            .map(|thread| thread.revision);
        if current != commit.expected_projection_revision {
            return Err(RuntimeStoreError::ProjectionConflict {
                expected: commit.expected_projection_revision,
                actual: current,
            });
        }
        if current.is_none()
            && state.threads.values().any(|thread| {
                thread.binding_id == commit.projection.binding_id
                    && thread.source_thread_id == commit.projection.source_thread_id
            })
        {
            return Err(RuntimeStoreError::ProjectionConflict {
                expected: commit.expected_projection_revision,
                actual: current,
            });
        }
        if let Some(operation) = &commit.operation {
            if state.operations.contains_key(&operation.operation_id) {
                return Err(RuntimeStoreError::OperationConflict {
                    operation_id: operation.operation_id.clone(),
                });
            }
            if let Some(existing) = state.idempotency.get(&(
                operation.thread_id.clone(),
                operation.idempotency_key.clone(),
            )) {
                return Err(RuntimeStoreError::IdempotencyConflict {
                    operation_id: existing.clone(),
                });
            }
        }
        for (operation_id, _) in &commit.operation_terminals {
            let operation = state.operations.get(operation_id).ok_or_else(|| {
                RuntimeStoreError::OperationConflict {
                    operation_id: operation_id.clone(),
                }
            })?;
            if operation.terminal.is_some() {
                return Err(RuntimeStoreError::OperationConflict {
                    operation_id: operation_id.clone(),
                });
            }
        }
        let mut staged = state.clone();
        staged
            .threads
            .insert(commit.projection.thread_id.clone(), commit.projection);
        self.inject_failure(CommitFailurePoint::AfterProjection)?;
        if let Some(operation) = commit.operation {
            staged.idempotency.insert(
                (
                    operation.thread_id.clone(),
                    operation.idempotency_key.clone(),
                ),
                operation.operation_id.clone(),
            );
            staged
                .operations
                .insert(operation.operation_id.clone(), operation);
        }
        for (operation_id, terminal) in commit.operation_terminals {
            staged
                .operations
                .get_mut(&operation_id)
                .expect("operation terminal was validated")
                .terminal = Some(terminal);
        }
        self.inject_failure(CommitFailurePoint::AfterOperation)?;
        for event in commit.events {
            staged
                .events
                .entry(event.thread_id.clone())
                .or_default()
                .push(event);
        }
        self.inject_failure(CommitFailurePoint::AfterEvents)?;
        staged.outbox.extend(commit.outbox);
        for entry in commit.context_activation_outbox {
            if let Some(existing) = staged.context_activation_outbox.get(&entry.activation_id)
                && existing != &entry
            {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::ActivationDispatchIdentity,
                });
            }
            staged
                .context_activation_outbox
                .insert(entry.activation_id.clone(), entry);
        }
        for work_item in commit.context_preparation_work_items {
            if let Some(active) = staged
                .context_preparation_work_items
                .values()
                .find(|existing| {
                    existing.thread_id == work_item.thread_id
                        && existing.compaction_id != work_item.compaction_id
                        && preparation_is_active(existing, &staged.context_activations)
                })
            {
                return Err(RuntimeStoreError::ContextCompactionConflict {
                    operation_id: active.operation_id.clone(),
                });
            }
            if let Some(existing) = staged
                .context_preparation_work_items
                .get(&work_item.compaction_id)
            {
                if existing.operation_id != work_item.operation_id
                    || existing.thread_id != work_item.thread_id
                    || existing.trigger != work_item.trigger
                    || existing.expected_base_checkpoint_id != work_item.expected_base_checkpoint_id
                    || existing.expected_base_revision != work_item.expected_base_revision
                {
                    return Err(RuntimeStoreError::ContextInvariant {
                        violation: crate::ContextStoreInvariant::PreparationIdentity,
                    });
                }
                let legal = matches!(
                    (&existing.status, &work_item.status),
                    (
                        crate::ContextPreparationStatus::Pending,
                        crate::ContextPreparationStatus::Prepared { .. }
                    ) | (
                        crate::ContextPreparationStatus::Prepared { .. },
                        crate::ContextPreparationStatus::Terminal { .. }
                    )
                ) || existing.status == work_item.status;
                if !legal {
                    return Err(RuntimeStoreError::ContextInvariant {
                        violation: crate::ContextStoreInvariant::PreparationTransition,
                    });
                }
            } else if !matches!(work_item.status, crate::ContextPreparationStatus::Pending) {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::PreparationTransition,
                });
            }
            staged
                .context_preparation_work_items
                .insert(work_item.compaction_id.clone(), work_item);
        }
        for checkpoint in commit.context_checkpoints {
            if staged.context_checkpoints.values().any(|existing| {
                existing.thread_id == checkpoint.thread_id
                    && existing.revision == checkpoint.revision
                    && existing.checkpoint_id != checkpoint.checkpoint_id
            }) {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::CheckpointRevisionReused,
                });
            }
            if let Some(existing) = staged.context_checkpoints.get(&checkpoint.checkpoint_id)
                && existing != &checkpoint
            {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::CheckpointIdentity,
                });
            }
            staged
                .context_checkpoints
                .insert(checkpoint.checkpoint_id.clone(), checkpoint);
        }
        for candidate in commit.context_candidates {
            if staged.context_candidates.values().any(|existing| {
                existing.candidate_id == candidate.candidate_id
                    && existing.compaction_id != candidate.compaction_id
            }) {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::CandidateIdReused,
                });
            }
            let candidate_coordinates_match = candidate.checkpoint.thread_id == candidate.thread_id
                && candidate.checkpoint.revision
                    == agentdash_agent_runtime_contract::ContextRevision(
                        candidate.expected_base_revision.0 + 1,
                    )
                && staged
                    .context_checkpoints
                    .get(&candidate.checkpoint.checkpoint_id)
                    == Some(&candidate.checkpoint)
                && staged
                    .context_preparation_work_items
                    .get(&candidate.compaction_id)
                    .is_some_and(|work| {
                        work.operation_id == candidate.operation_id
                            && work.thread_id == candidate.thread_id
                            && work.trigger == candidate.trigger
                            && work.expected_base_checkpoint_id
                                == candidate.expected_base_checkpoint_id
                            && work.expected_base_revision == candidate.expected_base_revision
                            && matches!(
                                &work.status,
                                crate::ContextPreparationStatus::Prepared {
                                    candidate_id,
                                    activation_id,
                                } if candidate_id == &candidate.candidate_id
                                    && activation_id == &candidate.activation_id
                            )
                    });
            if !candidate_coordinates_match {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::CandidateCoordinates,
                });
            }
            if let Some(existing) = staged.context_candidates.get(&candidate.compaction_id)
                && existing != &candidate
            {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::CandidateIdentity,
                });
            }
            staged
                .context_candidates
                .insert(candidate.compaction_id.clone(), candidate);
        }
        for activation in commit.context_activations {
            let activation_coordinates_match = staged
                .context_candidates
                .get(&activation.compaction_id)
                .is_some_and(|candidate| {
                    candidate.activation_id == activation.activation_id
                        && candidate.candidate_id == activation.candidate_id
                        && candidate.thread_id == activation.thread_id
                });
            if !activation_coordinates_match {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::ActivationCoordinates,
                });
            }
            if let Some(existing) = staged.context_activations.get(&activation.activation_id) {
                let mut expected = existing.clone();
                expected.status = activation.status.clone();
                if expected != activation {
                    return Err(RuntimeStoreError::ContextInvariant {
                        violation: crate::ContextStoreInvariant::ActivationIdentity,
                    });
                }
                if !valid_activation_transition(&existing.status, &activation.status) {
                    return Err(RuntimeStoreError::ContextInvariant {
                        violation: crate::ContextStoreInvariant::ActivationTransition,
                    });
                }
            } else if !matches!(activation.status, crate::ContextActivationStatus::Prepared) {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::ActivationTransition,
                });
            }
            staged
                .context_activations
                .insert(activation.activation_id.clone(), activation);
        }
        for dispatch in staged.context_activation_outbox.values() {
            let coordinates_match = staged
                .context_candidates
                .get(&dispatch.compaction_id)
                .is_some_and(|candidate| {
                    candidate.activation_id == dispatch.activation_id
                        && candidate.candidate_id == dispatch.candidate_id
                        && candidate.thread_id == dispatch.thread_id
                        && candidate.checkpoint.checkpoint_id == dispatch.checkpoint_id
                        && candidate.checkpoint.materialized.digest == dispatch.digest
                });
            if !coordinates_match {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::ActivationDispatchIdentity,
                });
            }
        }
        if let Some(write) = commit.context_head {
            let actual = staged
                .context_heads
                .get(&write.head.thread_id)
                .map(|head| head.revision);
            if actual != write.expected_revision {
                return Err(RuntimeStoreError::ContextHeadConflict {
                    expected: write.expected_revision,
                    actual,
                });
            }
            let checkpoint = staged
                .context_checkpoints
                .get(&write.head.checkpoint_id)
                .ok_or(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::HeadCheckpointMismatch,
                })?;
            if checkpoint.thread_id != write.head.thread_id
                || checkpoint.revision != write.head.revision
                || checkpoint.materialized.digest != write.head.digest
                || checkpoint.materialized.recipe.provenance != write.head.provenance
                || checkpoint.materialized.fidelity != write.head.fidelity
            {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::HeadCheckpointMismatch,
                });
            }
            let expected_next_revision = write
                .expected_revision
                .map_or(1, |revision| revision.0.saturating_add(1));
            if write.head.revision.0 != expected_next_revision
                || staged
                    .threads
                    .get(&write.head.thread_id)
                    .is_none_or(|thread| {
                        thread.active_checkpoint_id.as_ref() != Some(&write.head.checkpoint_id)
                            || thread.context_revision != write.head.revision
                    })
            {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: crate::ContextStoreInvariant::HeadCheckpointMismatch,
                });
            }
            staged
                .context_heads
                .insert(write.head.thread_id.clone(), write.head);
        }
        let projection = staged
            .threads
            .get(&commit_thread_id)
            .expect("projection was staged above");
        let durable_head = staged.context_heads.get(&commit_thread_id);
        if projection.active_checkpoint_id.as_ref() != durable_head.map(|head| &head.checkpoint_id)
            || projection.context_revision
                != durable_head.map_or(
                    agentdash_agent_runtime_contract::ContextRevision(0),
                    |head| head.revision,
                )
        {
            return Err(RuntimeStoreError::ContextInvariant {
                violation: crate::ContextStoreInvariant::HeadCheckpointMismatch,
            });
        }
        self.inject_failure(CommitFailurePoint::AfterContext)?;
        staged.quarantine.extend(commit.quarantine);
        self.inject_failure(CommitFailurePoint::AfterOutbox)?;
        *state = staged;
        Ok(())
    }

    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError> {
        self.state.lock().await.quarantine.push(event);
        Ok(())
    }
}

#[async_trait]
impl RuntimeTransientEvents for InMemoryRuntimeStore {
    async fn publish(&self, event: RuntimeEventEnvelope) {
        self.state
            .lock()
            .await
            .transient
            .entry(event.thread_id.clone())
            .or_default()
            .push(event);
    }

    async fn read(&self, thread_id: &RuntimeThreadId) -> Vec<RuntimeEventEnvelope> {
        self.state
            .lock()
            .await
            .transient
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }
}

fn valid_activation_transition(
    current: &crate::ContextActivationStatus,
    next: &crate::ContextActivationStatus,
) -> bool {
    use crate::ContextActivationStatus::{Applied, Prepared, Terminal};

    if current == next {
        return true;
    }
    match (current, next) {
        (Prepared, Applied { .. }) => true,
        (
            Prepared,
            Terminal {
                terminal: crate::CompactionTerminal::Lost { .. },
                applied: None,
            },
        ) => true,
        (
            Applied {
                digest,
                driver_context_revision,
            },
            Terminal {
                applied: Some(applied),
                ..
            },
        ) => {
            applied.digest == *digest && applied.driver_context_revision == *driver_context_revision
        }
        _ => false,
    }
}

fn preparation_is_active(
    work: &crate::ContextPreparationWorkItem,
    activations: &BTreeMap<
        agentdash_agent_runtime_contract::ContextActivationId,
        crate::ContextActivation,
    >,
) -> bool {
    match &work.status {
        crate::ContextPreparationStatus::Pending => true,
        crate::ContextPreparationStatus::Prepared { activation_id, .. } => {
            activations.get(activation_id).is_none_or(|activation| {
                !matches!(
                    activation.status,
                    crate::ContextActivationStatus::Terminal { .. }
                )
            })
        }
        crate::ContextPreparationStatus::Terminal { .. } => false,
    }
}
