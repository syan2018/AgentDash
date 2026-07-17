use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    BindingEpoch, ContextRevision, EventSequence, IdempotencyKey, OperationReceipt,
    OperationSequence, PresentationTurnId, ProfileDigest, RuntimeActor, RuntimeBindingId,
    RuntimeCarrierMetadata, RuntimeCommand, RuntimeDriverGeneration, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeInteractionId, RuntimeItemId, RuntimeJournalFact,
    RuntimeJournalRecord, RuntimeOperationId, RuntimeOperationTerminal,
    RuntimePresentationCoordinate, RuntimeProfile, RuntimeRevision, RuntimeSnapshot,
    RuntimeThreadId, RuntimeThreadStatus, RuntimeTranscriptItem, RuntimeTurnId,
    ThreadSettingsRevision, ToolSetRevision,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityPhase<T> {
    Active,
    Terminal(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTurnState {
    pub presentation_turn_id: PresentationTurnId,
    pub phase: EntityPhase<agentdash_agent_runtime_contract::RuntimeTurnTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeItemState {
    pub turn_id: RuntimeTurnId,
    pub initial_content: agentdash_agent_runtime_contract::RuntimeItemContent,
    pub phase: EntityPhase<agentdash_agent_runtime_contract::RuntimeItemTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInteractionState {
    pub turn_id: RuntimeTurnId,
    pub item_id: Option<RuntimeItemId>,
    pub request: agentdash_agent_runtime_contract::RuntimeInteractionRequest,
    pub phase: EntityPhase<agentdash_agent_runtime_contract::RuntimeInteractionTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeOperationRecord {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: IdempotencyKey,
    pub actor: RuntimeActor,
    pub thread_id: RuntimeThreadId,
    pub operation_sequence: OperationSequence,
    pub accepted_revision: RuntimeRevision,
    pub presentation: Vec<agentdash_agent_runtime_contract::RuntimePresentationInput>,
    pub command: RuntimeCommand,
    pub terminal: Option<RuntimeOperationTerminal>,
}

impl RuntimeOperationRecord {
    pub fn receipt(&self, duplicate: bool) -> OperationReceipt {
        OperationReceipt {
            operation_id: self.operation_id.clone(),
            operation_sequence: self.operation_sequence,
            thread_id: Some(self.thread_id.clone()),
            accepted_revision: self.accepted_revision,
            duplicate,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeThreadState {
    pub thread_id: RuntimeThreadId,
    pub presentation_thread_id: agentdash_agent_runtime_contract::PresentationThreadId,
    pub revision: RuntimeRevision,
    pub next_event_sequence: EventSequence,
    pub next_operation_sequence: OperationSequence,
    pub status: RuntimeThreadStatus,
    pub active_turn_id: Option<RuntimeTurnId>,
    pub binding_id: RuntimeBindingId,
    pub binding_epoch: BindingEpoch,
    pub driver_generation: RuntimeDriverGeneration,
    pub source_thread_id: agentdash_agent_runtime_contract::DriverThreadId,
    /// Current Agent conversation name projected from the standard
    /// `thread/name/updated` presentation journal.
    #[serde(deserialize_with = "deserialize_required_thread_name")]
    pub thread_name: Option<String>,
    pub profile_digest: ProfileDigest,
    pub bound_profile: RuntimeProfile,
    pub surface: agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor,
    pub active_checkpoint_id: Option<agentdash_agent_runtime_contract::ContextCheckpointId>,
    pub context_revision: ContextRevision,
    pub settings_revision: ThreadSettingsRevision,
    pub tool_set_revision: ToolSetRevision,
    pub hook_plan_revision: Option<agentdash_agent_runtime_contract::HookPlanRevision>,
    pub hook_plan_digest: Option<agentdash_agent_runtime_contract::HookPlanDigest>,
    pub operations: BTreeMap<RuntimeOperationId, EntityPhase<RuntimeOperationTerminal>>,
    pub turns: BTreeMap<RuntimeTurnId, RuntimeTurnState>,
    pub items: BTreeMap<RuntimeItemId, RuntimeItemState>,
    pub item_order: Vec<RuntimeItemId>,
    /// Complete terminal presentation events, indexed only from producer-owned
    /// payloads. Runtime item summaries are never used to populate this list.
    pub presentation_transcript: Vec<RuntimeTranscriptItem>,
    pub interactions: BTreeMap<RuntimeInteractionId, RuntimeInteractionState>,
}

fn deserialize_required_thread_name<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum TransitionError {
    #[error("invalid runtime journal fact: {message}")]
    InvalidJournalFact { message: String },
    #[error("operation {operation_id} was already accepted")]
    OperationAlreadyAccepted { operation_id: RuntimeOperationId },
    #[error("operation {operation_id} was not accepted")]
    OperationNotAccepted { operation_id: RuntimeOperationId },
    #[error("operation {operation_id} already reached terminal")]
    DuplicateOperationTerminal { operation_id: RuntimeOperationId },
    #[error("turn {turn_id} cannot start while another turn is active")]
    TurnCannotStart { turn_id: RuntimeTurnId },
    #[error("turn {turn_id} was not started")]
    TurnNotStarted { turn_id: RuntimeTurnId },
    #[error("turn {turn_id} already reached terminal")]
    DuplicateTurnTerminal { turn_id: RuntimeTurnId },
    #[error("turn {turn_id} is not the active turn")]
    TurnNotActive { turn_id: RuntimeTurnId },
    #[error("turn {turn_id} cannot reach terminal while an item or interaction is active")]
    TurnHasActiveChildren { turn_id: RuntimeTurnId },
    #[error("item {item_id} was already started")]
    ItemAlreadyStarted { item_id: RuntimeItemId },
    #[error("item {item_id} was not started")]
    ItemNotStarted { item_id: RuntimeItemId },
    #[error("item {item_id} belongs to turn {expected_turn_id}, not {actual_turn_id}")]
    ItemParentMismatch {
        item_id: RuntimeItemId,
        expected_turn_id: RuntimeTurnId,
        actual_turn_id: RuntimeTurnId,
    },
    #[error("item {item_id} already reached terminal")]
    DuplicateItemTerminal { item_id: RuntimeItemId },
    #[error("item {item_id} received a delta after terminal")]
    ItemDeltaAfterTerminal { item_id: RuntimeItemId },
    #[error("presentation item {source_item_id} already reached terminal")]
    DuplicatePresentationTerminal { source_item_id: String },
    #[error("interaction {interaction_id} was already requested")]
    InteractionAlreadyRequested {
        interaction_id: RuntimeInteractionId,
    },
    #[error("interaction {interaction_id} was not requested")]
    InteractionNotRequested {
        interaction_id: RuntimeInteractionId,
    },
    #[error(
        "interaction {interaction_id} belongs to turn {expected_turn_id}, not {actual_turn_id}"
    )]
    InteractionParentMismatch {
        interaction_id: RuntimeInteractionId,
        expected_turn_id: RuntimeTurnId,
        actual_turn_id: RuntimeTurnId,
    },
    #[error("interaction {interaction_id} already reached terminal")]
    DuplicateInteractionTerminal {
        interaction_id: RuntimeInteractionId,
    },
    #[error("context revision must advance from {current:?} to {expected:?}, got {actual:?}")]
    ContextRevisionMismatch {
        current: ContextRevision,
        expected: ContextRevision,
        actual: ContextRevision,
    },
    #[error("binding event references {actual_binding_id}, expected {expected_binding_id}")]
    BindingMismatch {
        expected_binding_id: RuntimeBindingId,
        actual_binding_id: RuntimeBindingId,
    },
    #[error("binding generation {actual_generation:?} does not match {expected_generation:?}")]
    BindingGenerationMismatch {
        expected_generation: RuntimeDriverGeneration,
        actual_generation: RuntimeDriverGeneration,
    },
    #[error("binding epoch must advance beyond {current:?}, got {actual:?}")]
    BindingEpochDidNotAdvance {
        current: BindingEpoch,
        actual: BindingEpoch,
    },
}

impl TransitionError {
    pub fn is_duplicate_terminal(&self) -> bool {
        matches!(
            self,
            Self::DuplicateOperationTerminal { .. }
                | Self::DuplicateTurnTerminal { .. }
                | Self::DuplicateItemTerminal { .. }
                | Self::DuplicateInteractionTerminal { .. }
        )
    }
}

impl RuntimeThreadState {
    pub fn apply_journal_record(
        &mut self,
        record: &RuntimeJournalRecord,
    ) -> Result<(), TransitionError> {
        self.apply_journal_fact(record.fact())
    }

    fn apply_journal_fact(&mut self, fact: &RuntimeJournalFact) -> Result<(), TransitionError> {
        match fact {
            RuntimeJournalFact::Internal(event) => self.apply_authoritative(event),
            RuntimeJournalFact::Presentation(event) => {
                if let agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(notification) =
                    &event.event
                {
                    if notification.thread_id != self.source_thread_id.as_str() {
                        return Err(TransitionError::InvalidJournalFact {
                            message: format!(
                                "thread name source `{}` does not match bound source thread `{}`",
                                notification.thread_id, self.source_thread_id
                            ),
                        });
                    }
                    if notification
                        .thread_name
                        .as_deref()
                        .is_some_and(|name| name.trim().is_empty())
                    {
                        return Err(TransitionError::InvalidJournalFact {
                            message: "thread name must not be blank".to_string(),
                        });
                    }
                    self.thread_name = notification.thread_name.clone();
                    return Ok(());
                }
                let Some(item) = presentation_terminal_item(event) else {
                    return Ok(());
                };
                if self
                    .presentation_transcript
                    .iter()
                    .any(|existing| existing.source_item_id == item.source_item_id)
                {
                    return Err(TransitionError::DuplicatePresentationTerminal {
                        source_item_id: item.source_item_id,
                    });
                }
                self.presentation_transcript.push(item);
                Ok(())
            }
        }
    }

    pub fn apply_authoritative(&mut self, event: &RuntimeEvent) -> Result<(), TransitionError> {
        match event {
            RuntimeEvent::OperationAccepted { operation_id } => {
                if self.operations.contains_key(operation_id) {
                    return Err(TransitionError::OperationAlreadyAccepted {
                        operation_id: operation_id.clone(),
                    });
                }
                self.operations
                    .insert(operation_id.clone(), EntityPhase::Active);
            }
            RuntimeEvent::OperationTerminal {
                operation_id,
                terminal,
            } => match self.operations.get_mut(operation_id) {
                Some(phase @ EntityPhase::Active) => {
                    *phase = EntityPhase::Terminal(terminal.clone());
                }
                Some(EntityPhase::Terminal(_)) => {
                    return Err(TransitionError::DuplicateOperationTerminal {
                        operation_id: operation_id.clone(),
                    });
                }
                None => {
                    return Err(TransitionError::OperationNotAccepted {
                        operation_id: operation_id.clone(),
                    });
                }
            },
            RuntimeEvent::ThreadStatusChanged { status } => self.status = *status,
            RuntimeEvent::TurnStarted {
                turn_id,
                presentation_turn_id,
            } => {
                if self.active_turn_id.is_some() || self.turns.contains_key(turn_id) {
                    return Err(TransitionError::TurnCannotStart {
                        turn_id: turn_id.clone(),
                    });
                }
                self.active_turn_id = Some(turn_id.clone());
                self.turns.insert(
                    turn_id.clone(),
                    RuntimeTurnState {
                        presentation_turn_id: presentation_turn_id.clone(),
                        phase: EntityPhase::Active,
                    },
                );
            }
            RuntimeEvent::TurnTerminal {
                turn_id, terminal, ..
            } => {
                self.require_active_turn(turn_id)?;
                let has_active_children = self.items.values().any(|item| {
                    item.turn_id == *turn_id && matches!(item.phase, EntityPhase::Active)
                }) || self.interactions.values().any(|interaction| {
                    interaction.turn_id == *turn_id
                        && matches!(interaction.phase, EntityPhase::Active)
                });
                if has_active_children {
                    return Err(TransitionError::TurnHasActiveChildren {
                        turn_id: turn_id.clone(),
                    });
                }
                let state =
                    self.turns
                        .get_mut(turn_id)
                        .ok_or_else(|| TransitionError::TurnNotStarted {
                            turn_id: turn_id.clone(),
                        })?;
                if !matches!(state.phase, EntityPhase::Active) {
                    return Err(TransitionError::DuplicateTurnTerminal {
                        turn_id: turn_id.clone(),
                    });
                }
                state.phase = EntityPhase::Terminal(*terminal);
                if self.active_turn_id.as_ref() == Some(turn_id) {
                    self.active_turn_id = None;
                }
            }
            RuntimeEvent::ItemStarted {
                turn_id,
                item_id,
                initial_content,
            } => {
                self.require_active_turn(turn_id)?;
                if self.items.contains_key(item_id) {
                    return Err(TransitionError::ItemAlreadyStarted {
                        item_id: item_id.clone(),
                    });
                }
                self.items.insert(
                    item_id.clone(),
                    RuntimeItemState {
                        turn_id: turn_id.clone(),
                        initial_content: initial_content.clone(),
                        phase: EntityPhase::Active,
                    },
                );
                self.item_order.push(item_id.clone());
            }
            RuntimeEvent::ConversationDelta {
                turn_id, item_id, ..
            } => {
                let state = self.require_item(turn_id, item_id)?;
                if !matches!(state.phase, EntityPhase::Active) {
                    return Err(TransitionError::ItemDeltaAfterTerminal {
                        item_id: item_id.clone(),
                    });
                }
            }
            RuntimeEvent::ItemTerminal {
                turn_id,
                item_id,
                terminal,
            } => {
                let state = self.require_item_mut(turn_id, item_id)?;
                if !matches!(state.phase, EntityPhase::Active) {
                    return Err(TransitionError::DuplicateItemTerminal {
                        item_id: item_id.clone(),
                    });
                }
                state.phase = EntityPhase::Terminal(terminal.clone());
            }
            RuntimeEvent::InteractionRequested {
                turn_id,
                item_id,
                interaction_id,
                request,
            } => {
                self.require_active_turn(turn_id)?;
                if let Some(item_id) = item_id {
                    self.require_item(turn_id, item_id)?;
                }
                if self.interactions.contains_key(interaction_id) {
                    return Err(TransitionError::InteractionAlreadyRequested {
                        interaction_id: interaction_id.clone(),
                    });
                }
                self.interactions.insert(
                    interaction_id.clone(),
                    RuntimeInteractionState {
                        turn_id: turn_id.clone(),
                        item_id: item_id.clone(),
                        request: request.clone(),
                        phase: EntityPhase::Active,
                    },
                );
            }
            RuntimeEvent::InteractionTerminal {
                turn_id,
                interaction_id,
                terminal,
            } => {
                let state = self.interactions.get_mut(interaction_id).ok_or_else(|| {
                    TransitionError::InteractionNotRequested {
                        interaction_id: interaction_id.clone(),
                    }
                })?;
                if state.turn_id != *turn_id {
                    return Err(TransitionError::InteractionParentMismatch {
                        interaction_id: interaction_id.clone(),
                        expected_turn_id: state.turn_id.clone(),
                        actual_turn_id: turn_id.clone(),
                    });
                }
                if !matches!(state.phase, EntityPhase::Active) {
                    return Err(TransitionError::DuplicateInteractionTerminal {
                        interaction_id: interaction_id.clone(),
                    });
                }
                state.phase = EntityPhase::Terminal(*terminal);
            }
            RuntimeEvent::ContextCheckpointActivated {
                checkpoint_id,
                context_revision,
                ..
            } => {
                let expected = ContextRevision(self.context_revision.0.saturating_add(1));
                if *context_revision != expected {
                    return Err(TransitionError::ContextRevisionMismatch {
                        current: self.context_revision,
                        expected,
                        actual: *context_revision,
                    });
                }
                self.active_checkpoint_id = Some(checkpoint_id.clone());
                self.context_revision = *context_revision;
            }
            RuntimeEvent::BindingLost { binding_id, .. } => {
                self.require_binding(binding_id)?;
                self.status = RuntimeThreadStatus::Lost;
            }
            RuntimeEvent::BindingReestablished {
                recovery_intent_id: _,
                binding_epoch,
                old_binding_id,
                old_driver_generation,
                new_binding_id,
                new_driver_generation,
                source_thread_id,
                profile_digest,
                bound_profile,
            } => {
                self.require_binding(old_binding_id)?;
                if self.driver_generation != *old_driver_generation {
                    return Err(TransitionError::BindingGenerationMismatch {
                        expected_generation: self.driver_generation,
                        actual_generation: *old_driver_generation,
                    });
                }
                if *binding_epoch <= self.binding_epoch {
                    return Err(TransitionError::BindingEpochDidNotAdvance {
                        current: self.binding_epoch,
                        actual: *binding_epoch,
                    });
                }
                self.binding_epoch = *binding_epoch;
                self.binding_id = new_binding_id.clone();
                self.driver_generation = *new_driver_generation;
                self.source_thread_id = source_thread_id.clone();
                self.profile_digest = profile_digest.clone();
                self.bound_profile = (**bound_profile).clone();
                self.status = RuntimeThreadStatus::Active;
            }
            RuntimeEvent::ProtocolViolation { critical: true, .. } => {
                self.status = RuntimeThreadStatus::Lost;
            }
            RuntimeEvent::BindingEstablished { binding_id } => self.require_binding(binding_id)?,
            RuntimeEvent::ProtocolViolation {
                critical: false, ..
            }
            | RuntimeEvent::ContextCheckpointPrepared { .. }
            | RuntimeEvent::ContextActivationApplied { .. }
            | RuntimeEvent::ContextCompactionTerminal { .. }
            | RuntimeEvent::DriverContextCompactedOpaque
            | RuntimeEvent::TokenUsageUpdated { .. }
            | RuntimeEvent::ConversationError { .. }
            | RuntimeEvent::ProviderStatus { .. }
            | RuntimeEvent::HookRunAccepted { .. }
            | RuntimeEvent::HookRunStarted { .. }
            | RuntimeEvent::HookRunTerminal { .. } => {}
            RuntimeEvent::HookPlanBound {
                plan_revision,
                plan_digest,
            } => {
                self.hook_plan_revision = Some(*plan_revision);
                self.hook_plan_digest = Some(plan_digest.clone());
            }
        }
        Ok(())
    }

    fn require_active_turn(&self, turn_id: &RuntimeTurnId) -> Result<(), TransitionError> {
        if self.active_turn_id.as_ref() == Some(turn_id) {
            Ok(())
        } else {
            Err(TransitionError::TurnNotActive {
                turn_id: turn_id.clone(),
            })
        }
    }

    fn require_item(
        &self,
        turn_id: &RuntimeTurnId,
        item_id: &RuntimeItemId,
    ) -> Result<&RuntimeItemState, TransitionError> {
        let state = self
            .items
            .get(item_id)
            .ok_or_else(|| TransitionError::ItemNotStarted {
                item_id: item_id.clone(),
            })?;
        if state.turn_id != *turn_id {
            return Err(TransitionError::ItemParentMismatch {
                item_id: item_id.clone(),
                expected_turn_id: state.turn_id.clone(),
                actual_turn_id: turn_id.clone(),
            });
        }
        Ok(state)
    }

    fn require_item_mut(
        &mut self,
        turn_id: &RuntimeTurnId,
        item_id: &RuntimeItemId,
    ) -> Result<&mut RuntimeItemState, TransitionError> {
        let state = self
            .items
            .get_mut(item_id)
            .ok_or_else(|| TransitionError::ItemNotStarted {
                item_id: item_id.clone(),
            })?;
        if state.turn_id != *turn_id {
            return Err(TransitionError::ItemParentMismatch {
                item_id: item_id.clone(),
                expected_turn_id: state.turn_id.clone(),
                actual_turn_id: turn_id.clone(),
            });
        }
        Ok(state)
    }

    fn require_binding(&self, binding_id: &RuntimeBindingId) -> Result<(), TransitionError> {
        if self.binding_id == *binding_id {
            Ok(())
        } else {
            Err(TransitionError::BindingMismatch {
                expected_binding_id: self.binding_id.clone(),
                actual_binding_id: binding_id.clone(),
            })
        }
    }

    pub fn lost_terminal_events(&self, message: Option<String>) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        for (item_id, item) in &self.items {
            if matches!(item.phase, EntityPhase::Active) {
                events.push(RuntimeEvent::ItemTerminal {
                    turn_id: item.turn_id.clone(),
                    item_id: item_id.clone(),
                    terminal: agentdash_agent_runtime_contract::RuntimeItemTerminal::Lost {
                        message: message.clone(),
                    },
                });
            }
        }
        for (interaction_id, interaction) in &self.interactions {
            if matches!(interaction.phase, EntityPhase::Active) {
                events.push(RuntimeEvent::InteractionTerminal {
                    turn_id: interaction.turn_id.clone(),
                    interaction_id: interaction_id.clone(),
                    terminal: agentdash_agent_runtime_contract::RuntimeInteractionTerminal::Lost,
                });
            }
        }
        if let Some(turn_id) = &self.active_turn_id {
            events.push(RuntimeEvent::TurnTerminal {
                turn_id: turn_id.clone(),
                terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal::Lost,
                message: message.clone(),
                diagnostic: None,
            });
        }
        for (operation_id, operation) in &self.operations {
            if matches!(operation, EntityPhase::Active) {
                events.push(RuntimeEvent::OperationTerminal {
                    operation_id: operation_id.clone(),
                    terminal: RuntimeOperationTerminal::Lost {
                        retryable: true,
                        message: message.clone(),
                    },
                });
            }
        }
        events
    }

    pub fn append_events(
        &mut self,
        events: impl IntoIterator<Item = RuntimeEvent>,
    ) -> Result<Vec<RuntimeEventEnvelope>, TransitionError> {
        let mut envelopes = Vec::new();
        for event in events {
            self.apply_authoritative(&event)?;
            self.revision.0 += 1;
            self.next_event_sequence.0 += 1;
            envelopes.push(RuntimeEventEnvelope {
                thread_id: self.thread_id.clone(),
                occurred_at_ms: current_time_ms(),
                sequence: Some(self.next_event_sequence),
                transient: None,
                revision: self.revision,
                event,
            });
        }
        Ok(envelopes)
    }

    pub fn append_durable_fact(
        &mut self,
        fact: RuntimeJournalFact,
        recorded_at_ms: u64,
        binding_id: Option<RuntimeBindingId>,
        append_idempotency_key: Option<IdempotencyKey>,
        coordinate: RuntimePresentationCoordinate,
    ) -> Result<RuntimeJournalRecord, TransitionError> {
        self.apply_journal_fact(&fact)?;
        self.revision.0 += 1;
        self.next_event_sequence.0 += 1;
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: self.thread_id.clone(),
                recorded_at_ms,
                sequence: Some(self.next_event_sequence),
                transient: None,
                revision: self.revision,
                operation_id: None,
                append_idempotency_key,
                binding_id,
                coordinate,
            },
            fact,
        )
        .map_err(|error| TransitionError::InvalidJournalFact {
            message: error.to_string(),
        })
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        let pending_interactions: Vec<_> = self
            .interactions
            .iter()
            .filter_map(|(id, state)| {
                matches!(state.phase, EntityPhase::Active).then_some(id.clone())
            })
            .collect();
        let pending_interaction_details = self
            .interactions
            .iter()
            .filter_map(|(interaction_id, state)| {
                matches!(state.phase, EntityPhase::Active).then(|| {
                    agentdash_agent_runtime_contract::PendingRuntimeInteractionView {
                        interaction_id: interaction_id.clone(),
                        turn_id: state.turn_id.clone(),
                        item_id: state.item_id.clone(),
                        request: state.request.clone(),
                    }
                })
            })
            .collect();
        let command_availability = agentdash_agent_runtime_contract::RuntimeCommandKind::all()
            .into_iter()
            .map(|kind| {
                let state = agentdash_agent_runtime_contract::AvailabilityState {
                    thread_status: self.status,
                    has_active_turn: self.active_turn_id.is_some(),
                    has_pending_interaction: !pending_interactions.is_empty(),
                };
                (
                    kind,
                    agentdash_agent_runtime_contract::command_availability(
                        kind,
                        &self.bound_profile,
                        &state,
                    ),
                )
            })
            .collect();
        RuntimeSnapshot {
            thread_id: self.thread_id.clone(),
            revision: self.revision,
            latest_event_sequence: self.next_event_sequence,
            captured_at_ms: current_time_ms(),
            status: self.status,
            thread_name: self.thread_name.clone(),
            active_turn_id: self.active_turn_id.clone(),
            active_presentation_turn_id: self.active_turn_id.as_ref().and_then(|turn_id| {
                self.turns
                    .get(turn_id)
                    .map(|turn| turn.presentation_turn_id.clone())
            }),
            binding_id: self.binding_id.clone(),
            binding_epoch: self.binding_epoch,
            profile_digest: self.profile_digest.clone(),
            bound_profile: self.bound_profile.clone(),
            surface: self.surface.clone(),
            active_checkpoint_id: self.active_checkpoint_id.clone(),
            context_revision: self.context_revision,
            settings_revision: self.settings_revision,
            tool_set_revision: self.tool_set_revision,
            pending_interactions,
            pending_interaction_details,
            command_availability,
            transcript: self.presentation_transcript.clone(),
            transcript_fidelity: agentdash_agent_runtime_contract::ContextFidelity::EventProjected,
        }
    }
}

fn presentation_terminal_item(
    event: &agentdash_agent_runtime_contract::ImmutablePresentationEvent,
) -> Option<RuntimeTranscriptItem> {
    let agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed) = &event.event else {
        return None;
    };
    Some(RuntimeTranscriptItem {
        source_thread_id: completed.thread_id.clone(),
        source_turn_id: completed.turn_id.clone(),
        source_item_id: completed.item.id().to_string(),
        terminal_event: event.clone(),
    })
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use agentdash_agent_runtime_contract::{ImmutablePresentationEvent, PresentationDurability};

    #[test]
    fn transcript_index_retains_the_complete_terminal_event() {
        let event: agentdash_agent_protocol::BackboneEvent =
            serde_json::from_value(serde_json::json!({
                "type": "item_completed",
                "payload": {
                    "item": {
                        "type": "dynamicToolCall",
                        "id": "source-item-1",
                        "namespace": null,
                        "tool": "fixture",
                        "arguments": { "nullable": null },
                        "status": "completed",
                        "contentItems": null,
                        "success": true,
                        "durationMs": null
                    },
                    "threadId": "source-thread-1",
                    "turnId": "source-turn-1",
                    "completedAtMs": 123_i64
                }
            }))
            .expect("terminal presentation event");
        let event = ImmutablePresentationEvent::new(PresentationDurability::Durable, event);

        let indexed = presentation_terminal_item(&event).expect("terminal transcript item");
        assert_eq!(indexed.source_thread_id, "source-thread-1");
        assert_eq!(indexed.source_turn_id, "source-turn-1");
        assert_eq!(indexed.source_item_id, "source-item-1");
        assert_eq!(
            serde_json::to_value(indexed.terminal_event).expect("serialize indexed terminal"),
            serde_json::to_value(event).expect("serialize source terminal")
        );
    }

    #[test]
    fn non_terminal_presentation_does_not_enter_the_snapshot_transcript() {
        let event: agentdash_agent_protocol::BackboneEvent =
            serde_json::from_value(serde_json::json!({
                "type": "agent_message_delta",
                "payload": {
                    "delta": "token",
                    "itemId": "source-item-1",
                    "threadId": "source-thread-1",
                    "turnId": "source-turn-1"
                }
            }))
            .expect("delta presentation event");
        let event = ImmutablePresentationEvent::new(PresentationDurability::Ephemeral, event);
        assert!(presentation_terminal_item(&event).is_none());
    }
}

pub(crate) fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

trait RuntimeCommandKinds {
    fn all() -> [agentdash_agent_runtime_contract::RuntimeCommandKind; 12];
}

impl RuntimeCommandKinds for agentdash_agent_runtime_contract::RuntimeCommandKind {
    fn all() -> [agentdash_agent_runtime_contract::RuntimeCommandKind; 12] {
        use agentdash_agent_runtime_contract::RuntimeCommandKind::*;
        [
            ThreadStart,
            ThreadResume,
            ThreadRebind,
            ThreadFork,
            ThreadSettingsUpdate,
            TurnStart,
            TurnSteer,
            TurnInterrupt,
            InteractionRespond,
            ContextCompact,
            ToolSetReplace,
            SurfaceAdopt,
        ]
    }
}
