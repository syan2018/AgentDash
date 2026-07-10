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
}

#[async_trait]
impl RuntimeUnitOfWork for InMemoryRuntimeStore {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError> {
        self.inject_failure(CommitFailurePoint::BeforeWrite)?;
        let mut state = self.state.lock().await;
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
