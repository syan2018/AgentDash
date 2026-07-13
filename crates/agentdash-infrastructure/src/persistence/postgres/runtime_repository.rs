use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime::{
    ActiveContextHead, ContextActivation, ContextActivationOutboxEntry, ContextActivationStatus,
    ContextCandidate, ContextCheckpoint, ContextHeadWrite, ContextPreparationStatus,
    ContextPreparationWorkItem, ContextStoreInvariant, EntityPhase, HookEffect, HookRun,
    HookRunStatus, QuarantinedDriverEvent, RuntimeCommit, RuntimeEventBatch,
    RuntimeHookPlanBinding, RuntimeInteractionState, RuntimeItemState, RuntimeOperationRecord,
    RuntimeOutboxEntry, RuntimeRepository, RuntimeStoreError, RuntimeThreadState,
    RuntimeTransientEvents, RuntimeTurnState, RuntimeUnitOfWork, RuntimeWorkClaim,
    RuntimeWorkClaimRequest, RuntimeWorkClaimToken, RuntimeWorkIdentity, RuntimeWorkKind,
    RuntimeWorkPayload, RuntimeWorkQueue, RuntimeWorkerId,
};
use agentdash_agent_runtime_contract::{
    ContextActivationId, ContextCheckpointId, ContextCompactionId, ContextFidelity, EventSequence,
    HookEffectId, HookRunId, IdempotencyKey, RuntimeBindingId, RuntimeEventEnvelope,
    RuntimeOperationId, RuntimeOperationTerminal, RuntimeRevision, RuntimeThreadId,
};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Clone)]
pub struct PostgresRuntimeRepository {
    pool: PgPool,
    transient: Arc<tokio::sync::Mutex<BTreeMap<RuntimeThreadId, Vec<RuntimeEventEnvelope>>>>,
    live: Arc<
        tokio::sync::Mutex<
            BTreeMap<RuntimeThreadId, tokio::sync::broadcast::Sender<RuntimeEventEnvelope>>,
        >,
    >,
    #[cfg(test)]
    fail_at: Arc<std::sync::atomic::AtomicU8>,
}

impl PostgresRuntimeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            transient: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            live: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            #[cfg(test)]
            fail_at: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    #[cfg(test)]
    fn fail_next_commit_at(&self, point: TestCommitFailurePoint) {
        use std::sync::atomic::Ordering;
        self.fail_at.store(point as u8, Ordering::SeqCst);
    }

    fn inject_failure(&self, _point: u8) -> Result<(), RuntimeStoreError> {
        #[cfg(test)]
        {
            use std::sync::atomic::Ordering;
            if self
                .fail_at
                .compare_exchange(_point, 0, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Err(RuntimeStoreError::Unavailable(format!(
                    "injected postgres commit failure at stage {_point}"
                )));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl RuntimeTransientEvents for PostgresRuntimeRepository {
    async fn publish_transient(
        &self,
        thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
        binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
        stream_generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
        turn_id: Option<agentdash_agent_runtime_contract::RuntimeTurnId>,
        revision: agentdash_agent_runtime_contract::RuntimeRevision,
        event_value: agentdash_agent_runtime_contract::RuntimeEvent,
    ) {
        const ACTIVE_TURN_REPLAY_LIMIT: usize = 512;
        let mut event = RuntimeEventEnvelope {
            thread_id,
            occurred_at_ms: current_time_ms(),
            sequence: None,
            transient: None,
            revision,
            event: event_value,
        };
        let mut transient = self.transient.lock().await;
        let entries = transient.entry(event.thread_id.clone()).or_default();
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
        drop(transient);
        self.publish_durable(event).await;
    }

    async fn publish_durable(&self, event: RuntimeEventEnvelope) {
        let closes_channel = matches!(
            event.event,
            agentdash_agent_runtime_contract::RuntimeEvent::BindingLost { .. }
                | agentdash_agent_runtime_contract::RuntimeEvent::ThreadStatusChanged {
                    status: agentdash_agent_runtime_contract::RuntimeThreadStatus::Closed
                        | agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost
                }
        );
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
        self.transient
            .lock()
            .await
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

    async fn clear(&self, thread_id: &RuntimeThreadId) {
        self.transient.lock().await.remove(thread_id);
    }
}

const ACTIVE_LIVE_CHANNEL_CAPACITY: usize = 1024;

fn encode<T: Serialize>(value: &T, coordinate: &'static str) -> Result<Value, RuntimeStoreError> {
    serde_json::to_value(value).map_err(|error| {
        RuntimeStoreError::Unavailable(format!("cannot encode {coordinate}: {error}"))
    })
}

fn decode<T: DeserializeOwned>(
    value: Value,
    coordinate: &'static str,
) -> Result<T, RuntimeStoreError> {
    serde_json::from_value(value).map_err(|error| {
        RuntimeStoreError::Unavailable(format!("cannot decode {coordinate}: {error}"))
    })
}

fn sql_error(error: sqlx::Error) -> RuntimeStoreError {
    RuntimeStoreError::Unavailable(error.to_string())
}

fn u64_to_i64(value: u64, coordinate: &'static str) -> Result<i64, RuntimeStoreError> {
    i64::try_from(value).map_err(|_| {
        RuntimeStoreError::Unavailable(format!("{coordinate} exceeds PostgreSQL bigint"))
    })
}

fn i64_to_u64(value: i64, coordinate: &'static str) -> Result<u64, RuntimeStoreError> {
    u64::try_from(value).map_err(|_| {
        RuntimeStoreError::Unavailable(format!("{coordinate} contains a negative sequence"))
    })
}

fn thread_status(state: &RuntimeThreadState) -> &'static str {
    use agentdash_agent_runtime_contract::RuntimeThreadStatus::*;
    match state.status {
        Active => "active",
        Suspended => "suspended",
        Desynchronized => "desynchronized",
        Closed => "closed",
        Lost => "lost",
    }
}

fn operation_status(terminal: Option<&RuntimeOperationTerminal>) -> &'static str {
    match terminal {
        None => "active",
        Some(RuntimeOperationTerminal::Succeeded) => "succeeded",
        Some(RuntimeOperationTerminal::Failed { .. }) => "failed",
        Some(RuntimeOperationTerminal::Lost { .. }) => "lost",
    }
}

fn entity_phase<T>(phase: &EntityPhase<T>) -> &'static str {
    match phase {
        EntityPhase::Active => "active",
        EntityPhase::Terminal(_) => "terminal",
    }
}

fn fidelity(value: ContextFidelity) -> &'static str {
    match value {
        ContextFidelity::Opaque => "opaque",
        ContextFidelity::EventProjected => "event_projected",
        ContextFidelity::AgentReplay => "agent_replay",
        ContextFidelity::DriverExact => "driver_exact",
        ContextFidelity::PlatformExact => "platform_exact",
    }
}

fn preparation_status(value: &ContextPreparationStatus) -> &'static str {
    match value {
        ContextPreparationStatus::Pending => "pending",
        ContextPreparationStatus::Prepared { .. } => "prepared",
        ContextPreparationStatus::Terminal { .. } => "terminal",
    }
}

fn activation_status(value: &ContextActivationStatus) -> &'static str {
    match value {
        ContextActivationStatus::Prepared => "prepared",
        ContextActivationStatus::Applied { .. } => "applied",
        ContextActivationStatus::Terminal { .. } => "terminal",
    }
}

fn trigger(value: agentdash_agent_runtime_contract::ContextCompactionTrigger) -> &'static str {
    match value {
        agentdash_agent_runtime_contract::ContextCompactionTrigger::Manual => "manual",
        agentdash_agent_runtime_contract::ContextCompactionTrigger::Automatic => "automatic",
    }
}

fn hook_point(value: agentdash_agent_runtime_contract::HookPoint) -> &'static str {
    use agentdash_agent_runtime_contract::HookPoint::*;
    match value {
        BeforeThreadStart => "before_thread_start",
        AfterThreadStart => "after_thread_start",
        BeforeTurn => "before_turn",
        AfterTurn => "after_turn",
        BeforeProviderRequest => "before_provider_request",
        BeforeTool => "before_tool",
        AfterTool => "after_tool",
        BeforeContextCompact => "before_context_compact",
        AfterContextCompact => "after_context_compact",
        BeforeStop => "before_stop",
        AfterItem => "after_item",
    }
}

fn hook_run_status(value: HookRunStatus) -> &'static str {
    match value {
        HookRunStatus::Accepted => "accepted",
        HookRunStatus::Running => "running",
        HookRunStatus::Completed => "completed",
        HookRunStatus::Blocked => "blocked",
        HookRunStatus::Failed => "failed",
        HookRunStatus::Stopped => "stopped",
        HookRunStatus::Cancelled => "cancelled",
    }
}

async fn fetch_document<T: DeserializeOwned>(
    pool: &PgPool,
    sql: &str,
    id: &str,
    coordinate: &'static str,
) -> Result<Option<T>, RuntimeStoreError> {
    let row = sqlx::query(sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(sql_error)?;
    row.map(|row| decode(row.get::<Value, _>(0), coordinate))
        .transpose()
}

#[async_trait]
impl RuntimeRepository for PostgresRuntimeRepository {
    async fn load_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT projection FROM agent_runtime_thread WHERE id=$1",
            thread_id.as_str(),
            "agent_runtime_thread.projection",
        )
        .await
    }

    async fn find_thread_by_source(
        &self,
        binding_id: &RuntimeBindingId,
        source_thread_id: &agentdash_agent_runtime_contract::DriverThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError> {
        let row = sqlx::query(
            "SELECT t.projection FROM agent_runtime_source_coordinate s \
             JOIN agent_runtime_thread t ON t.id=s.thread_id \
             WHERE s.binding_id=$1 AND s.source_thread_id=$2",
        )
        .bind(binding_id.as_str())
        .bind(source_thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|row| decode(row.get::<Value, _>(0), "agent_runtime_thread.projection"))
            .transpose()
    }

    async fn find_operation(
        &self,
        operation_id: &RuntimeOperationId,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_runtime_operation WHERE id=$1",
            operation_id.as_str(),
            "agent_runtime_operation.record",
        )
        .await
    }

    async fn find_idempotency(
        &self,
        thread_id: &RuntimeThreadId,
        key: &IdempotencyKey,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError> {
        let row = sqlx::query(
            "SELECT record FROM agent_runtime_operation WHERE thread_id=$1 AND idempotency_key=$2",
        )
        .bind(thread_id.as_str())
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|row| decode(row.get::<Value, _>(0), "agent_runtime_operation.record"))
            .transpose()
    }

    async fn events_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeEventBatch, RuntimeStoreError> {
        let latest = sqlx::query_scalar::<_, i64>(
            "SELECT next_event_sequence FROM agent_runtime_thread WHERE id=$1",
        )
        .bind(thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?
        .ok_or(RuntimeStoreError::NotFound)?;
        let rows = sqlx::query(
            "SELECT event_sequence,envelope FROM agent_runtime_event \
             WHERE thread_id=$1 AND event_sequence>$2 ORDER BY event_sequence",
        )
        .bind(thread_id.as_str())
        .bind(u64_to_i64(
            after.unwrap_or(EventSequence(0)).0,
            "event cursor",
        )?)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        let earliest = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(event_sequence) FROM agent_runtime_event WHERE thread_id=$1",
        )
        .bind(thread_id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?;
        let latest = i64_to_u64(latest, "agent_runtime_thread.next_event_sequence")?;
        Ok(RuntimeEventBatch {
            earliest_available: EventSequence(match earliest {
                Some(value) => i64_to_u64(value, "agent_runtime_event.event_sequence")?,
                None => latest.saturating_add(1),
            }),
            latest_available: EventSequence(latest),
            events: rows
                .into_iter()
                .map(|row| decode(row.get::<Value, _>(1), "agent_runtime_event.envelope"))
                .collect::<Result<_, _>>()?,
        })
    }

    async fn load_context_head(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<ActiveContextHead>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_context_head WHERE thread_id=$1",
            thread_id.as_str(),
            "agent_context_head.record",
        )
        .await
    }

    async fn load_context_checkpoint(
        &self,
        checkpoint_id: &ContextCheckpointId,
    ) -> Result<Option<ContextCheckpoint>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_context_checkpoint WHERE id=$1",
            checkpoint_id.as_str(),
            "agent_context_checkpoint.record",
        )
        .await
    }

    async fn load_context_candidate(
        &self,
        compaction_id: &ContextCompactionId,
    ) -> Result<Option<ContextCandidate>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_context_candidate WHERE compaction_id=$1",
            compaction_id.as_str(),
            "agent_context_candidate.record",
        )
        .await
    }

    async fn load_context_activation(
        &self,
        activation_id: &ContextActivationId,
    ) -> Result<Option<ContextActivation>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_context_activation WHERE id=$1",
            activation_id.as_str(),
            "agent_context_activation.record",
        )
        .await
    }

    async fn load_context_preparation(
        &self,
        compaction_id: &ContextCompactionId,
    ) -> Result<Option<ContextPreparationWorkItem>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_context_preparation WHERE compaction_id=$1",
            compaction_id.as_str(),
            "agent_context_preparation.record",
        )
        .await
    }

    async fn pending_context_preparations(
        &self,
    ) -> Result<Vec<ContextPreparationWorkItem>, RuntimeStoreError> {
        load_documents(
            &self.pool,
            "SELECT record FROM agent_context_preparation WHERE status='pending' ORDER BY created_at",
            "agent_context_preparation.record",
        )
        .await
    }

    async fn pending_context_activations(
        &self,
    ) -> Result<Vec<ContextActivationOutboxEntry>, RuntimeStoreError> {
        load_documents(
            &self.pool,
            "SELECT d.payload FROM agent_context_activation_dispatch d \
             JOIN agent_context_activation a ON a.id=d.activation_id \
             WHERE d.dispatched_at IS NULL AND a.status='prepared' ORDER BY d.created_at",
            "agent_context_activation_dispatch.payload",
        )
        .await
    }

    async fn recoverable_context_activations(
        &self,
    ) -> Result<Vec<ContextActivation>, RuntimeStoreError> {
        load_documents(
            &self.pool,
            "SELECT record FROM agent_context_activation WHERE status IN ('prepared','applied') ORDER BY updated_at",
            "agent_context_activation.record",
        )
        .await
    }

    async fn load_hook_run(
        &self,
        hook_run_id: &HookRunId,
    ) -> Result<Option<HookRun>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT record FROM agent_runtime_hook_run WHERE id=$1",
            hook_run_id.as_str(),
            "agent_runtime_hook_run.record",
        )
        .await
    }

    async fn recoverable_hook_runs(&self) -> Result<Vec<HookRun>, RuntimeStoreError> {
        load_documents(
            &self.pool,
            "SELECT record FROM agent_runtime_hook_run WHERE status IN ('accepted','running') ORDER BY updated_at",
            "agent_runtime_hook_run.record",
        )
        .await
    }

    async fn load_hook_plan(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeHookPlanBinding>, RuntimeStoreError> {
        fetch_document(
            &self.pool,
            "SELECT binding FROM agent_runtime_hook_plan WHERE thread_id=$1 ORDER BY revision DESC LIMIT 1",
            thread_id.as_str(),
            "agent_runtime_hook_plan.binding",
        )
        .await
    }

    async fn hook_effects(
        &self,
        hook_run_id: &HookRunId,
    ) -> Result<Vec<HookEffect>, RuntimeStoreError> {
        let rows = sqlx::query(
            "SELECT record FROM agent_runtime_hook_effect WHERE hook_run_id=$1 ORDER BY id",
        )
        .bind(hook_run_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| decode(row.get(0), "agent_runtime_hook_effect.record"))
            .collect()
    }
}

async fn load_documents<T: DeserializeOwned>(
    pool: &PgPool,
    sql: &str,
    coordinate: &'static str,
) -> Result<Vec<T>, RuntimeStoreError> {
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?
        .into_iter()
        .map(|row| decode(row.get::<Value, _>(0), coordinate))
        .collect()
}

#[async_trait]
impl RuntimeUnitOfWork for PostgresRuntimeRepository {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError> {
        let live_events = commit.events.clone();
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let thread_id = commit.projection.thread_id.clone();
        let current = sqlx::query(
            "SELECT revision,next_event_sequence,next_operation_sequence \
             FROM agent_runtime_thread WHERE id=$1 FOR UPDATE",
        )
        .bind(thread_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(sql_error)?;
        let actual = current
            .as_ref()
            .map(|row| i64_to_u64(row.get(0), "agent_runtime_thread.revision"))
            .transpose()?
            .map(RuntimeRevision);
        if actual != commit.expected_projection_revision {
            return Err(RuntimeStoreError::ProjectionConflict {
                expected: commit.expected_projection_revision,
                actual,
            });
        }
        let previous_event_sequence = current
            .as_ref()
            .map(|row| i64_to_u64(row.get(1), "agent_runtime_thread.next_event_sequence"))
            .transpose()?
            .unwrap_or(0);
        let previous_operation_sequence = current
            .as_ref()
            .map(|row| i64_to_u64(row.get(2), "agent_runtime_thread.next_operation_sequence"))
            .transpose()?
            .unwrap_or(0);
        validate_commit_sequences(
            &commit,
            previous_event_sequence,
            previous_operation_sequence,
        )?;

        write_thread(&mut tx, &commit.projection, actual.is_some()).await?;
        if let Err(error) = self.inject_failure(1) {
            tx.rollback().await.map_err(sql_error)?;
            return Err(error);
        }
        write_operation(&mut tx, commit.operation.as_ref()).await?;
        write_operation_terminals(&mut tx, &commit.operation_terminals).await?;
        if let Err(error) = self.inject_failure(2) {
            tx.rollback().await.map_err(sql_error)?;
            return Err(error);
        }
        write_events(&mut tx, &commit.events).await?;
        write_entity_projection(&mut tx, &commit.projection).await?;
        if let Err(error) = self.inject_failure(3) {
            tx.rollback().await.map_err(sql_error)?;
            return Err(error);
        }
        write_context(&mut tx, &commit).await?;
        write_hook_state(&mut tx, &commit).await?;
        if let Err(error) = self.inject_failure(4) {
            tx.rollback().await.map_err(sql_error)?;
            return Err(error);
        }
        write_outbox(&mut tx, &commit.outbox).await?;
        write_activation_outbox(&mut tx, &commit.context_activation_outbox).await?;
        write_quarantine(&mut tx, &commit.quarantine).await?;
        validate_head_projection(&mut tx, &commit.projection).await?;
        validate_hook_plan_projection(&mut tx, &commit.projection).await?;
        if let Err(error) = self.inject_failure(5) {
            tx.rollback().await.map_err(sql_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(sql_error)?;
        for event in live_events {
            self.publish_durable(event).await;
        }
        Ok(())
    }

    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError> {
        let record = encode(&event, "agent_runtime_quarantine.record")?;
        let (thread_id, binding_id, generation) = quarantine_coordinates(&event);
        sqlx::query(
            "INSERT INTO agent_runtime_quarantine \
             (thread_id,binding_id,driver_generation,reason_kind,record) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(thread_id)
        .bind(binding_id)
        .bind(generation)
        .bind(quarantine_reason(&event))
        .bind(record)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }
}

fn validate_commit_sequences(
    commit: &RuntimeCommit,
    previous_event_sequence: u64,
    previous_operation_sequence: u64,
) -> Result<(), RuntimeStoreError> {
    let mut expected_event_sequence = previous_event_sequence;
    for event in &commit.events {
        expected_event_sequence = expected_event_sequence.checked_add(1).ok_or_else(|| {
            RuntimeStoreError::Unavailable("runtime event sequence overflow".to_string())
        })?;
        if event.thread_id != commit.projection.thread_id
            || event.sequence != Some(EventSequence(expected_event_sequence))
        {
            return Err(RuntimeStoreError::Unavailable(
                "runtime commit contains a non-contiguous or cross-thread event sequence"
                    .to_string(),
            ));
        }
    }
    if commit.projection.next_event_sequence != EventSequence(expected_event_sequence) {
        return Err(RuntimeStoreError::Unavailable(
            "runtime projection event cursor does not match the committed journal".to_string(),
        ));
    }

    let expected_operation_sequence = if let Some(operation) = &commit.operation {
        let next = previous_operation_sequence.checked_add(1).ok_or_else(|| {
            RuntimeStoreError::Unavailable("runtime operation sequence overflow".to_string())
        })?;
        if operation.thread_id != commit.projection.thread_id
            || operation.operation_sequence.0 != next
        {
            return Err(RuntimeStoreError::Unavailable(
                "runtime commit contains a non-contiguous or cross-thread operation sequence"
                    .to_string(),
            ));
        }
        next
    } else {
        previous_operation_sequence
    };
    if commit.projection.next_operation_sequence.0 != expected_operation_sequence {
        return Err(RuntimeStoreError::Unavailable(
            "runtime projection operation cursor does not match the committed operation"
                .to_string(),
        ));
    }
    Ok(())
}

#[async_trait]
impl RuntimeWorkQueue for PostgresRuntimeRepository {
    async fn claim(
        &self,
        request: RuntimeWorkClaimRequest,
    ) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
        if request.owner.as_str().trim().is_empty() {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "owner must not be empty".to_string(),
            ));
        }
        if request.lease_duration_ms == 0 || request.limit == 0 {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "lease duration and limit must be positive".to_string(),
            ));
        }
        // Lease ownership is measured by the database clock so workers with skewed host clocks
        // cannot take work early or retain it past its actual expiry.
        let now: i64 =
            sqlx::query_scalar("SELECT (extract(epoch FROM clock_timestamp()) * 1000)::bigint")
                .fetch_one(&self.pool)
                .await
                .map_err(sql_error)?;
        let duration = i64::try_from(request.lease_duration_ms).map_err(|_| {
            RuntimeStoreError::InvalidWorkClaim("lease duration exceeds bigint".to_string())
        })?;
        let expires = now.checked_add(duration).ok_or_else(|| {
            RuntimeStoreError::InvalidWorkClaim("lease expiration overflow".to_string())
        })?;
        let token = RuntimeWorkClaimToken(uuid::Uuid::new_v4().to_string());
        let limit = i64::from(request.limit);
        match request.kind {
            RuntimeWorkKind::RuntimeOutbox => {
                claim_runtime_outbox(&self.pool, &request.owner, &token, now, expires, limit).await
            }
            RuntimeWorkKind::ContextPreparation => {
                claim_context_preparation(&self.pool, &request.owner, &token, now, expires, limit)
                    .await
            }
            RuntimeWorkKind::ContextActivationDispatch => {
                claim_activation_dispatch(&self.pool, &request.owner, &token, now, expires, limit)
                    .await
            }
            RuntimeWorkKind::ContextActivationRecovery => {
                claim_activation_recovery(&self.pool, &request.owner, &token, now, expires, limit)
                    .await
            }
            RuntimeWorkKind::HookEffect => {
                claim_hook_effect(&self.pool, &request.owner, &token, now, expires, limit).await
            }
            RuntimeWorkKind::HookRunRecovery => {
                claim_hook_run_recovery(&self.pool, &request.owner, &token, now, expires, limit)
                    .await
            }
        }
    }

    async fn ack(&self, claim: &RuntimeWorkClaim) -> Result<(), RuntimeStoreError> {
        update_claim(&self.pool, claim, None, true).await
    }

    async fn release(
        &self,
        claim: &RuntimeWorkClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError> {
        update_claim(&self.pool, claim, Some(error), false).await
    }
}

#[derive(Clone, Copy)]
struct ClaimLease<'a> {
    token: &'a RuntimeWorkClaimToken,
    owner: &'a RuntimeWorkerId,
    expires_at_ms: i64,
}

fn claimed<T>(
    kind: RuntimeWorkKind,
    identity: RuntimeWorkIdentity,
    lease: ClaimLease<'_>,
    attempt: i32,
    payload: T,
    wrap: impl FnOnce(T) -> RuntimeWorkPayload,
) -> Result<RuntimeWorkClaim, RuntimeStoreError> {
    Ok(RuntimeWorkClaim {
        kind,
        identity,
        token: lease.token.clone(),
        owner: lease.owner.clone(),
        lease_expires_at_ms: lease.expires_at_ms,
        attempt: u32::try_from(attempt).map_err(|_| {
            RuntimeStoreError::Unavailable("negative runtime work attempt".to_string())
        })?,
        payload: wrap(payload),
    })
}

async fn claim_runtime_outbox(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT operation_id FROM agent_runtime_outbox \
         WHERE dispatched_at IS NULL AND (claim_token IS NULL OR claim_expires_at_ms <= $1) \
         ORDER BY created_at LIMIT $5 FOR UPDATE SKIP LOCKED) \
         UPDATE agent_runtime_outbox q SET claim_token=$2,claim_owner=$3,claim_expires_at_ms=$4,\
         attempt_count=q.attempt_count+1,last_error=NULL,updated_at=now() FROM candidates c \
         WHERE q.operation_id=c.operation_id RETURNING q.operation_id,q.payload,q.attempt_count",
    )
    .bind(now)
    .bind(token.as_str())
    .bind(owner.as_str())
    .bind(expires)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload = decode(row.get(1), "agent_runtime_outbox.payload")?;
            claimed(
                RuntimeWorkKind::RuntimeOutbox,
                RuntimeWorkIdentity::Operation(runtime_operation_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::RuntimeOutbox,
            )
        })
        .collect()
}

async fn claim_context_preparation(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT compaction_id FROM agent_context_preparation \
         WHERE status='pending' AND (claim_token IS NULL OR claim_expires_at_ms <= $1) \
         ORDER BY created_at LIMIT $5 FOR UPDATE SKIP LOCKED) \
         UPDATE agent_context_preparation q SET claim_token=$2,claim_owner=$3,claim_expires_at_ms=$4,\
         attempt_count=q.attempt_count+1,last_error=NULL,updated_at=now() FROM candidates c \
         WHERE q.compaction_id=c.compaction_id RETURNING q.compaction_id,q.record,q.attempt_count",
    )
    .bind(now).bind(token.as_str()).bind(owner.as_str()).bind(expires).bind(limit)
    .fetch_all(pool).await.map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload = decode(row.get(1), "agent_context_preparation.record")?;
            claimed(
                RuntimeWorkKind::ContextPreparation,
                RuntimeWorkIdentity::Compaction(context_compaction_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::ContextPreparation,
            )
        })
        .collect()
}

async fn claim_activation_dispatch(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT d.activation_id FROM agent_context_activation_dispatch d \
         JOIN agent_context_activation a ON a.id=d.activation_id \
         WHERE d.dispatched_at IS NULL AND a.status='prepared' \
           AND (d.claim_token IS NULL OR d.claim_expires_at_ms <= $1) \
         ORDER BY d.created_at LIMIT $5 FOR UPDATE OF d SKIP LOCKED) \
         UPDATE agent_context_activation_dispatch q SET claim_token=$2,claim_owner=$3,claim_expires_at_ms=$4,\
         attempt_count=q.attempt_count+1,last_error=NULL,updated_at=now() FROM candidates c \
         WHERE q.activation_id=c.activation_id RETURNING q.activation_id,q.payload,q.attempt_count",
    )
    .bind(now).bind(token.as_str()).bind(owner.as_str()).bind(expires).bind(limit)
    .fetch_all(pool).await.map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload = decode(row.get(1), "agent_context_activation_dispatch.payload")?;
            claimed(
                RuntimeWorkKind::ContextActivationDispatch,
                RuntimeWorkIdentity::Activation(context_activation_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::ContextActivationDispatch,
            )
        })
        .collect()
}

async fn claim_activation_recovery(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT id FROM agent_context_activation \
         WHERE status IN ('prepared','applied') AND (recovery_claim_token IS NULL OR recovery_claim_expires_at_ms <= $1) \
         ORDER BY updated_at LIMIT $5 FOR UPDATE SKIP LOCKED) \
         UPDATE agent_context_activation q SET recovery_claim_token=$2,recovery_claim_owner=$3,\
         recovery_claim_expires_at_ms=$4,recovery_attempt_count=q.recovery_attempt_count+1,\
         recovery_last_error=NULL,updated_at=now() FROM candidates c WHERE q.id=c.id \
         RETURNING q.id,q.record,q.recovery_attempt_count",
    )
    .bind(now).bind(token.as_str()).bind(owner.as_str()).bind(expires).bind(limit)
    .fetch_all(pool).await.map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload = decode(row.get(1), "agent_context_activation.record")?;
            claimed(
                RuntimeWorkKind::ContextActivationRecovery,
                RuntimeWorkIdentity::Activation(context_activation_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::ContextActivationRecovery,
            )
        })
        .collect()
}

async fn claim_hook_effect(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT id FROM agent_runtime_hook_effect \
         WHERE dispatched_at IS NULL AND attempt_count <= retry_limit \
           AND (claim_token IS NULL OR claim_expires_at_ms <= $1) \
         ORDER BY created_at LIMIT $5 FOR UPDATE SKIP LOCKED) \
         UPDATE agent_runtime_hook_effect q SET claim_token=$2,claim_owner=$3,claim_expires_at_ms=$4,\
         attempt_count=q.attempt_count+1,last_error=NULL,updated_at=now() FROM candidates c \
         WHERE q.id=c.id RETURNING q.id,q.record,q.attempt_count",
    )
    .bind(now)
    .bind(token.as_str())
    .bind(owner.as_str())
    .bind(expires)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload: HookEffect = decode(row.get(1), "agent_runtime_hook_effect.record")?;
            claimed(
                RuntimeWorkKind::HookEffect,
                RuntimeWorkIdentity::HookEffect(hook_effect_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::HookEffect,
            )
        })
        .collect()
}

async fn claim_hook_run_recovery(
    pool: &PgPool,
    owner: &RuntimeWorkerId,
    token: &RuntimeWorkClaimToken,
    now: i64,
    expires: i64,
    limit: i64,
) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError> {
    let rows = sqlx::query(
        "WITH candidates AS (SELECT id FROM agent_runtime_hook_run \
         WHERE status IN ('accepted','running') \
           AND (recovery_claim_token IS NULL OR recovery_claim_expires_at_ms <= $1) \
         ORDER BY updated_at LIMIT $5 FOR UPDATE SKIP LOCKED) \
         UPDATE agent_runtime_hook_run q SET recovery_claim_token=$2,recovery_claim_owner=$3,\
         recovery_claim_expires_at_ms=$4,recovery_attempt_count=q.recovery_attempt_count+1,\
         recovery_last_error=NULL,updated_at=now() FROM candidates c WHERE q.id=c.id \
         RETURNING q.id,q.record,q.recovery_attempt_count",
    )
    .bind(now)
    .bind(token.as_str())
    .bind(owner.as_str())
    .bind(expires)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let payload: HookRun = decode(row.get(1), "agent_runtime_hook_run.record")?;
            claimed(
                RuntimeWorkKind::HookRunRecovery,
                RuntimeWorkIdentity::HookRun(hook_run_id(id)?),
                ClaimLease {
                    token,
                    owner,
                    expires_at_ms: expires,
                },
                row.get(2),
                payload,
                RuntimeWorkPayload::HookRunRecovery,
            )
        })
        .collect()
}

async fn update_claim(
    pool: &PgPool,
    claim: &RuntimeWorkClaim,
    error: Option<String>,
    ack: bool,
) -> Result<(), RuntimeStoreError> {
    let (sql, id, expiry_column) = match (&claim.kind, &claim.identity) {
        (RuntimeWorkKind::RuntimeOutbox, RuntimeWorkIdentity::Operation(id)) => (
            if ack {
                "UPDATE agent_runtime_outbox SET dispatched_at=now(),claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=NULL,updated_at=now() WHERE operation_id=$1 AND claim_owner=$2 AND claim_token=$3"
            } else {
                "UPDATE agent_runtime_outbox SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=$4,updated_at=now() WHERE operation_id=$1 AND claim_owner=$2 AND claim_token=$3"
            },
            id.as_str(),
            "claim_expires_at_ms",
        ),
        (RuntimeWorkKind::ContextPreparation, RuntimeWorkIdentity::Compaction(id)) => (
            if ack {
                "UPDATE agent_context_preparation SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=NULL,updated_at=now() WHERE compaction_id=$1 AND claim_owner=$2 AND claim_token=$3"
            } else {
                "UPDATE agent_context_preparation SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=$4,updated_at=now() WHERE compaction_id=$1 AND claim_owner=$2 AND claim_token=$3"
            },
            id.as_str(),
            "claim_expires_at_ms",
        ),
        (RuntimeWorkKind::ContextActivationDispatch, RuntimeWorkIdentity::Activation(id)) => (
            if ack {
                "UPDATE agent_context_activation_dispatch SET dispatched_at=now(),claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=NULL,updated_at=now() WHERE activation_id=$1 AND claim_owner=$2 AND claim_token=$3"
            } else {
                "UPDATE agent_context_activation_dispatch SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=$4,updated_at=now() WHERE activation_id=$1 AND claim_owner=$2 AND claim_token=$3"
            },
            id.as_str(),
            "claim_expires_at_ms",
        ),
        (RuntimeWorkKind::ContextActivationRecovery, RuntimeWorkIdentity::Activation(id)) => (
            if ack {
                "UPDATE agent_context_activation SET recovery_claim_token=NULL,recovery_claim_owner=NULL,recovery_claim_expires_at_ms=NULL,recovery_last_error=NULL,updated_at=now() WHERE id=$1 AND recovery_claim_owner=$2 AND recovery_claim_token=$3"
            } else {
                "UPDATE agent_context_activation SET recovery_claim_token=NULL,recovery_claim_owner=NULL,recovery_claim_expires_at_ms=NULL,recovery_last_error=$4,updated_at=now() WHERE id=$1 AND recovery_claim_owner=$2 AND recovery_claim_token=$3"
            },
            id.as_str(),
            "recovery_claim_expires_at_ms",
        ),
        (RuntimeWorkKind::HookEffect, RuntimeWorkIdentity::HookEffect(id)) => (
            if ack {
                "UPDATE agent_runtime_hook_effect SET dispatched_at=now(),claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=NULL,updated_at=now() WHERE id=$1 AND claim_owner=$2 AND claim_token=$3"
            } else {
                "UPDATE agent_runtime_hook_effect SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=$4,updated_at=now() WHERE id=$1 AND claim_owner=$2 AND claim_token=$3"
            },
            id.as_str(),
            "claim_expires_at_ms",
        ),
        (RuntimeWorkKind::HookRunRecovery, RuntimeWorkIdentity::HookRun(id)) => (
            if ack {
                "UPDATE agent_runtime_hook_run SET recovery_claim_token=NULL,recovery_claim_owner=NULL,recovery_claim_expires_at_ms=NULL,recovery_last_error=NULL,updated_at=now() WHERE id=$1 AND recovery_claim_owner=$2 AND recovery_claim_token=$3"
            } else {
                "UPDATE agent_runtime_hook_run SET recovery_claim_token=NULL,recovery_claim_owner=NULL,recovery_claim_expires_at_ms=NULL,recovery_last_error=$4,updated_at=now() WHERE id=$1 AND recovery_claim_owner=$2 AND recovery_claim_token=$3"
            },
            id.as_str(),
            "recovery_claim_expires_at_ms",
        ),
        _ => {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "work kind and identity do not match".to_string(),
            ));
        }
    };
    let sql = format!(
        "{sql} AND {expiry_column} > (extract(epoch FROM clock_timestamp()) * 1000)::bigint"
    );
    let mut query = sqlx::query(&sql)
        .bind(id)
        .bind(claim.owner.as_str())
        .bind(claim.token.as_str());
    if !ack {
        query = query.bind(error.unwrap_or_default());
    }
    let result = query.execute(pool).await.map_err(sql_error)?;
    if result.rows_affected() != 1 {
        return Err(RuntimeStoreError::WorkClaimConflict);
    }
    Ok(())
}

fn runtime_operation_id(value: String) -> Result<RuntimeOperationId, RuntimeStoreError> {
    RuntimeOperationId::new(value)
        .map_err(|error| RuntimeStoreError::Unavailable(error.to_string()))
}

fn context_compaction_id(value: String) -> Result<ContextCompactionId, RuntimeStoreError> {
    ContextCompactionId::new(value)
        .map_err(|error| RuntimeStoreError::Unavailable(error.to_string()))
}

fn context_activation_id(value: String) -> Result<ContextActivationId, RuntimeStoreError> {
    ContextActivationId::new(value)
        .map_err(|error| RuntimeStoreError::Unavailable(error.to_string()))
}

fn hook_effect_id(value: String) -> Result<HookEffectId, RuntimeStoreError> {
    HookEffectId::new(value).map_err(|error| RuntimeStoreError::Unavailable(error.to_string()))
}

fn hook_run_id(value: String) -> Result<HookRunId, RuntimeStoreError> {
    HookRunId::new(value).map_err(|error| RuntimeStoreError::Unavailable(error.to_string()))
}

async fn write_thread(
    tx: &mut Transaction<'_, Postgres>,
    state: &RuntimeThreadState,
    exists: bool,
) -> Result<(), RuntimeStoreError> {
    let projection = encode(state, "agent_runtime_thread.projection")?;
    let values = (
        u64_to_i64(state.revision.0, "runtime revision")?,
        u64_to_i64(state.next_event_sequence.0, "event sequence")?,
        u64_to_i64(state.next_operation_sequence.0, "operation sequence")?,
        u64_to_i64(state.driver_generation.0, "driver generation")?,
        u64_to_i64(state.context_revision.0, "context revision")?,
        u64_to_i64(state.settings_revision.0, "settings revision")?,
        u64_to_i64(state.tool_set_revision.0, "tool set revision")?,
    );
    let mut query = if exists {
        sqlx::query(
            "UPDATE agent_runtime_thread SET revision=$2,next_event_sequence=$3,\
             next_operation_sequence=$4,status=$5,active_turn_id=$6,binding_id=$7,\
             driver_generation=$8,source_thread_id=$9,profile_digest=$10,active_checkpoint_id=$11,\
             context_revision=$12,settings_revision=$13,tool_set_revision=$14,projection=$15,updated_at=now() \
             WHERE id=$1",
        )
    } else {
        sqlx::query(
            "INSERT INTO agent_runtime_thread \
             (id,revision,next_event_sequence,next_operation_sequence,status,active_turn_id,binding_id,\
              driver_generation,source_thread_id,profile_digest,active_checkpoint_id,context_revision,\
              settings_revision,tool_set_revision,projection) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) \
             ON CONFLICT (id) DO NOTHING",
        )
    };
    query = query
        .bind(state.thread_id.as_str())
        .bind(values.0)
        .bind(values.1)
        .bind(values.2)
        .bind(thread_status(state))
        .bind(state.active_turn_id.as_ref().map(|id| id.as_str()))
        .bind(state.binding_id.as_str())
        .bind(values.3)
        .bind(state.source_thread_id.as_str())
        .bind(state.profile_digest.as_str())
        .bind(state.active_checkpoint_id.as_ref().map(|id| id.as_str()))
        .bind(values.4)
        .bind(values.5)
        .bind(values.6)
        .bind(projection);
    let result = query.execute(&mut **tx).await.map_err(sql_error)?;
    if !exists && result.rows_affected() != 1 {
        let actual =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM agent_runtime_thread WHERE id=$1")
                .bind(state.thread_id.as_str())
                .fetch_optional(&mut **tx)
                .await
                .map_err(sql_error)?
                .map(|value| i64_to_u64(value, "agent_runtime_thread.revision"))
                .transpose()?
                .map(RuntimeRevision);
        return Err(RuntimeStoreError::ProjectionConflict {
            expected: None,
            actual,
        });
    }
    Ok(())
}

async fn write_operation(
    tx: &mut Transaction<'_, Postgres>,
    operation: Option<&RuntimeOperationRecord>,
) -> Result<(), RuntimeStoreError> {
    let Some(operation) = operation else {
        return Ok(());
    };
    let terminal = operation
        .terminal
        .as_ref()
        .map(|value| encode(value, "operation terminal"))
        .transpose()?;
    let result = sqlx::query(
        "INSERT INTO agent_runtime_operation \
         (id,thread_id,operation_sequence,idempotency_key,accepted_revision,status,actor,command,terminal,record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
    )
    .bind(operation.operation_id.as_str())
    .bind(operation.thread_id.as_str())
    .bind(u64_to_i64(operation.operation_sequence.0, "operation sequence")?)
    .bind(operation.idempotency_key.as_str())
    .bind(u64_to_i64(operation.accepted_revision.0, "accepted revision")?)
    .bind(operation_status(operation.terminal.as_ref()))
    .bind(encode(&operation.actor, "operation actor")?)
    .bind(encode(&operation.command, "operation command")?)
    .bind(terminal)
    .bind(encode(operation, "operation record")?)
    .execute(&mut **tx)
    .await;
    match result {
        Ok(result) if result.rows_affected() == 1 => Ok(()),
        Ok(_) => {
            if sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM agent_runtime_operation WHERE id=$1)",
            )
            .bind(operation.operation_id.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)?
            {
                return Err(RuntimeStoreError::OperationConflict {
                    operation_id: operation.operation_id.clone(),
                });
            }
            let existing = sqlx::query_scalar::<_, String>(
                "SELECT id FROM agent_runtime_operation WHERE thread_id=$1 AND idempotency_key=$2",
            )
            .bind(operation.thread_id.as_str())
            .bind(operation.idempotency_key.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(sql_error)?;
            match existing {
                Some(existing) => Err(RuntimeStoreError::IdempotencyConflict {
                    operation_id: runtime_operation_id(existing)?,
                }),
                None => Err(RuntimeStoreError::OperationConflict {
                    operation_id: operation.operation_id.clone(),
                }),
            }
        }
        Err(error) => Err(sql_error(error)),
    }
}

async fn write_operation_terminals(
    tx: &mut Transaction<'_, Postgres>,
    terminals: &[(RuntimeOperationId, RuntimeOperationTerminal)],
) -> Result<(), RuntimeStoreError> {
    for (operation_id, terminal) in terminals {
        let row = sqlx::query("SELECT record FROM agent_runtime_operation WHERE id=$1 FOR UPDATE")
            .bind(operation_id.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| RuntimeStoreError::OperationConflict {
                operation_id: operation_id.clone(),
            })?;
        let mut record: RuntimeOperationRecord =
            decode(row.get(0), "agent_runtime_operation.record")?;
        if record.terminal.is_some() {
            return Err(RuntimeStoreError::OperationConflict {
                operation_id: operation_id.clone(),
            });
        }
        record.terminal = Some(terminal.clone());
        sqlx::query(
            "UPDATE agent_runtime_operation SET status=$2,terminal=$3,record=$4,updated_at=now() \
             WHERE id=$1 AND terminal IS NULL",
        )
        .bind(operation_id.as_str())
        .bind(operation_status(Some(terminal)))
        .bind(encode(terminal, "operation terminal")?)
        .bind(encode(&record, "operation record")?)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    }
    Ok(())
}

async fn write_events(
    tx: &mut Transaction<'_, Postgres>,
    events: &[RuntimeEventEnvelope],
) -> Result<(), RuntimeStoreError> {
    for event in events {
        let sequence = event.sequence.ok_or_else(|| {
            RuntimeStoreError::Unavailable(
                "transient event cannot enter durable journal".to_string(),
            )
        })?;
        let value = encode(event, "agent_runtime_event.envelope")?;
        let kind = value
            .get("event")
            .and_then(|event| event.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        sqlx::query(
            "INSERT INTO agent_runtime_event (thread_id,event_sequence,revision,event_kind,envelope) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(event.thread_id.as_str())
        .bind(u64_to_i64(sequence.0, "event sequence")?)
        .bind(u64_to_i64(event.revision.0, "event revision")?)
        .bind(kind)
        .bind(value)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    }
    Ok(())
}

async fn write_entity_projection(
    tx: &mut Transaction<'_, Postgres>,
    state: &RuntimeThreadState,
) -> Result<(), RuntimeStoreError> {
    sqlx::query("DELETE FROM agent_runtime_interaction WHERE thread_id=$1")
        .bind(state.thread_id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    sqlx::query("DELETE FROM agent_runtime_item WHERE thread_id=$1")
        .bind(state.thread_id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    sqlx::query("DELETE FROM agent_runtime_turn WHERE thread_id=$1")
        .bind(state.thread_id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    for (id, turn) in &state.turns {
        insert_turn(tx, state, id.as_str(), turn).await?;
    }
    for (index, id) in state.item_order.iter().enumerate() {
        let item = state.items.get(id).ok_or_else(|| {
            RuntimeStoreError::Unavailable("item order references missing item".to_string())
        })?;
        insert_item(tx, state, id.as_str(), index, item).await?;
    }
    for (id, interaction) in &state.interactions {
        insert_interaction(tx, state, id.as_str(), interaction).await?;
    }
    Ok(())
}

async fn insert_turn(
    tx: &mut Transaction<'_, Postgres>,
    thread: &RuntimeThreadState,
    id: &str,
    state: &RuntimeTurnState,
) -> Result<(), RuntimeStoreError> {
    sqlx::query("INSERT INTO agent_runtime_turn (id,thread_id,phase,state) VALUES ($1,$2,$3,$4)")
        .bind(id)
        .bind(thread.thread_id.as_str())
        .bind(entity_phase(&state.phase))
        .bind(encode(state, "agent_runtime_turn.state")?)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    Ok(())
}

async fn insert_item(
    tx: &mut Transaction<'_, Postgres>,
    thread: &RuntimeThreadState,
    id: &str,
    index: usize,
    state: &RuntimeItemState,
) -> Result<(), RuntimeStoreError> {
    sqlx::query("INSERT INTO agent_runtime_item (id,thread_id,turn_id,sort_order,phase,state) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(thread.thread_id.as_str()).bind(state.turn_id.as_str())
        .bind(i64::try_from(index).map_err(|_| RuntimeStoreError::Unavailable("item order overflow".to_string()))?)
        .bind(entity_phase(&state.phase)).bind(encode(state, "agent_runtime_item.state")?)
        .execute(&mut **tx).await.map_err(sql_error)?;
    Ok(())
}

async fn insert_interaction(
    tx: &mut Transaction<'_, Postgres>,
    thread: &RuntimeThreadState,
    id: &str,
    state: &RuntimeInteractionState,
) -> Result<(), RuntimeStoreError> {
    sqlx::query("INSERT INTO agent_runtime_interaction (id,thread_id,turn_id,phase,state) VALUES ($1,$2,$3,$4,$5)")
        .bind(id).bind(thread.thread_id.as_str()).bind(state.turn_id.as_str())
        .bind(entity_phase(&state.phase)).bind(encode(state, "agent_runtime_interaction.state")?)
        .execute(&mut **tx).await.map_err(sql_error)?;
    Ok(())
}

async fn write_context(
    tx: &mut Transaction<'_, Postgres>,
    commit: &RuntimeCommit,
) -> Result<(), RuntimeStoreError> {
    for checkpoint in &commit.context_checkpoints {
        write_checkpoint(tx, checkpoint).await?;
    }
    for work in &commit.context_preparation_work_items {
        write_preparation(tx, work).await?;
    }
    for candidate in &commit.context_candidates {
        write_candidate(tx, candidate).await?;
    }
    for activation in &commit.context_activations {
        write_activation(tx, activation).await?;
    }
    if let Some(head) = &commit.context_head {
        write_head(tx, head).await?;
    }
    Ok(())
}

async fn write_checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    checkpoint: &ContextCheckpoint,
) -> Result<(), RuntimeStoreError> {
    let result = sqlx::query(
        "INSERT INTO agent_context_checkpoint \
         (id,thread_id,revision,digest,fidelity,settings_revision,tool_set_revision,record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (id) DO NOTHING",
    )
    .bind(checkpoint.checkpoint_id.as_str())
    .bind(checkpoint.thread_id.as_str())
    .bind(u64_to_i64(checkpoint.revision.0, "checkpoint revision")?)
    .bind(checkpoint.materialized.digest.as_str())
    .bind(fidelity(checkpoint.materialized.fidelity))
    .bind(u64_to_i64(
        checkpoint
            .materialized
            .recipe
            .provenance
            .settings_revision
            .0,
        "settings revision",
    )?)
    .bind(u64_to_i64(
        checkpoint
            .materialized
            .recipe
            .provenance
            .tool_set_revision
            .0,
        "tool set revision",
    )?)
    .bind(encode(checkpoint, "agent_context_checkpoint.record")?)
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    if result.rows_affected() == 0 {
        let existing: Value =
            sqlx::query_scalar("SELECT record FROM agent_context_checkpoint WHERE id=$1")
                .bind(checkpoint.checkpoint_id.as_str())
                .fetch_one(&mut **tx)
                .await
                .map_err(sql_error)?;
        if existing != encode(checkpoint, "agent_context_checkpoint.record")? {
            return Err(RuntimeStoreError::ContextInvariant {
                violation: ContextStoreInvariant::CheckpointIdentity,
            });
        }
    }
    Ok(())
}

async fn write_preparation(
    tx: &mut Transaction<'_, Postgres>,
    work: &ContextPreparationWorkItem,
) -> Result<(), RuntimeStoreError> {
    let result = sqlx::query(
        "INSERT INTO agent_context_preparation \
         (compaction_id,operation_id,thread_id,trigger_kind,expected_base_checkpoint_id,expected_base_revision,status,record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (compaction_id) DO UPDATE SET \
         status=excluded.status,record=excluded.record,updated_at=now() \
         WHERE agent_context_preparation.operation_id=excluded.operation_id \
           AND agent_context_preparation.thread_id=excluded.thread_id \
           AND agent_context_preparation.trigger_kind=excluded.trigger_kind \
           AND agent_context_preparation.expected_base_checkpoint_id IS NOT DISTINCT FROM excluded.expected_base_checkpoint_id \
           AND agent_context_preparation.expected_base_revision=excluded.expected_base_revision \
           AND ((agent_context_preparation.status='pending' AND excluded.status IN ('pending','prepared')) \
             OR (agent_context_preparation.status='prepared' AND excluded.status IN ('prepared','terminal')) \
             OR (agent_context_preparation.status='terminal' AND excluded.status='terminal'))",
    )
    .bind(work.compaction_id.as_str()).bind(work.operation_id.as_str()).bind(work.thread_id.as_str())
    .bind(trigger(work.trigger)).bind(work.expected_base_checkpoint_id.as_ref().map(|id| id.as_str()))
    .bind(u64_to_i64(work.expected_base_revision.0, "expected base revision")?)
    .bind(preparation_status(&work.status)).bind(encode(work, "agent_context_preparation.record")?)
    .execute(&mut **tx).await.map_err(sql_error)?;
    if result.rows_affected() != 1 {
        return Err(RuntimeStoreError::ContextInvariant {
            violation: ContextStoreInvariant::PreparationTransition,
        });
    }
    Ok(())
}

async fn write_candidate(
    tx: &mut Transaction<'_, Postgres>,
    candidate: &ContextCandidate,
) -> Result<(), RuntimeStoreError> {
    let result = sqlx::query(
        "INSERT INTO agent_context_candidate \
         (id,compaction_id,operation_id,activation_id,thread_id,checkpoint_id,expected_base_checkpoint_id,expected_base_revision,record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (compaction_id) DO NOTHING",
    )
    .bind(candidate.candidate_id.as_str()).bind(candidate.compaction_id.as_str())
    .bind(candidate.operation_id.as_str()).bind(candidate.activation_id.as_str())
    .bind(candidate.thread_id.as_str()).bind(candidate.checkpoint.checkpoint_id.as_str())
    .bind(candidate.expected_base_checkpoint_id.as_ref().map(|id| id.as_str()))
    .bind(u64_to_i64(candidate.expected_base_revision.0, "expected base revision")?)
    .bind(encode(candidate, "agent_context_candidate.record")?)
    .execute(&mut **tx).await.map_err(sql_error)?;
    if result.rows_affected() == 0 {
        let existing: Value =
            sqlx::query_scalar("SELECT record FROM agent_context_candidate WHERE compaction_id=$1")
                .bind(candidate.compaction_id.as_str())
                .fetch_one(&mut **tx)
                .await
                .map_err(sql_error)?;
        if existing != encode(candidate, "agent_context_candidate.record")? {
            return Err(RuntimeStoreError::ContextInvariant {
                violation: ContextStoreInvariant::CandidateIdentity,
            });
        }
    }
    Ok(())
}

async fn write_activation(
    tx: &mut Transaction<'_, Postgres>,
    activation: &ContextActivation,
) -> Result<(), RuntimeStoreError> {
    let current = sqlx::query_scalar::<_, Value>(
        "SELECT record FROM agent_context_activation WHERE id=$1 FOR UPDATE",
    )
    .bind(activation.activation_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(sql_error)?;
    if let Some(current) = current {
        let current: ContextActivation = decode(current, "agent_context_activation.record")?;
        if current.candidate_id != activation.candidate_id
            || current.compaction_id != activation.compaction_id
            || current.thread_id != activation.thread_id
            || !valid_activation_transition(&current.status, &activation.status)
        {
            return Err(RuntimeStoreError::ContextInvariant {
                violation: ContextStoreInvariant::ActivationTransition,
            });
        }
        let (digest, driver_revision) = activation_applied(activation);
        sqlx::query("UPDATE agent_context_activation SET status=$2,applied_digest=$3,driver_context_revision=$4,record=$5,updated_at=now() WHERE id=$1")
            .bind(activation.activation_id.as_str()).bind(activation_status(&activation.status))
            .bind(digest).bind(driver_revision).bind(encode(activation, "agent_context_activation.record")?)
            .execute(&mut **tx).await.map_err(sql_error)?;
    } else {
        if !matches!(activation.status, ContextActivationStatus::Prepared) {
            return Err(RuntimeStoreError::ContextInvariant {
                violation: ContextStoreInvariant::ActivationTransition,
            });
        }
        sqlx::query("INSERT INTO agent_context_activation (id,candidate_id,compaction_id,thread_id,status,record) VALUES ($1,$2,$3,$4,'prepared',$5)")
            .bind(activation.activation_id.as_str()).bind(activation.candidate_id.as_str())
            .bind(activation.compaction_id.as_str()).bind(activation.thread_id.as_str())
            .bind(encode(activation, "agent_context_activation.record")?)
            .execute(&mut **tx).await.map_err(sql_error)?;
    }
    Ok(())
}

fn activation_applied(activation: &ContextActivation) -> (Option<&str>, Option<&str>) {
    match &activation.status {
        ContextActivationStatus::Applied {
            digest,
            driver_context_revision,
        } => (
            Some(digest.as_str()),
            Some(driver_context_revision.as_str()),
        ),
        ContextActivationStatus::Terminal {
            applied: Some(applied),
            ..
        } => (
            Some(applied.digest.as_str()),
            Some(applied.driver_context_revision.as_str()),
        ),
        _ => (None, None),
    }
}

fn valid_activation_transition(
    current: &ContextActivationStatus,
    next: &ContextActivationStatus,
) -> bool {
    if current == next {
        return true;
    }
    match (current, next) {
        (ContextActivationStatus::Prepared, ContextActivationStatus::Applied { .. }) => true,
        (
            ContextActivationStatus::Prepared,
            ContextActivationStatus::Terminal { applied: None, .. },
        ) => true,
        (
            ContextActivationStatus::Applied {
                digest,
                driver_context_revision,
            },
            ContextActivationStatus::Terminal {
                applied: Some(applied),
                ..
            },
        ) => {
            applied.digest == *digest && applied.driver_context_revision == *driver_context_revision
        }
        _ => false,
    }
}

async fn write_head(
    tx: &mut Transaction<'_, Postgres>,
    write: &ContextHeadWrite,
) -> Result<(), RuntimeStoreError> {
    let actual = sqlx::query_scalar::<_, i64>(
        "SELECT revision FROM agent_context_head WHERE thread_id=$1 FOR UPDATE",
    )
    .bind(write.head.thread_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(sql_error)?
    .map(|value| i64_to_u64(value, "context head revision"))
    .transpose()?
    .map(agentdash_agent_runtime_contract::ContextRevision);
    if actual != write.expected_revision {
        return Err(RuntimeStoreError::ContextHeadConflict {
            expected: write.expected_revision,
            actual,
        });
    }
    let next = write
        .expected_revision
        .map_or(1, |revision| revision.0.saturating_add(1));
    if write.head.revision.0 != next || write.head.fidelity == ContextFidelity::Opaque {
        return Err(RuntimeStoreError::ContextInvariant {
            violation: ContextStoreInvariant::HeadCheckpointMismatch,
        });
    }
    let settings_revision = u64_to_i64(
        write.head.provenance.settings_revision.0,
        "head settings revision",
    )?;
    let tool_set_revision = u64_to_i64(
        write.head.provenance.tool_set_revision.0,
        "head tool set revision",
    )?;
    let checkpoint_matches: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_context_checkpoint \
         WHERE thread_id=$1 AND id=$2 AND revision=$3 AND digest=$4 AND fidelity=$5 \
           AND settings_revision=$6 AND tool_set_revision=$7)",
    )
    .bind(write.head.thread_id.as_str())
    .bind(write.head.checkpoint_id.as_str())
    .bind(u64_to_i64(write.head.revision.0, "context head revision")?)
    .bind(write.head.digest.as_str())
    .bind(fidelity(write.head.fidelity))
    .bind(settings_revision)
    .bind(tool_set_revision)
    .fetch_one(&mut **tx)
    .await
    .map_err(sql_error)?;
    if !checkpoint_matches {
        return Err(RuntimeStoreError::ContextInvariant {
            violation: ContextStoreInvariant::HeadCheckpointMismatch,
        });
    }
    sqlx::query(
        "INSERT INTO agent_context_head (thread_id,checkpoint_id,revision,digest,fidelity,settings_revision,tool_set_revision,provenance,record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (thread_id) DO UPDATE SET \
         checkpoint_id=excluded.checkpoint_id,revision=excluded.revision,digest=excluded.digest,\
         fidelity=excluded.fidelity,settings_revision=excluded.settings_revision,\
         tool_set_revision=excluded.tool_set_revision,provenance=excluded.provenance,\
         record=excluded.record,updated_at=now()",
    )
    .bind(write.head.thread_id.as_str()).bind(write.head.checkpoint_id.as_str())
    .bind(u64_to_i64(write.head.revision.0, "context head revision")?)
    .bind(write.head.digest.as_str()).bind(fidelity(write.head.fidelity))
    .bind(settings_revision).bind(tool_set_revision)
    .bind(encode(&write.head.provenance, "agent_context_head.provenance")?)
    .bind(encode(&write.head, "agent_context_head.record")?)
    .execute(&mut **tx).await.map_err(sql_error)?;
    Ok(())
}

async fn write_outbox(
    tx: &mut Transaction<'_, Postgres>,
    entries: &[RuntimeOutboxEntry],
) -> Result<(), RuntimeStoreError> {
    for entry in entries {
        let payload = encode(entry, "agent_runtime_outbox.payload")?;
        let result = sqlx::query("INSERT INTO agent_runtime_outbox (operation_id,thread_id,driver_generation,payload) VALUES ($1,$2,$3,$4) ON CONFLICT (operation_id) DO NOTHING")
            .bind(entry.operation_id.as_str()).bind(entry.thread_id.as_str())
            .bind(u64_to_i64(entry.generation.0, "driver generation")?).bind(payload.clone())
            .execute(&mut **tx).await.map_err(sql_error)?;
        if result.rows_affected() == 0 {
            let current: Value = sqlx::query_scalar(
                "SELECT payload FROM agent_runtime_outbox WHERE operation_id=$1",
            )
            .bind(entry.operation_id.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)?;
            if current != payload {
                return Err(RuntimeStoreError::OperationConflict {
                    operation_id: entry.operation_id.clone(),
                });
            }
        }
    }
    Ok(())
}

async fn write_hook_state(
    tx: &mut Transaction<'_, Postgres>,
    commit: &RuntimeCommit,
) -> Result<(), RuntimeStoreError> {
    if let Some(binding) = &commit.hook_plan_binding {
        if binding.thread_id != commit.projection.thread_id
            || commit.projection.hook_plan_revision != Some(binding.plan.revision)
            || commit.projection.hook_plan_digest.as_ref() != Some(&binding.plan.digest)
        {
            return Err(RuntimeStoreError::Unavailable(
                "hook plan binding does not match thread projection".to_string(),
            ));
        }
        let current = sqlx::query(
            "SELECT revision,digest,binding FROM agent_runtime_hook_plan WHERE thread_id=$1 ORDER BY revision DESC LIMIT 1",
        )
        .bind(binding.thread_id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(sql_error)?;
        if let Some(row) = current {
            let revision = i64_to_u64(row.get(0), "hook plan revision")?;
            let digest: String = row.get(1);
            let record: Value = row.get(2);
            let encoded = encode(binding, "agent_runtime_hook_plan.binding")?;
            if revision == binding.plan.revision.0
                && digest == binding.plan.digest.as_str()
                && record == encoded
            {
                // Exact replay is idempotent.
            } else if binding.plan.revision.0 == revision.saturating_add(1) {
                sqlx::query(
                    "INSERT INTO agent_runtime_hook_plan (thread_id,revision,digest,binding) VALUES ($1,$2,$3,$4)",
                )
                .bind(binding.thread_id.as_str())
                .bind(u64_to_i64(binding.plan.revision.0, "hook plan revision")?)
                .bind(binding.plan.digest.as_str())
                .bind(encoded)
                .execute(&mut **tx)
                .await
                .map_err(sql_error)?;
            } else {
                return Err(RuntimeStoreError::Unavailable(
                    "hook plan revision must advance exactly once".to_string(),
                ));
            }
        } else {
            if binding.plan.revision.0 != 1 {
                return Err(RuntimeStoreError::Unavailable(
                    "first hook plan revision must be one".to_string(),
                ));
            }
            sqlx::query(
                "INSERT INTO agent_runtime_hook_plan (thread_id,revision,digest,binding) VALUES ($1,$2,$3,$4)",
            )
            .bind(binding.thread_id.as_str())
            .bind(u64_to_i64(binding.plan.revision.0, "hook plan revision")?)
            .bind(binding.plan.digest.as_str())
            .bind(encode(binding, "agent_runtime_hook_plan.binding")?)
            .execute(&mut **tx)
            .await
            .map_err(sql_error)?;
        }
    }

    for run in &commit.hook_runs {
        let current =
            sqlx::query("SELECT record FROM agent_runtime_hook_run WHERE id=$1 FOR UPDATE")
                .bind(run.hook_run_id.as_str())
                .fetch_optional(&mut **tx)
                .await
                .map_err(sql_error)?;
        if let Some(row) = current {
            let existing: HookRun = decode(row.get(0), "agent_runtime_hook_run.record")?;
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
            if existing != *run
                && !(immutable_matches
                    && ((existing.status == HookRunStatus::Accepted
                        && run.status == HookRunStatus::Running)
                        || (existing.status == HookRunStatus::Running && run.status.is_terminal())))
            {
                return Err(RuntimeStoreError::Unavailable(
                    "invalid hook run transition".to_string(),
                ));
            }
            sqlx::query(
                "UPDATE agent_runtime_hook_run SET status=$2,record=$3,updated_at=now() WHERE id=$1",
            )
            .bind(run.hook_run_id.as_str())
            .bind(hook_run_status(run.status))
            .bind(encode(run, "agent_runtime_hook_run.record")?)
            .execute(&mut **tx)
            .await
            .map_err(sql_error)?;
        } else {
            if run.status != HookRunStatus::Accepted {
                return Err(RuntimeStoreError::Unavailable(
                    "new hook run must be accepted".to_string(),
                ));
            }
            sqlx::query(
                "INSERT INTO agent_runtime_hook_run (id,thread_id,definition_id,point,plan_revision,plan_digest,operation_id,turn_id,item_id,interaction_id,status,record) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            )
            .bind(run.hook_run_id.as_str())
            .bind(run.thread_id.as_str())
            .bind(run.definition_id.as_str())
            .bind(hook_point(run.point))
            .bind(u64_to_i64(run.plan_revision.0, "hook plan revision")?)
            .bind(run.plan_digest.as_str())
            .bind(run.correlation.operation_id.as_ref().map(|id| id.as_str()))
            .bind(run.correlation.turn_id.as_ref().map(|id| id.as_str()))
            .bind(run.correlation.item_id.as_ref().map(|id| id.as_str()))
            .bind(run.correlation.interaction_id.as_ref().map(|id| id.as_str()))
            .bind(hook_run_status(run.status))
            .bind(encode(run, "agent_runtime_hook_run.record")?)
            .execute(&mut **tx)
            .await
            .map_err(sql_error)?;
        }
    }

    for effect in &commit.hook_effects {
        let run_record: Option<Value> = sqlx::query_scalar(
            "SELECT record FROM agent_runtime_hook_run WHERE id=$1 AND thread_id=$2",
        )
        .bind(effect.hook_run_id.as_str())
        .bind(effect.thread_id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(sql_error)?;
        let Some(run_record) = run_record else {
            return Err(RuntimeStoreError::Unavailable(
                "hook effect requires a terminal hook run".to_string(),
            ));
        };
        let run: HookRun = decode(run_record, "agent_runtime_hook_run.record")?;
        if !run.status.is_terminal() {
            return Err(RuntimeStoreError::Unavailable(
                "hook effect requires a terminal hook run".to_string(),
            ));
        }
        run.validate_effect(effect).map_err(|error| {
            RuntimeStoreError::Unavailable(format!("invalid hook effect: {error}"))
        })?;
        let encoded = encode(effect, "agent_runtime_hook_effect.record")?;
        let result = sqlx::query(
            "INSERT INTO agent_runtime_hook_effect (id,hook_run_id,thread_id,idempotency_key,effect_type,schema_version,target_authority,retry_limit,payload_digest,record) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
        )
        .bind(effect.effect_id.as_str())
        .bind(effect.hook_run_id.as_str())
        .bind(effect.thread_id.as_str())
        .bind(&effect.idempotency_key)
        .bind(&effect.descriptor.effect_type)
        .bind(i32::try_from(effect.descriptor.schema_version).map_err(|_| RuntimeStoreError::Unavailable("hook effect schema version exceeds integer".to_string()))?)
        .bind(&effect.descriptor.target_authority)
        .bind(i32::try_from(effect.descriptor.retry_limit).map_err(|_| RuntimeStoreError::Unavailable("hook effect retry limit exceeds integer".to_string()))?)
        .bind(&effect.descriptor.payload_digest)
        .bind(encoded.clone())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            let current: Option<Value> = sqlx::query_scalar(
                "SELECT record FROM agent_runtime_hook_effect WHERE id=$1 OR (hook_run_id=$2 AND idempotency_key=$3)",
            )
            .bind(effect.effect_id.as_str())
            .bind(effect.hook_run_id.as_str())
            .bind(&effect.idempotency_key)
            .fetch_optional(&mut **tx)
            .await
            .map_err(sql_error)?;
            if current.as_ref() != Some(&encoded) {
                return Err(RuntimeStoreError::Unavailable(
                    "hook effect identity or idempotency key was reused".to_string(),
                ));
            }
        }
    }
    Ok(())
}

async fn write_activation_outbox(
    tx: &mut Transaction<'_, Postgres>,
    entries: &[ContextActivationOutboxEntry],
) -> Result<(), RuntimeStoreError> {
    for entry in entries {
        let payload = encode(entry, "agent_context_activation_dispatch.payload")?;
        let result = sqlx::query("INSERT INTO agent_context_activation_dispatch (activation_id,candidate_id,compaction_id,thread_id,binding_id,driver_generation,checkpoint_id,digest,payload) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (activation_id) DO NOTHING")
            .bind(entry.activation_id.as_str()).bind(entry.candidate_id.as_str()).bind(entry.compaction_id.as_str())
            .bind(entry.thread_id.as_str()).bind(entry.binding_id.as_str())
            .bind(u64_to_i64(entry.generation.0, "driver generation")?)
            .bind(entry.checkpoint_id.as_str()).bind(entry.digest.as_str()).bind(payload.clone())
            .execute(&mut **tx).await.map_err(sql_error)?;
        if result.rows_affected() == 0 {
            let current: Value = sqlx::query_scalar(
                "SELECT payload FROM agent_context_activation_dispatch WHERE activation_id=$1",
            )
            .bind(entry.activation_id.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)?;
            if current != payload {
                return Err(RuntimeStoreError::ContextInvariant {
                    violation: ContextStoreInvariant::ActivationDispatchIdentity,
                });
            }
        }
    }
    Ok(())
}

async fn write_quarantine(
    tx: &mut Transaction<'_, Postgres>,
    entries: &[QuarantinedDriverEvent],
) -> Result<(), RuntimeStoreError> {
    for entry in entries {
        let (thread_id, binding_id, generation) = quarantine_coordinates(entry);
        sqlx::query("INSERT INTO agent_runtime_quarantine (thread_id,binding_id,driver_generation,reason_kind,record) VALUES ($1,$2,$3,$4,$5)")
            .bind(thread_id).bind(binding_id).bind(generation).bind(quarantine_reason(entry))
            .bind(encode(entry, "agent_runtime_quarantine.record")?).execute(&mut **tx).await.map_err(sql_error)?;
    }
    Ok(())
}

fn quarantine_coordinates(
    entry: &QuarantinedDriverEvent,
) -> (Option<&str>, Option<&str>, Option<i64>) {
    (
        None,
        Some(entry.event.binding_id.as_str()),
        i64::try_from(entry.event.generation.0).ok(),
    )
}

fn quarantine_reason(entry: &QuarantinedDriverEvent) -> &'static str {
    use agentdash_agent_runtime::DriverEventQuarantineReason::*;
    match entry.reason {
        CanonicalThreadNotFound => "canonical_thread_not_found",
        StaleBinding { .. } => "stale_binding",
        DriverOperationAcceptance => "driver_operation_acceptance",
        DriverRuntimeOwnedContextEvent => "driver_runtime_owned_context_event",
        DriverRuntimeOwnedHookEvent => "driver_runtime_owned_hook_event",
        DriverRuntimeOwnedBindingEvent => "driver_runtime_owned_binding_event",
        InvalidTransition { .. } => "invalid_transition",
    }
}

async fn validate_head_projection(
    tx: &mut Transaction<'_, Postgres>,
    projection: &RuntimeThreadState,
) -> Result<(), RuntimeStoreError> {
    let head =
        sqlx::query("SELECT checkpoint_id,revision FROM agent_context_head WHERE thread_id=$1")
            .bind(projection.thread_id.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(sql_error)?;
    let durable_checkpoint = head.as_ref().map(|row| row.get::<String, _>(0));
    let durable_revision = head.as_ref().map(|row| row.get::<i64, _>(1));
    if durable_checkpoint.as_deref()
        != projection
            .active_checkpoint_id
            .as_ref()
            .map(|id| id.as_str())
        || durable_revision.unwrap_or(0)
            != u64_to_i64(projection.context_revision.0, "context revision")?
    {
        return Err(RuntimeStoreError::ContextInvariant {
            violation: ContextStoreInvariant::HeadCheckpointMismatch,
        });
    }
    Ok(())
}

async fn validate_hook_plan_projection(
    tx: &mut Transaction<'_, Postgres>,
    projection: &RuntimeThreadState,
) -> Result<(), RuntimeStoreError> {
    let current = sqlx::query(
        "SELECT revision,digest FROM agent_runtime_hook_plan WHERE thread_id=$1 ORDER BY revision DESC LIMIT 1",
    )
    .bind(projection.thread_id.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(sql_error)?;
    let durable_revision = current
        .as_ref()
        .map(|row| i64_to_u64(row.get(0), "hook plan revision"))
        .transpose()?
        .map(agentdash_agent_runtime_contract::HookPlanRevision);
    let durable_digest = current.as_ref().map(|row| row.get::<String, _>(1));
    if projection.hook_plan_revision != durable_revision
        || projection
            .hook_plan_digest
            .as_ref()
            .map(|value| value.as_str())
            != durable_digest.as_deref()
    {
        return Err(RuntimeStoreError::Unavailable(
            "thread hook plan projection does not match durable binding".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[repr(u8)]
#[derive(Clone, Copy)]
enum TestCommitFailurePoint {
    Projection = 1,
    Operation = 2,
    Events = 3,
    Context = 4,
    Outbox = 5,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr, sync::Arc};

    use agentdash_agent_runtime::{
        BoundRuntimeHookEntry, BoundRuntimeHookPlan, CompactionPreparation, HookAdmission,
        HookCompletion, HookCorrelation, HookEffect, HookEffectDescriptor, HookExecutionSite,
        HookGateDecision, HookRunStatus, ManagedAgentRuntime, RuntimeHookInvocation,
        RuntimeHookPlanBinding, RuntimeRepository, RuntimeStoreError, RuntimeUnitOfWork,
        RuntimeWorkClaimRequest, RuntimeWorkKind, RuntimeWorkPayload, RuntimeWorkQueue,
        RuntimeWorkerId,
    };
    use agentdash_agent_runtime_contract::*;

    use super::{PostgresRuntimeRepository, TestCommitFailurePoint};

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            reference_class: ReferenceRuntimeClass::ManagedThread,
            input: InputProfile {
                modalities: BTreeSet::new(),
            },
            instruction: InstructionProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            tools: ToolProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
                cancellation: true,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::Native,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: true,
            },
            lifecycle: [LifecycleCapability::ThreadStart].into_iter().collect(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            context: ContextProfile {
                capabilities: [
                    ContextCapability::Read,
                    ContextCapability::PrepareCompaction,
                    ContextCapability::ActivateCheckpoint,
                ]
                .into_iter()
                .collect(),
                fidelity: ContextFidelity::PlatformExact,
                activation_idempotent: true,
            },
            telemetry_config: BTreeSet::new(),
        }
    }

    struct Fixture {
        store: Arc<PostgresRuntimeRepository>,
        runtime: ManagedAgentRuntime<PostgresRuntimeRepository>,
        thread_id: RuntimeThreadId,
        suffix: String,
        _database: TestDatabase,
    }

    struct TestDatabase {
        pool: sqlx::PgPool,
        _runtime: Option<crate::postgres_runtime::PostgresRuntime>,
    }

    async fn serial_test_guard() -> tokio::sync::SemaphorePermit<'static> {
        static SERIAL: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        SERIAL
            .acquire()
            .await
            .expect("runtime postgres test semaphore")
    }

    async fn runtime_test_database() -> TestDatabase {
        if crate::persistence::postgres::test_database_url().is_some() {
            return TestDatabase {
                pool: crate::persistence::postgres::test_pg_pool("managed runtime postgres")
                    .await
                    .expect("configured postgres test pool"),
                _runtime: None,
            };
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/managed-runtime-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "managed-runtime-tests",
            32,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL for managed runtime tests");
        let database_name = format!("runtime_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create empty runtime test database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(16)
            .connect_with(options)
            .await
            .expect("connect empty runtime test database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate empty managed runtime test database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("managed runtime schema readiness");
        TestDatabase {
            pool,
            _runtime: Some(runtime),
        }
    }

    async fn fixture(_suite: &str) -> Fixture {
        let database = runtime_test_database().await;
        let pool = database.pool.clone();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let binding_id = format!("binding-{suffix}");
        let source_id = format!("source-{suffix}");
        let thread_id = format!("thread-{source_id}");
        sqlx::query("INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ($1,7,$2)")
            .bind(&binding_id).bind(format!("profile-{suffix}"))
            .execute(&pool).await.expect("seed host-owned binding");
        sqlx::query("INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)")
            .bind(&binding_id).bind(&source_id).bind(&thread_id)
            .execute(&pool).await.expect("seed host-owned source coordinate");
        let store = Arc::new(PostgresRuntimeRepository::new(pool));
        let runtime = ManagedAgentRuntime::new(store.clone());
        Fixture {
            store,
            runtime,
            thread_id: id(&thread_id),
            suffix,
            _database: database,
        }
    }

    fn start(fixture: &Fixture) -> RuntimeCommandEnvelope {
        RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id(&format!("operation-{}", fixture.suffix)),
                idempotency_key: id(&format!("key-{}", fixture.suffix)),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "postgres-runtime-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: fixture.thread_id.clone(),
                binding_id: id(&format!("binding-{}", fixture.suffix)),
                driver_generation: RuntimeDriverGeneration(7),
                source_thread_id: id(&format!("source-{}", fixture.suffix)),
                profile_digest: id(&format!("profile-{}", fixture.suffix)),
                bound_profile: Box::new(profile()),
                input: Vec::new(),
                surface_digest: id(&format!("surface-{}", fixture.suffix)),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id(&format!("hook-plan-{}", fixture.suffix)),
                    entries: Vec::new(),
                },
            },
        }
    }

    #[tokio::test]
    async fn migration_physically_removes_legacy_runtime_session_schema() {
        let _serial = serial_test_guard().await;
        let database = runtime_test_database().await;
        let pool = &database.pool;
        crate::migration::assert_postgres_tables_absent(
            pool,
            &[
                "runtime_session_compaction_requests",
                "runtime_session_execution_anchors",
                "runtime_session_delivery_commands",
                "runtime_session_projection_segments",
                "runtime_session_projection_heads",
                "runtime_session_lineage",
                "runtime_session_compactions",
                "runtime_session_events",
                "runtime_sessions",
            ],
        )
        .await
        .expect("legacy RuntimeSession schema is physically absent");
    }

    #[tokio::test]
    async fn conversation_contract_reset_clears_runtime_graph_without_rewriting_migration_history()
    {
        let _serial = serial_test_guard().await;
        let fixture = fixture("conversation contract reset").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("seed runtime thread and journal");
        let pool = fixture.store.pool();
        let history_before: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .expect("migration history before reset");

        let binding_id = format!("binding-{}", fixture.suffix);
        let mut seed = pool.begin().await.expect("seed transaction");
        sqlx::query("INSERT INTO projects (id,name,created_at,updated_at) VALUES ('reset-project','Reset project',now(),now())")
            .execute(&mut *seed).await.expect("seed project");
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ('reset-run','reset-project','plain','active',now(),now(),now())")
            .execute(&mut *seed).await.expect("seed lifecycle run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status,bootstrap_status) VALUES ('reset-agent','reset-run','reset-project','primary','active','not_applicable')")
            .execute(&mut *seed).await.expect("seed lifecycle agent");
        sqlx::query("INSERT INTO agent_runtime_service_instance (id,definition_id,definition_build_digest,revision,config,credentials,placement,desired_state,observed_state,active_generation) VALUES ('reset-service','reset-definition','sha256:definition',1,'{}','{}','{}','active','{}',7)")
            .execute(&mut *seed).await.expect("seed service instance");
        sqlx::query("INSERT INTO agent_runtime_service_instance_revision (service_instance_id,revision,instance_snapshot) VALUES ('reset-service',1,'{}')")
            .execute(&mut *seed).await.expect("seed service revision");
        sqlx::query("INSERT INTO agent_runtime_service_activation (service_instance_id,instance_revision,driver_generation,protocol_revision,effective_profile,profile_digest,conformance_evidence,instance_snapshot) VALUES ('reset-service',1,7,1,'{}',$1,'{}','{}')")
            .bind(format!("profile-{}", fixture.suffix)).execute(&mut *seed).await.expect("seed service activation");
        sqlx::query("INSERT INTO agent_runtime_offer (id,service_instance_id,instance_revision,driver_generation,profile_digest,available,offer) VALUES ('reset-offer','reset-service',1,7,$1,true,'{}')")
            .bind(format!("profile-{}", fixture.suffix)).execute(&mut *seed).await.expect("seed runtime offer");
        sqlx::query("INSERT INTO agent_runtime_host_binding (binding_id,thread_id,offer_id,service_instance_id,instance_revision,driver_generation,profile_digest,state,lease_epoch,binding) VALUES ($1,$2,'reset-offer','reset-service',1,7,$3,'active',1,'{}')")
            .bind(&binding_id).bind(fixture.thread_id.as_str()).bind(format!("profile-{}", fixture.suffix))
            .execute(&mut *seed).await.expect("seed host binding");
        sqlx::query("INSERT INTO agent_run_runtime_thread_anchor (run_id,agent_id,runtime_thread_id,bootstrap_runtime_binding_id) VALUES ('reset-run','reset-agent',$1,$2)")
            .bind(fixture.thread_id.as_str()).bind(&binding_id)
            .execute(&mut *seed).await.expect("seed AgentRun runtime anchor");
        sqlx::query("INSERT INTO agent_run_runtime_binding_lineage (run_id,agent_id,binding_epoch,runtime_binding_id,binding) VALUES ('reset-run','reset-agent',1,$1,'{}'::jsonb)")
            .bind(&binding_id).execute(&mut *seed).await.expect("seed binding lineage");
        seed.commit().await.expect("commit valid reset graph");

        sqlx::raw_sql(include_str!(
            "../../../migrations/0069_reset_runtime_conversation_contract.sql"
        ))
        .execute(pool)
        .await
        .expect("reapply conversation reset migration body");

        for table in [
            "agent_runtime_thread",
            "agent_runtime_binding",
            "agent_runtime_source_coordinate",
            "agent_runtime_event",
            "agent_run_runtime_thread_anchor",
            "agent_run_runtime_binding_lineage",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(pool)
                .await
                .expect("count reset table");
            assert_eq!(count, 0, "{table} must be rebuilt by reprovisioning");
        }
        let history_after: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .expect("migration history after reset");
        assert_eq!(history_after, history_before);
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=69 AND success)",
        )
        .fetch_one(pool)
        .await
        .expect("0069 migration history");
        assert!(applied);
        for table in [
            "projects",
            "lifecycle_runs",
            "lifecycle_agents",
            "agent_runtime_service_instance",
            "agent_runtime_offer",
        ] {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {table} WHERE id LIKE 'reset-%'"
            ))
            .fetch_one(pool)
            .await
            .expect("count preserved owner fact");
            assert_eq!(count, 1, "{table} must survive Runtime reprovision reset");
        }
    }

    #[tokio::test]
    async fn five_commit_stages_rollback_the_whole_write_set() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime transaction rollback").await;
        for stage in [
            TestCommitFailurePoint::Projection,
            TestCommitFailurePoint::Operation,
            TestCommitFailurePoint::Events,
            TestCommitFailurePoint::Context,
            TestCommitFailurePoint::Outbox,
        ] {
            fixture.store.fail_next_commit_at(stage);
            assert!(fixture.runtime.execute(start(&fixture)).await.is_err());
            assert!(
                fixture
                    .store
                    .load_thread(&fixture.thread_id)
                    .await
                    .expect("read thread")
                    .is_none()
            );
            let operation_id: RuntimeOperationId = id(&format!("operation-{}", fixture.suffix));
            assert!(
                fixture
                    .store
                    .find_operation(&operation_id)
                    .await
                    .expect("read operation")
                    .is_none()
            );
            let outbox: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM agent_runtime_outbox WHERE thread_id=$1")
                    .bind(fixture.thread_id.as_str())
                    .fetch_one(fixture.store.pool())
                    .await
                    .expect("count outbox");
            assert_eq!(outbox, 0);
        }
    }

    #[tokio::test]
    async fn concurrent_projection_cas_has_one_winner_without_sequence_consumption() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime concurrent cas").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        let mut left = base.clone();
        left.revision = RuntimeRevision(base.revision.0 + 1);
        left.settings_revision = ThreadSettingsRevision(1);
        let mut right = base.clone();
        right.revision = RuntimeRevision(base.revision.0 + 1);
        right.settings_revision = ThreadSettingsRevision(2);
        let empty = |projection| agentdash_agent_runtime::RuntimeCommit {
            expected_projection_revision: Some(base.revision),
            projection,
            operation: None,
            operation_terminals: Vec::new(),
            events: Vec::new(),
            outbox: Vec::new(),
            context_activation_outbox: Vec::new(),
            context_preparation_work_items: Vec::new(),
            context_checkpoints: Vec::new(),
            context_candidates: Vec::new(),
            context_activations: Vec::new(),
            context_head: None,
            hook_plan_binding: None,
            hook_runs: Vec::new(),
            hook_effects: Vec::new(),
            quarantine: Vec::new(),
        };
        let (left, right) = tokio::join!(
            fixture.store.commit(empty(left)),
            fixture.store.commit(empty(right))
        );
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        let current = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        assert_eq!(current.revision, RuntimeRevision(base.revision.0 + 1));
        assert_eq!(current.next_event_sequence, base.next_event_sequence);
        assert_eq!(
            current.next_operation_sequence,
            base.next_operation_sequence
        );
    }

    #[tokio::test]
    async fn projection_cursors_cannot_advance_without_matching_durable_facts() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime sequence validation").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        let mut invalid = base.clone();
        invalid.next_event_sequence = EventSequence(base.next_event_sequence.0 + 1);
        let result = fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(base.revision),
                projection: invalid,
                operation: None,
                operation_terminals: Vec::new(),
                events: Vec::new(),
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await;
        assert!(matches!(
            result,
            Err(agentdash_agent_runtime::RuntimeStoreError::Unavailable(_))
        ));
        let current = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        assert_eq!(current.next_event_sequence, base.next_event_sequence);
        assert_eq!(current.revision, base.revision);
    }

    #[tokio::test]
    async fn database_idempotency_constraint_rolls_back_projection_and_journal() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime database idempotency").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        let existing_id: RuntimeOperationId = id(&format!("operation-{}", fixture.suffix));
        let existing = fixture
            .store
            .find_operation(&existing_id)
            .await
            .expect("operation")
            .expect("accepted operation");
        let conflicting_id: RuntimeOperationId =
            id(&format!("conflicting-operation-{}", fixture.suffix));
        let mut projection = base.clone();
        projection.next_operation_sequence = OperationSequence(base.next_operation_sequence.0 + 1);
        let events = projection
            .append_events([RuntimeEvent::OperationAccepted {
                operation_id: conflicting_id.clone(),
            }])
            .expect("valid projected acceptance");
        let operation = agentdash_agent_runtime::RuntimeOperationRecord {
            operation_id: conflicting_id.clone(),
            idempotency_key: existing.idempotency_key,
            actor: RuntimeActor::System {
                component: "idempotency-conflict".to_string(),
            },
            thread_id: fixture.thread_id.clone(),
            operation_sequence: projection.next_operation_sequence,
            accepted_revision: projection.revision,
            command: RuntimeCommand::ThreadResume {
                thread_id: fixture.thread_id.clone(),
            },
            terminal: None,
        };
        let result = fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(base.revision),
                projection,
                operation: Some(operation),
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await;
        assert!(matches!(
            result,
            Err(agentdash_agent_runtime::RuntimeStoreError::IdempotencyConflict { .. })
        ));
        let current = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load")
            .expect("thread");
        assert_eq!(current.revision, base.revision);
        assert!(
            fixture
                .store
                .find_operation(&conflicting_id)
                .await
                .expect("conflicting operation")
                .is_none()
        );
        assert_eq!(
            fixture
                .store
                .events_after(&fixture.thread_id, None)
                .await
                .expect("events")
                .latest_available,
            base.next_event_sequence
        );
    }

    #[tokio::test]
    async fn work_claim_checks_owner_token_and_supports_expiry_takeover() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime work lease").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let request = |owner: &str| RuntimeWorkClaimRequest {
            kind: RuntimeWorkKind::RuntimeOutbox,
            owner: RuntimeWorkerId(owner.to_string()),
            lease_duration_ms: 30_000,
            limit: 100,
        };
        let operation_id: RuntimeOperationId = id(&format!("operation-{}", fixture.suffix));
        let first = fixture.store.claim(request("worker-a")).await.expect("claim").into_iter()
            .find(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &operation_id))
            .expect("fixture work");
        let blocked = fixture
            .store
            .claim(request("worker-b"))
            .await
            .expect("blocked claim");
        assert!(!blocked.iter().any(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &operation_id)));
        let mut forged = first.clone();
        forged.owner = RuntimeWorkerId("worker-b".to_string());
        assert!(matches!(
            fixture.store.ack(&forged).await,
            Err(agentdash_agent_runtime::RuntimeStoreError::WorkClaimConflict)
        ));
        sqlx::query("UPDATE agent_runtime_outbox SET claim_expires_at_ms=0 WHERE operation_id=$1")
            .bind(match &first.identity {
                agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) => id.as_str(),
                _ => unreachable!(),
            })
            .execute(fixture.store.pool())
            .await
            .expect("expire lease");
        assert!(matches!(
            fixture.store.ack(&first).await,
            Err(agentdash_agent_runtime::RuntimeStoreError::WorkClaimConflict)
        ));
        let takeover = fixture.store.claim(request("worker-b")).await.expect("takeover").into_iter()
            .find(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &operation_id))
            .expect("expired fixture work");
        assert_eq!(takeover.attempt, 2);
        fixture.store.ack(&takeover).await.expect("ack takeover");
        let after_ack = fixture
            .store
            .claim(request("worker-c"))
            .await
            .expect("post-ack claim");
        assert!(!after_ack.iter().any(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &operation_id)));
    }

    #[tokio::test]
    async fn context_work_queues_cover_pending_dispatch_recovery_and_head_checkpoint_consistency() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime context recovery queues").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let compaction_id: ContextCompactionId = id(&format!("compaction-{}", fixture.suffix));
        let operation_id: RuntimeOperationId = id(&format!("compact-operation-{}", fixture.suffix));
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                meta: OperationMeta {
                    operation_id: operation_id.clone(),
                    idempotency_key: id(&format!("compact-key-{}", fixture.suffix)),
                    expected_thread_revision: None,
                    actor: RuntimeActor::System {
                        component: "postgres-context-test".to_string(),
                    },
                },
                command: RuntimeCommand::ContextCompact {
                    thread_id: fixture.thread_id.clone(),
                    compaction_id: compaction_id.clone(),
                    trigger: ContextCompactionTrigger::Manual,
                    base_checkpoint_id: None,
                    expected_context_revision: ContextRevision(0),
                },
            })
            .await
            .expect("accept compaction");

        let request = |kind, owner: &str| RuntimeWorkClaimRequest {
            kind,
            owner: RuntimeWorkerId(owner.to_string()),
            lease_duration_ms: 30_000,
            limit: 1,
        };
        let preparation_claim = fixture
            .store
            .claim(request(
                RuntimeWorkKind::ContextPreparation,
                "prepare-worker",
            ))
            .await
            .expect("claim preparation")
            .pop()
            .expect("pending preparation");
        let candidate_id: ContextCandidateId = id(&format!("candidate-{}", fixture.suffix));
        let activation_id: ContextActivationId = id(&format!("activation-{}", fixture.suffix));
        let checkpoint_id: ContextCheckpointId = id(&format!("checkpoint-{}", fixture.suffix));
        let digest: ContextDigest = id(&format!("digest-{}", fixture.suffix));
        fixture
            .runtime
            .prepare_compaction(CompactionPreparation {
                candidate_id: candidate_id.clone(),
                compaction_id: compaction_id.clone(),
                activation_id: activation_id.clone(),
                operation_id: operation_id.clone(),
                thread_id: fixture.thread_id.clone(),
                trigger: ContextCompactionTrigger::Manual,
                expected_base_checkpoint_id: None,
                expected_base_revision: ContextRevision(0),
                checkpoint_id: checkpoint_id.clone(),
                materialized: MaterializedContext {
                    recipe: ContextRecipe {
                        revision: ContextRecipeRevision(1),
                        provenance: ContextProvenance {
                            settings_revision: ThreadSettingsRevision(0),
                            tool_set_revision: ToolSetRevision(0),
                        },
                        source_item_ids: Vec::new(),
                    },
                    blocks: vec![ContextBlock::CompactionSummary {
                        summary: "durable summary".to_string(),
                    }],
                    digest: digest.clone(),
                    fidelity: ContextFidelity::PlatformExact,
                },
            })
            .await
            .expect("prepare compaction");
        fixture
            .store
            .ack(&preparation_claim)
            .await
            .expect("ack preparation lease");

        let dispatch_claim = fixture
            .store
            .claim(request(
                RuntimeWorkKind::ContextActivationDispatch,
                "dispatch-worker",
            ))
            .await
            .expect("claim dispatch")
            .pop()
            .expect("activation dispatch");
        fixture
            .store
            .ack(&dispatch_claim)
            .await
            .expect("ack dispatch");
        let driver_revision: DriverContextRevision =
            id(&format!("driver-revision-{}", fixture.suffix));
        fixture
            .runtime
            .confirm_compaction_activation(&compaction_id, digest.clone(), driver_revision)
            .await
            .expect("persist applied observation");

        let recovery_claim = fixture
            .store
            .claim(request(
                RuntimeWorkKind::ContextActivationRecovery,
                "recovery-worker",
            ))
            .await
            .expect("claim recovery")
            .pop()
            .expect("applied activation recovery");
        fixture
            .runtime
            .finalize_compaction(&compaction_id)
            .await
            .expect("finalize head cas");
        fixture
            .store
            .ack(&recovery_claim)
            .await
            .expect("ack recovery");

        let head = fixture
            .store
            .load_context_head(&fixture.thread_id)
            .await
            .expect("head")
            .expect("active head");
        let checkpoint = fixture
            .store
            .load_context_checkpoint(&checkpoint_id)
            .await
            .expect("checkpoint")
            .expect("durable checkpoint");
        assert_eq!(head.checkpoint_id, checkpoint.checkpoint_id);
        assert_eq!(head.revision, checkpoint.revision);
        assert_eq!(head.digest, checkpoint.materialized.digest);
        assert!(
            fixture
                .store
                .claim(request(
                    RuntimeWorkKind::ContextActivationRecovery,
                    "other-worker"
                ))
                .await
                .expect("terminal recovery scan")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn hook_plan_run_terminal_and_effect_lease_are_durable_in_postgres() {
        let _guard = serial_test_guard().await;
        let fixture = fixture("hook orchestration").await;
        let plan = RuntimeHookPlanBinding {
            thread_id: fixture.thread_id.clone(),
            plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: id(&format!("hook-plan-{}", fixture.suffix)),
                entries: vec![BoundRuntimeHookEntry {
                    definition_id: id(&format!("hook-definition-{}", fixture.suffix)),
                    point: HookPoint::BeforeTurn,
                    actions: [HookAction::Block, HookAction::EmitEffect]
                        .into_iter()
                        .collect(),
                    delivered_strength: SemanticStrength::ExactDurableBoundary,
                    failure_policy: HookFailurePolicy::FailClosed,
                    required: true,
                    site: HookExecutionSite::ManagedRuntime,
                }],
            },
        };
        let mut start_command = start(&fixture);
        let RuntimeCommand::ThreadStart { hook_plan, .. } = &mut start_command.command else {
            unreachable!("start fixture always emits ThreadStart")
        };
        *hook_plan = plan.plan.clone();
        fixture
            .runtime
            .execute(start_command)
            .await
            .expect("start runtime thread with durable hook plan");
        fixture
            .runtime
            .bind_hook_plan(plan.clone())
            .await
            .expect("bind durable hook plan");
        let run_id: HookRunId = id(&format!("hook-run-{}", fixture.suffix));
        let HookAdmission::Durable(run) = fixture
            .runtime
            .accept_hook(RuntimeHookInvocation {
                hook_run_id: run_id.clone(),
                thread_id: fixture.thread_id.clone(),
                definition_id: plan.plan.entries[0].definition_id.clone(),
                point: HookPoint::BeforeTurn,
                correlation: HookCorrelation {
                    operation_id: Some(id(&format!("operation-{}", fixture.suffix))),
                    turn_id: None,
                    item_id: None,
                    interaction_id: None,
                },
                input: serde_json::json!({"turn": "admission"}),
            })
            .await
            .expect("accept durable hook")
        else {
            panic!("actionful hook is durable")
        };
        assert_eq!(run.status, HookRunStatus::Accepted);
        assert_eq!(
            fixture
                .store
                .recoverable_hook_runs()
                .await
                .expect("recoverable runs")
                .len(),
            1
        );
        let recovery_claim = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::HookRunRecovery,
                owner: RuntimeWorkerId("hook-recovery-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 1,
            })
            .await
            .expect("claim hook recovery")
            .pop()
            .expect("accepted hook recovery");
        assert!(matches!(
            &recovery_claim.payload,
            RuntimeWorkPayload::HookRunRecovery(recovery) if recovery.hook_run_id == run_id
        ));
        assert!(
            fixture
                .store
                .claim(RuntimeWorkClaimRequest {
                    kind: RuntimeWorkKind::HookRunRecovery,
                    owner: RuntimeWorkerId("other-recovery-worker".to_string()),
                    lease_duration_ms: 30_000,
                    limit: 1,
                })
                .await
                .expect("competing recovery claim")
                .is_empty()
        );
        sqlx::query("UPDATE agent_runtime_hook_run SET recovery_claim_expires_at_ms=0 WHERE id=$1")
            .bind(run_id.as_str())
            .execute(&fixture._database.pool)
            .await
            .expect("expire first hook recovery lease");
        let recovery_takeover = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::HookRunRecovery,
                owner: RuntimeWorkerId("hook-recovery-takeover".to_string()),
                lease_duration_ms: 30_000,
                limit: 1,
            })
            .await
            .expect("take over expired hook recovery")
            .pop()
            .expect("expired recovery is claimable");
        assert!(matches!(
            fixture.store.ack(&recovery_claim).await,
            Err(RuntimeStoreError::WorkClaimConflict)
        ));
        assert!(recovery_takeover.attempt > recovery_claim.attempt);

        let run = fixture
            .runtime
            .start_hook(&run.hook_run_id)
            .await
            .expect("start durable hook");
        assert_eq!(run.status, HookRunStatus::Running);
        let mut next_plan = plan.clone();
        next_plan.plan.revision = HookPlanRevision(2);
        // Plan history is revisioned; a later business revision may intentionally reuse content.
        next_plan.plan.digest = plan.plan.digest.clone();
        let effect_payload = serde_json::json!({"message": "continue"});
        fixture
            .runtime
            .bind_hook_plan(next_plan.clone())
            .await
            .expect("append next immutable hook plan revision");

        let effect_id: HookEffectId = id(&format!("hook-effect-{}", fixture.suffix));
        fixture
            .runtime
            .complete_hook(
                &run_id,
                HookCompletion {
                    status: HookRunStatus::Blocked,
                    decision: HookGateDecision::Block,
                    message: Some("policy denied".to_string()),
                },
                vec![HookEffect {
                    effect_id: effect_id.clone(),
                    hook_run_id: run_id.clone(),
                    thread_id: fixture.thread_id.clone(),
                    idempotency_key: format!("mailbox:{}", fixture.suffix),
                    descriptor: HookEffectDescriptor {
                        effect_type: "mailbox.enqueue".to_string(),
                        schema_version: 1,
                        target_authority: "agent_run_mailbox".to_string(),
                        retry_limit: 3,
                        payload_digest: agentdash_agent_runtime::hook_effect_payload_digest(
                            &effect_payload,
                        ),
                    },
                    payload: effect_payload,
                }],
            )
            .await
            .expect("persist terminal and effect");
        fixture
            .store
            .ack(&recovery_takeover)
            .await
            .expect("ack terminal recovery");
        assert!(
            fixture
                .store
                .recoverable_hook_runs()
                .await
                .expect("terminal scan")
                .is_empty()
        );

        let conflicting_run_id: HookRunId =
            id(&format!("hook-run-effect-conflict-{}", fixture.suffix));
        let HookAdmission::Durable(conflicting_run) = fixture
            .runtime
            .accept_hook(RuntimeHookInvocation {
                hook_run_id: conflicting_run_id.clone(),
                thread_id: fixture.thread_id.clone(),
                definition_id: next_plan.plan.entries[0].definition_id.clone(),
                point: HookPoint::BeforeTurn,
                correlation: HookCorrelation {
                    operation_id: None,
                    turn_id: None,
                    item_id: None,
                    interaction_id: None,
                },
                input: serde_json::json!({"turn": "effect-conflict"}),
            })
            .await
            .expect("accept conflicting effect run")
        else {
            panic!("actionful hook is durable")
        };
        fixture
            .runtime
            .start_hook(&conflicting_run.hook_run_id)
            .await
            .expect("start conflicting effect run");
        let before_conflict = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load thread")
            .expect("thread")
            .next_event_sequence;
        let conflicting_payload = serde_json::json!({"message": "different"});
        fixture
            .runtime
            .complete_hook(
                &conflicting_run_id,
                HookCompletion {
                    status: HookRunStatus::Blocked,
                    decision: HookGateDecision::Block,
                    message: Some("different denial".to_string()),
                },
                vec![HookEffect {
                    effect_id: effect_id.clone(),
                    hook_run_id: conflicting_run_id.clone(),
                    thread_id: fixture.thread_id.clone(),
                    idempotency_key: format!("mailbox:conflict:{}", fixture.suffix),
                    descriptor: HookEffectDescriptor {
                        effect_type: "mailbox.enqueue".to_string(),
                        schema_version: 1,
                        target_authority: "agent_run_mailbox".to_string(),
                        retry_limit: 3,
                        payload_digest: agentdash_agent_runtime::hook_effect_payload_digest(
                            &conflicting_payload,
                        ),
                    },
                    payload: conflicting_payload,
                }],
            )
            .await
            .expect_err("effect identity conflict rolls back terminal transaction");
        assert_eq!(
            fixture
                .store
                .load_hook_run(&conflicting_run_id)
                .await
                .expect("load conflicting run")
                .expect("conflicting run")
                .status,
            HookRunStatus::Running
        );
        assert_eq!(
            fixture
                .store
                .load_thread(&fixture.thread_id)
                .await
                .expect("load thread")
                .expect("thread")
                .next_event_sequence,
            before_conflict
        );

        let claim = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::HookEffect,
                owner: RuntimeWorkerId("hook-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 1,
            })
            .await
            .expect("claim effect")
            .pop()
            .expect("pending effect");
        assert!(matches!(
            &claim.payload,
            RuntimeWorkPayload::HookEffect(effect) if effect.effect_id == effect_id
        ));
        sqlx::query("UPDATE agent_runtime_hook_effect SET claim_expires_at_ms=0 WHERE id=$1")
            .bind(effect_id.as_str())
            .execute(&fixture._database.pool)
            .await
            .expect("expire first hook effect lease");
        let takeover = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::HookEffect,
                owner: RuntimeWorkerId("hook-effect-takeover".to_string()),
                lease_duration_ms: 30_000,
                limit: 1,
            })
            .await
            .expect("take over expired hook effect")
            .pop()
            .expect("expired hook effect is claimable");
        assert!(matches!(
            fixture.store.ack(&claim).await,
            Err(RuntimeStoreError::WorkClaimConflict)
        ));
        assert!(takeover.attempt > claim.attempt);
        fixture.store.ack(&takeover).await.expect("ack effect");
        assert!(
            fixture
                .store
                .claim(RuntimeWorkClaimRequest {
                    kind: RuntimeWorkKind::HookEffect,
                    owner: RuntimeWorkerId("other-worker".to_string()),
                    lease_duration_ms: 30_000,
                    limit: 1,
                })
                .await
                .expect("claim after ack")
                .is_empty()
        );
    }
}
