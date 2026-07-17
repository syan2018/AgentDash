use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU8, Ordering},
};

use agentdash_agent_runtime_contract::{
    EventSequence, IdempotencyKey, ImmutablePresentationEvent, RuntimeBindingId,
    RuntimeCarrierMetadata, RuntimeDriverGeneration, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeJournalFact, RuntimeJournalRecord, RuntimeOperationId, RuntimePresentationCoordinate,
    RuntimeRevision, RuntimeThreadId, RuntimeTurnId,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{
    QuarantinedDriverEvent, RuntimeCommit, RuntimeJournalBatch, RuntimeOperationRecord,
    RuntimeRepository, RuntimeStoreError, RuntimeThreadState, RuntimeTransientEvents,
    RuntimeUnitOfWork,
};

#[derive(Clone, Default)]
struct MemoryState {
    threads: BTreeMap<RuntimeThreadId, RuntimeThreadState>,
    operations: BTreeMap<RuntimeOperationId, RuntimeOperationRecord>,
    idempotency: BTreeMap<(RuntimeThreadId, IdempotencyKey), RuntimeOperationId>,
    journal: BTreeMap<RuntimeThreadId, Vec<RuntimeJournalRecord>>,
    outbox: Vec<crate::RuntimeOutboxEntry>,
    terminal_application_effects:
        BTreeMap<crate::RuntimeTerminalApplicationEffectId, MemoryTerminalApplicationEffectState>,
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
    hook_runs: BTreeMap<agentdash_agent_runtime_contract::HookRunId, crate::HookRun>,
    hook_effects: BTreeMap<agentdash_agent_runtime_contract::HookEffectId, crate::HookEffect>,
    hook_plans: BTreeMap<RuntimeThreadId, crate::RuntimeHookPlanBinding>,
    quarantine: Vec<QuarantinedDriverEvent>,
    transient: BTreeMap<RuntimeThreadId, Vec<RuntimeEventEnvelope>>,
    presentation_transient: BTreeMap<RuntimeThreadId, Vec<RuntimeJournalRecord>>,
}

#[derive(Clone)]
struct MemoryTerminalApplicationEffectState {
    entry: crate::RuntimeTerminalApplicationEffectOutboxEntry,
    attempt_count: u32,
    claim: Option<MemoryTerminalApplicationEffectClaimLease>,
    completed: bool,
    last_error: Option<String>,
}

#[derive(Clone)]
struct MemoryTerminalApplicationEffectClaimLease {
    token: crate::RuntimeWorkClaimToken,
    owner: crate::RuntimeWorkerId,
    expires_at_ms: i64,
}

/// Transactional fixture used by interface tests and future infrastructure adapters.
#[derive(Default)]
pub struct RuntimeStoreFixture {
    state: Mutex<MemoryState>,
    live: Mutex<BTreeMap<RuntimeThreadId, tokio::sync::broadcast::Sender<RuntimeEventEnvelope>>>,
    presentation_live:
        Mutex<BTreeMap<RuntimeThreadId, tokio::sync::broadcast::Sender<RuntimeJournalRecord>>>,
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

impl RuntimeStoreFixture {
    pub async fn live_sender_count(&self) -> usize {
        self.live.lock().await.len()
    }
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

    pub async fn terminal_application_effects(
        &self,
    ) -> Vec<crate::RuntimeTerminalApplicationEffectOutboxEntry> {
        self.state
            .lock()
            .await
            .terminal_application_effects
            .values()
            .map(|state| state.entry.clone())
            .collect()
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
        if let Some(records) = self.state.lock().await.journal.get_mut(thread_id) {
            records.retain(|record| {
                record
                    .carrier()
                    .sequence
                    .is_none_or(|current| current > sequence)
            });
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
impl RuntimeRepository for RuntimeStoreFixture {
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

    async fn journal_records_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeJournalBatch, RuntimeStoreError> {
        let state = self.state.lock().await;
        let retained = state.journal.get(thread_id);
        let latest_available = state
            .threads
            .get(thread_id)
            .map_or(EventSequence(0), |thread| thread.next_event_sequence);
        Ok(RuntimeJournalBatch {
            earliest_available: retained
                .and_then(|records| records.first())
                .and_then(|record| record.carrier().sequence)
                .unwrap_or(EventSequence(latest_available.0.saturating_add(1))),
            latest_available,
            records: retained
                .into_iter()
                .flatten()
                .filter(|record| {
                    after.is_none_or(|cursor| {
                        record
                            .carrier()
                            .sequence
                            .is_some_and(|sequence| sequence > cursor)
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

    async fn load_hook_run(
        &self,
        hook_run_id: &agentdash_agent_runtime_contract::HookRunId,
    ) -> Result<Option<crate::HookRun>, RuntimeStoreError> {
        Ok(self.state.lock().await.hook_runs.get(hook_run_id).cloned())
    }

    async fn recoverable_hook_runs(&self) -> Result<Vec<crate::HookRun>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .hook_runs
            .values()
            .filter(|run| !run.status.is_terminal())
            .cloned()
            .collect())
    }

    async fn load_hook_plan(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<crate::RuntimeHookPlanBinding>, RuntimeStoreError> {
        Ok(self.state.lock().await.hook_plans.get(thread_id).cloned())
    }

    async fn hook_effects(
        &self,
        hook_run_id: &agentdash_agent_runtime_contract::HookRunId,
    ) -> Result<Vec<crate::HookEffect>, RuntimeStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .hook_effects
            .values()
            .filter(|effect| effect.hook_run_id == *hook_run_id)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl RuntimeUnitOfWork for RuntimeStoreFixture {
    async fn commit_with_live_presentation_publication(
        &self,
        commit: RuntimeCommit,
        publish_live_presentations: bool,
    ) -> Result<(), RuntimeStoreError> {
        let live_events = commit
            .records
            .iter()
            .filter_map(RuntimeJournalRecord::to_internal_envelope)
            .collect::<Vec<_>>();
        let live_presentations = commit
            .records
            .iter()
            .filter(|record| record.as_presentation().is_some())
            .cloned()
            .collect::<Vec<_>>();
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
        let previous_event_sequence = state
            .threads
            .get(&commit.projection.thread_id)
            .map_or(EventSequence(0), |thread| thread.next_event_sequence);
        validate_journal_records(&commit, previous_event_sequence)?;
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
        for record in commit.records {
            staged
                .journal
                .entry(record.carrier().thread_id.clone())
                .or_default()
                .push(record);
        }
        self.inject_failure(CommitFailurePoint::AfterEvents)?;
        if let Some(binding) = commit.hook_plan_binding {
            if binding.thread_id != commit_thread_id
                || staged.threads.get(&commit_thread_id).is_none_or(|thread| {
                    thread.hook_plan_revision != Some(binding.plan.revision)
                        || thread.hook_plan_digest.as_ref() != Some(&binding.plan.digest)
                })
            {
                return Err(RuntimeStoreError::Unavailable(
                    "hook plan binding does not match thread projection".to_string(),
                ));
            }
            if let Some(existing) = staged.hook_plans.get(&binding.thread_id) {
                let valid = existing == &binding
                    || binding.plan.revision.0 == existing.plan.revision.0.saturating_add(1);
                if !valid {
                    return Err(RuntimeStoreError::Unavailable(
                        "hook plan revision must advance exactly once".to_string(),
                    ));
                }
            } else if binding.plan.revision.0 != 1 {
                return Err(RuntimeStoreError::Unavailable(
                    "first hook plan revision must be one".to_string(),
                ));
            }
            staged.hook_plans.insert(binding.thread_id.clone(), binding);
        }
        for run in commit.hook_runs {
            if let Some(existing) = staged.hook_runs.get(&run.hook_run_id) {
                let immutable_matches = existing.thread_id == run.thread_id
                    && existing.definition_id == run.definition_id
                    && existing.point == run.point
                    && existing.plan_revision == run.plan_revision
                    && existing.plan_digest == run.plan_digest
                    && existing.actions == run.actions
                    && existing.delivered_strength == run.delivered_strength
                    && existing.failure_policy == run.failure_policy
                    && existing.site == run.site
                    && existing.correlation == run.correlation
                    && existing.input == run.input;
                let valid_transition = immutable_matches
                    && ((existing.status == crate::HookRunStatus::Accepted
                        && run.status == crate::HookRunStatus::Running)
                        || (existing.status == crate::HookRunStatus::Running
                            && run.status.is_terminal()));
                if !valid_transition && existing != &run {
                    return Err(RuntimeStoreError::Unavailable(
                        "invalid hook run transition".to_string(),
                    ));
                }
            } else if run.status != crate::HookRunStatus::Accepted {
                return Err(RuntimeStoreError::Unavailable(
                    "new hook run must be accepted".to_string(),
                ));
            }
            staged.hook_runs.insert(run.hook_run_id.clone(), run);
        }
        for effect in commit.hook_effects {
            let run = staged.hook_runs.get(&effect.hook_run_id).ok_or_else(|| {
                RuntimeStoreError::Unavailable("hook effect run was not found".to_string())
            })?;
            run.validate_effect(&effect).map_err(|error| {
                RuntimeStoreError::Unavailable(format!("invalid hook effect: {error}"))
            })?;
            if !run.status.is_terminal() {
                return Err(RuntimeStoreError::Unavailable(
                    "hook effect requires a terminal hook run".to_string(),
                ));
            }
            if let Some(existing) = staged.hook_effects.get(&effect.effect_id)
                && existing != &effect
            {
                return Err(RuntimeStoreError::Unavailable(
                    "hook effect identity was reused".to_string(),
                ));
            }
            if staged.hook_effects.values().any(|existing| {
                existing.hook_run_id == effect.hook_run_id
                    && existing.idempotency_key == effect.idempotency_key
                    && existing != &effect
            }) {
                return Err(RuntimeStoreError::Unavailable(
                    "hook effect idempotency key was reused".to_string(),
                ));
            }
            staged.hook_effects.insert(effect.effect_id.clone(), effect);
        }
        staged.outbox.extend(commit.outbox);
        for entry in commit.terminal_application_effects {
            if entry.runtime_thread_id != commit_thread_id
                || !staged
                    .journal
                    .get(&commit_thread_id)
                    .into_iter()
                    .flatten()
                    .any(|record| {
                        record.carrier().sequence == Some(entry.terminal_event_sequence)
                            && is_turn_terminal_presentation(record)
                    })
            {
                return Err(RuntimeStoreError::Unavailable(
                    "terminal application effect must reference its committed turn_terminal presentation"
                        .to_string(),
                ));
            }
            if let Some(existing) = staged.terminal_application_effects.get(&entry.effect_id) {
                if existing.entry != entry {
                    return Err(RuntimeStoreError::Unavailable(
                        "terminal application effect identity was reused".to_string(),
                    ));
                }
                continue;
            }
            staged.terminal_application_effects.insert(
                entry.effect_id.clone(),
                MemoryTerminalApplicationEffectState {
                    entry,
                    attempt_count: 0,
                    claim: None,
                    completed: false,
                    last_error: None,
                },
            );
        }
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
                        crate::ContextPreparationStatus::Pending,
                        crate::ContextPreparationStatus::Terminal { .. }
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
        let durable_hook_plan = staged.hook_plans.get(&commit_thread_id);
        if projection.hook_plan_revision != durable_hook_plan.map(|binding| binding.plan.revision)
            || projection.hook_plan_digest.as_ref()
                != durable_hook_plan.map(|binding| &binding.plan.digest)
        {
            return Err(RuntimeStoreError::Unavailable(
                "thread hook plan projection does not match durable binding".to_string(),
            ));
        }
        self.inject_failure(CommitFailurePoint::AfterContext)?;
        staged.quarantine.extend(commit.quarantine);
        self.inject_failure(CommitFailurePoint::AfterOutbox)?;
        *state = staged;
        drop(state);
        for event in live_events {
            self.publish_durable(event).await;
        }
        if publish_live_presentations {
            for record in live_presentations {
                self.publish_durable_presentation(record).await;
            }
        }
        Ok(())
    }

    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError> {
        self.state.lock().await.quarantine.push(event);
        Ok(())
    }
}

#[async_trait]
impl crate::RuntimeTerminalApplicationEffectOutbox for RuntimeStoreFixture {
    async fn claim_terminal_application_effects(
        &self,
        request: crate::RuntimeTerminalApplicationEffectClaimRequest,
    ) -> Result<Vec<crate::RuntimeTerminalApplicationEffectClaim>, RuntimeStoreError> {
        if request.owner.as_str().trim().is_empty()
            || request.lease_duration_ms == 0
            || request.limit == 0
        {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "terminal application effect claim requires owner, positive lease, and positive limit"
                    .to_string(),
            ));
        }
        let now = i64::try_from(crate::model::current_time_ms()).map_err(|_| {
            RuntimeStoreError::InvalidWorkClaim("current time exceeds i64".to_string())
        })?;
        let lease_duration = i64::try_from(request.lease_duration_ms).map_err(|_| {
            RuntimeStoreError::InvalidWorkClaim("lease duration exceeds i64".to_string())
        })?;
        let expires_at_ms = now.checked_add(lease_duration).ok_or_else(|| {
            RuntimeStoreError::InvalidWorkClaim("lease expiration overflow".to_string())
        })?;
        let mut state = self.state.lock().await;
        let mut claims = Vec::new();
        for effect in state.terminal_application_effects.values_mut() {
            if claims.len() >= request.limit as usize
                || effect.completed
                || effect
                    .claim
                    .as_ref()
                    .is_some_and(|claim| claim.expires_at_ms > now)
            {
                continue;
            }
            effect.attempt_count = effect.attempt_count.saturating_add(1);
            effect.last_error = None;
            let token = crate::RuntimeWorkClaimToken(format!(
                "terminal-effect:{}:{}:{}",
                effect.entry.effect_id.as_str(),
                effect.attempt_count,
                now
            ));
            effect.claim = Some(MemoryTerminalApplicationEffectClaimLease {
                token: token.clone(),
                owner: request.owner.clone(),
                expires_at_ms,
            });
            claims.push(crate::RuntimeTerminalApplicationEffectClaim {
                entry: effect.entry.clone(),
                token,
                owner: request.owner.clone(),
                lease_expires_at_ms: expires_at_ms,
                attempt: effect.attempt_count,
            });
        }
        Ok(claims)
    }

    async fn ack_terminal_application_effect(
        &self,
        claim: &crate::RuntimeTerminalApplicationEffectClaim,
    ) -> Result<(), RuntimeStoreError> {
        let mut state = self.state.lock().await;
        let effect = state
            .terminal_application_effects
            .get_mut(&claim.entry.effect_id)
            .ok_or(RuntimeStoreError::WorkClaimConflict)?;
        let owned = effect
            .claim
            .as_ref()
            .is_some_and(|lease| lease.owner == claim.owner && lease.token == claim.token);
        if !owned || effect.completed {
            return Err(RuntimeStoreError::WorkClaimConflict);
        }
        effect.completed = true;
        effect.claim = None;
        effect.last_error = None;
        Ok(())
    }

    async fn release_terminal_application_effect(
        &self,
        claim: &crate::RuntimeTerminalApplicationEffectClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError> {
        let mut state = self.state.lock().await;
        let effect = state
            .terminal_application_effects
            .get_mut(&claim.entry.effect_id)
            .ok_or(RuntimeStoreError::WorkClaimConflict)?;
        let owned = effect
            .claim
            .as_ref()
            .is_some_and(|lease| lease.owner == claim.owner && lease.token == claim.token);
        if !owned || effect.completed {
            return Err(RuntimeStoreError::WorkClaimConflict);
        }
        effect.claim = None;
        effect.last_error = Some(error);
        Ok(())
    }
}

fn validate_journal_records(
    commit: &RuntimeCommit,
    previous: EventSequence,
) -> Result<(), RuntimeStoreError> {
    let mut expected = previous.0;
    for record in &commit.records {
        expected = expected.checked_add(1).ok_or_else(|| {
            RuntimeStoreError::Unavailable("runtime journal sequence overflow".to_string())
        })?;
        if record.carrier().thread_id != commit.projection.thread_id
            || record.carrier().sequence != Some(EventSequence(expected))
            || record.carrier().transient.is_some()
        {
            return Err(RuntimeStoreError::Unavailable(
                "runtime commit contains a non-contiguous, transient, or cross-thread journal record"
                    .to_string(),
            ));
        }
    }
    if commit.projection.next_event_sequence != EventSequence(expected) {
        return Err(RuntimeStoreError::Unavailable(
            "runtime projection event cursor does not match the committed journal".to_string(),
        ));
    }
    Ok(())
}

fn is_turn_terminal_presentation(record: &RuntimeJournalRecord) -> bool {
    matches!(
        record.fact(),
        RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
            event: agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
            ),
            ..
        }) if key == "turn_terminal"
    )
}

#[async_trait]
impl RuntimeTransientEvents for RuntimeStoreFixture {
    async fn publish_transient(
        &self,
        thread_id: RuntimeThreadId,
        binding_id: RuntimeBindingId,
        stream_generation: RuntimeDriverGeneration,
        turn_id: Option<RuntimeTurnId>,
        revision: RuntimeRevision,
        event: RuntimeEvent,
    ) {
        const ACTIVE_TURN_REPLAY_LIMIT: usize = 512;
        let mut event = RuntimeEventEnvelope {
            thread_id,
            occurred_at_ms: crate::model::current_time_ms(),
            sequence: None,
            transient: None,
            revision,
            event,
        };
        let mut state = self.state.lock().await;
        let entries = state.transient.entry(event.thread_id.clone()).or_default();
        if entries
            .last()
            .and_then(|item| item.transient.as_ref())
            .is_some_and(|current| {
                current.binding_id != binding_id
                    || current.stream_generation != stream_generation
                    || current.turn_id != turn_id
            })
        {
            entries.clear();
        }
        let sequence = agentdash_agent_runtime_contract::RuntimeTransientSequence(
            entries
                .last()
                .and_then(|item| item.transient.as_ref())
                .map_or(1, |item| item.sequence.0 + 1),
        );
        let event_id = agentdash_agent_runtime_contract::RuntimeTransientEventId::new(format!(
            "{}:{}:{}:{}",
            binding_id,
            stream_generation.0,
            turn_id.as_ref().map_or("thread", |turn| turn.as_str()),
            sequence.0
        ))
        .expect("generated transient id");
        event.transient = Some(
            agentdash_agent_runtime_contract::RuntimeTransientCoordinate {
                binding_id,
                stream_generation,
                sequence,
                event_id,
                turn_id,
            },
        );
        entries.push(event.clone());
        if entries.len() > ACTIVE_TURN_REPLAY_LIMIT {
            entries.remove(0);
        }
        drop(state);
        self.publish_durable(event).await;
    }

    async fn publish_transient_presentation(
        &self,
        thread_id: RuntimeThreadId,
        binding_id: RuntimeBindingId,
        stream_generation: RuntimeDriverGeneration,
        turn_id: Option<RuntimeTurnId>,
        revision: RuntimeRevision,
        mut coordinate: RuntimePresentationCoordinate,
        event: ImmutablePresentationEvent,
    ) {
        const ACTIVE_TURN_REPLAY_LIMIT: usize = 512;
        let mut state = self.state.lock().await;
        let entries = state
            .presentation_transient
            .entry(thread_id.clone())
            .or_default();
        if entries
            .last()
            .and_then(|record| record.carrier().transient.as_ref())
            .is_some_and(|current| {
                current.binding_id != binding_id
                    || current.stream_generation != stream_generation
                    || current.turn_id != turn_id
            })
        {
            entries.clear();
        }
        let sequence = agentdash_agent_runtime_contract::RuntimeTransientSequence(
            entries
                .last()
                .and_then(|record| record.carrier().transient.as_ref())
                .map_or(1, |current| current.sequence.0 + 1),
        );
        let event_id = agentdash_agent_runtime_contract::RuntimeTransientEventId::new(format!(
            "{}:{}:{}:{}",
            binding_id,
            stream_generation.0,
            turn_id.as_ref().map_or("thread", |turn| turn.as_str()),
            sequence.0
        ))
        .expect("generated transient presentation id");
        coordinate.runtime_turn_id = turn_id.clone().or(coordinate.runtime_turn_id);
        let record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: crate::model::current_time_ms(),
                sequence: None,
                transient: Some(
                    agentdash_agent_runtime_contract::RuntimeTransientCoordinate {
                        binding_id: binding_id.clone(),
                        stream_generation,
                        sequence,
                        event_id,
                        turn_id,
                    },
                ),
                revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: Some(binding_id),
                coordinate,
            },
            RuntimeJournalFact::Presentation(event),
        )
        .expect("ephemeral presentation carrier");
        entries.push(record.clone());
        if entries.len() > ACTIVE_TURN_REPLAY_LIMIT {
            entries.remove(0);
        }
        drop(state);
        let mut live = self.presentation_live.lock().await;
        let sender = live
            .entry(thread_id)
            .or_insert_with(|| tokio::sync::broadcast::channel(ACTIVE_LIVE_CHANNEL_CAPACITY).0);
        let _ = sender.send(record);
    }

    async fn publish_durable_presentation(&self, record: RuntimeJournalRecord) {
        debug_assert!(record.carrier().sequence.is_some());
        debug_assert!(record.as_presentation().is_some());
        let thread_id = record.carrier().thread_id.clone();
        let mut live = self.presentation_live.lock().await;
        let sender = live
            .entry(thread_id)
            .or_insert_with(|| tokio::sync::broadcast::channel(ACTIVE_LIVE_CHANNEL_CAPACITY).0);
        let _ = sender.send(record);
    }

    async fn publish_durable(&self, event: RuntimeEventEnvelope) {
        let closes_channel = closes_live_channel(&event);
        let thread_id = event.thread_id.clone();
        let mut live = self.live.lock().await;
        let sender = live
            .entry(event.thread_id.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(ACTIVE_LIVE_CHANNEL_CAPACITY).0);
        let _ = sender.send(event);
        if closes_channel {
            live.remove(&thread_id);
        }
    }

    async fn subscribe(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> tokio::sync::broadcast::Receiver<RuntimeEventEnvelope> {
        self.live
            .lock()
            .await
            .entry(thread_id.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(ACTIVE_LIVE_CHANNEL_CAPACITY).0)
            .subscribe()
    }

    async fn read(
        &self,
        thread_id: &RuntimeThreadId,
        stream_generation: Option<agentdash_agent_runtime_contract::RuntimeDriverGeneration>,
        after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    ) -> Vec<RuntimeEventEnvelope> {
        self.state
            .lock()
            .await
            .transient
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|event| {
                event.transient.as_ref().is_some_and(|coordinate| {
                    stream_generation
                        .is_none_or(|generation| coordinate.stream_generation == generation)
                        && after.is_none_or(|after| coordinate.sequence > after)
                })
            })
            .collect()
    }

    async fn subscribe_presentation(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> tokio::sync::broadcast::Receiver<RuntimeJournalRecord> {
        self.presentation_live
            .lock()
            .await
            .entry(thread_id.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(ACTIVE_LIVE_CHANNEL_CAPACITY).0)
            .subscribe()
    }

    async fn read_presentation(
        &self,
        thread_id: &RuntimeThreadId,
        stream_generation: Option<RuntimeDriverGeneration>,
        after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    ) -> Vec<RuntimeJournalRecord> {
        self.state
            .lock()
            .await
            .presentation_transient
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|record| {
                record
                    .carrier()
                    .transient
                    .as_ref()
                    .is_some_and(|coordinate| {
                        stream_generation
                            .is_none_or(|generation| coordinate.stream_generation == generation)
                            && after.is_none_or(|after| coordinate.sequence > after)
                    })
            })
            .collect()
    }

    async fn clear(&self, thread_id: &RuntimeThreadId) {
        let mut state = self.state.lock().await;
        state.transient.remove(thread_id);
        state.presentation_transient.remove(thread_id);
        drop(state);
    }
}

const ACTIVE_LIVE_CHANNEL_CAPACITY: usize = 1024;

fn closes_live_channel(event: &RuntimeEventEnvelope) -> bool {
    matches!(
        event.event,
        agentdash_agent_runtime_contract::RuntimeEvent::BindingLost { .. }
            | agentdash_agent_runtime_contract::RuntimeEvent::ThreadStatusChanged {
                status: agentdash_agent_runtime_contract::RuntimeThreadStatus::Closed
                    | agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost
            }
    )
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
