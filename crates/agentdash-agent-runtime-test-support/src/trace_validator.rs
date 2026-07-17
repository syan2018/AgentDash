use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    RuntimeEvent, RuntimeEventEnvelope, RuntimeInteractionId, RuntimeItemId, RuntimeOperationId,
    RuntimeThreadId, RuntimeTurnId,
};
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
            RuntimeEvent::OperationAccepted { operation_id } => {
                if self
                    .operations
                    .insert(
                        operation_id.clone(),
                        ScopedPhase {
                            phase: Phase::Active,
                            thread_id: envelope.thread_id.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(ConformanceViolation::DuplicateOperationAcceptance(
                        operation_id.clone(),
                    ));
                }
            }
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
            RuntimeEvent::TurnStarted { turn_id, .. } => {
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
            RuntimeEvent::ConversationDelta {
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

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_contract::{
        RuntimeEvent, RuntimeEventEnvelope, RuntimeOperationTerminal, RuntimeRevision,
    };

    use super::*;

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid test id")
    }

    fn envelope(event: RuntimeEvent) -> RuntimeEventEnvelope {
        RuntimeEventEnvelope {
            thread_id: id("thread-1"),
            occurred_at_ms: 1,
            sequence: None,
            transient: None,
            revision: RuntimeRevision(1),
            event,
        }
    }

    #[test]
    fn accepts_a_terminal_operation_trace() {
        let operation_id = id("operation-1");
        let mut validator = RuntimeTraceValidator::default();
        validator
            .observe(&envelope(RuntimeEvent::OperationAccepted {
                operation_id: operation_id.clone(),
            }))
            .expect("accepted operation");
        validator
            .observe(&envelope(RuntimeEvent::OperationTerminal {
                operation_id,
                terminal: RuntimeOperationTerminal::Succeeded,
            }))
            .expect("terminal operation");
        validator.finish().expect("complete trace");
    }

    #[test]
    fn rejects_a_trace_with_an_active_operation() {
        let mut validator = RuntimeTraceValidator::default();
        validator
            .observe(&envelope(RuntimeEvent::OperationAccepted {
                operation_id: id("operation-active"),
            }))
            .expect("accepted operation");
        assert_eq!(
            validator.finish(),
            Err(ConformanceViolation::MissingTerminal)
        );
    }
}
