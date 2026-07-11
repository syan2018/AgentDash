//! Shared executable behavior checks for runtime and driver implementations.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, DriverCommandEnvelope, DriverDispatchReceipt, DriverError, DriverEventSink,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeInteractionId, RuntimeItemId, RuntimeOperationId,
    RuntimeThreadId, RuntimeTurnId,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConformanceViolation {
    #[error("operation {0} reached terminal without acceptance")]
    OperationTerminalWithoutAcceptance(RuntimeOperationId),
    #[error("operation {0} has more than one acceptance")]
    DuplicateOperationAcceptance(RuntimeOperationId),
    #[error("operation {0} has more than one terminal")]
    DuplicateOperationTerminal(RuntimeOperationId),
    #[error("turn {0} has more than one start or terminal")]
    InvalidTurnTransition(RuntimeTurnId),
    #[error("item {0} has more than one start or terminal")]
    InvalidItemTransition(RuntimeItemId),
    #[error("item {0} received a delta after terminal")]
    DeltaAfterItemTerminal(RuntimeItemId),
    #[error("interaction {0} has more than one request or terminal")]
    InvalidInteractionTransition(RuntimeInteractionId),
    #[error("operation {0} changed its parent thread coordinate")]
    OperationThreadMismatch(RuntimeOperationId),
    #[error("turn {0} changed its parent thread coordinate")]
    TurnThreadMismatch(RuntimeTurnId),
    #[error("item {0} changed its parent thread or turn coordinate")]
    ItemParentMismatch(RuntimeItemId),
    #[error("interaction {0} changed its parent thread or turn coordinate")]
    InteractionParentMismatch(RuntimeInteractionId),
    #[error("trace ended with non-terminal operations, turns, items, or interactions")]
    MissingTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HarnessError {
    #[error("driver did not return DriverError::Unsupported")]
    DidNotReturnUnsupported,
    #[error("unsupported command produced a side effect")]
    UnsupportedProducedSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Active,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopedPhase {
    phase: Phase,
    thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TurnScopedPhase {
    phase: Phase,
    thread_id: RuntimeThreadId,
    turn_id: RuntimeTurnId,
}

#[derive(Debug, Default)]
pub struct RuntimeTraceValidator {
    operations: BTreeMap<RuntimeOperationId, ScopedPhase>,
    turns: BTreeMap<RuntimeTurnId, ScopedPhase>,
    items: BTreeMap<RuntimeItemId, TurnScopedPhase>,
    interactions: BTreeMap<RuntimeInteractionId, TurnScopedPhase>,
}

impl RuntimeTraceValidator {
    pub fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> Result<(), ConformanceViolation> {
        match &envelope.event {
            RuntimeEvent::OperationAccepted { operation_id }
                if self
                    .operations
                    .insert(
                        operation_id.clone(),
                        ScopedPhase {
                            phase: Phase::Active,
                            thread_id: envelope.thread_id.clone(),
                        },
                    )
                    .is_some() =>
            {
                return Err(ConformanceViolation::DuplicateOperationAcceptance(
                    operation_id.clone(),
                ));
            }
            RuntimeEvent::OperationAccepted { .. } => {}
            RuntimeEvent::OperationTerminal { operation_id, .. } => {
                terminal_scoped(
                    &mut self.operations,
                    operation_id,
                    &envelope.thread_id,
                    ConformanceViolation::OperationTerminalWithoutAcceptance(operation_id.clone()),
                    ConformanceViolation::DuplicateOperationTerminal(operation_id.clone()),
                    ConformanceViolation::OperationThreadMismatch(operation_id.clone()),
                )?;
            }
            RuntimeEvent::TurnStarted { turn_id } => {
                start_scoped(
                    &mut self.turns,
                    turn_id,
                    &envelope.thread_id,
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                )?;
            }
            RuntimeEvent::TurnTerminal { turn_id, .. } => {
                terminal_scoped(
                    &mut self.turns,
                    turn_id,
                    &envelope.thread_id,
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                    ConformanceViolation::TurnThreadMismatch(turn_id.clone()),
                )?;
            }
            RuntimeEvent::ItemStarted {
                turn_id, item_id, ..
            } => {
                start_turn_scoped(
                    &mut self.items,
                    item_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidItemTransition(item_id.clone()),
                )?;
            }
            RuntimeEvent::ItemDelta {
                turn_id, item_id, ..
            } => {
                let Some(state) = self.items.get(item_id) else {
                    return Err(ConformanceViolation::DeltaAfterItemTerminal(
                        item_id.clone(),
                    ));
                };
                if state.thread_id != envelope.thread_id || state.turn_id != *turn_id {
                    return Err(ConformanceViolation::ItemParentMismatch(item_id.clone()));
                }
                if state.phase != Phase::Active {
                    return Err(ConformanceViolation::DeltaAfterItemTerminal(
                        item_id.clone(),
                    ));
                }
            }
            RuntimeEvent::ItemTerminal {
                turn_id, item_id, ..
            } => {
                terminal_turn_scoped(
                    &mut self.items,
                    item_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidItemTransition(item_id.clone()),
                    ConformanceViolation::ItemParentMismatch(item_id.clone()),
                )?;
            }
            RuntimeEvent::InteractionRequested {
                turn_id,
                interaction_id,
                ..
            } => {
                start_turn_scoped(
                    &mut self.interactions,
                    interaction_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidInteractionTransition(interaction_id.clone()),
                )?;
            }
            RuntimeEvent::InteractionTerminal {
                turn_id,
                interaction_id,
                ..
            } => {
                terminal_turn_scoped(
                    &mut self.interactions,
                    interaction_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidInteractionTransition(interaction_id.clone()),
                    ConformanceViolation::InteractionParentMismatch(interaction_id.clone()),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self) -> Result<(), ConformanceViolation> {
        let all_terminal = self
            .operations
            .values()
            .chain(self.turns.values())
            .all(|state| state.phase == Phase::Terminal)
            && self
                .items
                .values()
                .chain(self.interactions.values())
                .all(|state| state.phase == Phase::Terminal);
        if all_terminal {
            Ok(())
        } else {
            Err(ConformanceViolation::MissingTerminal)
        }
    }
}

fn start_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, ScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    error: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    if states
        .insert(
            key.clone(),
            ScopedPhase {
                phase: Phase::Active,
                thread_id: thread_id.clone(),
            },
        )
        .is_some()
    {
        return Err(error);
    }
    Ok(())
}

fn terminal_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, ScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    missing: ConformanceViolation,
    duplicate: ConformanceViolation,
    mismatch: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    match states.get_mut(key) {
        Some(state) if state.thread_id != *thread_id => Err(mismatch),
        Some(ScopedPhase {
            phase: phase @ Phase::Active,
            ..
        }) => {
            *phase = Phase::Terminal;
            Ok(())
        }
        Some(ScopedPhase {
            phase: Phase::Terminal,
            ..
        }) => Err(duplicate),
        None => Err(missing),
    }
}

fn start_turn_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, TurnScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    turn_id: &RuntimeTurnId,
    error: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    if states
        .insert(
            key.clone(),
            TurnScopedPhase {
                phase: Phase::Active,
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            },
        )
        .is_some()
    {
        return Err(error);
    }
    Ok(())
}

fn terminal_turn_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, TurnScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    turn_id: &RuntimeTurnId,
    transition_error: ConformanceViolation,
    mismatch: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    match states.get_mut(key) {
        Some(state) if state.thread_id != *thread_id || state.turn_id != *turn_id => Err(mismatch),
        Some(TurnScopedPhase {
            phase: phase @ Phase::Active,
            ..
        }) => {
            *phase = Phase::Terminal;
            Ok(())
        }
        Some(TurnScopedPhase {
            phase: Phase::Terminal,
            ..
        })
        | None => Err(transition_error),
    }
}

#[derive(Default)]
pub struct RecordingEventSink {
    pub events: tokio::sync::Mutex<Vec<agentdash_agent_runtime_contract::DriverEventEnvelope>>,
}

#[async_trait]
impl DriverEventSink for RecordingEventSink {
    async fn emit(
        &self,
        event: agentdash_agent_runtime_contract::DriverEventEnvelope,
    ) -> Result<(), DriverError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[async_trait]
pub trait SideEffectProbe: Send + Sync {
    async fn side_effect_count(&self) -> usize;
}

/// Verifies that an unsupported dispatch is rejected and produces no observable side effect.
pub async fn assert_unsupported_before_side_effect<D>(
    driver: &D,
    command: DriverCommandEnvelope,
) -> Result<(), HarnessError>
where
    D: AgentRuntimeDriver + SideEffectProbe,
{
    let before = driver.side_effect_count().await;
    let sink = Arc::new(RecordingEventSink::default());
    let result = driver.dispatch(command, sink).await;
    let after = driver.side_effect_count().await;
    if !matches!(result, Err(DriverError::Unsupported { .. })) {
        return Err(HarnessError::DidNotReturnUnsupported);
    }
    if before != after {
        return Err(HarnessError::UnsupportedProducedSideEffect);
    }
    Ok(())
}

/// A minimal unsupported driver useful for adopting the shared test suite.
pub struct UnsupportedRecordingDriver {
    pub descriptor: agentdash_agent_runtime_contract::RuntimeDescriptor,
    side_effects: AtomicUsize,
}

impl UnsupportedRecordingDriver {
    pub fn new(descriptor: agentdash_agent_runtime_contract::RuntimeDescriptor) -> Self {
        Self {
            descriptor,
            side_effects: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl SideEffectProbe for UnsupportedRecordingDriver {
    async fn side_effect_count(&self) -> usize {
        self.side_effects.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AgentRuntimeDriver for UnsupportedRecordingDriver {
    async fn describe(
        &self,
        _request: agentdash_agent_runtime_contract::DriverDescribeRequest,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeDescriptor, DriverError> {
        Ok(self.descriptor.clone())
    }

    async fn bind(
        &self,
        _request: agentdash_agent_runtime_contract::DriverBindRequest,
    ) -> Result<agentdash_agent_runtime_contract::DriverBinding, DriverError> {
        Err(DriverError::Unsupported {
            reason: "binding is unsupported".to_string(),
        })
    }

    async fn dispatch(
        &self,
        _command: DriverCommandEnvelope,
        _sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        Err(DriverError::Unsupported {
            reason: "command is unsupported".to_string(),
        })
    }

    async fn inspect(
        &self,
        _query: agentdash_agent_runtime_contract::DriverInspectionQuery,
    ) -> Result<agentdash_agent_runtime_contract::DriverInspection, DriverError> {
        Err(DriverError::Unsupported {
            reason: "inspection is unsupported".to_string(),
        })
    }
}

pub fn set<T: Ord>(values: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    values.into_iter().collect()
}

#[cfg(test)]
mod tests;
