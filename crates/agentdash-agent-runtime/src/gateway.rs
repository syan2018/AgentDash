use std::{collections::VecDeque, sync::Arc};

use agentdash_agent_runtime_contract::{
    AgentRuntimeGateway, ContextRevision, DriverEventEnvelope, EventSequence,
    OperationConflictKind, OperationSequence, ProfileDigest, RuntimeBindingId, RuntimeCommand,
    RuntimeCommandEnvelope, RuntimeDriverGeneration, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventStream, RuntimeExecuteError, RuntimeInteractionTerminal, RuntimeProfile,
    RuntimeProtocolViolationCode, RuntimeRevision, RuntimeSnapshotError, RuntimeSnapshotQuery,
    RuntimeSnapshotResult, RuntimeSubscribeError, RuntimeThreadId, RuntimeThreadStatus,
};
use async_trait::async_trait;

use crate::{
    DriverEventQuarantineReason, QuarantinedDriverEvent, RuntimeCommit, RuntimeOperationRecord,
    RuntimeOutboxEntry, RuntimeRepository, RuntimeStoreError, RuntimeThreadState,
    RuntimeTransientEvents, RuntimeUnitOfWork, TransitionError,
};

#[derive(Debug, Clone)]
pub struct RuntimeKernelDefaults {
    pub binding_id: RuntimeBindingId,
    pub driver_generation: RuntimeDriverGeneration,
    pub source_thread_id: agentdash_agent_runtime_contract::DriverThreadId,
    pub profile_digest: ProfileDigest,
    pub bound_profile: RuntimeProfile,
}

pub struct ManagedAgentRuntime<S> {
    store: Arc<S>,
    defaults: RuntimeKernelDefaults,
}

impl<S> ManagedAgentRuntime<S> {
    pub fn new(store: Arc<S>, defaults: RuntimeKernelDefaults) -> Self {
        Self { store, defaults }
    }

    pub(crate) fn store(&self) -> &S {
        &self.store
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverEventAdmission {
    Durable { sequence: EventSequence },
    Transient,
    Quarantined,
}

impl<S> ManagedAgentRuntime<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + RuntimeTransientEvents,
{
    pub async fn ingest_driver_event(
        &self,
        source: DriverEventEnvelope,
    ) -> Result<DriverEventAdmission, RuntimeExecuteError> {
        let Some(mut state) = self
            .store
            .find_thread_by_source(&source.binding_id, &source.source_thread_id)
            .await
            .map_err(store_execute_error)?
        else {
            self.quarantine(source, DriverEventQuarantineReason::CanonicalThreadNotFound)
                .await?;
            return Ok(DriverEventAdmission::Quarantined);
        };

        if source.binding_id != state.binding_id
            || source.generation != state.driver_generation
            || source.source_thread_id != state.source_thread_id
        {
            let reason = DriverEventQuarantineReason::StaleBinding {
                expected_binding_id: state.binding_id.clone(),
                expected_generation: state.driver_generation,
            };
            self.quarantine(source, reason).await?;
            return Ok(DriverEventAdmission::Quarantined);
        }
        if matches!(source.event, RuntimeEvent::OperationAccepted { .. }) {
            return self
                .persist_protocol_violation(
                    state,
                    source,
                    DriverEventQuarantineReason::DriverOperationAcceptance,
                    RuntimeProtocolViolationCode::DriverOperationAcceptance,
                    "driver attempted to accept a runtime-owned operation".to_string(),
                )
                .await;
        }
        if matches!(
            source.event,
            RuntimeEvent::ContextCheckpointPrepared { .. }
                | RuntimeEvent::ContextActivationApplied { .. }
                | RuntimeEvent::ContextCompactionTerminal { .. }
                | RuntimeEvent::ContextCheckpointActivated { .. }
        ) {
            return self
                .persist_protocol_violation(
                    state,
                    source,
                    DriverEventQuarantineReason::DriverRuntimeOwnedContextEvent,
                    RuntimeProtocolViolationCode::DriverRuntimeOwnedContextEvent,
                    "driver attempted to emit a runtime-owned context transition".to_string(),
                )
                .await;
        }

        if source.event.is_transient() {
            if let Err(error) = state.apply_authoritative(&source.event) {
                return self
                    .persist_transition_violation(state, source, error)
                    .await;
            }
            self.store
                .publish(RuntimeEventEnvelope {
                    thread_id: state.thread_id,
                    sequence: None,
                    revision: state.revision,
                    event: source.event,
                })
                .await;
            return Ok(DriverEventAdmission::Transient);
        }

        let expected = state.revision;
        let mut canonical_events = vec![source.event.clone()];
        if let Some(message) = loss_reason(&source.event) {
            canonical_events.extend(state.lost_terminal_events(Some(message)));
        }
        let events = match state.append_events(canonical_events) {
            Ok(events) => events,
            Err(error) => {
                return self
                    .persist_transition_violation(state, source, error)
                    .await;
            }
        };
        let sequence = events[0].sequence.expect("authoritative event has cursor");
        let operation_terminals = operation_terminals(&events);
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state,
                operation: None,
                operation_terminals,
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_execute_error)?;
        Ok(DriverEventAdmission::Durable { sequence })
    }

    async fn quarantine(
        &self,
        event: DriverEventEnvelope,
        reason: DriverEventQuarantineReason,
    ) -> Result<(), RuntimeExecuteError> {
        self.store
            .quarantine(QuarantinedDriverEvent { event, reason })
            .await
            .map_err(store_execute_error)
    }

    async fn persist_transition_violation(
        &self,
        state: RuntimeThreadState,
        source: DriverEventEnvelope,
        error: TransitionError,
    ) -> Result<DriverEventAdmission, RuntimeExecuteError> {
        let code = if error.is_duplicate_terminal() {
            RuntimeProtocolViolationCode::DuplicateTerminal
        } else {
            RuntimeProtocolViolationCode::InvalidLifecycleTransition
        };
        let message = error.to_string();
        self.persist_protocol_violation(
            state,
            source,
            DriverEventQuarantineReason::InvalidTransition { error },
            code,
            message,
        )
        .await
    }

    async fn persist_protocol_violation(
        &self,
        mut state: RuntimeThreadState,
        source: DriverEventEnvelope,
        quarantine_reason: DriverEventQuarantineReason,
        code: RuntimeProtocolViolationCode,
        message: String,
    ) -> Result<DriverEventAdmission, RuntimeExecuteError> {
        let expected = state.revision;
        let mut canonical_events = vec![RuntimeEvent::ProtocolViolation {
            code,
            message: message.clone(),
            critical: true,
        }];
        canonical_events.extend(state.lost_terminal_events(Some(message)));
        let events = state
            .append_events(canonical_events)
            .map_err(transition_execute_error)?;
        let sequence = events[0].sequence.expect("authoritative event has cursor");
        let operation_terminals = operation_terminals(&events);
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state,
                operation: None,
                operation_terminals,
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                quarantine: vec![QuarantinedDriverEvent {
                    event: source,
                    reason: quarantine_reason,
                }],
            })
            .await
            .map_err(store_execute_error)?;
        Ok(DriverEventAdmission::Durable { sequence })
    }
}

#[async_trait]
impl<S> AgentRuntimeGateway for ManagedAgentRuntime<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + RuntimeTransientEvents + 'static,
{
    async fn execute(
        &self,
        envelope: RuntimeCommandEnvelope,
    ) -> Result<agentdash_agent_runtime_contract::OperationReceipt, RuntimeExecuteError> {
        if let Some(existing) = self
            .store
            .find_operation(&envelope.meta.operation_id)
            .await
            .map_err(store_execute_error)?
        {
            return duplicate_receipt(
                existing,
                &envelope,
                OperationConflictKind::OperationIdReused,
            );
        }

        let target = command_thread_id(&envelope.command);
        let (mut state, expected_projection_revision) = match target {
            Some(thread_id) => {
                let state = self
                    .store
                    .load_thread(&thread_id)
                    .await
                    .map_err(store_execute_error)?
                    .ok_or_else(|| RuntimeExecuteError::InvalidCommand {
                        reason: format!("thread {thread_id} was not found"),
                    })?;
                (state.clone(), Some(state.revision))
            }
            None if matches!(envelope.command, RuntimeCommand::ThreadStart { .. }) => {
                (self.new_thread(), None)
            }
            None => {
                return Err(RuntimeExecuteError::InvalidCommand {
                    reason: "command does not identify a thread".to_string(),
                });
            }
        };

        if let Some(existing) = self
            .store
            .find_idempotency(&state.thread_id, &envelope.meta.idempotency_key)
            .await
            .map_err(store_execute_error)?
        {
            return duplicate_receipt(
                existing,
                &envelope,
                OperationConflictKind::IdempotencyKeyReused,
            );
        }

        if let Some(expected) = envelope.meta.expected_thread_revision
            && expected != state.revision
        {
            return Err(RuntimeExecuteError::RevisionConflict {
                expected,
                actual: state.revision,
            });
        }
        let availability_state = agentdash_agent_runtime_contract::AvailabilityState {
            thread_status: state.status,
            has_active_turn: state.active_turn_id.is_some(),
            has_pending_interaction: state
                .interactions
                .values()
                .any(|item| matches!(item.phase, crate::EntityPhase::Active)),
        };
        if let agentdash_agent_runtime_contract::CommandAvailability::Unavailable {
            reason, ..
        } = agentdash_agent_runtime_contract::command_availability(
            envelope.command.kind(),
            &state.bound_profile,
            &availability_state,
        ) {
            return Err(RuntimeExecuteError::Unsupported {
                command: envelope.command.kind(),
                reason,
            });
        }
        validate_command(&state, &envelope.command)?;
        if let RuntimeCommand::ContextCompact {
            base_checkpoint_id,
            expected_context_revision,
            ..
        } = &envelope.command
            && (state.active_checkpoint_id.as_ref() != base_checkpoint_id.as_ref()
                || state.context_revision != *expected_context_revision)
        {
            return invalid("active context head changed before compaction admission");
        }

        state.next_operation_sequence.0 += 1;
        let operation_sequence = state.next_operation_sequence;
        let accepted = RuntimeEvent::OperationAccepted {
            operation_id: envelope.meta.operation_id.clone(),
        };
        let mut transition_events = vec![accepted];
        apply_command_projection(
            &mut state,
            &envelope.command,
            &envelope.meta.operation_id,
            &mut transition_events,
        )?;
        let events = state
            .append_events(transition_events)
            .map_err(transition_execute_error)?;
        let accepted_revision = events
            .first()
            .expect("every mutation starts with operation acceptance")
            .revision;
        let record = RuntimeOperationRecord {
            operation_id: envelope.meta.operation_id.clone(),
            idempotency_key: envelope.meta.idempotency_key.clone(),
            actor: envelope.meta.actor.clone(),
            thread_id: state.thread_id.clone(),
            operation_sequence,
            accepted_revision,
            command: envelope.command.clone(),
            terminal: None,
        };
        let receipt = record.receipt(false);
        let context_preparation_work_items = match &record.command {
            RuntimeCommand::ContextCompact {
                thread_id,
                compaction_id,
                trigger,
                base_checkpoint_id,
                expected_context_revision,
            } => vec![crate::ContextPreparationWorkItem {
                compaction_id: compaction_id.clone(),
                operation_id: record.operation_id.clone(),
                thread_id: thread_id.clone(),
                trigger: *trigger,
                expected_base_checkpoint_id: base_checkpoint_id.clone(),
                expected_base_revision: *expected_context_revision,
                status: crate::ContextPreparationStatus::Pending,
            }],
            _ => Vec::new(),
        };
        let outbox = if matches!(envelope.command, RuntimeCommand::ContextCompact { .. }) {
            Vec::new()
        } else {
            vec![RuntimeOutboxEntry {
                operation_id: record.operation_id.clone(),
                thread_id: state.thread_id.clone(),
                generation: state.driver_generation,
                command: envelope.command,
            }]
        };
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision,
                projection: state,
                operation: Some(record),
                operation_terminals: Vec::new(),
                events,
                outbox,
                context_activation_outbox: Vec::new(),
                context_preparation_work_items,
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_execute_error)?;
        Ok(receipt)
    }

    async fn snapshot(
        &self,
        query: RuntimeSnapshotQuery,
    ) -> Result<RuntimeSnapshotResult, RuntimeSnapshotError> {
        let (thread_id, at_revision, at_context_revision, context_query) = match query {
            RuntimeSnapshotQuery::Thread {
                thread_id,
                at_revision,
            } => (thread_id, at_revision, None, false),
            RuntimeSnapshotQuery::Context {
                thread_id,
                at_context_revision,
            } => (thread_id, None, at_context_revision, true),
        };
        let state = self
            .store
            .load_thread(&thread_id)
            .await
            .map_err(|error| RuntimeSnapshotError::Unavailable {
                reason: error.to_string(),
            })?
            .ok_or(RuntimeSnapshotError::NotFound)?;
        if let Some(requested) = at_revision
            && requested != state.revision
        {
            return Err(RuntimeSnapshotError::RevisionUnavailable {
                requested,
                current: state.revision,
            });
        }
        if context_query {
            if let Some(requested) = at_context_revision
                && requested != state.context_revision
            {
                return Err(RuntimeSnapshotError::ContextRevisionUnavailable {
                    requested,
                    current: state.context_revision,
                });
            }
            let context = self
                .context_view(&thread_id)
                .await
                .map_err(|error| match error {
                    crate::ContextRuntimeError::InconsistentStore(code) => {
                        RuntimeSnapshotError::InconsistentContext { code }
                    }
                    other => RuntimeSnapshotError::Unavailable {
                        reason: other.to_string(),
                    },
                })?;
            if context
                .head
                .as_ref()
                .map_or(ContextRevision(0), |head| head.revision)
                != state.context_revision
            {
                return Err(RuntimeSnapshotError::InconsistentContext {
                    code: agentdash_agent_runtime_contract::ContextSnapshotConsistencyCode::ProjectionHeadRevisionMismatch,
                });
            }
            Ok(RuntimeSnapshotResult::Context {
                context: Box::new(context),
            })
        } else {
            Ok(RuntimeSnapshotResult::Thread {
                snapshot: Box::new(state.snapshot()),
            })
        }
    }

    async fn events(
        &self,
        subscription: agentdash_agent_runtime_contract::RuntimeEventSubscription,
    ) -> Result<Box<dyn RuntimeEventStream>, RuntimeSubscribeError> {
        if self
            .store
            .load_thread(&subscription.thread_id)
            .await
            .map_err(subscribe_store_error)?
            .is_none()
        {
            return Err(RuntimeSubscribeError::NotFound);
        }
        let current = self
            .store
            .load_thread(&subscription.thread_id)
            .await
            .map_err(subscribe_store_error)?
            .expect("thread existence was checked")
            .next_event_sequence;
        if subscription.after.is_some_and(|after| after > current) {
            return Err(RuntimeSubscribeError::InvalidCursor);
        }
        let batch = self
            .store
            .events_after(&subscription.thread_id, subscription.after)
            .await
            .map_err(subscribe_store_error)?;
        if let Some(requested) = subscription.after
            && requested.0.saturating_add(1) < batch.earliest_available.0
        {
            return Err(RuntimeSubscribeError::CursorGap {
                requested,
                earliest_available: batch.earliest_available,
                latest_available: batch.latest_available,
            });
        }
        let mut events = batch.events;
        if subscription.include_transient {
            events.extend(self.store.read(&subscription.thread_id).await);
        }
        Ok(Box::new(VecEventStream {
            events: events.into(),
        }))
    }
}

impl<S> ManagedAgentRuntime<S> {
    fn new_thread(&self) -> RuntimeThreadState {
        let thread_id = RuntimeThreadId::new(format!("thread-{}", self.defaults.source_thread_id))
            .expect("configured source thread id is valid");
        RuntimeThreadState {
            thread_id,
            revision: RuntimeRevision(0),
            next_event_sequence: EventSequence(0),
            next_operation_sequence: OperationSequence(0),
            status: RuntimeThreadStatus::Active,
            active_turn_id: None,
            binding_id: self.defaults.binding_id.clone(),
            driver_generation: self.defaults.driver_generation,
            source_thread_id: self.defaults.source_thread_id.clone(),
            profile_digest: self.defaults.profile_digest.clone(),
            bound_profile: self.defaults.bound_profile.clone(),
            active_checkpoint_id: None,
            context_revision: agentdash_agent_runtime_contract::ContextRevision(0),
            settings_revision: agentdash_agent_runtime_contract::ThreadSettingsRevision(0),
            tool_set_revision: agentdash_agent_runtime_contract::ToolSetRevision(0),
            operations: Default::default(),
            turns: Default::default(),
            items: Default::default(),
            item_order: Vec::new(),
            interactions: Default::default(),
        }
    }
}

fn command_thread_id(command: &RuntimeCommand) -> Option<RuntimeThreadId> {
    match command {
        RuntimeCommand::ThreadStart { .. } => None,
        RuntimeCommand::ThreadResume { thread_id }
        | RuntimeCommand::ThreadFork { thread_id, .. }
        | RuntimeCommand::ThreadSettingsUpdate { thread_id, .. }
        | RuntimeCommand::TurnStart { thread_id, .. }
        | RuntimeCommand::TurnSteer { thread_id, .. }
        | RuntimeCommand::TurnInterrupt { thread_id, .. }
        | RuntimeCommand::InteractionRespond { thread_id, .. }
        | RuntimeCommand::ContextCompact { thread_id, .. }
        | RuntimeCommand::ToolSetReplace { thread_id, .. } => Some(thread_id.clone()),
    }
}

fn validate_command(
    state: &RuntimeThreadState,
    command: &RuntimeCommand,
) -> Result<(), RuntimeExecuteError> {
    match command {
        RuntimeCommand::ThreadStart { .. } => {}
        RuntimeCommand::TurnStart { .. } if state.active_turn_id.is_some() => {
            return invalid("a turn is already active");
        }
        RuntimeCommand::TurnSteer {
            expected_turn_id, ..
        }
        | RuntimeCommand::TurnInterrupt {
            expected_turn_id, ..
        } if state.active_turn_id.as_ref() != Some(expected_turn_id) => {
            return invalid("expected turn is not active");
        }
        RuntimeCommand::InteractionRespond { interaction_id, .. }
            if !state
                .interactions
                .get(interaction_id)
                .is_some_and(|interaction| {
                    matches!(interaction.phase, crate::EntityPhase::Active)
                }) =>
        {
            return invalid("interaction is not pending");
        }
        RuntimeCommand::ToolSetReplace {
            expected_tool_set_revision,
            ..
        } if *expected_tool_set_revision != state.tool_set_revision => {
            return invalid("tool set revision is stale");
        }
        _ => {}
    }
    Ok(())
}

fn apply_command_projection(
    state: &mut RuntimeThreadState,
    command: &RuntimeCommand,
    operation_id: &agentdash_agent_runtime_contract::RuntimeOperationId,
    events: &mut Vec<RuntimeEvent>,
) -> Result<(), RuntimeExecuteError> {
    match command {
        RuntimeCommand::ThreadStart { .. } => events.push(RuntimeEvent::ThreadStatusChanged {
            status: RuntimeThreadStatus::Active,
        }),
        RuntimeCommand::ThreadResume { .. } => events.push(RuntimeEvent::ThreadStatusChanged {
            status: RuntimeThreadStatus::Active,
        }),
        RuntimeCommand::ThreadSettingsUpdate { .. } => state.settings_revision.0 += 1,
        RuntimeCommand::TurnStart { .. } => {
            let turn_id = agentdash_agent_runtime_contract::RuntimeTurnId::new(format!(
                "turn-{operation_id}"
            ))
            .expect("derived turn id is valid");
            events.push(RuntimeEvent::TurnStarted { turn_id });
        }
        RuntimeCommand::InteractionRespond { interaction_id, .. } => {
            let turn_id = state
                .interactions
                .get(interaction_id)
                .expect("validated interaction")
                .turn_id
                .clone();
            events.push(RuntimeEvent::InteractionTerminal {
                turn_id,
                interaction_id: interaction_id.clone(),
                terminal: RuntimeInteractionTerminal::Resolved,
            });
        }
        RuntimeCommand::ToolSetReplace { .. } => state.tool_set_revision.0 += 1,
        RuntimeCommand::ThreadFork { .. }
        | RuntimeCommand::TurnSteer { .. }
        | RuntimeCommand::TurnInterrupt { .. }
        | RuntimeCommand::ContextCompact { .. } => {}
    }
    Ok(())
}

fn duplicate_receipt(
    existing: RuntimeOperationRecord,
    envelope: &RuntimeCommandEnvelope,
    conflict: OperationConflictKind,
) -> Result<agentdash_agent_runtime_contract::OperationReceipt, RuntimeExecuteError> {
    if existing.operation_id != envelope.meta.operation_id
        || existing.idempotency_key != envelope.meta.idempotency_key
        || existing.actor != envelope.meta.actor
        || existing.command != envelope.command
    {
        return Err(RuntimeExecuteError::OperationConflict {
            existing_operation_id: existing.operation_id,
            conflict,
        });
    }
    Ok(existing.receipt(true))
}

fn invalid<T>(reason: &str) -> Result<T, RuntimeExecuteError> {
    Err(RuntimeExecuteError::InvalidCommand {
        reason: reason.to_string(),
    })
}

fn transition_execute_error(error: crate::TransitionError) -> RuntimeExecuteError {
    RuntimeExecuteError::InvalidCommand {
        reason: error.to_string(),
    }
}

fn store_execute_error(error: RuntimeStoreError) -> RuntimeExecuteError {
    match error {
        RuntimeStoreError::ProjectionConflict {
            expected: Some(expected),
            actual: Some(actual),
        } => RuntimeExecuteError::RevisionConflict { expected, actual },
        RuntimeStoreError::OperationConflict { operation_id } => {
            RuntimeExecuteError::OperationConflict {
                existing_operation_id: operation_id,
                conflict: OperationConflictKind::OperationIdReused,
            }
        }
        RuntimeStoreError::IdempotencyConflict { operation_id } => {
            RuntimeExecuteError::OperationConflict {
                existing_operation_id: operation_id,
                conflict: OperationConflictKind::IdempotencyKeyReused,
            }
        }
        RuntimeStoreError::ContextCompactionConflict { operation_id } => {
            RuntimeExecuteError::ContextCompactionInProgress { operation_id }
        }
        other => RuntimeExecuteError::Persistence {
            reason: other.to_string(),
            retryable: true,
        },
    }
}

fn subscribe_store_error(error: RuntimeStoreError) -> RuntimeSubscribeError {
    RuntimeSubscribeError::Unavailable {
        reason: error.to_string(),
        retryable: true,
    }
}

fn loss_reason(event: &RuntimeEvent) -> Option<String> {
    match event {
        RuntimeEvent::BindingLost { reason, .. } => Some(reason.clone()),
        RuntimeEvent::ProtocolViolation {
            message,
            critical: true,
            ..
        } => Some(message.clone()),
        _ => None,
    }
}

fn operation_terminals(
    events: &[RuntimeEventEnvelope],
) -> Vec<(
    agentdash_agent_runtime_contract::RuntimeOperationId,
    agentdash_agent_runtime_contract::RuntimeOperationTerminal,
)> {
    events
        .iter()
        .filter_map(|event| match &event.event {
            RuntimeEvent::OperationTerminal {
                operation_id,
                terminal,
            } => Some((operation_id.clone(), terminal.clone())),
            _ => None,
        })
        .collect()
}

struct VecEventStream {
    events: VecDeque<RuntimeEventEnvelope>,
}

#[async_trait]
impl RuntimeEventStream for VecEventStream {
    async fn next(&mut self) -> Option<Result<RuntimeEventEnvelope, RuntimeSubscribeError>> {
        self.events.pop_front().map(Ok)
    }
}
