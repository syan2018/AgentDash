use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime::{
    ActiveContextHead, ContextActivation, ContextActivationOutboxEntry, ContextActivationStatus,
    ContextCandidate, ContextCheckpoint, ContextHeadWrite, ContextPreparationStatus,
    ContextPreparationWorkItem, ContextStoreInvariant, EntityPhase, HookEffect, HookRun,
    HookRunStatus, QuarantinedDriverEvent, RUNTIME_CONTEXT_PRESENTATION_EFFECT_TYPE, RuntimeCommit,
    RuntimeHookPlanBinding, RuntimeInteractionState, RuntimeItemState, RuntimeJournalBatch,
    RuntimeOperationRecord, RuntimeOutboxEntry, RuntimeRepository, RuntimeStoreError,
    RuntimeTerminalApplicationEffectClaim, RuntimeTerminalApplicationEffectClaimRequest,
    RuntimeTerminalApplicationEffectOutbox, RuntimeTerminalApplicationEffectOutboxEntry,
    RuntimeThreadState, RuntimeTransientEvents, RuntimeTurnState, RuntimeUnitOfWork,
    RuntimeWorkClaim, RuntimeWorkClaimRequest, RuntimeWorkClaimToken, RuntimeWorkIdentity,
    RuntimeWorkKind, RuntimeWorkPayload, RuntimeWorkQueue, RuntimeWorkerId,
};
use agentdash_agent_runtime_contract::{
    ContextActivationId, ContextCheckpointId, ContextCompactionId, ContextFidelity, EventSequence,
    HookEffectId, HookRunId, IdempotencyKey, ImmutablePresentationEvent, RuntimeBindingId,
    RuntimeCarrierMetadata, RuntimeDriverGeneration, RuntimeEventEnvelope, RuntimeJournalFact,
    RuntimeJournalRecord, RuntimeOperationId, RuntimeOperationTerminal,
    RuntimePresentationCoordinate, RuntimeRevision, RuntimeThreadId, RuntimeTurnId,
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
    presentation_transient:
        Arc<tokio::sync::Mutex<BTreeMap<RuntimeThreadId, Vec<RuntimeJournalRecord>>>>,
    live: Arc<
        tokio::sync::Mutex<
            BTreeMap<RuntimeThreadId, tokio::sync::broadcast::Sender<RuntimeEventEnvelope>>,
        >,
    >,
    presentation_live: Arc<
        tokio::sync::Mutex<
            BTreeMap<RuntimeThreadId, tokio::sync::broadcast::Sender<RuntimeJournalRecord>>,
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
            presentation_transient: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            live: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            presentation_live: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
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
        let mut transient = self.presentation_transient.lock().await;
        let entries = transient.entry(thread_id.clone()).or_default();
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
                recorded_at_ms: current_time_ms(),
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
        drop(transient);

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
        self.presentation_transient
            .lock()
            .await
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
        self.transient.lock().await.remove(thread_id);
        self.presentation_transient.lock().await.remove(thread_id);
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

    async fn journal_records_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeJournalBatch, RuntimeStoreError> {
        let latest = sqlx::query_scalar::<_, i64>(
            "SELECT next_event_sequence FROM agent_runtime_thread WHERE id=$1",
        )
        .bind(thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?
        .ok_or(RuntimeStoreError::NotFound)?;
        let rows = sqlx::query(
            "SELECT event_sequence,record FROM agent_runtime_event \
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
        Ok(RuntimeJournalBatch {
            earliest_available: EventSequence(match earliest {
                Some(value) => i64_to_u64(value, "agent_runtime_event.event_sequence")?,
                None => latest.saturating_add(1),
            }),
            latest_available: EventSequence(latest),
            records: rows
                .into_iter()
                .map(|row| decode(row.get::<Value, _>(1), "agent_runtime_event.record"))
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
    async fn commit_with_live_presentation_publication(
        &self,
        commit: RuntimeCommit,
        publish_live_presentations: bool,
    ) -> Result<(), RuntimeStoreError> {
        validate_terminal_application_effects(&commit)?;
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
        write_journal_records(&mut tx, &commit.records).await?;
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
        write_terminal_application_effect_outbox(&mut tx, &commit.terminal_application_effects)
            .await?;
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
        if publish_live_presentations {
            for record in live_presentations {
                self.publish_durable_presentation(record).await;
            }
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
    for record in &commit.records {
        expected_event_sequence = expected_event_sequence.checked_add(1).ok_or_else(|| {
            RuntimeStoreError::Unavailable("runtime event sequence overflow".to_string())
        })?;
        if record.carrier().thread_id != commit.projection.thread_id
            || record.carrier().sequence != Some(EventSequence(expected_event_sequence))
            || record.carrier().transient.is_some()
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
impl RuntimeTerminalApplicationEffectOutbox for PostgresRuntimeRepository {
    async fn claim_terminal_application_effects(
        &self,
        request: RuntimeTerminalApplicationEffectClaimRequest,
    ) -> Result<Vec<RuntimeTerminalApplicationEffectClaim>, RuntimeStoreError> {
        if request.owner.as_str().trim().is_empty()
            || request.lease_duration_ms == 0
            || request.limit == 0
        {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "terminal application effect claim requires owner, positive lease, and positive limit"
                    .to_string(),
            ));
        }
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
        let rows = sqlx::query(
            "WITH candidates AS ( \
             SELECT effect_id FROM agent_runtime_terminal_application_effect_outbox \
             WHERE completed_at IS NULL AND (claim_expires_at_ms IS NULL OR claim_expires_at_ms <= $1) \
             ORDER BY created_at,effect_id FOR UPDATE SKIP LOCKED LIMIT $2 \
             ) UPDATE agent_runtime_terminal_application_effect_outbox q SET \
             claim_token=$3,claim_owner=$4,claim_expires_at_ms=$5,attempt_count=q.attempt_count+1, \
             last_error=NULL,updated_at=now() FROM candidates c WHERE q.effect_id=c.effect_id \
             RETURNING q.record,q.attempt_count",
        )
        .bind(now)
        .bind(i64::from(request.limit))
        .bind(token.as_str())
        .bind(request.owner.as_str())
        .bind(expires)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(RuntimeTerminalApplicationEffectClaim {
                    entry: decode(
                        row.get(0),
                        "agent_runtime_terminal_application_effect_outbox.record",
                    )?,
                    token: token.clone(),
                    owner: request.owner.clone(),
                    lease_expires_at_ms: expires,
                    attempt: u32::try_from(row.get::<i32, _>(1)).map_err(|_| {
                        RuntimeStoreError::Unavailable(
                            "terminal application effect attempt is invalid".to_string(),
                        )
                    })?,
                })
            })
            .collect()
    }

    async fn ack_terminal_application_effect(
        &self,
        claim: &RuntimeTerminalApplicationEffectClaim,
    ) -> Result<(), RuntimeStoreError> {
        update_terminal_application_effect_claim(&self.pool, claim, None, true).await
    }

    async fn release_terminal_application_effect(
        &self,
        claim: &RuntimeTerminalApplicationEffectClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError> {
        update_terminal_application_effect_claim(&self.pool, claim, Some(error), false).await
    }
}

async fn update_terminal_application_effect_claim(
    pool: &PgPool,
    claim: &RuntimeTerminalApplicationEffectClaim,
    error: Option<String>,
    ack: bool,
) -> Result<(), RuntimeStoreError> {
    let result = if ack {
        sqlx::query(
            "UPDATE agent_runtime_terminal_application_effect_outbox SET completed_at=now(),claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=NULL,updated_at=now() WHERE effect_id=$1 AND claim_owner=$2 AND claim_token=$3 AND completed_at IS NULL",
        )
        .bind(claim.entry.effect_id.as_str())
        .bind(claim.owner.as_str())
        .bind(claim.token.as_str())
        .execute(pool)
        .await
        .map_err(sql_error)?
    } else {
        sqlx::query(
            "UPDATE agent_runtime_terminal_application_effect_outbox SET claim_token=NULL,claim_owner=NULL,claim_expires_at_ms=NULL,last_error=$4,updated_at=now() WHERE effect_id=$1 AND claim_owner=$2 AND claim_token=$3 AND completed_at IS NULL",
        )
        .bind(claim.entry.effect_id.as_str())
        .bind(claim.owner.as_str())
        .bind(claim.token.as_str())
        .bind(error)
        .execute(pool)
        .await
        .map_err(sql_error)?
    };
    if result.rows_affected() != 1 {
        return Err(RuntimeStoreError::WorkClaimConflict);
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
        "WITH candidates AS (SELECT q.operation_id FROM agent_runtime_outbox q \
         JOIN agent_runtime_operation o ON o.id=q.operation_id AND o.thread_id=q.thread_id \
         WHERE q.dispatched_at IS NULL \
         AND (q.claim_token IS NULL OR q.claim_expires_at_ms <= $1) \
         AND NOT EXISTS ( \
             SELECT 1 FROM agent_runtime_outbox predecessor \
             JOIN agent_runtime_operation predecessor_operation \
               ON predecessor_operation.id=predecessor.operation_id \
              AND predecessor_operation.thread_id=predecessor.thread_id \
             WHERE predecessor.thread_id=q.thread_id \
               AND predecessor.dispatched_at IS NULL \
               AND predecessor_operation.operation_sequence < o.operation_sequence \
         ) \
         ORDER BY q.created_at,q.operation_id LIMIT $5 FOR UPDATE OF q SKIP LOCKED) \
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
        "WITH candidates AS (SELECT q.compaction_id FROM agent_context_preparation q \
         JOIN agent_runtime_thread t ON t.id=q.thread_id \
         WHERE q.status='pending' AND t.active_turn_id IS NULL \
         AND (q.claim_token IS NULL OR q.claim_expires_at_ms <= $1) \
         ORDER BY q.created_at LIMIT $5 FOR UPDATE OF q SKIP LOCKED) \
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
         WHERE dispatched_at IS NULL AND effect_type <> $6 AND attempt_count <= retry_limit \
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
    .bind(RUNTIME_CONTEXT_PRESENTATION_EFFECT_TYPE)
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

async fn write_journal_records(
    tx: &mut Transaction<'_, Postgres>,
    records: &[RuntimeJournalRecord],
) -> Result<(), RuntimeStoreError> {
    for record in records {
        let sequence = record.carrier().sequence.ok_or_else(|| {
            RuntimeStoreError::Unavailable(
                "transient record cannot enter durable journal".to_string(),
            )
        })?;
        let value = encode(record, "agent_runtime_event.record")?;
        let kind = match record.fact() {
            RuntimeJournalFact::Presentation(_) => "presentation",
            RuntimeJournalFact::Internal(_) => "internal",
        };
        sqlx::query(
            "INSERT INTO agent_runtime_event (thread_id,event_sequence,revision,fact_kind,record) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(record.carrier().thread_id.as_str())
        .bind(u64_to_i64(sequence.0, "event sequence")?)
        .bind(u64_to_i64(record.carrier().revision.0, "event revision")?)
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
    let result = sqlx::query(
        "INSERT INTO agent_runtime_turn (id,thread_id,phase,state) VALUES ($1,$2,$3,$4) \
         ON CONFLICT (id) DO UPDATE SET phase=EXCLUDED.phase,state=EXCLUDED.state \
         WHERE agent_runtime_turn.thread_id=EXCLUDED.thread_id",
    )
    .bind(id)
    .bind(thread.thread_id.as_str())
    .bind(entity_phase(&state.phase))
    .bind(encode(state, "agent_runtime_turn.state")?)
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    ensure_projection_identity(result.rows_affected(), "turn", id)?;
    Ok(())
}

async fn insert_item(
    tx: &mut Transaction<'_, Postgres>,
    thread: &RuntimeThreadState,
    id: &str,
    index: usize,
    state: &RuntimeItemState,
) -> Result<(), RuntimeStoreError> {
    let result = sqlx::query("INSERT INTO agent_runtime_item (id,thread_id,turn_id,sort_order,phase,state) VALUES ($1,$2,$3,$4,$5,$6) \
        ON CONFLICT (id) DO UPDATE SET sort_order=EXCLUDED.sort_order,phase=EXCLUDED.phase,state=EXCLUDED.state \
        WHERE agent_runtime_item.thread_id=EXCLUDED.thread_id AND agent_runtime_item.turn_id=EXCLUDED.turn_id")
        .bind(id).bind(thread.thread_id.as_str()).bind(state.turn_id.as_str())
        .bind(i64::try_from(index).map_err(|_| RuntimeStoreError::Unavailable("item order overflow".to_string()))?)
        .bind(entity_phase(&state.phase)).bind(encode(state, "agent_runtime_item.state")?)
        .execute(&mut **tx).await.map_err(sql_error)?;
    ensure_projection_identity(result.rows_affected(), "item", id)?;
    Ok(())
}

async fn insert_interaction(
    tx: &mut Transaction<'_, Postgres>,
    thread: &RuntimeThreadState,
    id: &str,
    state: &RuntimeInteractionState,
) -> Result<(), RuntimeStoreError> {
    let result = sqlx::query("INSERT INTO agent_runtime_interaction (id,thread_id,turn_id,phase,state) VALUES ($1,$2,$3,$4,$5) \
        ON CONFLICT (id) DO UPDATE SET phase=EXCLUDED.phase,state=EXCLUDED.state \
        WHERE agent_runtime_interaction.thread_id=EXCLUDED.thread_id AND agent_runtime_interaction.turn_id=EXCLUDED.turn_id")
        .bind(id).bind(thread.thread_id.as_str()).bind(state.turn_id.as_str())
        .bind(entity_phase(&state.phase)).bind(encode(state, "agent_runtime_interaction.state")?)
        .execute(&mut **tx).await.map_err(sql_error)?;
    ensure_projection_identity(result.rows_affected(), "interaction", id)?;
    Ok(())
}

fn ensure_projection_identity(
    rows_affected: u64,
    entity: &'static str,
    id: &str,
) -> Result<(), RuntimeStoreError> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(RuntimeStoreError::Unavailable(format!(
            "runtime {entity} {id} changed immutable projection identity"
        )))
    }
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
           AND agent_context_preparation.record->'source_end_event_sequence'=excluded.record->'source_end_event_sequence' \
           AND ((agent_context_preparation.status='pending' AND excluded.status IN ('pending','prepared','terminal')) \
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
        let result = sqlx::query("INSERT INTO agent_runtime_outbox (operation_id,thread_id,binding_id,binding_epoch,driver_generation,payload) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (operation_id) DO NOTHING")
            .bind(entry.operation_id.as_str()).bind(entry.thread_id.as_str())
            .bind(entry.binding_id.as_str())
            .bind(u64_to_i64(entry.binding_epoch.0, "binding epoch")?)
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

async fn write_terminal_application_effect_outbox(
    tx: &mut Transaction<'_, Postgres>,
    entries: &[RuntimeTerminalApplicationEffectOutboxEntry],
) -> Result<(), RuntimeStoreError> {
    for entry in entries {
        let record = encode(
            entry,
            "agent_runtime_terminal_application_effect_outbox.record",
        )?;
        let result = sqlx::query(
            "INSERT INTO agent_runtime_terminal_application_effect_outbox \
             (effect_id,runtime_thread_id,terminal_event_sequence,record) \
             VALUES ($1,$2,$3,$4) ON CONFLICT (effect_id) DO NOTHING",
        )
        .bind(entry.effect_id.as_str())
        .bind(entry.runtime_thread_id.as_str())
        .bind(u64_to_i64(
            entry.terminal_event_sequence.0,
            "terminal event sequence",
        )?)
        .bind(record.clone())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            let current: Value = sqlx::query_scalar(
                "SELECT record FROM agent_runtime_terminal_application_effect_outbox WHERE effect_id=$1",
            )
            .bind(entry.effect_id.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)?;
            if current != record {
                return Err(RuntimeStoreError::Unavailable(
                    "terminal application effect identity was reused".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_terminal_application_effects(commit: &RuntimeCommit) -> Result<(), RuntimeStoreError> {
    for entry in &commit.terminal_application_effects {
        let valid = entry.runtime_thread_id == commit.projection.thread_id
            && commit.records.iter().any(|record| {
                record.carrier().sequence == Some(entry.terminal_event_sequence)
                    && matches!(
                        record.fact(),
                        RuntimeJournalFact::Presentation(event)
                            if matches!(
                                &event.event,
                                agentdash_agent_protocol::BackboneEvent::Platform(
                                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                                ) if key == "turn_terminal"
                            )
                    )
            });
        if !valid {
            return Err(RuntimeStoreError::Unavailable(
                "terminal application effect must reference its committed turn_terminal presentation"
                    .to_string(),
            ));
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
            "INSERT INTO agent_runtime_hook_effect (id,hook_run_id,thread_id,idempotency_key,effect_type,schema_version,target_authority,retry_limit,payload_digest,record,dispatched_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,CASE WHEN $11 THEN NULL ELSE now() END) ON CONFLICT DO NOTHING",
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
        .bind(effect.requires_external_dispatch())
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
        EmptyFactBatch => "empty_fact_batch",
        TransientInternalFact => "transient_internal_fact",
        StaleBinding { .. } => "stale_binding",
        DriverOperationAcceptance => "driver_operation_acceptance",
        DriverRuntimeOwnedContextEvent => "driver_runtime_owned_context_event",
        DriverRuntimeOwnedHookEvent => "driver_runtime_owned_hook_event",
        DriverRuntimeOwnedBindingEvent => "driver_runtime_owned_binding_event",
        InvalidTransition { .. } => "invalid_transition",
        InvalidDriverFact { .. } => "invalid_driver_fact",
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
    use std::{collections::BTreeSet, str::FromStr, sync::Arc, time::Duration};

    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, UserInputSource, UserInputSubmissionKind,
        UserInputSubmittedNotification,
    };
    use agentdash_agent_runtime::{
        BoundRuntimeHookEntry, BoundRuntimeHookPlan, CompactionPreparation, DriverEventAdmission,
        DriverEventQuarantineReason, HookAdmission, HookCompletion, HookCorrelation, HookEffect,
        HookEffectDescriptor, HookExecutionSite, HookGateDecision, HookRunStatus,
        ManagedAgentRuntime, QuarantinedDriverEvent, RuntimeHookInvocation, RuntimeHookPlanBinding,
        RuntimeRepository, RuntimeStoreError, RuntimeTerminalApplicationEffectClaimRequest,
        RuntimeTerminalApplicationEffectId, RuntimeTerminalApplicationEffectOutbox,
        RuntimeTerminalApplicationEffectOutboxEntry, RuntimeTransientEvents, RuntimeUnitOfWork,
        RuntimeWorkClaimRequest, RuntimeWorkKind, RuntimeWorkPayload, RuntimeWorkQueue,
        RuntimeWorkerId,
    };
    use agentdash_agent_runtime_contract::*;
    use agentdash_application_ports::agent_run_control_effect::{
        AgentRunControlEffectKind, AgentRunControlEffectStatus, AgentRunControlEffectStore,
        NewAgentRunControlEffectRecord,
    };
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunContextDeliveryTarget, AgentRunRuntimeBinding, AgentRunRuntimeBindingRepository,
        AgentRunRuntimeTarget,
    };
    use agentdash_integration_api::{
        AgentRuntimeContextBroker, DriverContextError, DriverTranscriptRequest,
    };

    use super::{PostgresRuntimeRepository, TestCommitFailurePoint, quarantine_reason};
    use crate::{
        PostgresAgentRunControlEffectStore, PostgresAgentRuntimeCompositionRepository,
        PostgresAgentRuntimeContextBroker,
    };

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    struct AllowSurface;

    #[async_trait::async_trait]
    impl agentdash_agent_runtime::RuntimeSurfaceReferenceValidator for AllowSurface {
        async fn validate_surface_reference(
            &self,
            _binding_id: &RuntimeBindingId,
            _runtime_thread_id: &RuntimeThreadId,
            _target: &RuntimeSurfaceDescriptor,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    async fn seed_agent_run_target(pool: &sqlx::PgPool) -> AgentRunRuntimeTarget {
        let target = AgentRunRuntimeTarget {
            run_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
        };
        let project_id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,status,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'plain','ready',$3,$3,$3)",
        )
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .bind(now)
        .execute(pool)
        .await
        .expect("seed runtime binding lifecycle run");
        sqlx::query(
            "INSERT INTO lifecycle_agents (id,run_id,project_id,source,status) \
             VALUES ($1,$2,$3,'primary','active')",
        )
        .bind(target.agent_id.to_string())
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("seed runtime binding lifecycle agent");
        target
    }

    async fn seed_runtime_host_binding(fixture: &Fixture) {
        let service_id = format!("service-{}", fixture.suffix);
        let offer_id = format!("offer-{}", fixture.suffix);
        let binding_id = format!("binding-{}", fixture.suffix);
        let profile_digest = format!("profile-{}", fixture.suffix);
        let mut tx = fixture
            .store
            .pool()
            .begin()
            .await
            .expect("begin runtime Host seed");
        sqlx::query(
            "INSERT INTO agent_runtime_service_instance \
             (id,definition_id,definition_build_digest,revision,config,credentials,placement,\
              desired_state,observed_state,active_generation) \
             VALUES ($1,'test-definition','sha256:test-definition',1,'{}','{}','{}','active','{}',7)",
        )
        .bind(&service_id)
        .execute(&mut *tx)
        .await
        .expect("seed runtime service instance");
        sqlx::query(
            "INSERT INTO agent_runtime_service_instance_revision \
             (service_instance_id,revision,instance_snapshot) VALUES ($1,1,'{}')",
        )
        .bind(&service_id)
        .execute(&mut *tx)
        .await
        .expect("seed runtime service revision");
        sqlx::query(
            "INSERT INTO agent_runtime_service_activation \
             (service_instance_id,instance_revision,driver_generation,protocol_revision,\
              effective_profile,profile_digest,conformance_evidence,instance_snapshot) \
             VALUES ($1,1,7,1,'{}',$2,'{}','{}')",
        )
        .bind(&service_id)
        .bind(&profile_digest)
        .execute(&mut *tx)
        .await
        .expect("seed runtime service activation");
        sqlx::query(
            "INSERT INTO agent_runtime_offer \
             (id,service_instance_id,instance_revision,driver_generation,profile_digest,available,offer) \
             VALUES ($1,$2,1,7,$3,true,'{}')",
        )
        .bind(&offer_id)
        .bind(&service_id)
        .bind(&profile_digest)
        .execute(&mut *tx)
        .await
        .expect("seed runtime offer");
        sqlx::query(
            "INSERT INTO agent_runtime_host_binding \
             (binding_id,thread_id,offer_id,service_instance_id,instance_revision,\
              driver_generation,profile_digest,state,lease_epoch,binding) \
             VALUES ($1,$2,$3,$4,1,7,$5,'active',1,'{}')",
        )
        .bind(&binding_id)
        .bind(fixture.thread_id.as_str())
        .bind(&offer_id)
        .bind(&service_id)
        .bind(&profile_digest)
        .execute(&mut *tx)
        .await
        .expect("seed runtime Host binding");
        tx.commit().await.expect("commit runtime Host seed");
    }

    struct TestTerminalPresentationProjector;

    impl RuntimeApplicationPresentationProjector for TestTerminalPresentationProjector {
        fn project_terminal(
            &self,
            context: RuntimeTerminalPresentationContext,
        ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError>
        {
            let terminal_type = match context.terminal {
                RuntimeTurnTerminal::Completed => "turn_completed",
                RuntimeTurnTerminal::Interrupted => "turn_interrupted",
                RuntimeTurnTerminal::Lost => "turn_lost",
                RuntimeTurnTerminal::Refused
                | RuntimeTurnTerminal::LimitReached
                | RuntimeTurnTerminal::Failed => "turn_failed",
            };
            Ok(vec![RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(context.runtime_turn_id.clone()),
                    presentation_turn_id: Some(context.presentation_turn_id.clone()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(context.presentation_thread_id.to_string()),
                    source_turn_id: Some(context.presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some(format!(
                        "test-turn-terminal:{}:{terminal_type}",
                        context.runtime_turn_id
                    )),
                    source_entry_index: None,
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: "turn_terminal".into(),
                            value: serde_json::json!({
                                "terminal_type": terminal_type,
                                "message": context.message,
                                "diagnostic": context.diagnostic,
                                "started_at_ms": context.started_at_ms,
                                "completed_at_ms": context.completed_at_ms,
                                "duration_ms": context.started_at_ms.map(|started_at_ms| {
                                    context.completed_at_ms.saturating_sub(started_at_ms)
                                }),
                            }),
                        },
                    ),
                ),
            }])
        }
    }

    #[test]
    fn fact_batch_quarantine_reasons_roundtrip_with_stable_storage_kinds() {
        for (reason, expected_kind) in [
            (
                DriverEventQuarantineReason::EmptyFactBatch,
                "empty_fact_batch",
            ),
            (
                DriverEventQuarantineReason::TransientInternalFact,
                "transient_internal_fact",
            ),
        ] {
            let entry = QuarantinedDriverEvent {
                event: DriverEventEnvelope {
                    binding_id: id("binding-quarantine"),
                    generation: RuntimeDriverGeneration(3),
                    operation_id: None,
                    source_thread_id: id("source-quarantine"),
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: Some("request-quarantine".to_string()),
                    source_entry_index: None,
                    facts: Vec::new(),
                },
                reason,
            };
            let encoded = serde_json::to_value(&entry).expect("encode quarantine record");
            let decoded: QuarantinedDriverEvent =
                serde_json::from_value(encoded.clone()).expect("decode quarantine record");

            assert_eq!(serde_json::to_value(&decoded).unwrap(), encoded);
            assert_eq!(quarantine_reason(&decoded), expected_kind);
        }
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
            lifecycle: [
                LifecycleCapability::ThreadStart,
                LifecycleCapability::ThreadResume,
            ]
            .into_iter()
            .collect(),
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
        let runtime =
            ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector))
                .with_surface_validator(Arc::new(AllowSurface));
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
            presentation: Vec::new(),
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
                presentation_thread_id: id(&format!("presentation-{}", fixture.suffix)),
                presentation_turn_id: None,
                binding_id: id(&format!("binding-{}", fixture.suffix)),
                driver_generation: RuntimeDriverGeneration(7),
                source_thread_id: id(&format!("source-{}", fixture.suffix)),
                profile_digest: id(&format!("profile-{}", fixture.suffix)),
                bound_profile: Box::new(profile()),
                input: Vec::new(),
                surface: Box::new(RuntimeSurfaceDescriptor {
                    source_frame_id: format!("frame-{}", fixture.suffix),
                    surface_revision: SurfaceRevision(1),
                    surface_digest: id(&format!("surface-{}", fixture.suffix)),
                    vfs_digest: format!("vfs-{}", fixture.suffix),
                    context_recipe_revision: ContextRecipeRevision(1),
                    context_digest: id(&format!("context-{}", fixture.suffix)),
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(0),
                    tool_set_digest: format!("tools-{}", fixture.suffix),
                    hook_plan: BoundRuntimeHookPlan {
                        revision: HookPlanRevision(1),
                        digest: id(&format!("hook-plan-{}", fixture.suffix)),
                        entries: Vec::new(),
                    },
                    terminal_hook_effect_binding: None,
                }),
                settings_revision: ThreadSettingsRevision(0),
            },
        }
    }

    fn abstract_presentation_record(
        fixture: &Fixture,
        sequence: u64,
        revision: RuntimeRevision,
        label: &str,
    ) -> RuntimeJournalRecord {
        let event = serde_json::from_value(serde_json::json!({
            "type": "item_completed",
            "payload": {
                "item": {
                    "type": "dynamicToolCall",
                    "id": format!("abstract-{label}"),
                    "namespace": null,
                    "tool": "persistence_fixture",
                    "arguments": { "label": label, "explicit_null": null, "ordered": [1, 2] },
                    "status": "completed",
                    "contentItems": null,
                    "success": true,
                    "durationMs": null
                },
                "threadId": "source-thread",
                "turnId": "source-turn",
                "completedAtMs": 1_712_345_678_901_i64 + sequence as i64
            }
        }))
        .expect("valid abstract presentation fixture");
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: fixture.thread_id.clone(),
                recorded_at_ms: 9_000 + sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: Some(id(&format!("binding-{}", fixture.suffix))),
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: Some(id("source-turn")),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("source-thread".to_string()),
                    source_turn_id: Some("source-turn".to_string()),
                    source_item_id: Some(format!("abstract-{label}")),
                    source_request_id: None,
                    source_entry_index: Some(sequence as u32),
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                event,
            )),
        )
        .expect("valid durable presentation record")
    }

    fn abstract_internal_record(
        fixture: &Fixture,
        sequence: u64,
        revision: RuntimeRevision,
    ) -> RuntimeJournalRecord {
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: fixture.thread_id.clone(),
                recorded_at_ms: 9_000 + sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: Some(id(&format!("binding-{}", fixture.suffix))),
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("source-thread".to_string()),
                    source_turn_id: Some("source-turn".to_string()),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Internal(RuntimeEvent::DriverContextCompactedOpaque),
        )
        .expect("valid durable internal record")
    }

    #[tokio::test]
    async fn postgres_repository_preserves_ordered_journal_records_without_rewriting_payload() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("presentation journal roundtrip").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load thread")
            .expect("thread");
        let first_sequence = base.next_event_sequence.0 + 1;
        let records = vec![
            abstract_presentation_record(&fixture, first_sequence, base.revision, "A"),
            abstract_internal_record(&fixture, first_sequence + 1, base.revision),
            abstract_presentation_record(&fixture, first_sequence + 2, base.revision, "B"),
        ];
        let protected_before = records
            .iter()
            .filter_map(|record| {
                record
                    .as_presentation()
                    .map(|presentation| serde_json::to_value(&presentation.event))
            })
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize protected bodies");
        let mut projection = base.clone();
        projection.next_event_sequence = EventSequence(first_sequence + 2);
        let mut presentation_live = fixture
            .store
            .subscribe_presentation(&fixture.thread_id)
            .await;
        fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(base.revision),
                projection,
                operation: None,
                operation_terminals: Vec::new(),
                records: records.clone(),
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
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
            .await
            .expect("commit presentation records");

        for expected in records
            .iter()
            .filter(|record| record.as_presentation().is_some())
        {
            let received = tokio::time::timeout(Duration::from_secs(1), presentation_live.recv())
                .await
                .expect("durable presentation live delivery timed out")
                .expect("durable presentation live sender closed");
            assert_eq!(&received, expected);
        }

        let replay = fixture
            .store
            .journal_records_after(&fixture.thread_id, Some(base.next_event_sequence))
            .await
            .expect("replay presentation records");
        assert_eq!(replay.records, records);
        let protected_after = replay
            .records
            .iter()
            .filter_map(|record| {
                record
                    .as_presentation()
                    .map(|presentation| serde_json::to_value(&presentation.event))
            })
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize replayed bodies");
        assert_eq!(protected_after, protected_before);
        assert_eq!(
            protected_after[0].pointer("/payload/item/arguments/explicit_null"),
            Some(&serde_json::Value::Null)
        );
        let kinds: Vec<String> = sqlx::query_scalar(
            "SELECT fact_kind FROM agent_runtime_event WHERE thread_id=$1 \
             AND event_sequence>$2 ORDER BY event_sequence",
        )
        .bind(fixture.thread_id.as_str())
        .bind(i64::try_from(base.next_event_sequence.0).expect("fixture cursor fits i64"))
        .fetch_all(fixture.store.pool())
        .await
        .expect("read stored fact kinds");
        assert_eq!(kinds, vec!["presentation", "internal", "presentation"]);
        let stored_fact_kinds: Vec<String> = sqlx::query_scalar(
            "SELECT record->'fact'->>'kind' FROM agent_runtime_event WHERE thread_id=$1 \
             AND event_sequence>$2 ORDER BY event_sequence",
        )
        .bind(fixture.thread_id.as_str())
        .bind(i64::try_from(base.next_event_sequence.0).expect("fixture cursor fits i64"))
        .fetch_all(fixture.store.pool())
        .await
        .expect("read stored record fact kinds");
        assert_eq!(stored_fact_kinds, kinds);

        let ephemeral = records[0]
            .as_presentation()
            .expect("presentation fixture")
            .clone();
        fixture
            .store
            .publish_transient_presentation(
                fixture.thread_id.clone(),
                id(&format!("binding-{}", fixture.suffix)),
                RuntimeDriverGeneration(1),
                None,
                base.revision,
                RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("source-thread".to_string()),
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(1),
                },
                ImmutablePresentationEvent::new(
                    PresentationDurability::Ephemeral,
                    ephemeral.event.clone(),
                ),
            )
            .await;
        tokio::time::timeout(Duration::from_secs(1), presentation_live.recv())
            .await
            .expect("ephemeral presentation live delivery timed out")
            .expect("ephemeral presentation live sender closed");
        assert_eq!(
            fixture
                .store
                .read_presentation(&fixture.thread_id, None, None)
                .await
                .len(),
            1
        );

        let started = start(&fixture);
        let RuntimeCommand::ThreadStart {
            presentation_thread_id,
            binding_id,
            driver_generation,
            source_thread_id,
            profile_digest,
            bound_profile,
            surface,
            settings_revision,
            ..
        } = started.command
        else {
            unreachable!("fixture starts a thread")
        };
        let mut projection = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load compacted projection")
            .expect("compacted thread");
        let expected_projection_revision = projection.revision;
        let tail_turn_id = format!("presentation-tail-{}", fixture.suffix);
        let tail_tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::DynamicToolCall {
                id: "turn_009:tool_014".to_string(),
                tool: "fs_glob".to_string(),
                arguments: serde_json::json!({"pattern":"**/*.rs"}),
                status: codex::DynamicToolCallStatus::Completed,
                content_items: Some(Some(vec![
                    codex::DynamicToolCallOutputContentItem::InputText {
                        text: "src/lib.rs".to_string(),
                    },
                ])),
                duration_ms: None,
                namespace: None,
                success: Some(Some(true)),
            },
            presentation_thread_id.to_string(),
            tail_turn_id.clone(),
        ));
        let tail_assistant = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::AgentMessage {
                id: "assistant-after-compaction".to_string(),
                text: "assistant tail".to_string(),
                phase: None,
                memory_citation: None,
            },
            presentation_thread_id.to_string(),
            tail_turn_id.clone(),
        ));
        let tail_records = [tail_tool, tail_assistant]
            .into_iter()
            .enumerate()
            .map(|(entry_index, event)| {
                projection
                    .append_durable_fact(
                        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                            PresentationDurability::Durable,
                            event,
                        )),
                        1_710_000_000_100 + u64::try_from(entry_index).unwrap_or_default(),
                        Some(binding_id.clone()),
                        None,
                        RuntimePresentationCoordinate {
                            runtime_turn_id: None,
                            presentation_turn_id: Some(id(&tail_turn_id)),
                            runtime_item_id: None,
                            interaction_id: None,
                            source_thread_id: Some(source_thread_id.to_string()),
                            source_turn_id: Some(tail_turn_id.clone()),
                            source_item_id: None,
                            source_request_id: None,
                            source_entry_index: Some(
                                u32::try_from(entry_index).unwrap_or(u32::MAX),
                            ),
                        },
                    )
                    .expect("append post-compaction presentation tail")
            })
            .collect::<Vec<_>>();
        let expected_latest_available = projection.next_event_sequence;
        let expected_tail_records = tail_records.clone();
        fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection,
                operation: None,
                operation_terminals: Vec::new(),
                records: tail_records,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
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
            .await
            .expect("persist post-compaction presentation tail");
        let composition = Arc::new(PostgresAgentRuntimeCompositionRepository::new(
            fixture.store.pool().clone(),
        ));
        seed_runtime_host_binding(&fixture).await;
        let target = seed_agent_run_target(fixture.store.pool()).await;
        composition
            .insert(AgentRunRuntimeBinding {
                target,
                presentation_thread_id,
                thread_id: fixture.thread_id.clone(),
                binding_id: binding_id.clone(),
                binding_epoch: BindingEpoch(1),
                driver_generation,
                source_thread_id,
                profile_digest,
                profile_provenance: ProfileProvenance {
                    service_digest: id("profile-service"),
                    transport_digest: id("profile-transport"),
                    host_policy_digest: id("profile-host"),
                },
                bound_profile: *bound_profile,
                surface: *surface,
                settings_revision,
                context_delivery_target: AgentRunContextDeliveryTarget {
                    connector_id: "native".to_string(),
                    executor: "NATIVE".to_string(),
                },
            })
            .await
            .expect("persist application binding lineage");
        let broker = PostgresAgentRuntimeContextBroker::new(fixture.store.clone(), composition);
        let transcript = broker
            .load_transcript(DriverTranscriptRequest {
                binding_id: binding_id.clone(),
                generation: driver_generation,
                runtime_thread_id: fixture.thread_id.clone(),
            })
            .await
            .expect("load complete durable transcript through production broker");
        assert_eq!(transcript.latest_available, expected_latest_available);
        assert_eq!(transcript.active_compaction_source_end, None);
        assert!(transcript.records.ends_with(&expected_tail_records));
        assert!(matches!(
            broker
                .load_transcript(DriverTranscriptRequest {
                    binding_id,
                    generation: driver_generation,
                    runtime_thread_id: id("different-runtime-thread"),
                })
                .await,
            Err(DriverContextError::Stale)
        ));
        fixture.store.clear(&fixture.thread_id).await;
        assert!(
            fixture
                .store
                .read_presentation(&fixture.thread_id, None, None)
                .await
                .is_empty()
        );
        fixture
            .store
            .publish_transient_presentation(
                fixture.thread_id.clone(),
                id(&format!("binding-{}", fixture.suffix)),
                RuntimeDriverGeneration(1),
                None,
                base.revision,
                RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("source-thread".to_string()),
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(2),
                },
                ImmutablePresentationEvent::new(PresentationDurability::Ephemeral, ephemeral.event),
            )
            .await;
        tokio::time::timeout(Duration::from_secs(1), presentation_live.recv())
            .await
            .expect("existing presentation receiver timed out after replay clear")
            .expect("presentation sender was replaced by replay clear");

        let mismatch = sqlx::query(
            "UPDATE agent_runtime_event SET fact_kind='internal' WHERE thread_id=$1 \
             AND event_sequence=$2",
        )
        .bind(fixture.thread_id.as_str())
        .bind(i64::try_from(first_sequence).expect("fixture sequence fits i64"))
        .execute(fixture.store.pool())
        .await
        .expect_err("database must reject fact_kind/record.fact mismatch");
        assert_eq!(
            mismatch
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("23514")
        );
    }

    #[tokio::test]
    async fn terminal_application_effect_outbox_is_atomic_and_lease_fenced_in_postgres() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("terminal application effect outbox").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load thread")
            .expect("thread");
        let terminal_sequence = EventSequence(base.next_event_sequence.0 + 1);
        let runtime_turn_id: RuntimeTurnId = id(&format!("runtime-turn-{}", fixture.suffix));
        let presentation_turn_id: PresentationTurnId =
            id(&format!("presentation-turn-{}", fixture.suffix));
        let terminal_record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: fixture.thread_id.clone(),
                recorded_at_ms: 12_345,
                sequence: Some(terminal_sequence),
                transient: None,
                revision: base.revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: Some(id(&format!("binding-{}", fixture.suffix))),
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(runtime_turn_id.clone()),
                    presentation_turn_id: Some(presentation_turn_id.clone()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(base.presentation_thread_id.to_string()),
                    source_turn_id: Some(presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some("terminal-fixture".to_string()),
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                        key: "turn_terminal".to_string(),
                        value: serde_json::json!({
                            "terminal_type": "turn_completed",
                            "message": null,
                            "diagnostic": null,
                            "started_at_ms": 12_000,
                            "completed_at_ms": 12_345,
                            "duration_ms": 345,
                        }),
                    },
                ),
            )),
        )
        .expect("terminal presentation record");
        let effect = RuntimeTerminalApplicationEffectOutboxEntry {
            effect_id: RuntimeTerminalApplicationEffectId::new(format!(
                "terminal-effect-{}",
                fixture.suffix
            ))
            .expect("effect id"),
            runtime_thread_id: fixture.thread_id.clone(),
            presentation_thread_id: base.presentation_thread_id.clone(),
            runtime_turn_id: runtime_turn_id.clone(),
            presentation_turn_id: presentation_turn_id.clone(),
            terminal_event_sequence: terminal_sequence,
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
            started_at_ms: Some(12_000),
            completed_at_ms: 12_345,
            binding_id: id(&format!("binding-{}", fixture.suffix)),
            driver_generation: RuntimeDriverGeneration(7),
            surface_revision: base.surface.surface_revision,
            surface_digest: base.surface.surface_digest.clone(),
            source_thread_id: format!("source-{}", fixture.suffix),
            source_turn_id: Some("source-turn".to_string()),
            terminal_hook_effect_binding: base.surface.terminal_hook_effect_binding.clone(),
        };
        let mut projection = base.clone();
        projection.next_event_sequence = terminal_sequence;
        fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(base.revision),
                projection,
                operation: None,
                operation_terminals: Vec::new(),
                records: vec![terminal_record],
                outbox: Vec::new(),
                terminal_application_effects: vec![effect.clone()],
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
            .await
            .expect("commit terminal presentation and effect atomically");

        let request = RuntimeTerminalApplicationEffectClaimRequest {
            owner: RuntimeWorkerId("postgres-terminal-worker".to_string()),
            lease_duration_ms: 30_000,
            limit: 1,
        };
        let first = fixture
            .store
            .claim_terminal_application_effects(request.clone())
            .await
            .expect("claim terminal effect")
            .pop()
            .expect("terminal effect claim");
        assert_eq!(first.entry, effect);
        assert_eq!(first.attempt, 1);
        fixture
            .store
            .release_terminal_application_effect(&first, "retry".to_string())
            .await
            .expect("release terminal effect");
        let second = fixture
            .store
            .claim_terminal_application_effects(request.clone())
            .await
            .expect("reclaim terminal effect")
            .pop()
            .expect("retried terminal effect claim");
        assert_eq!(second.entry, effect);
        assert_eq!(second.attempt, 2);
        assert!(matches!(
            fixture.store.ack_terminal_application_effect(&first).await,
            Err(RuntimeStoreError::WorkClaimConflict)
        ));
        fixture
            .store
            .ack_terminal_application_effect(&second)
            .await
            .expect("ack current terminal effect claim");
        assert!(
            fixture
                .store
                .claim_terminal_application_effects(request)
                .await
                .expect("claim after completion")
                .is_empty()
        );

        let current = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load current thread")
            .expect("current thread");
        let conflict_sequence = EventSequence(current.next_event_sequence.0 + 1);
        let conflict_record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: fixture.thread_id.clone(),
                recorded_at_ms: 12_500,
                sequence: Some(conflict_sequence),
                transient: None,
                revision: current.revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: Some(id(&format!("binding-{}", fixture.suffix))),
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(runtime_turn_id),
                    presentation_turn_id: Some(presentation_turn_id.clone()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(current.presentation_thread_id.to_string()),
                    source_turn_id: Some(presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some("terminal-conflict".to_string()),
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                        key: "turn_terminal".to_string(),
                        value: serde_json::json!({"terminal_type": "turn_failed"}),
                    },
                ),
            )),
        )
        .expect("conflicting terminal presentation record");
        let mut conflicting_effect = effect.clone();
        conflicting_effect.terminal_event_sequence = conflict_sequence;
        conflicting_effect.terminal = RuntimeTurnTerminal::Failed;
        let mut conflicting_projection = current.clone();
        conflicting_projection.next_event_sequence = conflict_sequence;
        let conflict = fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(current.revision),
                projection: conflicting_projection,
                operation: None,
                operation_terminals: Vec::new(),
                records: vec![conflict_record],
                outbox: Vec::new(),
                terminal_application_effects: vec![conflicting_effect],
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
        assert!(matches!(conflict, Err(RuntimeStoreError::Unavailable(_))));
        let after_conflict = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load after conflict")
            .expect("thread after conflict");
        assert_eq!(after_conflict.next_event_sequence, terminal_sequence);
        let conflicting_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM agent_runtime_event WHERE thread_id=$1 AND event_sequence=$2",
        )
        .bind(fixture.thread_id.as_str())
        .bind(i64::try_from(conflict_sequence.0).expect("conflict sequence fits bigint"))
        .fetch_one(fixture.store.pool())
        .await
        .expect("count rolled back conflicting event");
        assert_eq!(conflicting_rows, 0);
    }

    #[tokio::test]
    async fn agent_run_control_effect_store_preserves_exact_evidence_and_retry_fencing() {
        let _serial = serial_test_guard().await;
        let database = runtime_test_database().await;
        let store = PostgresAgentRunControlEffectStore::new(database.pool.clone());
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let effect = NewAgentRunControlEffectRecord {
            dedup_key: format!("runtime_terminal:{suffix}:delivery"),
            presentation_thread_id: format!("presentation-{suffix}")
                .parse()
                .expect("presentation thread"),
            presentation_turn_id: format!("turn-{suffix}").parse().expect("presentation turn"),
            terminal_event_sequence: EventSequence(9),
            effect_kind: AgentRunControlEffectKind::DeliveryConvergence,
            payload: serde_json::json!({
                "terminal": "completed",
                "diagnostic": null,
            }),
        };

        let inserted = store
            .insert_or_get(effect.clone())
            .await
            .expect("insert control effect");
        assert_eq!(inserted.status, AgentRunControlEffectStatus::Pending);
        let replay = store
            .insert_or_get(effect.clone())
            .await
            .expect("exact control effect replay");
        assert_eq!(replay.id, inserted.id);
        let mut conflicting = effect.clone();
        conflicting.payload = serde_json::json!({"terminal": "failed"});
        let error = store
            .insert_or_get(conflicting)
            .await
            .expect_err("immutable evidence conflict");
        assert!(error.contains("reused with different immutable evidence"));

        let first = store
            .claim(&effect.dedup_key, "delivery-worker", 30_000)
            .await
            .expect("claim control effect")
            .expect("control effect claim");
        assert_eq!(first.status, AgentRunControlEffectStatus::Running);
        let first_token = first.claim_token.expect("first claim token");
        assert!(
            store
                .claim(&effect.dedup_key, "other-worker", 30_000)
                .await
                .expect("competing claim")
                .is_none()
        );
        store
            .mark_failed(first.id, first_token, "retry".to_string())
            .await
            .expect("release failed control effect");
        let second = store
            .claim(&effect.dedup_key, "delivery-worker", 30_000)
            .await
            .expect("reclaim control effect")
            .expect("retried control effect claim");
        let second_token = second.claim_token.expect("second claim token");
        assert_ne!(second_token, first_token);
        assert!(
            store
                .mark_succeeded(second.id, first_token)
                .await
                .expect_err("stale control effect token")
                .contains("is not owned by claim")
        );
        store
            .mark_succeeded(second.id, second_token)
            .await
            .expect("complete current control effect claim");
        assert!(
            store
                .claim(&effect.dedup_key, "delivery-worker", 30_000)
                .await
                .expect("claim completed control effect")
                .is_none()
        );
    }

    #[tokio::test]
    async fn runtime_thread_name_migration_upgrades_strict_projection_and_clears_only_legacy_titles()
     {
        let _serial = serial_test_guard().await;
        let fixture = fixture("runtime thread name migration").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("seed runtime thread projection");
        let pool = fixture.store.pool();

        sqlx::query(
            "UPDATE agent_runtime_thread \
             SET projection=projection - 'thread_name' \
             WHERE id=$1",
        )
        .bind(fixture.thread_id.as_str())
        .execute(pool)
        .await
        .expect("simulate pre-0082 runtime projection");
        let strict_error = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect_err("strict projection must reject a missing thread_name field");
        assert!(
            strict_error.to_string().contains("thread_name"),
            "{strict_error}"
        );

        let run_id = uuid::Uuid::new_v4().to_string();
        let project_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,status,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'plain','ready',$3,$3,$3)",
        )
        .bind(&run_id)
        .bind(&project_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed lifecycle run for title migration");
        for source in ["auto", "codex", "user", "source"] {
            sqlx::query(
                "INSERT INTO lifecycle_agents \
                 (id,run_id,project_id,source,status,workspace_title,workspace_title_source) \
                 VALUES ($1,$2,$3,'primary','active',$4,$5)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&run_id)
            .bind(&project_id)
            .bind(format!("{source}-title"))
            .bind(source)
            .execute(pool)
            .await
            .expect("seed lifecycle workspace title");
        }

        sqlx::raw_sql(include_str!(
            "../../../migrations/0082_runtime_thread_name_projection.sql"
        ))
        .execute(pool)
        .await
        .expect("apply runtime thread name projection migration");

        let projection = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load migrated strict projection")
            .expect("migrated runtime thread");
        assert_eq!(projection.thread_name, None);
        let titles: Vec<(Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT workspace_title_source,workspace_title,id \
             FROM lifecycle_agents \
             WHERE run_id=$1 \
             ORDER BY workspace_title_source NULLS FIRST,id",
        )
        .bind(&run_id)
        .fetch_all(pool)
        .await
        .expect("load migrated lifecycle titles");
        assert_eq!(titles.len(), 4);
        assert_eq!(
            titles
                .iter()
                .filter(|(source, title, _)| source.is_none() && title.is_none())
                .count(),
            2,
            "auto/codex titles must be cleared"
        );
        assert!(titles.iter().any(|(source, title, _)| {
            source.as_deref() == Some("source") && title.as_deref() == Some("source-title")
        }));
        assert!(titles.iter().any(|(source, title, _)| {
            source.as_deref() == Some("user") && title.as_deref() == Some("user-title")
        }));
    }

    #[tokio::test]
    async fn migration_exposes_only_runtime_journal_record_columns() {
        let _serial = serial_test_guard().await;
        let database = runtime_test_database().await;
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema='public' AND table_name='agent_runtime_event' \
             ORDER BY column_name",
        )
        .fetch_all(&database.pool)
        .await
        .expect("read runtime journal columns");
        assert!(columns.iter().any(|column| column == "record"));
        assert!(columns.iter().any(|column| column == "fact_kind"));
        assert!(!columns.iter().any(|column| column == "envelope"));
        assert!(!columns.iter().any(|column| column == "event_kind"));
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=70 AND success)",
        )
        .fetch_one(&database.pool)
        .await
        .expect("0070 migration history");
        assert!(applied);
        let terminal_effect_schema: bool = sqlx::query_scalar(
            "SELECT to_regclass('public.agent_runtime_terminal_application_effect_outbox') IS NOT NULL \
             AND EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=73 AND success)",
        )
        .fetch_one(&database.pool)
        .await
        .expect("0073 terminal application effect schema");
        assert!(terminal_effect_schema);
        let outbox_binding_schema: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=78 AND success) \
             AND EXISTS(SELECT 1 FROM information_schema.columns \
                        WHERE table_schema='public' AND table_name='agent_runtime_outbox' \
                          AND column_name='binding_id' AND is_nullable='NO') \
             AND EXISTS(SELECT 1 FROM information_schema.columns \
                        WHERE table_schema='public' AND table_name='agent_runtime_outbox' \
                          AND column_name='binding_epoch' AND is_nullable='NO')",
        )
        .fetch_one(&database.pool)
        .await
        .expect("0078 immutable outbox binding schema");
        assert!(outbox_binding_schema);
        let outbox_binding_fk: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
             WHERE conrelid='agent_runtime_outbox'::regclass \
               AND conname='agent_runtime_outbox_binding_generation_fkey'",
        )
        .fetch_one(&database.pool)
        .await
        .expect("0078 outbox binding generation foreign key");
        assert!(outbox_binding_fk.contains("FOREIGN KEY (binding_id, driver_generation)"));
        assert!(outbox_binding_fk.contains("agent_runtime_binding(id, driver_generation)"));
        let old_thread_generation_fk_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_constraint \
             WHERE conrelid='agent_runtime_outbox'::regclass \
               AND conname='agent_runtime_outbox_thread_generation_fkey')",
        )
        .fetch_one(&database.pool)
        .await
        .expect("retired mutable thread generation foreign key");
        assert!(!old_thread_generation_fk_exists);
    }

    #[tokio::test]
    async fn thread_rebind_preserves_historical_outbox_binding_fence() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("rebind outbox coordinates").await;
        fixture
            .runtime
            .execute(start(&fixture))
            .await
            .expect("start runtime thread");
        let original_binding_id = format!("binding-{}", fixture.suffix);
        let source_thread_id = format!("source-{}", fixture.suffix);
        let operation_id = format!("operation-{}", fixture.suffix);
        let before: (String, i64, i64, serde_json::Value) = sqlx::query_as(
            "SELECT binding_id,binding_epoch,driver_generation,payload \
             FROM agent_runtime_outbox WHERE operation_id=$1",
        )
        .bind(&operation_id)
        .fetch_one(fixture.store.pool())
        .await
        .expect("load original outbox fence");
        assert_eq!(before.0, original_binding_id);
        assert_eq!(before.1, 1);
        assert_eq!(before.2, 7);

        fixture
            .runtime
            .ingest_driver_event(DriverEventEnvelope {
                binding_id: id(&original_binding_id),
                generation: RuntimeDriverGeneration(7),
                operation_id: None,
                source_thread_id: id(&source_thread_id),
                source_turn_id: None,
                source_item_id: None,
                source_request_id: Some(format!("lost-{}", fixture.suffix)),
                source_entry_index: None,
                facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                    binding_id: id(&original_binding_id),
                    reason: "fixture driver restart".to_string(),
                })],
            })
            .await
            .expect("mark runtime thread lost");
        let lost = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load lost thread")
            .expect("lost thread");
        assert_eq!(lost.status, RuntimeThreadStatus::Lost);

        let new_binding_id = format!("{original_binding_id}-epoch-2");
        let rebound_profile = profile();
        let rebound_profile_digest = runtime_profile_digest(&rebound_profile);
        sqlx::query(
            "INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) \
             VALUES ($1,8,$2)",
        )
        .bind(&new_binding_id)
        .bind(rebound_profile_digest.as_str())
        .execute(fixture.store.pool())
        .await
        .expect("seed replacement Host binding");
        sqlx::query(
            "INSERT INTO agent_runtime_source_coordinate \
             (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)",
        )
        .bind(&new_binding_id)
        .bind(&source_thread_id)
        .bind(fixture.thread_id.as_str())
        .execute(fixture.store.pool())
        .await
        .expect("seed replacement source coordinate");
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: id(&format!("rebind-operation-{}", fixture.suffix)),
                    idempotency_key: id(&format!("rebind-key-{}", fixture.suffix)),
                    expected_thread_revision: Some(lost.revision),
                    actor: RuntimeActor::System {
                        component: "postgres-runtime-recovery-test".to_string(),
                    },
                },
                command: RuntimeCommand::ThreadRebind {
                    thread_id: fixture.thread_id.clone(),
                    recovery_intent_id: id(&format!("recovery-{}", fixture.suffix)),
                    binding_epoch: BindingEpoch(2),
                    expected_binding_id: id(&original_binding_id),
                    expected_driver_generation: RuntimeDriverGeneration(7),
                    new_binding_id: id(&new_binding_id),
                    new_driver_generation: RuntimeDriverGeneration(8),
                    source_thread_id: id(&source_thread_id),
                    profile_digest: rebound_profile_digest,
                    bound_profile: Box::new(rebound_profile),
                },
            })
            .await
            .expect("commit RuntimeRebind with historical outbox present");

        let rebound = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load rebound thread")
            .expect("rebound thread");
        assert_eq!(rebound.binding_id, id(&new_binding_id));
        assert_eq!(rebound.binding_epoch, BindingEpoch(2));
        assert_eq!(rebound.driver_generation, RuntimeDriverGeneration(8));
        let after: (String, i64, i64, serde_json::Value) = sqlx::query_as(
            "SELECT binding_id,binding_epoch,driver_generation,payload \
             FROM agent_runtime_outbox WHERE operation_id=$1",
        )
        .bind(&operation_id)
        .fetch_one(fixture.store.pool())
        .await
        .expect("reload historical outbox fence");
        assert_eq!(after, before, "RuntimeRebind must not rewrite history");
    }

    #[tokio::test]
    async fn outbox_binding_coordinate_migration_upgrades_schema_77_rows() {
        let _serial = serial_test_guard().await;
        let database = runtime_test_database().await;
        let pool = &database.pool;
        sqlx::raw_sql(
            "ALTER TABLE agent_runtime_outbox \
                 DROP CONSTRAINT agent_runtime_outbox_binding_generation_fkey, \
                 DROP CONSTRAINT agent_runtime_outbox_binding_epoch_check, \
                 DROP COLUMN binding_id, \
                 DROP COLUMN binding_epoch; \
             ALTER TABLE agent_runtime_outbox \
                 ADD CONSTRAINT agent_runtime_outbox_thread_generation_fkey \
                 FOREIGN KEY (thread_id,driver_generation) \
                 REFERENCES agent_runtime_thread(id,driver_generation) ON DELETE CASCADE;",
        )
        .execute(pool)
        .await
        .expect("restore schema 77 outbox coordinates");
        sqlx::query(
            "INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) \
             VALUES ('upgrade-binding',7,'upgrade-profile')",
        )
        .execute(pool)
        .await
        .expect("seed upgrade binding");
        sqlx::query(
            "INSERT INTO agent_runtime_source_coordinate \
             (binding_id,source_thread_id,thread_id) \
             VALUES ('upgrade-binding','upgrade-source','upgrade-thread')",
        )
        .execute(pool)
        .await
        .expect("seed upgrade source");
        sqlx::query(
            "INSERT INTO agent_runtime_thread \
             (id,revision,next_event_sequence,next_operation_sequence,status,binding_id,driver_generation,source_thread_id,profile_digest,context_revision,settings_revision,tool_set_revision,projection) \
             VALUES ('upgrade-thread',0,0,1,'active','upgrade-binding',7,'upgrade-source','upgrade-profile',0,0,0,'{}')",
        )
        .execute(pool)
        .await
        .expect("seed upgrade thread");
        sqlx::query(
            "INSERT INTO agent_runtime_operation \
             (id,thread_id,operation_sequence,idempotency_key,accepted_revision,status,actor,command,record) \
             VALUES ('upgrade-operation','upgrade-thread',1,'upgrade-key',0,'active','{}','{}','{}')",
        )
        .execute(pool)
        .await
        .expect("seed upgrade operation");
        sqlx::query(
            "INSERT INTO agent_runtime_outbox \
             (operation_id,thread_id,driver_generation,payload) \
             VALUES ('upgrade-operation','upgrade-thread',7,$1)",
        )
        .bind(serde_json::json!({
            "binding_id": "upgrade-binding",
            "binding_epoch": 1,
        }))
        .execute(pool)
        .await
        .expect("seed schema 77 outbox row");

        sqlx::raw_sql(include_str!(
            "../../../migrations/0078_rebind_safe_runtime_outbox_coordinates.sql"
        ))
        .execute(pool)
        .await
        .expect("apply schema 78 outbox coordinate migration");
        let upgraded: (String, i64, i64) = sqlx::query_as(
            "SELECT binding_id,binding_epoch,driver_generation \
             FROM agent_runtime_outbox WHERE operation_id='upgrade-operation'",
        )
        .fetch_one(pool)
        .await
        .expect("load upgraded outbox row");
        assert_eq!(upgraded, ("upgrade-binding".to_string(), 1, 7));
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
    async fn presentation_contract_upgrade_clears_runtime_graph_without_rewriting_migration_history()
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

        let terminal_event_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT max(event_sequence) FROM agent_runtime_event WHERE thread_id=$1",
        )
        .bind(fixture.thread_id.as_str())
        .fetch_one(pool)
        .await
        .expect("runtime event for reset outbox fixture");
        let terminal_event_sequence =
            terminal_event_sequence.expect("started Runtime must have a journal event");
        sqlx::query(
            "INSERT INTO agent_runtime_terminal_application_effect_outbox \
             (effect_id,runtime_thread_id,terminal_event_sequence,record) \
             VALUES ('reset-terminal-effect',$1,$2,'{}'::jsonb)",
        )
        .bind(fixture.thread_id.as_str())
        .bind(terminal_event_sequence)
        .execute(pool)
        .await
        .expect("seed terminal application effect outbox");
        sqlx::query(
            "INSERT INTO agent_run_control_effects \
             (id,dedup_key,delivery_runtime_session_id,turn_id,terminal_event_seq,effect_kind,\
              payload_json,status,attempt_count,created_at_ms,updated_at_ms) \
             VALUES ('00000000-0000-0000-0000-000000000070','reset-control-effect',\
                     'reset-presentation-thread','reset-presentation-turn',1,\
                     'agent_run_delivery_convergence','{}'::jsonb,'succeeded',0,0,0)",
        )
        .execute(pool)
        .await
        .expect("seed AgentRun control effect ledger");

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

        sqlx::raw_sql(
            "ALTER TABLE agent_runtime_event \
             DROP CONSTRAINT agent_runtime_event_fact_kind_check; \
             ALTER TABLE agent_runtime_event RENAME COLUMN fact_kind TO event_kind; \
             ALTER TABLE agent_runtime_event RENAME COLUMN record TO envelope;",
        )
        .execute(pool)
        .await
        .expect("restore pre-0070 journal schema");

        sqlx::raw_sql(include_str!(
            "../../../migrations/0070_runtime_journal_records.sql"
        ))
        .execute(pool)
        .await
        .expect("apply journal record upgrade migration body");
        sqlx::raw_sql(include_str!(
            "../../../migrations/0074_reset_runtime_presentation_contract.sql"
        ))
        .execute(pool)
        .await
        .expect("apply immutable presentation contract reset migration body");

        for table in [
            "agent_runtime_thread",
            "agent_runtime_binding",
            "agent_runtime_source_coordinate",
            "agent_runtime_event",
            "agent_runtime_terminal_application_effect_outbox",
            "agent_run_runtime_thread_anchor",
            "agent_run_runtime_binding_lineage",
            "agent_run_control_effects",
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
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=70 AND success) \
             AND EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=74 AND success)",
        )
        .fetch_one(pool)
        .await
        .expect("0070/0074 migration history");
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
    async fn critical_driver_violation_is_one_atomic_postgres_commit_from_the_committed_base() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("critical driver violation atomicity").await;
        let mut initial = start(&fixture);
        let RuntimeCommand::ThreadStart { bound_profile, .. } = &mut initial.command else {
            unreachable!("fixture starts a Runtime thread")
        };
        bound_profile
            .lifecycle
            .insert(LifecycleCapability::TurnStart);
        fixture
            .runtime
            .execute(initial)
            .await
            .expect("start runtime thread");
        let started = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load started thread")
            .expect("started thread");
        let turn_operation_id: RuntimeOperationId =
            id(&format!("operation-violation-{}", fixture.suffix));
        let runtime_turn_id: RuntimeTurnId = id(&format!("turn-{turn_operation_id}"));
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: turn_operation_id.clone(),
                    idempotency_key: id(&format!("key-violation-{}", fixture.suffix)),
                    expected_thread_revision: Some(started.revision),
                    actor: RuntimeActor::System {
                        component: "postgres-critical-violation-test".to_string(),
                    },
                },
                command: RuntimeCommand::TurnStart {
                    thread_id: fixture.thread_id.clone(),
                    presentation_turn_id: id(&format!(
                        "presentation-turn-violation-{}",
                        fixture.suffix
                    )),
                    input: Vec::new(),
                },
            })
            .await
            .expect("start active turn");
        let committed_base = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load committed base")
            .expect("committed base");
        let event = DriverEventEnvelope {
            binding_id: id(&format!("binding-{}", fixture.suffix)),
            generation: RuntimeDriverGeneration(7),
            operation_id: Some(turn_operation_id.clone()),
            source_thread_id: id(&format!("source-{}", fixture.suffix)),
            source_turn_id: Some(id(&format!("source-turn-{}", fixture.suffix))),
            source_item_id: None,
            source_request_id: Some(format!("request-violation-{}", fixture.suffix)),
            source_entry_index: None,
            facts: vec![
                RuntimeJournalFact::Internal(RuntimeEvent::ConversationError {
                    turn_id: Some(runtime_turn_id.clone()),
                    error: RuntimeConversationError {
                        code: Some("postgres-staged-prefix-must-not-commit".into()),
                        message: "valid staged prefix".into(),
                        retryable: true,
                        details: None,
                    },
                }),
                RuntimeJournalFact::Internal(RuntimeEvent::ItemTerminal {
                    turn_id: runtime_turn_id,
                    item_id: id(&format!("missing-item-{}", fixture.suffix)),
                    terminal: RuntimeItemTerminal::Lost {
                        message: Some("invalid suffix".into()),
                    },
                }),
            ],
        };

        fixture
            .store
            .fail_next_commit_at(TestCommitFailurePoint::Outbox);
        assert!(
            fixture
                .runtime
                .ingest_driver_event(event.clone())
                .await
                .is_err()
        );
        let rolled_back_projection = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load rolled-back projection")
            .expect("rolled-back projection");
        assert_eq!(
            serde_json::to_value(&rolled_back_projection).expect("encode rolled-back projection"),
            serde_json::to_value(&committed_base).expect("encode committed base"),
            "a late UoW failure must roll back the violation projection"
        );
        let rolled_back_side_effects: (i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT COUNT(*) FROM agent_runtime_quarantine WHERE binding_id=$2),\
             (SELECT COUNT(*) FROM agent_runtime_terminal_application_effect_outbox \
              WHERE runtime_thread_id=$1)",
        )
        .bind(fixture.thread_id.as_str())
        .bind(format!("binding-{}", fixture.suffix))
        .fetch_one(fixture.store.pool())
        .await
        .expect("count rolled-back violation side effects");
        assert_eq!(rolled_back_side_effects, (0, 0));

        assert!(matches!(
            fixture
                .runtime
                .ingest_driver_event(event)
                .await
                .expect("retry critical violation from durable committed base"),
            DriverEventAdmission::Terminalized { .. }
        ));
        let final_projection = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load terminalized projection")
            .expect("terminalized projection");
        assert_eq!(final_projection.status, RuntimeThreadStatus::Lost);
        assert!(final_projection.active_turn_id.is_none());
        let operation = fixture
            .store
            .find_operation(&turn_operation_id)
            .await
            .expect("load terminalized operation")
            .expect("terminalized operation");
        assert!(matches!(
            operation.terminal,
            Some(RuntimeOperationTerminal::Lost { .. })
        ));

        let records = fixture
            .store
            .journal_records_after(&fixture.thread_id, None)
            .await
            .expect("load canonical violation journal")
            .records;
        assert!(records.iter().all(|record| !matches!(
            record.fact(),
            RuntimeJournalFact::Internal(RuntimeEvent::ConversationError { error, .. })
                if error.code.as_deref() == Some("postgres-staged-prefix-must-not-commit")
        )));
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(
                    record.fact(),
                    RuntimeJournalFact::Internal(RuntimeEvent::ProtocolViolation {
                        critical: true,
                        ..
                    })
                ))
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(
                    record.fact(),
                    RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                        terminal: RuntimeTurnTerminal::Lost,
                        ..
                    })
                ))
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(
                    record.fact(),
                    RuntimeJournalFact::Presentation(event)
                        if matches!(
                            &event.event,
                            BackboneEvent::Platform(
                                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                                    key,
                                    ..
                                }
                            ) if key == "turn_terminal"
                        )
                ))
                .count(),
            1
        );
        let durable_coordinates: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT t.revision,(t.projection->>'revision')::bigint,\
                    t.next_event_sequence,MAX(e.revision),MAX(e.event_sequence) \
             FROM agent_runtime_thread t \
             JOIN agent_runtime_event e ON e.thread_id=t.id \
             WHERE t.id=$1 \
             GROUP BY t.revision,t.projection,t.next_event_sequence",
        )
        .bind(fixture.thread_id.as_str())
        .fetch_one(fixture.store.pool())
        .await
        .expect("load durable revision coordinates");
        assert_eq!(durable_coordinates.0, durable_coordinates.1);
        assert_eq!(durable_coordinates.0, durable_coordinates.3);
        assert_eq!(durable_coordinates.2, durable_coordinates.4);
        assert_eq!(durable_coordinates.0, final_projection.revision.0 as i64);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_runtime_quarantine WHERE binding_id=$1",
            )
            .bind(format!("binding-{}", fixture.suffix))
            .fetch_one(fixture.store.pool())
            .await
            .expect("count canonical quarantine"),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_runtime_terminal_application_effect_outbox \
                 WHERE runtime_thread_id=$1",
            )
            .bind(fixture.thread_id.as_str())
            .fetch_one(fixture.store.pool())
            .await
            .expect("count canonical terminal application effect"),
            1
        );
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
            records: Vec::new(),
            outbox: Vec::new(),
            terminal_application_effects: Vec::new(),
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
                records: Vec::new(),
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
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
            presentation: Vec::new(),
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
                records: agentdash_agent_runtime::internal_journal_records(events)
                    .expect("valid durable internal journal records"),
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
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
                .internal_events_after(&fixture.thread_id, None)
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
    async fn runtime_outbox_preserves_thread_causality_across_surface_adopt_retry() {
        let _serial = serial_test_guard().await;
        let fixture = fixture("surface adoption outbox causality").await;
        let mut initial = start(&fixture);
        let RuntimeCommand::ThreadStart { bound_profile, .. } = &mut initial.command else {
            unreachable!("fixture starts a Runtime thread")
        };
        bound_profile.lifecycle.extend([
            LifecycleCapability::SurfaceAdopt,
            LifecycleCapability::TurnStart,
        ]);
        let initial_operation_id = initial.meta.operation_id.clone();
        fixture
            .runtime
            .execute(initial)
            .await
            .expect("start causal Runtime thread");
        let initial_claim = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::RuntimeOutbox,
                owner: RuntimeWorkerId("causal-initial-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 8,
            })
            .await
            .expect("claim initial ThreadStart")
            .into_iter()
            .find(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &initial_operation_id))
            .expect("initial ThreadStart outbox claim");
        fixture
            .runtime
            .complete_driver_dispatch_operation(&fixture.thread_id, &initial_operation_id)
            .await
            .expect("complete initial delivery operation");
        fixture
            .store
            .ack(&initial_claim)
            .await
            .expect("ack initial ThreadStart");

        let before_adoption = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load Runtime thread before adoption")
            .expect("Runtime thread exists");
        let mut target = before_adoption.surface.clone();
        target.surface_revision = SurfaceRevision(target.surface_revision.0 + 1);
        target.surface_digest = id(&format!("surface-adopted-{}", fixture.suffix));
        target.source_frame_id = format!("frame-adopted-{}", fixture.suffix);
        target.hook_plan.revision = HookPlanRevision(target.hook_plan.revision.0 + 1);
        target.hook_plan.digest = id(&format!("hook-plan-adopted-{}", fixture.suffix));
        let adopt_operation_id: RuntimeOperationId =
            id(&format!("surface-adopt-operation-{}", fixture.suffix));
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: adopt_operation_id.clone(),
                    idempotency_key: id(&format!("surface-adopt-key-{}", fixture.suffix)),
                    expected_thread_revision: Some(before_adoption.revision),
                    actor: RuntimeActor::System {
                        component: "surface-adopt-causality-test".to_string(),
                    },
                },
                command: RuntimeCommand::SurfaceAdopt {
                    thread_id: fixture.thread_id.clone(),
                    expected_surface_revision: before_adoption.surface.surface_revision,
                    expected_surface_digest: before_adoption.surface.surface_digest,
                    target: Box::new(target),
                },
            })
            .await
            .expect("accept canonical SurfaceAdopt");
        let after_adoption = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load Runtime thread after adoption")
            .expect("Runtime thread exists");
        let turn_operation_id: RuntimeOperationId =
            id(&format!("turn-after-adopt-operation-{}", fixture.suffix));
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: turn_operation_id.clone(),
                    idempotency_key: id(&format!("turn-after-adopt-key-{}", fixture.suffix)),
                    expected_thread_revision: Some(after_adoption.revision),
                    actor: RuntimeActor::User {
                        subject: "causality-test-user".to_string(),
                    },
                },
                command: RuntimeCommand::TurnStart {
                    thread_id: fixture.thread_id.clone(),
                    presentation_turn_id: id(&format!(
                        "presentation-turn-after-adopt-{}",
                        fixture.suffix
                    )),
                    input: vec![RuntimeInput::text(
                        "must wait for adopted driver surface".to_string(),
                    )],
                },
            })
            .await
            .expect("accept TurnStart behind SurfaceAdopt");

        let other_suffix = uuid::Uuid::new_v4().simple().to_string();
        let other_binding_id = format!("binding-{other_suffix}");
        let other_source_id = format!("source-{other_suffix}");
        let other_thread_id = format!("thread-{other_source_id}");
        sqlx::query("INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ($1,7,$2)")
            .bind(&other_binding_id)
            .bind(format!("profile-{other_suffix}"))
            .execute(fixture.store.pool())
            .await
            .expect("seed independent binding");
        sqlx::query("INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)")
            .bind(&other_binding_id)
            .bind(&other_source_id)
            .bind(&other_thread_id)
            .execute(fixture.store.pool())
            .await
            .expect("seed independent source coordinate");
        let mut other_start = start(&fixture);
        let other_operation_id: RuntimeOperationId =
            id(&format!("other-thread-operation-{other_suffix}"));
        other_start.meta.operation_id = other_operation_id.clone();
        other_start.meta.idempotency_key = id(&format!("other-thread-key-{other_suffix}"));
        let RuntimeCommand::ThreadStart {
            thread_id,
            presentation_thread_id,
            presentation_turn_id,
            binding_id,
            source_thread_id,
            profile_digest,
            surface,
            ..
        } = &mut other_start.command
        else {
            unreachable!("fixture starts a Runtime thread")
        };
        *thread_id = id(&other_thread_id);
        *presentation_thread_id = id(&format!("presentation-{other_suffix}"));
        *presentation_turn_id = None;
        *binding_id = id(&other_binding_id);
        *source_thread_id = id(&other_source_id);
        *profile_digest = id(&format!("profile-{other_suffix}"));
        surface.source_frame_id = format!("frame-{other_suffix}");
        surface.surface_digest = id(&format!("surface-{other_suffix}"));
        surface.vfs_digest = format!("vfs-{other_suffix}");
        surface.context_digest = id(&format!("context-{other_suffix}"));
        surface.tool_set_digest = format!("tools-{other_suffix}");
        surface.hook_plan.digest = id(&format!("hook-plan-{other_suffix}"));
        fixture
            .runtime
            .execute(other_start)
            .await
            .expect("accept independent thread work");

        let first_batch = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::RuntimeOutbox,
                owner: RuntimeWorkerId("causal-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 8,
            })
            .await
            .expect("claim per-thread outbox heads");
        assert_eq!(first_batch.len(), 2, "one head per thread is claimable");
        assert!(first_batch.iter().any(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &adopt_operation_id)));
        assert!(first_batch.iter().any(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &other_operation_id)));
        assert!(!first_batch.iter().any(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &turn_operation_id)));

        let adopt_claim = first_batch
            .iter()
            .find(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &adopt_operation_id))
            .expect("SurfaceAdopt head claim")
            .clone();
        let other_claim = first_batch
            .iter()
            .find(|claim| matches!(&claim.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &other_operation_id))
            .expect("independent thread head claim")
            .clone();
        fixture
            .store
            .release(&adopt_claim, "connector surface sync failed".to_string())
            .await
            .expect("release failed SurfaceAdopt");
        fixture
            .runtime
            .complete_driver_dispatch_operation(&id(&other_thread_id), &other_operation_id)
            .await
            .expect("complete independent thread operation");
        fixture
            .store
            .ack(&other_claim)
            .await
            .expect("ack independent thread operation");

        let retry_batch = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::RuntimeOutbox,
                owner: RuntimeWorkerId("causal-retry-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 8,
            })
            .await
            .expect("retry failed causal head");
        assert_eq!(retry_batch.len(), 1);
        let adopt_retry = retry_batch.into_iter().next().expect("SurfaceAdopt retry");
        assert!(
            matches!(&adopt_retry.identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &adopt_operation_id)
        );
        assert_eq!(adopt_retry.attempt, 2);
        fixture
            .runtime
            .complete_driver_dispatch_operation(&fixture.thread_id, &adopt_operation_id)
            .await
            .expect("complete adopted driver surface sync");
        fixture
            .store
            .ack(&adopt_retry)
            .await
            .expect("ack adopted driver surface sync");

        let resumed = fixture
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::RuntimeOutbox,
                owner: RuntimeWorkerId("causal-turn-worker".to_string()),
                lease_duration_ms: 30_000,
                limit: 8,
            })
            .await
            .expect("claim TurnStart after adoption succeeds");
        assert_eq!(resumed.len(), 1);
        assert!(
            matches!(&resumed[0].identity, agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id) if id == &turn_operation_id)
        );
        assert_eq!(resumed[0].attempt, 1);
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
        let active_turn_id = format!("active-turn-{}", fixture.suffix);
        sqlx::query(
            "INSERT INTO agent_runtime_turn (id,thread_id,phase,state) VALUES ($1,$2,'active',$3)",
        )
        .bind(&active_turn_id)
        .bind(fixture.thread_id.as_str())
        .bind(serde_json::json!({}))
        .execute(fixture.store.pool())
        .await
        .expect("seed active turn projection");
        let compaction_id: ContextCompactionId = id(&format!("compaction-{}", fixture.suffix));
        let operation_id: RuntimeOperationId = id(&format!("compact-operation-{}", fixture.suffix));
        fixture
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
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
        sqlx::query("UPDATE agent_runtime_thread SET active_turn_id=$1 WHERE id=$2")
            .bind(&active_turn_id)
            .bind(fixture.thread_id.as_str())
            .execute(fixture.store.pool())
            .await
            .expect("mark active turn projection");

        let request = |kind, owner: &str| RuntimeWorkClaimRequest {
            kind,
            owner: RuntimeWorkerId(owner.to_string()),
            lease_duration_ms: 30_000,
            limit: 1,
        };
        let blocked_while_active = fixture
            .store
            .claim(request(
                RuntimeWorkKind::ContextPreparation,
                "blocked-prepare-worker",
            ))
            .await
            .expect("active turn blocks preparation");
        assert!(blocked_while_active.is_empty());
        sqlx::query("UPDATE agent_runtime_thread SET active_turn_id=NULL WHERE id=$1")
            .bind(fixture.thread_id.as_str())
            .execute(fixture.store.pool())
            .await
            .expect("finish active turn projection");
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
        let source_end_event_sequence = match &preparation_claim.payload {
            RuntimeWorkPayload::ContextPreparation(work) => work.source_end_event_sequence,
            other => panic!("unexpected preparation payload: {other:?}"),
        };

        // A later turn is allowed to complete while this already-admitted compaction is being
        // prepared. Its durable presentation facts must remain strictly after the frozen source
        // boundary so the compaction worker excludes them and cold rebind replays them as tail.
        let mut projection = fixture
            .store
            .load_thread(&fixture.thread_id)
            .await
            .expect("load projection for post-admission turn")
            .expect("runtime thread");
        let expected_projection_revision = projection.revision;
        let presentation_thread_id = format!("presentation-{}", fixture.suffix);
        let tail_turn_id = format!("post-admission-turn-{}", fixture.suffix);
        let tail_events = [
            BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                presentation_thread_id.clone(),
                tail_turn_id.clone(),
                "post-admission-user",
                UserInputSubmissionKind::Prompt,
                UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "post-admission prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            )),
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                codex::ThreadItem::DynamicToolCall {
                    id: "turn_009:tool_014".to_string(),
                    tool: "fs_glob".to_string(),
                    arguments: serde_json::json!({"pattern":"**/*.rs"}),
                    status: codex::DynamicToolCallStatus::Completed,
                    content_items: Some(Some(vec![
                        codex::DynamicToolCallOutputContentItem::InputText {
                            text: "src/lib.rs".to_string(),
                        },
                    ])),
                    duration_ms: None,
                    namespace: None,
                    success: Some(Some(true)),
                },
                presentation_thread_id.clone(),
                tail_turn_id.clone(),
            )),
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                codex::ThreadItem::AgentMessage {
                    id: "assistant-after-compaction-admission".to_string(),
                    text: "post-admission answer".to_string(),
                    phase: None,
                    memory_citation: None,
                },
                presentation_thread_id.clone(),
                tail_turn_id.clone(),
            )),
        ];
        let tail_records = tail_events
            .into_iter()
            .enumerate()
            .map(|(entry_index, event)| {
                projection
                    .append_durable_fact(
                        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                            PresentationDurability::Durable,
                            event,
                        )),
                        1_710_000_000_200 + u64::try_from(entry_index).unwrap_or_default(),
                        Some(id(&format!("binding-{}", fixture.suffix))),
                        None,
                        RuntimePresentationCoordinate {
                            runtime_turn_id: None,
                            presentation_turn_id: Some(id(&tail_turn_id)),
                            runtime_item_id: None,
                            interaction_id: None,
                            source_thread_id: Some(format!("source-{}", fixture.suffix)),
                            source_turn_id: Some(tail_turn_id.clone()),
                            source_item_id: None,
                            source_request_id: None,
                            source_entry_index: Some(
                                u32::try_from(entry_index).unwrap_or(u32::MAX),
                            ),
                        },
                    )
                    .expect("append post-admission turn presentation")
            })
            .collect::<Vec<_>>();
        assert!(tail_records.iter().all(|record| {
            record
                .carrier()
                .sequence
                .is_some_and(|sequence| sequence.0 > source_end_event_sequence.0)
        }));
        fixture
            .store
            .commit(agentdash_agent_runtime::RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection,
                operation: None,
                operation_terminals: Vec::new(),
                records: tail_records,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
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
            .await
            .expect("persist post-admission turn");

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
                source_end_event_sequence,
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
                presentation: Some(agentdash_agent_runtime::CompactionPresentationFacts {
                    summary: "durable summary".to_string(),
                    tokens_before: 100,
                    messages_compacted: 1,
                    compaction_id: Some(compaction_id.to_string()),
                    projection_version: Some(1),
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("manual".to_string()),
                    phase: Some("pre_provider".to_string()),
                    source_start_event_seq: Some(1),
                    source_end_event_seq: Some(2),
                    first_kept_event_seq: None,
                    compacted_until_ref: None,
                    timestamp_ms: Some(1_710_000_000_000),
                }),
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
        let started = start(&fixture);
        let RuntimeCommand::ThreadStart {
            presentation_thread_id,
            binding_id,
            driver_generation,
            source_thread_id,
            profile_digest,
            bound_profile,
            surface,
            settings_revision,
            ..
        } = started.command
        else {
            unreachable!("fixture starts a thread")
        };
        let composition = Arc::new(PostgresAgentRuntimeCompositionRepository::new(
            fixture.store.pool().clone(),
        ));
        seed_runtime_host_binding(&fixture).await;
        let target = seed_agent_run_target(fixture.store.pool()).await;
        composition
            .insert(AgentRunRuntimeBinding {
                target,
                presentation_thread_id,
                thread_id: fixture.thread_id.clone(),
                binding_id: binding_id.clone(),
                binding_epoch: BindingEpoch(1),
                driver_generation,
                source_thread_id,
                profile_digest,
                profile_provenance: ProfileProvenance {
                    service_digest: id("profile-service"),
                    transport_digest: id("profile-transport"),
                    host_policy_digest: id("profile-host"),
                },
                bound_profile: *bound_profile,
                surface: *surface,
                settings_revision,
                context_delivery_target: AgentRunContextDeliveryTarget {
                    connector_id: "native".to_string(),
                    executor: "NATIVE".to_string(),
                },
            })
            .await
            .expect("persist compacted application binding lineage");
        let transcript = PostgresAgentRuntimeContextBroker::new(fixture.store.clone(), composition)
            .load_transcript(DriverTranscriptRequest {
                binding_id,
                generation: driver_generation,
                runtime_thread_id: fixture.thread_id.clone(),
            })
            .await
            .expect("load compacted transcript through production broker");
        assert_eq!(
            transcript.active_compaction_source_end,
            Some(source_end_event_sequence)
        );
        assert!(
            transcript
                .completed_presentation_item_ids
                .iter()
                .any(|item_id| item_id == "turn_009:tool_014")
        );
        assert!(transcript.latest_available.0 > source_end_event_sequence.0);
        assert_eq!(
            transcript
                .records
                .iter()
                .filter(|record| {
                    record
                        .carrier()
                        .sequence
                        .is_some_and(|sequence| sequence.0 > source_end_event_sequence.0)
                        && record.as_presentation().is_some()
                        && record
                            .carrier()
                            .coordinate
                            .presentation_turn_id
                            .as_ref()
                            .is_some_and(|turn_id| turn_id.as_str() == tail_turn_id)
                })
                .count(),
            3
        );
        assert!(transcript.records.iter().any(|record| {
            matches!(
                record.as_presentation().map(|presentation| &presentation.event),
                Some(BackboneEvent::UserInputSubmitted(input))
                    if input.content.iter().any(|block| matches!(
                        block,
                        codex::UserInput::Text { text, .. } if text == "post-admission prompt"
                    ))
            )
        }));
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
        let RuntimeCommand::ThreadStart { surface, .. } = &mut start_command.command else {
            unreachable!("start fixture always emits ThreadStart")
        };
        surface.hook_plan = plan.plan.clone();
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
                    presentation: None,
                }],
            )
            .await
            .expect("persist terminal and effect");

        let presentation_run_id: HookRunId =
            id(&format!("hook-run-presentation-{}", fixture.suffix));
        let HookAdmission::Durable(presentation_run) = fixture
            .runtime
            .accept_hook(RuntimeHookInvocation {
                hook_run_id: presentation_run_id.clone(),
                thread_id: fixture.thread_id.clone(),
                definition_id: next_plan.plan.entries[0].definition_id.clone(),
                point: HookPoint::BeforeTurn,
                correlation: HookCorrelation {
                    operation_id: None,
                    turn_id: None,
                    item_id: None,
                    interaction_id: None,
                },
                input: serde_json::json!({"turn": "presentation"}),
            })
            .await
            .expect("accept presentation hook")
        else {
            panic!("actionful hook is durable")
        };
        let presentation_run = fixture
            .runtime
            .start_hook(&presentation_run.hook_run_id)
            .await
            .expect("start presentation hook");
        let mut presentation_effect = HookEffect {
            effect_id: id(&format!("hook-presentation-effect-{}", fixture.suffix)),
            hook_run_id: presentation_run.hook_run_id.clone(),
            thread_id: fixture.thread_id.clone(),
            idempotency_key: format!("presentation:{}", fixture.suffix),
            descriptor: HookEffectDescriptor {
                effect_type: agentdash_agent_runtime::RUNTIME_CONTEXT_PRESENTATION_EFFECT_TYPE
                    .to_string(),
                schema_version: 1,
                target_authority: "agent_runtime_context_projection".to_string(),
                retry_limit: 0,
                payload_digest: String::new(),
            },
            payload: serde_json::Value::Null,
            presentation: Some(agentdash_agent_runtime::ContextFrameFacts {
                kind: agentdash_agent_protocol::ContextFrameKind::SystemNotice,
                source: agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
                phase_node: None,
                apply_mode: None,
                delivery_status:
                    agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
                message_role: agentdash_agent_protocol::ContextMessageRole::User,
                rendered_text: "presentation fact".to_string(),
                sections: vec![
                    agentdash_agent_protocol::ContextFrameSection::SystemNotice {
                        title: "Runtime Notice".to_string(),
                        summary: "presentation fact".to_string(),
                        body: Some("presentation fact".to_string()),
                    },
                ],
            }),
        };
        presentation_effect.descriptor.payload_digest =
            agentdash_agent_runtime::hook_effect_content_digest(&presentation_effect);
        fixture
            .runtime
            .complete_hook(
                &presentation_run_id,
                HookCompletion {
                    status: HookRunStatus::Blocked,
                    decision: HookGateDecision::Block,
                    message: Some("presentation recorded".to_string()),
                },
                vec![presentation_effect.clone()],
            )
            .await
            .expect("persist presentation effect and terminal");
        assert_eq!(
            fixture
                .store
                .hook_effects(&presentation_run_id)
                .await
                .expect("load durable presentation effect"),
            vec![presentation_effect]
        );
        let presentation_dispatched_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT dispatched_at FROM agent_runtime_hook_effect WHERE id=$1")
                .bind(format!("hook-presentation-effect-{}", fixture.suffix))
                .fetch_one(&fixture._database.pool)
                .await
                .expect("load presentation terminal dispatch state");
        assert!(
            presentation_dispatched_at.is_some(),
            "runtime-owned presentation effect must not remain pending"
        );
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
                    presentation: None,
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
