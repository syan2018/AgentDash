use std::{collections::VecDeque, sync::Arc};

use agentdash_agent_runtime_contract::{
    AgentRuntimeGateway, ContextRevision, DriverEventEnvelope, EventSequence,
    OperationConflictKind, OperationSequence, RuntimeCommand, RuntimeCommandEnvelope, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventStream, RuntimeExecuteError, RuntimeInteractionTerminal,
    RuntimeJournalFact, RuntimeJournalRecord, RuntimePresentationCoordinate,
    RuntimeProtocolViolationCode, RuntimeRevision, RuntimeSnapshotError, RuntimeSnapshotQuery,
    RuntimeSnapshotResult, RuntimeSubscribeError, RuntimeTerminalPresentationContext,
    RuntimeThreadId, RuntimeThreadStatus,
};
use async_trait::async_trait;

use crate::{
    DriverEventQuarantineReason, QuarantinedDriverEvent, RuntimeCommit, RuntimeHookPlanBinding,
    RuntimeOperationRecord, RuntimeOutboxEntry, RuntimeRepository, RuntimeStoreError,
    RuntimeThreadState, RuntimeTransientEvents, RuntimeUnitOfWork, TransitionError,
};

pub struct ManagedAgentRuntime<S> {
    store: Arc<S>,
    application_presentation_projector:
        Arc<dyn agentdash_agent_runtime_contract::RuntimeApplicationPresentationProjector>,
    surface_validator: Option<Arc<dyn crate::RuntimeSurfaceReferenceValidator>>,
    driver_event_ingest: tokio::sync::Mutex<()>,
}

impl<S> ManagedAgentRuntime<S> {
    pub fn new(
        store: Arc<S>,
        application_presentation_projector: Arc<
            dyn agentdash_agent_runtime_contract::RuntimeApplicationPresentationProjector,
        >,
    ) -> Self {
        Self {
            store,
            application_presentation_projector,
            surface_validator: None,
            driver_event_ingest: tokio::sync::Mutex::new(()),
        }
    }

    pub fn with_surface_validator(
        mut self,
        validator: Arc<dyn crate::RuntimeSurfaceReferenceValidator>,
    ) -> Self {
        self.surface_validator = Some(validator);
        self
    }

    pub(crate) fn store(&self) -> &S {
        &self.store
    }

    pub(crate) async fn lock_mutation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.driver_event_ingest.lock().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverEventAdmission {
    Durable {
        sequence: EventSequence,
    },
    /// Managed Runtime atomically persisted a critical violation and terminalized the binding.
    /// The driver event pump must stop without emitting a second fallback terminal.
    Terminalized {
        sequence: EventSequence,
    },
    Transient,
    /// Driver 对 Managed Runtime 已建立的 canonical lifecycle transition 的同身份确认。
    Observed,
    Quarantined,
}

enum PresentationPublication {
    Durable(RuntimeJournalRecord),
    Transient {
        coordinate: RuntimePresentationCoordinate,
        event: agentdash_agent_runtime_contract::ImmutablePresentationEvent,
    },
}

impl<S> ManagedAgentRuntime<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + RuntimeTransientEvents,
{
    /// Closes a command whose business effect is complete at the driver acceptance boundary.
    ///
    /// Turn-start and managed-compaction commands have later business terminals and do not call
    /// this method. Delivery-only commands use it after the driver receipt so outbox ack cannot
    /// leave an accepted operation permanently active.
    pub async fn complete_driver_dispatch_operation(
        &self,
        thread_id: &RuntimeThreadId,
        operation_id: &agentdash_agent_runtime_contract::RuntimeOperationId,
    ) -> Result<bool, RuntimeExecuteError> {
        let _ingest = self.driver_event_ingest.lock().await;
        let mut state = self
            .store
            .load_thread(thread_id)
            .await
            .map_err(store_execute_error)?
            .ok_or_else(|| RuntimeExecuteError::InvalidCommand {
                reason: format!("Runtime thread {thread_id} does not exist"),
            })?;
        match state.operations.get(operation_id) {
            Some(crate::EntityPhase::Terminal(
                agentdash_agent_runtime_contract::RuntimeOperationTerminal::Succeeded,
            )) => return Ok(true),
            Some(crate::EntityPhase::Terminal(terminal)) => {
                return Err(RuntimeExecuteError::InvalidCommand {
                    reason: format!(
                        "driver dispatch operation {operation_id} already reached {terminal:?}"
                    ),
                });
            }
            Some(crate::EntityPhase::Active) => {}
            None => {
                return Err(RuntimeExecuteError::InvalidCommand {
                    reason: format!("driver dispatch operation {operation_id} was not accepted"),
                });
            }
        }

        let expected = state.revision;
        let record = state
            .append_durable_fact(
                RuntimeJournalFact::Internal(RuntimeEvent::OperationTerminal {
                    operation_id: operation_id.clone(),
                    terminal: agentdash_agent_runtime_contract::RuntimeOperationTerminal::Succeeded,
                }),
                crate::model::current_time_ms(),
                Some(state.binding_id.clone()),
                None,
                RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: None,
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            )
            .map_err(transition_execute_error)?;
        let operation_terminals = operation_terminals_from_records(std::slice::from_ref(&record));
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state,
                operation: None,
                operation_terminals,
                records: vec![record],
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
            .map_err(store_execute_error)?;
        Ok(false)
    }

    pub async fn append_transient_presentation(
        &self,
        request: crate::RuntimeTransientPresentationAppendRequest,
    ) -> Result<(), crate::RuntimePresentationAppendError> {
        if request.producer.trim().is_empty() {
            return Err(crate::RuntimePresentationAppendError::Invalid(
                "producer must not be empty".to_string(),
            ));
        }
        if request.events.is_empty()
            || request.events.iter().any(|input| {
                input.event.durability
                    != agentdash_agent_runtime_contract::PresentationDurability::Ephemeral
            })
        {
            return Err(crate::RuntimePresentationAppendError::Invalid(
                "transient append requires non-empty ephemeral presentation events".to_string(),
            ));
        }
        let _ingest = self.driver_event_ingest.lock().await;
        let state = self
            .store
            .load_thread(&request.runtime_thread_id)
            .await
            .map_err(|_| crate::RuntimePresentationAppendError::Unavailable)?
            .ok_or(crate::RuntimePresentationAppendError::ThreadNotFound)?;
        validate_presentation_thread_ids(&request.events, &state.presentation_thread_id)?;
        if state.status != RuntimeThreadStatus::Active {
            return Err(crate::RuntimePresentationAppendError::Invalid(format!(
                "transient presentation requires an active runtime thread, found {:?}",
                state.status
            )));
        }
        let active_turn_id = state.active_turn_id.as_ref().ok_or_else(|| {
            crate::RuntimePresentationAppendError::Invalid(
                "transient presentation requires an active runtime turn".to_string(),
            )
        })?;
        let mut events = request.events;
        if events
            .iter()
            .any(|input| input.coordinate.runtime_turn_id.as_ref() != Some(active_turn_id))
        {
            return Err(crate::RuntimePresentationAppendError::Invalid(format!(
                "transient presentation must target active runtime turn {active_turn_id}"
            )));
        }
        for input in &mut events {
            normalize_presentation_coordinate(&state, &mut input.coordinate)?;
        }
        for input in events {
            self.store
                .publish_transient_presentation(
                    request.runtime_thread_id.clone(),
                    state.binding_id.clone(),
                    state.driver_generation,
                    input.coordinate.runtime_turn_id.clone(),
                    state.revision,
                    input.coordinate,
                    input.event,
                )
                .await;
        }
        Ok(())
    }

    pub async fn append_presentation(
        &self,
        request: crate::RuntimePresentationAppendRequest,
    ) -> Result<crate::RuntimePresentationAppendReceipt, crate::RuntimePresentationAppendError>
    {
        let producer = request.producer.trim();
        if producer.is_empty() {
            return Err(crate::RuntimePresentationAppendError::Invalid(
                "producer must not be empty".to_string(),
            ));
        }
        if request.events.is_empty() {
            return Err(crate::RuntimePresentationAppendError::Invalid(
                "events must not be empty".to_string(),
            ));
        }
        if request.events.iter().any(|input| {
            input.event.durability
                != agentdash_agent_runtime_contract::PresentationDurability::Durable
        }) {
            return Err(crate::RuntimePresentationAppendError::Invalid(
                "canonical append accepts durable presentation events only".to_string(),
            ));
        }

        let _ingest = self.driver_event_ingest.lock().await;
        let Some(mut state) = self
            .store
            .load_thread(&request.runtime_thread_id)
            .await
            .map_err(|_| crate::RuntimePresentationAppendError::Unavailable)?
        else {
            return Err(crate::RuntimePresentationAppendError::ThreadNotFound);
        };
        validate_presentation_thread_ids(&request.events, &state.presentation_thread_id)?;
        let mut normalized_events = request.events;
        for input in &mut normalized_events {
            normalize_presentation_coordinate(&state, &mut input.coordinate)?;
        }

        let append_idempotency_key =
            agentdash_agent_runtime_contract::IdempotencyKey::new(format!(
                "presentation:{}:{producer}:{}",
                producer.len(),
                request.idempotency_key.as_str()
            ))
            .map_err(|error| crate::RuntimePresentationAppendError::Invalid(error.to_string()))?;
        let existing = self
            .store
            .journal_records_after(&request.runtime_thread_id, None)
            .await
            .map_err(|_| crate::RuntimePresentationAppendError::Unavailable)?
            .records
            .into_iter()
            .filter(|record| {
                record.carrier().append_idempotency_key.as_ref() == Some(&append_idempotency_key)
            })
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            let existing_events = existing
                .iter()
                .filter_map(|record| {
                    record.as_presentation().cloned().map(|event| {
                        agentdash_agent_runtime_contract::RuntimePresentationInput {
                            coordinate: record.carrier().coordinate.clone(),
                            event,
                        }
                    })
                })
                .collect::<Vec<_>>();
            if existing_events != normalized_events {
                return Err(crate::RuntimePresentationAppendError::IdempotencyConflict);
            }
            let first_sequence = existing
                .first()
                .and_then(|record| record.carrier().sequence)
                .expect("canonical append records are durable");
            let last_sequence = existing
                .last()
                .and_then(|record| record.carrier().sequence)
                .expect("canonical append records are durable");
            return Ok(crate::RuntimePresentationAppendReceipt {
                first_sequence,
                last_sequence,
                duplicate: true,
            });
        }

        let expected = state.revision;
        let binding_id = state.binding_id.clone();
        let recorded_at_ms = crate::model::current_time_ms();
        let mut records = Vec::with_capacity(normalized_events.len());
        for input in normalized_events {
            records.push(
                state
                    .append_durable_fact(
                        RuntimeJournalFact::Presentation(input.event),
                        recorded_at_ms,
                        Some(binding_id.clone()),
                        Some(append_idempotency_key.clone()),
                        input.coordinate,
                    )
                    .map_err(|error| {
                        crate::RuntimePresentationAppendError::Invalid(error.to_string())
                    })?,
            );
        }
        let first_sequence = records
            .first()
            .and_then(|record| record.carrier().sequence)
            .expect("canonical append records are durable");
        let last_sequence = records
            .last()
            .and_then(|record| record.carrier().sequence)
            .expect("canonical append records are durable");
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state,
                operation: None,
                operation_terminals: Vec::new(),
                records,
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
            .map_err(|_| crate::RuntimePresentationAppendError::Unavailable)?;
        Ok(crate::RuntimePresentationAppendReceipt {
            first_sequence,
            last_sequence,
            duplicate: false,
        })
    }

    pub async fn ingest_driver_event(
        &self,
        source: DriverEventEnvelope,
    ) -> Result<DriverEventAdmission, RuntimeExecuteError> {
        // Driver pumps and command callbacks may deliver events concurrently for one binding.
        // Serialize the read-transition-CAS boundary so a valid event never fails merely because
        // another valid event advanced the projection between load and commit.
        let _ingest = self.driver_event_ingest.lock().await;
        let Some(committed_state) = self
            .store
            .find_thread_by_source(&source.binding_id, &source.source_thread_id)
            .await
            .map_err(store_execute_error)?
        else {
            self.quarantine(source, DriverEventQuarantineReason::CanonicalThreadNotFound)
                .await?;
            return Ok(DriverEventAdmission::Quarantined);
        };

        if source.binding_id != committed_state.binding_id
            || source.generation != committed_state.driver_generation
            || source.source_thread_id != committed_state.source_thread_id
        {
            let reason = DriverEventQuarantineReason::StaleBinding {
                expected_binding_id: committed_state.binding_id.clone(),
                expected_generation: committed_state.driver_generation,
            };
            self.quarantine(source, reason).await?;
            return Ok(DriverEventAdmission::Quarantined);
        }
        if source.facts.is_empty() {
            self.quarantine(source, DriverEventQuarantineReason::EmptyFactBatch)
                .await?;
            return Ok(DriverEventAdmission::Quarantined);
        }

        for fact in &source.facts {
            if let RuntimeJournalFact::Internal(event) = fact
                && let Some((reason, code, message)) = forbidden_driver_internal_fact(event)
            {
                return self
                    .persist_protocol_violation(committed_state, source, reason, code, message)
                    .await;
            }
        }

        let thread_id = committed_state.thread_id.clone();
        let prior_records = self
            .store
            .journal_records_after(&thread_id, None)
            .await
            .map_err(store_execute_error)?
            .records;
        let expected = committed_state.revision;
        let observed_source_turn = source.facts.iter().find_map(|fact| {
            let RuntimeJournalFact::Internal(RuntimeEvent::TurnStarted { turn_id, .. }) = fact
            else {
                return None;
            };
            (committed_state.active_turn_id.as_ref() == Some(turn_id))
                .then(|| source.source_turn_id.as_ref())
                .flatten()
        });
        // Every fact in the envelope is reduced into this in-memory projection. The repository
        // revision remains the immutable committed base until the complete batch is valid.
        let mut state = committed_state.clone();
        let mut records = Vec::new();
        let mut presentation_publication = Vec::new();
        let mut observed = false;
        let mut runtime_coordinate = RuntimePresentationCoordinate {
            runtime_turn_id: None,
            presentation_turn_id: None,
            runtime_item_id: None,
            interaction_id: None,
            source_thread_id: Some(source.source_thread_id.as_str().to_string()),
            source_turn_id: source
                .source_turn_id
                .as_ref()
                .map(|id| id.as_str().to_string()),
            source_item_id: source
                .source_item_id
                .as_ref()
                .map(|id| id.as_str().to_string()),
            source_request_id: source.source_request_id.clone(),
            source_entry_index: source.source_entry_index,
        };
        let mut closes_live_stream = false;
        let mut terminal_context = None;
        let mut terminal_application_effects = Vec::new();

        for fact in source.facts.clone() {
            if let (Some(source_turn_id), RuntimeJournalFact::Presentation(event)) =
                (observed_source_turn, &fact)
                && let agentdash_agent_protocol::BackboneEvent::TurnStarted(notification) =
                    &event.event
                && notification.thread_id == source.source_thread_id.as_str()
                && notification.turn.id == source_turn_id.as_str()
            {
                observed = true;
                continue;
            }
            if let RuntimeJournalFact::Internal(event) = &fact {
                update_runtime_coordinate(&mut runtime_coordinate, event);
                let context =
                    match terminal_presentation_context(&state, event, &prior_records, &records) {
                        Ok(context) => context,
                        Err(error) => {
                            let message = error.to_string();
                            return self
                                .persist_protocol_violation(
                                    committed_state,
                                    source,
                                    DriverEventQuarantineReason::InvalidDriverFact {
                                        message: message.clone(),
                                    },
                                    RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                    message,
                                )
                                .await;
                        }
                    };
                if let Some(context) = context {
                    if terminal_context.replace(context).is_some() {
                        let message =
                            "driver fact batch contains multiple turn terminals".to_string();
                        return self
                            .persist_protocol_violation(
                                committed_state,
                                source,
                                DriverEventQuarantineReason::InvalidDriverFact {
                                    message: message.clone(),
                                },
                                RuntimeProtocolViolationCode::DuplicateTerminal,
                                message,
                            )
                            .await;
                    }
                }
                if let RuntimeEvent::TurnStarted { turn_id, .. } = event
                    && state.active_turn_id.as_ref() == Some(turn_id)
                {
                    if let Err(error) =
                        normalize_presentation_coordinate(&state, &mut runtime_coordinate)
                    {
                        let message = error.to_string();
                        return self
                            .persist_protocol_violation(
                                committed_state,
                                source,
                                DriverEventQuarantineReason::InvalidDriverFact {
                                    message: message.clone(),
                                },
                                RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                message,
                            )
                            .await;
                    }
                    observed = true;
                    continue;
                }
                if event.is_transient() {
                    return self
                        .persist_protocol_violation(
                            committed_state,
                            source,
                            DriverEventQuarantineReason::TransientInternalFact,
                            RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                            "driver transient summaries must be emitted as complete presentation facts"
                                .to_string(),
                        )
                        .await;
                }
                if let Err(error) =
                    normalize_presentation_coordinate(&state, &mut runtime_coordinate)
                {
                    let message = error.to_string();
                    return self
                        .persist_protocol_violation(
                            committed_state,
                            source,
                            DriverEventQuarantineReason::InvalidDriverFact {
                                message: message.clone(),
                            },
                            RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                            message,
                        )
                        .await;
                }
                closes_live_stream |= closes_driver_stream(event);
            }

            if fact.is_transient() {
                let RuntimeJournalFact::Presentation(event) = fact else {
                    unreachable!("transient internal facts were rejected before publication");
                };
                presentation_publication.push(PresentationPublication::Transient {
                    coordinate: runtime_coordinate.clone(),
                    event,
                });
                continue;
            }

            let loss_message = match &fact {
                RuntimeJournalFact::Internal(event) => loss_reason(event),
                RuntimeJournalFact::Presentation(_) => None,
            };
            let recorded_at_ms = terminal_context
                .as_ref()
                .filter(|context| {
                    matches!(
                        &fact,
                        RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                            turn_id,
                            ..
                        }) if turn_id == &context.runtime_turn_id
                    )
                })
                .map_or_else(crate::model::current_time_ms, |context| {
                    context.completed_at_ms
                });
            let record = match state.append_durable_fact(
                fact.clone(),
                recorded_at_ms,
                Some(source.binding_id.clone()),
                None,
                runtime_coordinate.clone(),
            ) {
                Ok(record) => record,
                Err(error) => {
                    return self
                        .persist_transition_violation(committed_state, source, error)
                        .await;
                }
            };
            if record.as_presentation().is_some() {
                presentation_publication.push(PresentationPublication::Durable(record.clone()));
            }
            records.push(record);

            if let RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal,
                message,
                ..
            }) = &fact
            {
                let source_operation_id = match source.operation_id.clone() {
                    Some(operation_id) => operation_id,
                    None => {
                        let message = format!(
                            "driver turn terminal {turn_id} is missing its accepted operation coordinate"
                        );
                        return self
                            .persist_protocol_violation(
                                committed_state,
                                source,
                                DriverEventQuarantineReason::InvalidDriverFact {
                                    message: message.clone(),
                                },
                                RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                message,
                            )
                            .await;
                    }
                };
                let owning_operation_id = match state
                    .operations
                    .keys()
                    .find(|operation_id| canonical_turn_id(operation_id) == *turn_id)
                    .cloned()
                {
                    Some(operation_id) => operation_id,
                    None => {
                        let message = format!(
                            "driver turn terminal {turn_id} has no owning TurnStart operation"
                        );
                        return self
                            .persist_protocol_violation(
                                committed_state,
                                source,
                                DriverEventQuarantineReason::InvalidDriverFact {
                                    message: message.clone(),
                                },
                                RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                message,
                            )
                            .await;
                    }
                };
                let operation_terminal = turn_operation_terminal(*terminal, message.clone());
                for operation_id in [owning_operation_id, source_operation_id] {
                    if records.iter().any(|record| {
                        matches!(
                            record.fact(),
                            RuntimeJournalFact::Internal(RuntimeEvent::OperationTerminal {
                                operation_id: recorded_operation_id,
                                ..
                            }) if recorded_operation_id == &operation_id
                        )
                    }) {
                        continue;
                    }
                    if !matches!(
                        state.operations.get(&operation_id),
                        Some(crate::EntityPhase::Active)
                    ) {
                        let message =
                            format!("driver terminal operation {operation_id} is not active");
                        return self
                            .persist_protocol_violation(
                                committed_state,
                                source,
                                DriverEventQuarantineReason::InvalidDriverFact {
                                    message: message.clone(),
                                },
                                RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                message,
                            )
                            .await;
                    }
                    update_runtime_coordinate(
                        &mut runtime_coordinate,
                        &RuntimeEvent::OperationTerminal {
                            operation_id: operation_id.clone(),
                            terminal: operation_terminal.clone(),
                        },
                    );
                    let operation_record = match state.append_durable_fact(
                        RuntimeJournalFact::Internal(RuntimeEvent::OperationTerminal {
                            operation_id,
                            terminal: operation_terminal.clone(),
                        }),
                        recorded_at_ms,
                        Some(source.binding_id.clone()),
                        None,
                        runtime_coordinate.clone(),
                    ) {
                        Ok(record) => record,
                        Err(error) => {
                            return self
                                .persist_transition_violation(committed_state, source, error)
                                .await;
                        }
                    };
                    records.push(operation_record);
                }
            }

            if let Some(message) = loss_message {
                for terminal in state.lost_terminal_events(Some(message)) {
                    let context = match terminal_presentation_context(
                        &state,
                        &terminal,
                        &prior_records,
                        &records,
                    ) {
                        Ok(context) => context,
                        Err(error) => {
                            let message = error.to_string();
                            return self
                                .persist_protocol_violation(
                                    committed_state,
                                    source,
                                    DriverEventQuarantineReason::InvalidDriverFact {
                                        message: message.clone(),
                                    },
                                    RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                    message,
                                )
                                .await;
                        }
                    };
                    if let Some(context) = context {
                        if terminal_context.replace(context).is_some() {
                            let message =
                                "binding loss produced multiple turn terminals".to_string();
                            return self
                                .persist_protocol_violation(
                                    committed_state,
                                    source,
                                    DriverEventQuarantineReason::InvalidDriverFact {
                                        message: message.clone(),
                                    },
                                    RuntimeProtocolViolationCode::DuplicateTerminal,
                                    message,
                                )
                                .await;
                        }
                    }
                    update_runtime_coordinate(&mut runtime_coordinate, &terminal);
                    let record = match state.append_durable_fact(
                        RuntimeJournalFact::Internal(terminal),
                        crate::model::current_time_ms(),
                        Some(source.binding_id.clone()),
                        None,
                        runtime_coordinate.clone(),
                    ) {
                        Ok(record) => record,
                        Err(error) => {
                            return self
                                .persist_transition_violation(committed_state, source, error)
                                .await;
                        }
                    };
                    records.push(record);
                }
            }
        }

        if let Some(context) = terminal_context {
            self.append_terminal_application_projection(
                &mut state,
                &source,
                &mut records,
                &mut presentation_publication,
                &mut terminal_application_effects,
                context,
            )?;
        }

        let first_sequence = records.first().and_then(|record| record.carrier().sequence);
        if !records.is_empty() {
            let operation_terminals = operation_terminals_from_records(&records);
            let commit = RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state.clone(),
                operation: None,
                operation_terminals,
                records,
                outbox: Vec::new(),
                terminal_application_effects,
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
            if presentation_publication.is_empty() {
                self.store
                    .commit(commit)
                    .await
                    .map_err(store_execute_error)?;
            } else {
                self.store
                    .commit_with_live_presentation_publication(commit, false)
                    .await
                    .map_err(store_execute_error)?;
            }
        }

        for publication in presentation_publication {
            match publication {
                PresentationPublication::Durable(record) => {
                    self.store.publish_durable_presentation(record).await;
                }
                PresentationPublication::Transient { coordinate, event } => {
                    self.store
                        .publish_transient_presentation(
                            thread_id.clone(),
                            source.binding_id.clone(),
                            source.generation,
                            coordinate.runtime_turn_id.clone(),
                            state.revision,
                            coordinate,
                            event,
                        )
                        .await;
                }
            }
        }
        if closes_live_stream {
            self.store.clear(&thread_id).await;
        }
        if let Some(sequence) = first_sequence {
            Ok(DriverEventAdmission::Durable { sequence })
        } else if source.facts.iter().any(RuntimeJournalFact::is_transient) {
            Ok(DriverEventAdmission::Transient)
        } else if observed {
            Ok(DriverEventAdmission::Observed)
        } else {
            Ok(DriverEventAdmission::Quarantined)
        }
    }

    fn append_terminal_application_projection(
        &self,
        state: &mut RuntimeThreadState,
        source: &DriverEventEnvelope,
        records: &mut Vec<RuntimeJournalRecord>,
        presentation_publication: &mut Vec<PresentationPublication>,
        terminal_application_effects: &mut Vec<crate::RuntimeTerminalApplicationEffectOutboxEntry>,
        mut context: RuntimeTerminalPresentationContext,
    ) -> Result<(), RuntimeExecuteError> {
        if context.diagnostic.is_none() {
            context.diagnostic = records
                .iter()
                .rev()
                .chain(context.prior_records.iter().rev())
                .find_map(|record| {
                    if record.carrier().coordinate.runtime_turn_id.as_ref()
                        != Some(&context.runtime_turn_id)
                    {
                        return None;
                    }
                    let RuntimeJournalFact::Presentation(event) = record.fact() else {
                        return None;
                    };
                    let agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::RuntimeTerminalDiagnostic(
                            diagnostic,
                        ),
                    ) = &event.event
                    else {
                        return None;
                    };
                    Some(diagnostic.clone())
                });
        }
        context.prior_records.extend(records.iter().cloned());
        let completed_at_ms = context.completed_at_ms;
        let effect_context = context.clone();
        let projected = self
            .application_presentation_projector
            .project_terminal(context)
            .map_err(|error| RuntimeExecuteError::InvalidCommand {
                reason: error.to_string(),
            })?;
        validate_presentation_thread_ids(&projected, &state.presentation_thread_id).map_err(
            |error| RuntimeExecuteError::InvalidCommand {
                reason: error.to_string(),
            },
        )?;
        let mut terminal_presentation_record = None;
        for input in projected {
            if input.event.durability
                != agentdash_agent_runtime_contract::PresentationDurability::Durable
            {
                return Err(RuntimeExecuteError::InvalidCommand {
                    reason: "application terminal projection must be durable".into(),
                });
            }
            let record = state
                .append_durable_fact(
                    RuntimeJournalFact::Presentation(input.event),
                    completed_at_ms,
                    Some(source.binding_id.clone()),
                    None,
                    input.coordinate,
                )
                .map_err(transition_execute_error)?;
            if is_turn_terminal_presentation(&record) {
                if terminal_presentation_record.is_some() {
                    return Err(RuntimeExecuteError::InvalidCommand {
                        reason: "application terminal projector produced multiple turn_terminal presentations"
                            .into(),
                    });
                }
                terminal_presentation_record = Some(record.clone());
            }
            presentation_publication.push(PresentationPublication::Durable(record.clone()));
            records.push(record);
        }
        let terminal_presentation_record =
            terminal_presentation_record.ok_or_else(|| RuntimeExecuteError::InvalidCommand {
                reason:
                    "application terminal projector must produce one turn_terminal presentation"
                        .into(),
            })?;
        let terminal_event_sequence =
            terminal_presentation_record
                .carrier()
                .sequence
                .ok_or_else(|| RuntimeExecuteError::InvalidCommand {
                    reason: "turn_terminal presentation must be durable".into(),
                })?;
        let effect_id = crate::RuntimeTerminalApplicationEffectId::new(format!(
            "terminal-effect:{}:{}:{}",
            state.thread_id, effect_context.runtime_turn_id, terminal_event_sequence.0
        ))
        .map_err(store_execute_error)?;
        terminal_application_effects.push(crate::RuntimeTerminalApplicationEffectOutboxEntry {
            effect_id,
            runtime_thread_id: state.thread_id.clone(),
            presentation_thread_id: effect_context.presentation_thread_id,
            runtime_turn_id: effect_context.runtime_turn_id,
            presentation_turn_id: effect_context.presentation_turn_id,
            terminal_event_sequence,
            terminal: effect_context.terminal,
            message: effect_context.message,
            diagnostic: effect_context.diagnostic,
            started_at_ms: effect_context.started_at_ms,
            completed_at_ms: effect_context.completed_at_ms,
            binding_id: source.binding_id.clone(),
            driver_generation: source.generation,
            surface_revision: state.surface.surface_revision,
            surface_digest: state.surface.surface_digest.clone(),
            source_thread_id: source.source_thread_id.as_str().to_string(),
            source_turn_id: source
                .source_turn_id
                .as_ref()
                .map(|turn_id| turn_id.as_str().to_string()),
            terminal_hook_effect_binding: state.surface.terminal_hook_effect_binding.clone(),
        });
        Ok(())
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
        committed_state: RuntimeThreadState,
        source: DriverEventEnvelope,
        quarantine_reason: DriverEventQuarantineReason,
        code: RuntimeProtocolViolationCode,
        message: String,
    ) -> Result<DriverEventAdmission, RuntimeExecuteError> {
        let expected = committed_state.revision;
        let prior_records = self
            .store
            .journal_records_after(&committed_state.thread_id, None)
            .await
            .map_err(store_execute_error)?
            .records;
        let mut canonical_events = vec![RuntimeEvent::ProtocolViolation {
            code,
            message: message.clone(),
            critical: true,
        }];
        canonical_events.extend(committed_state.lost_terminal_events(Some(message)));

        let mut state = committed_state;
        let mut records = Vec::new();
        let mut terminal_context = None;
        let mut coordinate = RuntimePresentationCoordinate {
            runtime_turn_id: None,
            presentation_turn_id: None,
            runtime_item_id: None,
            interaction_id: None,
            source_thread_id: Some(source.source_thread_id.as_str().to_string()),
            source_turn_id: source
                .source_turn_id
                .as_ref()
                .map(|turn_id| turn_id.as_str().to_string()),
            source_item_id: source
                .source_item_id
                .as_ref()
                .map(|item_id| item_id.as_str().to_string()),
            source_request_id: source.source_request_id.clone(),
            source_entry_index: source.source_entry_index,
        };
        for event in canonical_events {
            if let Some(context) =
                terminal_presentation_context(&state, &event, &prior_records, &records)?
            {
                terminal_context = Some(context);
            }
            update_runtime_coordinate(&mut coordinate, &event);
            normalize_presentation_coordinate(&state, &mut coordinate).map_err(|error| {
                RuntimeExecuteError::InvalidCommand {
                    reason: error.to_string(),
                }
            })?;
            let recorded_at_ms = terminal_context
                .as_ref()
                .filter(|context| {
                    matches!(
                        &event,
                        RuntimeEvent::TurnTerminal { turn_id, .. }
                            if turn_id == &context.runtime_turn_id
                    )
                })
                .map_or_else(crate::model::current_time_ms, |context| {
                    context.completed_at_ms
                });
            records.push(
                state
                    .append_durable_fact(
                        RuntimeJournalFact::Internal(event),
                        recorded_at_ms,
                        Some(source.binding_id.clone()),
                        None,
                        coordinate.clone(),
                    )
                    .map_err(transition_execute_error)?,
            );
        }
        let sequence = records
            .first()
            .and_then(|record| record.carrier().sequence)
            .expect("critical violation is durable");
        let mut presentation_publication = Vec::new();
        let mut terminal_application_effects = Vec::new();
        if let Some(context) = terminal_context {
            self.append_terminal_application_projection(
                &mut state,
                &source,
                &mut records,
                &mut presentation_publication,
                &mut terminal_application_effects,
                context,
            )?;
        }
        let operation_terminals = operation_terminals_from_records(&records);
        let thread_id = state.thread_id.clone();
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: state,
                operation: None,
                operation_terminals,
                records,
                outbox: Vec::new(),
                terminal_application_effects,
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: vec![QuarantinedDriverEvent {
                    event: source,
                    reason: quarantine_reason,
                }],
            })
            .await
            .map_err(store_execute_error)?;
        for publication in presentation_publication {
            let PresentationPublication::Durable(record) = publication else {
                unreachable!("terminal projection is durable");
            };
            self.store.publish_durable_presentation(record).await;
        }
        self.store.clear(&thread_id).await;
        Ok(DriverEventAdmission::Terminalized { sequence })
    }
}

fn validate_presentation_thread_ids(
    events: &[agentdash_agent_runtime_contract::RuntimePresentationInput],
    presentation_thread_id: &agentdash_agent_runtime_contract::PresentationThreadId,
) -> Result<(), crate::RuntimePresentationAppendError> {
    for input in events {
        let value = serde_json::to_value(&input.event.event).map_err(|error| {
            crate::RuntimePresentationAppendError::Invalid(format!(
                "presentation event cannot be inspected: {error}"
            ))
        })?;
        let Some(thread_id) = value.pointer("/payload/threadId") else {
            continue;
        };
        if thread_id.as_str() != Some(presentation_thread_id.as_str()) {
            return Err(crate::RuntimePresentationAppendError::Invalid(format!(
                "protected payload threadId does not match presentation thread {}",
                presentation_thread_id.as_str()
            )));
        }
    }
    Ok(())
}

fn normalize_presentation_coordinate(
    state: &RuntimeThreadState,
    coordinate: &mut RuntimePresentationCoordinate,
) -> Result<(), crate::RuntimePresentationAppendError> {
    let Some(runtime_turn_id) = coordinate.runtime_turn_id.as_ref() else {
        return Ok(());
    };
    let expected = state
        .turns
        .get(runtime_turn_id)
        .map(|turn| turn.presentation_turn_id.clone())
        .ok_or_else(|| {
            crate::RuntimePresentationAppendError::Invalid(format!(
                "presentation targets unknown runtime turn {runtime_turn_id}"
            ))
        })?;
    if coordinate
        .presentation_turn_id
        .as_ref()
        .is_some_and(|actual| actual != &expected)
    {
        return Err(crate::RuntimePresentationAppendError::Invalid(format!(
            "presentation turn identity does not match runtime turn {runtime_turn_id}"
        )));
    }
    coordinate.presentation_turn_id = Some(expected);
    Ok(())
}

fn turn_operation_terminal(
    terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal,
    message: Option<String>,
) -> agentdash_agent_runtime_contract::RuntimeOperationTerminal {
    use agentdash_agent_runtime_contract::{RuntimeOperationTerminal, RuntimeTurnTerminal};
    match terminal {
        RuntimeTurnTerminal::Completed => RuntimeOperationTerminal::Succeeded,
        RuntimeTurnTerminal::Lost => RuntimeOperationTerminal::Lost {
            retryable: false,
            message,
        },
        RuntimeTurnTerminal::Interrupted
        | RuntimeTurnTerminal::Refused
        | RuntimeTurnTerminal::LimitReached
        | RuntimeTurnTerminal::Failed => RuntimeOperationTerminal::Failed {
            retryable: false,
            message,
        },
    }
}

fn is_turn_terminal_presentation(record: &RuntimeJournalRecord) -> bool {
    matches!(
        record.fact(),
        RuntimeJournalFact::Presentation(event)
            if matches!(
                &event.event,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                ) if key == "turn_terminal"
            )
    )
}

#[async_trait]
impl<S> AgentRuntimeGateway for ManagedAgentRuntime<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + RuntimeTransientEvents + 'static,
{
    async fn append_presentation(
        &self,
        request: agentdash_agent_runtime_contract::RuntimePresentationAppendRequest,
    ) -> Result<
        agentdash_agent_runtime_contract::RuntimePresentationAppendReceipt,
        agentdash_agent_runtime_contract::RuntimePresentationAppendError,
    > {
        ManagedAgentRuntime::append_presentation(self, request).await
    }

    async fn execute(
        &self,
        envelope: RuntimeCommandEnvelope,
    ) -> Result<agentdash_agent_runtime_contract::OperationReceipt, RuntimeExecuteError> {
        let _mutation = self.driver_event_ingest.lock().await;
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
            Some(thread_id) if matches!(envelope.command, RuntimeCommand::ThreadStart { .. }) => {
                if self
                    .store
                    .load_thread(&thread_id)
                    .await
                    .map_err(store_execute_error)?
                    .is_some()
                {
                    return Err(RuntimeExecuteError::InvalidCommand {
                        reason: format!("thread {thread_id} already exists"),
                    });
                }
                (new_thread(&envelope.command)?, None)
            }
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
        if let RuntimeCommand::SurfaceAdopt { target, .. } = &envelope.command {
            self.surface_validator
                .as_ref()
                .ok_or_else(|| RuntimeExecuteError::Unavailable {
                    reason: "Runtime surface validator is not configured".to_string(),
                    retryable: false,
                })?
                .validate_surface_reference(&state.binding_id, &state.thread_id, target)
                .await
                .map_err(|reason| RuntimeExecuteError::InvalidCommand { reason })?;
        }
        let hook_plan_binding = match &envelope.command {
            RuntimeCommand::ThreadStart {
                thread_id, surface, ..
            } => {
                let hook_plan = &surface.hook_plan;
                crate::hook::validate_bound_hook_plan(hook_plan).map_err(|error| {
                    RuntimeExecuteError::InvalidCommand {
                        reason: error.to_string(),
                    }
                })?;
                if hook_plan.revision.0 != 1 {
                    return invalid("initial hook plan revision must be one");
                }
                Some(RuntimeHookPlanBinding {
                    thread_id: thread_id.clone(),
                    plan: hook_plan.clone(),
                })
            }
            RuntimeCommand::SurfaceAdopt {
                thread_id, target, ..
            } => Some(RuntimeHookPlanBinding {
                thread_id: thread_id.clone(),
                plan: target.hook_plan.clone(),
            }),
            _ => None,
        };
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

        // Freeze the compacted source boundary before admitting the compaction
        // operation itself. Later durable records are replayed as the tail
        // after the compacted context base during a cold driver rebind.
        let source_end_event_sequence = state.next_event_sequence;
        let current_tool_set_revision = state.tool_set_revision;
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
        let command_terminal = events.iter().find_map(|event| match &event.event {
            RuntimeEvent::OperationTerminal {
                operation_id,
                terminal,
            } if operation_id == &envelope.meta.operation_id => Some(terminal.clone()),
            _ => None,
        });
        let record = RuntimeOperationRecord {
            operation_id: envelope.meta.operation_id.clone(),
            idempotency_key: envelope.meta.idempotency_key.clone(),
            actor: envelope.meta.actor.clone(),
            thread_id: state.thread_id.clone(),
            operation_sequence,
            accepted_revision,
            presentation: envelope.presentation.clone(),
            command: envelope.command.clone(),
            terminal: command_terminal,
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
                source_end_event_sequence,
                status: crate::ContextPreparationStatus::Pending,
            }],
            _ => Vec::new(),
        };
        let driver_command = driver_dispatch_command(
            &state.bound_profile,
            &envelope.command,
            current_tool_set_revision,
        );
        let outbox = if matches!(
            envelope.command,
            RuntimeCommand::ContextCompact { .. } | RuntimeCommand::ThreadRebind { .. }
        ) {
            Vec::new()
        } else {
            vec![RuntimeOutboxEntry {
                operation_id: record.operation_id.clone(),
                thread_id: state.thread_id.clone(),
                presentation_thread_id: state.presentation_thread_id.clone(),
                binding_id: state.binding_id.clone(),
                binding_epoch: state.binding_epoch,
                generation: state.driver_generation,
                command: driver_command,
            }]
        };
        let operation_terminals = operation_terminals(&events)
            .into_iter()
            .filter(|(operation_id, _)| operation_id != &record.operation_id)
            .collect();
        let mut records = crate::internal_journal_records(events).map_err(store_execute_error)?;
        let presentation_binding_id = state.binding_id.clone();
        let presentation_operation_id = record.operation_id.clone();
        for input in envelope.presentation {
            if input.event.durability
                != agentdash_agent_runtime_contract::PresentationDurability::Durable
            {
                return invalid(
                    "command presentation batch only accepts durable producer-owned events",
                );
            }
            let mut coordinate = input.coordinate;
            match (
                coordinate.runtime_turn_id.as_ref(),
                state.active_turn_id.as_ref(),
            ) {
                (Some(provided), Some(active)) if provided != active => {
                    return invalid(
                        "command presentation runtime turn does not match the active canonical turn",
                    );
                }
                (Some(_), None) => {
                    return invalid(
                        "command presentation cannot claim a runtime turn when none is active",
                    );
                }
                (None, active) => coordinate.runtime_turn_id = active.cloned(),
                _ => {}
            }
            records.push(
                state
                    .append_durable_fact(
                        RuntimeJournalFact::Presentation(input.event),
                        crate::model::current_time_ms(),
                        Some(presentation_binding_id.clone()),
                        None,
                        coordinate,
                    )
                    .map_err(transition_execute_error)?
                    .with_operation_id(presentation_operation_id.clone()),
            );
        }
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision,
                projection: state,
                operation: Some(record),
                operation_terminals,
                records,
                outbox,
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items,
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
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
        let query = match query {
            RuntimeSnapshotQuery::Operation { operation_id } => {
                let operation = self
                    .store
                    .find_operation(&operation_id)
                    .await
                    .map_err(|error| RuntimeSnapshotError::Unavailable {
                        reason: error.to_string(),
                    })?
                    .ok_or(RuntimeSnapshotError::NotFound)?;
                return Ok(RuntimeSnapshotResult::Operation {
                    operation: Box::new(agentdash_agent_runtime_contract::RuntimeOperationView {
                        operation_id: operation.operation_id.clone(),
                        idempotency_key: operation.idempotency_key.clone(),
                        actor: operation.actor.clone(),
                        presentation: operation.presentation.clone(),
                        command: operation.command.clone(),
                        receipt: operation.receipt(false),
                        terminal: operation.terminal,
                    }),
                });
            }
            other => other,
        };
        let (thread_id, at_revision, at_context_revision, context_query) = match query {
            RuntimeSnapshotQuery::Operation { .. } => unreachable!("handled above"),
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
        let live = if subscription.include_transient {
            Some(self.store.subscribe(&subscription.thread_id).await)
        } else {
            None
        };
        let batch = self
            .store
            .internal_events_after(&subscription.thread_id, subscription.after)
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
            events.extend(
                self.store
                    .read(
                        &subscription.thread_id,
                        subscription.stream_generation,
                        subscription.transient_after,
                    )
                    .await,
            );
        }
        if let Some(live) = live {
            Ok(Box::new(LiveEventStream {
                events: events.into(),
                live,
                durable_cursor: subscription.after,
                transient_cursor: subscription.transient_after,
                stream_generation: subscription.stream_generation,
            }))
        } else {
            Ok(Box::new(VecEventStream {
                events: events.into(),
            }))
        }
    }
}

fn driver_dispatch_command(
    profile: &agentdash_agent_runtime_contract::RuntimeProfile,
    command: &RuntimeCommand,
    current_tool_set_revision: agentdash_agent_runtime_contract::ToolSetRevision,
) -> RuntimeCommand {
    match command {
        RuntimeCommand::SurfaceAdopt {
            thread_id, target, ..
        } if !profile
            .lifecycle
            .contains(&agentdash_agent_runtime_contract::LifecycleCapability::SurfaceAdopt) =>
        {
            RuntimeCommand::ToolSetReplace {
                thread_id: thread_id.clone(),
                expected_current_tool_set_revision: current_tool_set_revision,
                target_tool_set_revision: target.tool_set_revision,
                tool_set_digest: target.tool_set_digest.clone(),
            }
        }
        other => other.clone(),
    }
}

impl<S> ManagedAgentRuntime<S> {}

fn new_thread(command: &RuntimeCommand) -> Result<RuntimeThreadState, RuntimeExecuteError> {
    let RuntimeCommand::ThreadStart {
        thread_id,
        presentation_thread_id,
        binding_id,
        driver_generation,
        source_thread_id,
        profile_digest,
        bound_profile,
        surface,
        settings_revision,
        ..
    } = command
    else {
        return invalid("new runtime thread requires ThreadStart coordinates");
    };
    Ok(RuntimeThreadState {
        thread_id: thread_id.clone(),
        presentation_thread_id: presentation_thread_id.clone(),
        revision: RuntimeRevision(0),
        next_event_sequence: EventSequence(0),
        next_operation_sequence: OperationSequence(0),
        status: RuntimeThreadStatus::Active,
        active_turn_id: None,
        binding_id: binding_id.clone(),
        binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
        driver_generation: *driver_generation,
        source_thread_id: source_thread_id.clone(),
        profile_digest: profile_digest.clone(),
        bound_profile: (**bound_profile).clone(),
        surface: (**surface).clone(),
        active_checkpoint_id: None,
        context_revision: agentdash_agent_runtime_contract::ContextRevision(0),
        settings_revision: *settings_revision,
        tool_set_revision: surface.tool_set_revision,
        hook_plan_revision: None,
        hook_plan_digest: None,
        operations: Default::default(),
        turns: Default::default(),
        items: Default::default(),
        item_order: Vec::new(),
        presentation_transcript: Vec::new(),
        interactions: Default::default(),
    })
}

fn command_thread_id(command: &RuntimeCommand) -> Option<RuntimeThreadId> {
    match command {
        RuntimeCommand::ThreadStart { thread_id, .. }
        | RuntimeCommand::ThreadResume { thread_id }
        | RuntimeCommand::ThreadRebind { thread_id, .. }
        | RuntimeCommand::ThreadFork { thread_id, .. }
        | RuntimeCommand::ThreadSettingsUpdate { thread_id, .. }
        | RuntimeCommand::TurnStart { thread_id, .. }
        | RuntimeCommand::TurnSteer { thread_id, .. }
        | RuntimeCommand::TurnInterrupt { thread_id, .. }
        | RuntimeCommand::InteractionRespond { thread_id, .. }
        | RuntimeCommand::ContextCompact { thread_id, .. }
        | RuntimeCommand::ToolSetReplace { thread_id, .. }
        | RuntimeCommand::SurfaceAdopt { thread_id, .. } => Some(thread_id.clone()),
    }
}

fn validate_command(
    state: &RuntimeThreadState,
    command: &RuntimeCommand,
) -> Result<(), RuntimeExecuteError> {
    match command {
        RuntimeCommand::ThreadStart {
            presentation_turn_id,
            input,
            ..
        } => match (input.is_empty(), presentation_turn_id.is_some()) {
            (true, true) => {
                return invalid("ThreadStart without input cannot allocate a presentation turn");
            }
            (false, false) => {
                return invalid("ThreadStart with input requires a presentation turn identity");
            }
            _ => {}
        },
        RuntimeCommand::ThreadResume { .. } if state.status != RuntimeThreadStatus::Suspended => {
            return invalid("ThreadResume requires a suspended thread on the same binding");
        }
        RuntimeCommand::ThreadRebind {
            recovery_intent_id: _,
            binding_epoch,
            expected_binding_id,
            expected_driver_generation,
            new_binding_id,
            new_driver_generation: _,
            profile_digest,
            bound_profile,
            ..
        } => {
            if state.status != RuntimeThreadStatus::Lost {
                return invalid("ThreadRebind requires a lost thread");
            }
            if state.active_turn_id.is_some()
                || state
                    .interactions
                    .values()
                    .any(|interaction| matches!(interaction.phase, crate::EntityPhase::Active))
            {
                return invalid("ThreadRebind requires no active turn or interaction");
            }
            if state.binding_id != *expected_binding_id
                || state.driver_generation != *expected_driver_generation
            {
                return invalid("ThreadRebind expected binding coordinates are stale");
            }
            if *binding_epoch <= state.binding_epoch {
                return invalid("ThreadRebind binding epoch must advance");
            }
            if new_binding_id == expected_binding_id {
                return invalid("ThreadRebind requires a new binding identity");
            }
            if !bound_profile
                .lifecycle
                .contains(&agentdash_agent_runtime_contract::LifecycleCapability::ThreadResume)
            {
                return invalid("ThreadRebind profile must guarantee ThreadResume");
            }
            if state.bound_profile.intersect(bound_profile) != state.bound_profile {
                return invalid(
                    "ThreadRebind profile does not preserve the bound surface guarantees",
                );
            }
            if agentdash_agent_runtime_contract::runtime_profile_digest(bound_profile)
                != *profile_digest
            {
                return invalid("ThreadRebind profile digest does not match the bound profile");
            }
        }
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
            expected_current_tool_set_revision,
            target_tool_set_revision,
            tool_set_digest,
            ..
        } => {
            if *expected_current_tool_set_revision != state.tool_set_revision {
                return invalid("tool set revision is stale");
            }
            if target_tool_set_revision.0 <= expected_current_tool_set_revision.0 {
                return invalid("tool set replacement target revision must advance");
            }
            if tool_set_digest.trim().is_empty() {
                return invalid("tool set replacement target digest is empty");
            }
        }
        RuntimeCommand::SurfaceAdopt {
            expected_surface_revision,
            expected_surface_digest,
            target,
            ..
        } => {
            if *expected_surface_revision != state.surface.surface_revision
                || expected_surface_digest != &state.surface.surface_digest
            {
                return invalid("surface adoption expected reference is stale");
            }
            if target.surface_revision.0 <= state.surface.surface_revision.0 {
                return invalid("surface adoption revision must advance");
            }
            if target.source_frame_id.trim().is_empty() || target.vfs_digest.trim().is_empty() {
                return invalid("surface adoption target identity/digest is empty");
            }
            crate::hook::validate_bound_hook_plan(&target.hook_plan).map_err(|error| {
                RuntimeExecuteError::InvalidCommand {
                    reason: error.to_string(),
                }
            })?;
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
        RuntimeCommand::ThreadStart {
            surface,
            presentation_turn_id,
            input,
            ..
        } => {
            events.push(RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Active,
            });
            events.push(RuntimeEvent::HookPlanBound {
                plan_revision: surface.hook_plan.revision,
                plan_digest: surface.hook_plan.digest.clone(),
            });
            if !input.is_empty() {
                let turn_id = canonical_turn_id(operation_id);
                events.push(RuntimeEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                    presentation_turn_id: presentation_turn_id
                        .clone()
                        .expect("validated non-empty ThreadStart presentation turn"),
                });
                append_runtime_user_item(events, operation_id, turn_id, input)?;
            }
        }
        RuntimeCommand::ThreadResume { .. } => events.push(RuntimeEvent::ThreadStatusChanged {
            status: RuntimeThreadStatus::Active,
        }),
        RuntimeCommand::ThreadRebind {
            recovery_intent_id,
            binding_epoch,
            expected_binding_id,
            expected_driver_generation,
            new_binding_id,
            new_driver_generation,
            source_thread_id,
            profile_digest,
            bound_profile,
            ..
        } => {
            events.push(RuntimeEvent::BindingReestablished {
                recovery_intent_id: recovery_intent_id.clone(),
                binding_epoch: *binding_epoch,
                old_binding_id: expected_binding_id.clone(),
                old_driver_generation: *expected_driver_generation,
                new_binding_id: new_binding_id.clone(),
                new_driver_generation: *new_driver_generation,
                source_thread_id: source_thread_id.clone(),
                profile_digest: profile_digest.clone(),
                bound_profile: bound_profile.clone(),
            });
            events.push(RuntimeEvent::OperationTerminal {
                operation_id: operation_id.clone(),
                terminal: agentdash_agent_runtime_contract::RuntimeOperationTerminal::Succeeded,
            });
        }
        RuntimeCommand::ThreadSettingsUpdate { .. } => state.settings_revision.0 += 1,
        RuntimeCommand::TurnStart {
            presentation_turn_id,
            input,
            ..
        } => {
            let turn_id = canonical_turn_id(operation_id);
            events.push(RuntimeEvent::TurnStarted {
                turn_id: turn_id.clone(),
                presentation_turn_id: presentation_turn_id.clone(),
            });
            append_runtime_user_item(events, operation_id, turn_id, input)?;
        }
        RuntimeCommand::TurnSteer { input, .. } => append_runtime_user_item(
            events,
            operation_id,
            state
                .active_turn_id
                .clone()
                .expect("validated active turn for steer"),
            input,
        )?,
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
        RuntimeCommand::ToolSetReplace {
            target_tool_set_revision,
            tool_set_digest,
            ..
        } => {
            state.tool_set_revision = *target_tool_set_revision;
            state.surface.tool_set_revision = *target_tool_set_revision;
            state.surface.tool_set_digest = tool_set_digest.clone();
        }
        RuntimeCommand::SurfaceAdopt { target, .. } => {
            state.surface = (**target).clone();
            state.settings_revision = target.settings_revision;
            state.tool_set_revision = target.tool_set_revision;
            events.push(RuntimeEvent::HookPlanBound {
                plan_revision: target.hook_plan.revision,
                plan_digest: target.hook_plan.digest.clone(),
            });
        }
        RuntimeCommand::ThreadFork { .. }
        | RuntimeCommand::TurnInterrupt { .. }
        | RuntimeCommand::ContextCompact { .. } => {}
    }
    Ok(())
}

fn append_runtime_user_item(
    events: &mut Vec<RuntimeEvent>,
    operation_id: &agentdash_agent_runtime_contract::RuntimeOperationId,
    turn_id: agentdash_agent_runtime_contract::RuntimeTurnId,
    input: &[agentdash_agent_runtime_contract::RuntimeInput],
) -> Result<(), RuntimeExecuteError> {
    if input.is_empty() {
        return Ok(());
    }
    let item_id = agentdash_agent_runtime_contract::RuntimeItemId::new(format!(
        "{turn_id}:user:{operation_id}"
    ))
    .map_err(|error| RuntimeExecuteError::InvalidCommand {
        reason: format!("canonical user item identity is invalid: {error}"),
    })?;
    let content = agentdash_agent_runtime_contract::RuntimeItemContent::user_message(
        item_id.to_string(),
        input.to_vec(),
    );
    events.push(RuntimeEvent::ItemStarted {
        turn_id: turn_id.clone(),
        item_id: item_id.clone(),
        initial_content: content.clone(),
    });
    events.push(RuntimeEvent::ItemTerminal {
        turn_id,
        item_id,
        terminal: agentdash_agent_runtime_contract::RuntimeItemTerminal::Completed {
            final_content: content,
        },
    });
    Ok(())
}

pub fn canonical_turn_id(
    operation_id: &agentdash_agent_runtime_contract::RuntimeOperationId,
) -> agentdash_agent_runtime_contract::RuntimeTurnId {
    agentdash_agent_runtime_contract::RuntimeTurnId::new(format!("turn-{operation_id}"))
        .expect("derived turn id is valid")
}

fn duplicate_receipt(
    existing: RuntimeOperationRecord,
    envelope: &RuntimeCommandEnvelope,
    conflict: OperationConflictKind,
) -> Result<agentdash_agent_runtime_contract::OperationReceipt, RuntimeExecuteError> {
    if existing.operation_id != envelope.meta.operation_id
        || existing.idempotency_key != envelope.meta.idempotency_key
        || existing.actor != envelope.meta.actor
        || existing.presentation != envelope.presentation
        || existing.command != envelope.command
    {
        return Err(RuntimeExecuteError::OperationConflict {
            existing_operation_id: existing.operation_id,
            conflict,
        });
    }
    Ok(existing.receipt(true))
}

fn terminal_presentation_context(
    state: &RuntimeThreadState,
    event: &RuntimeEvent,
    prior_records: &[RuntimeJournalRecord],
    pending_records: &[RuntimeJournalRecord],
) -> Result<Option<RuntimeTerminalPresentationContext>, RuntimeExecuteError> {
    let RuntimeEvent::TurnTerminal {
        turn_id,
        terminal,
        message,
        diagnostic,
    } = event
    else {
        return Ok(None);
    };
    let presentation_turn_id = state
        .turns
        .get(turn_id)
        .map(|turn| turn.presentation_turn_id.clone())
        .ok_or_else(|| RuntimeExecuteError::InvalidCommand {
            reason: format!("turn terminal {turn_id} has no presentation turn mapping"),
        })?;
    let started_at_ms = prior_records
        .iter()
        .chain(pending_records.iter())
        .find_map(|record| {
            matches!(
                record.fact(),
                RuntimeJournalFact::Internal(RuntimeEvent::TurnStarted {
                    turn_id: started_turn_id,
                    ..
                }) if started_turn_id == turn_id
            )
            .then_some(record.carrier().recorded_at_ms)
        });
    Ok(Some(RuntimeTerminalPresentationContext {
        presentation_thread_id: state.presentation_thread_id.clone(),
        runtime_turn_id: turn_id.clone(),
        presentation_turn_id,
        terminal: *terminal,
        message: message.clone(),
        diagnostic: diagnostic.clone(),
        started_at_ms,
        completed_at_ms: crate::model::current_time_ms(),
        prior_records: prior_records.to_vec(),
    }))
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

fn closes_driver_stream(event: &RuntimeEvent) -> bool {
    matches!(
        event,
        RuntimeEvent::ItemTerminal { .. }
            | RuntimeEvent::TurnTerminal { .. }
            | RuntimeEvent::BindingLost { .. }
            | RuntimeEvent::BindingReestablished { .. }
    )
}

fn forbidden_driver_internal_fact(
    event: &RuntimeEvent,
) -> Option<(
    DriverEventQuarantineReason,
    RuntimeProtocolViolationCode,
    String,
)> {
    match event {
        RuntimeEvent::OperationAccepted { .. } => Some((
            DriverEventQuarantineReason::DriverOperationAcceptance,
            RuntimeProtocolViolationCode::DriverOperationAcceptance,
            "driver attempted to accept a runtime-owned operation".to_string(),
        )),
        RuntimeEvent::BindingReestablished { .. } => Some((
            DriverEventQuarantineReason::DriverRuntimeOwnedBindingEvent,
            RuntimeProtocolViolationCode::DriverRuntimeOwnedBindingEvent,
            "driver attempted to emit a runtime-owned binding transition".to_string(),
        )),
        RuntimeEvent::ContextCheckpointPrepared { .. }
        | RuntimeEvent::ContextActivationApplied { .. }
        | RuntimeEvent::ContextCompactionTerminal { .. }
        | RuntimeEvent::ContextCheckpointActivated { .. } => Some((
            DriverEventQuarantineReason::DriverRuntimeOwnedContextEvent,
            RuntimeProtocolViolationCode::DriverRuntimeOwnedContextEvent,
            "driver attempted to emit a runtime-owned context transition".to_string(),
        )),
        RuntimeEvent::HookRunAccepted { .. }
        | RuntimeEvent::HookRunStarted { .. }
        | RuntimeEvent::HookRunTerminal { .. }
        | RuntimeEvent::HookPlanBound { .. } => Some((
            DriverEventQuarantineReason::DriverRuntimeOwnedHookEvent,
            RuntimeProtocolViolationCode::DriverRuntimeOwnedHookEvent,
            "driver attempted to emit a runtime-owned hook transition".to_string(),
        )),
        _ => None,
    }
}

fn update_runtime_coordinate(coordinate: &mut RuntimePresentationCoordinate, event: &RuntimeEvent) {
    match event {
        RuntimeEvent::TurnStarted { turn_id, .. }
        | RuntimeEvent::TurnTerminal { turn_id, .. }
        | RuntimeEvent::TokenUsageUpdated { turn_id, .. }
        | RuntimeEvent::ProviderStatus { turn_id, .. } => {
            coordinate.runtime_turn_id = Some(turn_id.clone());
        }
        RuntimeEvent::ItemStarted {
            turn_id, item_id, ..
        }
        | RuntimeEvent::ConversationDelta {
            turn_id, item_id, ..
        }
        | RuntimeEvent::ItemTerminal {
            turn_id, item_id, ..
        } => {
            coordinate.runtime_turn_id = Some(turn_id.clone());
            coordinate.runtime_item_id = Some(item_id.clone());
        }
        RuntimeEvent::InteractionRequested {
            turn_id,
            item_id,
            interaction_id,
            ..
        } => {
            coordinate.runtime_turn_id = Some(turn_id.clone());
            coordinate.runtime_item_id.clone_from(item_id);
            coordinate.interaction_id = Some(interaction_id.clone());
        }
        RuntimeEvent::InteractionTerminal {
            turn_id,
            interaction_id,
            ..
        } => {
            coordinate.runtime_turn_id = Some(turn_id.clone());
            coordinate.interaction_id = Some(interaction_id.clone());
        }
        _ => {}
    }
}

fn operation_terminals_from_records(
    records: &[RuntimeJournalRecord],
) -> Vec<(
    agentdash_agent_runtime_contract::RuntimeOperationId,
    agentdash_agent_runtime_contract::RuntimeOperationTerminal,
)> {
    records
        .iter()
        .filter_map(RuntimeJournalRecord::to_internal_envelope)
        .filter_map(|envelope| match envelope.event {
            RuntimeEvent::OperationTerminal {
                operation_id,
                terminal,
            } => Some((operation_id, terminal)),
            _ => None,
        })
        .collect()
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

struct LiveEventStream {
    events: VecDeque<RuntimeEventEnvelope>,
    live: tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>,
    durable_cursor: Option<EventSequence>,
    transient_cursor: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    stream_generation: Option<agentdash_agent_runtime_contract::RuntimeDriverGeneration>,
}

#[async_trait]
impl RuntimeEventStream for LiveEventStream {
    async fn next(&mut self) -> Option<Result<RuntimeEventEnvelope, RuntimeSubscribeError>> {
        loop {
            let event = if let Some(event) = self.events.pop_front() {
                event
            } else {
                match self.live.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        return Some(Err(RuntimeSubscribeError::Unavailable {
                            reason: format!("live runtime stream lagged by {skipped} events"),
                            retryable: true,
                        }));
                    }
                }
            };
            if let Some(sequence) = event.sequence {
                if self.durable_cursor.is_some_and(|cursor| sequence <= cursor) {
                    continue;
                }
                self.durable_cursor = Some(sequence);
                if matches!(
                    event.event,
                    RuntimeEvent::TurnTerminal { .. }
                        | RuntimeEvent::BindingLost { .. }
                        | RuntimeEvent::BindingReestablished { .. }
                ) {
                    self.transient_cursor = None;
                    self.stream_generation = None;
                }
                return Some(Ok(event));
            }
            let Some(coordinate) = event.transient.as_ref() else {
                continue;
            };
            if self
                .stream_generation
                .is_some_and(|generation| coordinate.stream_generation != generation)
            {
                continue;
            }
            if self
                .transient_cursor
                .is_some_and(|cursor| coordinate.sequence <= cursor)
            {
                continue;
            }
            self.stream_generation = Some(coordinate.stream_generation);
            self.transient_cursor = Some(coordinate.sequence);
            return Some(Ok(event));
        }
    }
}

#[async_trait]
impl RuntimeEventStream for VecEventStream {
    async fn next(&mut self) -> Option<Result<RuntimeEventEnvelope, RuntimeSubscribeError>> {
        self.events.pop_front().map(Ok)
    }
}
