use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimeEntityStatus, ManagedRuntimeLifecycleStatus,
    ManagedRuntimeSourceProjectionDelta, RuntimeTurnId,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionQueryPort, AgentRunProductRuntimeChange,
    AgentRunProductRuntimeChangeObserver, AgentRunProductRuntimeChangeOutcome,
    AgentRunProductRuntimeSnapshotObservation,
};
use agentdash_domain::routine::{RoutineExecutionRepository, RoutineExecutionStatus};
use async_trait::async_trait;

pub struct RoutineRuntimeTurnTerminalObserver {
    product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    executions: Arc<dyn RoutineExecutionRepository>,
}

impl RoutineRuntimeTurnTerminalObserver {
    pub fn new(
        product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        executions: Arc<dyn RoutineExecutionRepository>,
    ) -> Self {
        Self {
            product_projection,
            executions,
        }
    }
}

#[async_trait]
impl AgentRunProductRuntimeChangeObserver for RoutineRuntimeTurnTerminalObserver {
    fn consumer_name(&self) -> &'static str {
        "routine_runtime_turn_terminal"
    }

    async fn observe_product_runtime_change(
        &self,
        input: &AgentRunProductRuntimeChange,
    ) -> Result<AgentRunProductRuntimeChangeOutcome, String> {
        if !change_can_contain_terminal_turn(&input.change.delta) {
            return Ok(AgentRunProductRuntimeChangeOutcome::Ignored);
        }
        let snapshot = match self
            .product_projection
            .runtime_snapshot_observation(&input.binding.target)
            .await
            .map_err(|error| error.to_string())?
        {
            AgentRunProductRuntimeSnapshotObservation::Current {
                product_binding,
                snapshot,
            } if product_binding == input.binding => snapshot,
            AgentRunProductRuntimeSnapshotObservation::Current { .. } => {
                return Err("Routine terminal Product binding changed".to_string());
            }
            AgentRunProductRuntimeSnapshotObservation::Absent { .. }
            | AgentRunProductRuntimeSnapshotObservation::Stale(_) => {
                return Err("Routine terminal Product snapshot is unavailable".to_string());
            }
        };
        if snapshot.thread_id != input.change.thread_id
            || snapshot.revision < input.change.revision
            || snapshot.latest_change_sequence < input.change.sequence
        {
            return Err("Routine terminal Product snapshot is behind Runtime change".to_string());
        }

        let mut terminal_turns = terminal_turns_from_change(&input.change.delta);
        if matches!(
            input.change.delta,
            ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                lifecycle: ManagedRuntimeLifecycleStatus::Lost
            }
        ) {
            for turn in &snapshot.turns {
                if is_terminal(turn.status)
                    && !terminal_turns
                        .iter()
                        .any(|(turn_id, _)| turn_id == &turn.id)
                {
                    terminal_turns.push((turn.id.clone(), turn.status));
                }
            }
        }
        let mut applied = false;
        for (turn_id, status) in terminal_turns {
            let Some(current_turn) = snapshot.turns.iter().find(|turn| turn.id == turn_id) else {
                continue;
            };
            if current_turn.status != status || !is_terminal(status) {
                continue;
            }
            let operation_ids = snapshot
                .operations
                .iter()
                .filter(|operation| operation.turn_id.as_ref() == Some(&turn_id))
                .map(|operation| operation.id.to_string())
                .collect::<Vec<_>>();
            for operation_id in operation_ids {
                let Some(mut execution) = self
                    .executions
                    .find_by_runtime_operation_id(&operation_id)
                    .await
                    .map_err(|error| error.to_string())?
                else {
                    continue;
                };
                if execution.status != RoutineExecutionStatus::Dispatched {
                    continue;
                }
                let (terminal_status, detail) = match status {
                    ManagedRuntimeEntityStatus::Completed => {
                        (RoutineExecutionStatus::Completed, None)
                    }
                    ManagedRuntimeEntityStatus::Interrupted => (
                        RoutineExecutionStatus::Interrupted,
                        Some("AgentRun turn interrupted".to_string()),
                    ),
                    ManagedRuntimeEntityStatus::Failed | ManagedRuntimeEntityStatus::Lost => (
                        RoutineExecutionStatus::Failed,
                        Some(format!("AgentRun turn ended with {status:?}")),
                    ),
                    _ => continue,
                };
                execution.mark_terminal(terminal_status, detail);
                self.executions
                    .update(&execution)
                    .await
                    .map_err(|error| error.to_string())?;
                applied = true;
            }
        }
        Ok(if applied {
            AgentRunProductRuntimeChangeOutcome::Applied
        } else {
            AgentRunProductRuntimeChangeOutcome::Ignored
        })
    }
}

fn change_can_contain_terminal_turn(delta: &ManagedRuntimeChangeDelta) -> bool {
    matches!(
        delta,
        ManagedRuntimeChangeDelta::SourceProjectionChanged {
            delta: ManagedRuntimeSourceProjectionDelta::SnapshotReplaced { .. }
                | ManagedRuntimeSourceProjectionDelta::TurnsChanged { .. },
            ..
        } | ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
            lifecycle: ManagedRuntimeLifecycleStatus::Lost
        }
    )
}

fn terminal_turns_from_change(
    delta: &ManagedRuntimeChangeDelta,
) -> Vec<(RuntimeTurnId, ManagedRuntimeEntityStatus)> {
    let turns = match delta {
        ManagedRuntimeChangeDelta::SourceProjectionChanged {
            delta: ManagedRuntimeSourceProjectionDelta::SnapshotReplaced { turns, .. },
            ..
        }
        | ManagedRuntimeChangeDelta::SourceProjectionChanged {
            delta: ManagedRuntimeSourceProjectionDelta::TurnsChanged { turns },
            ..
        } => turns,
        _ => return Vec::new(),
    };
    turns
        .iter()
        .filter(|turn| is_terminal(turn.status))
        .map(|turn| (turn.id.clone(), turn.status))
        .collect()
}

fn is_terminal(status: ManagedRuntimeEntityStatus) -> bool {
    matches!(
        status,
        ManagedRuntimeEntityStatus::Completed
            | ManagedRuntimeEntityStatus::Failed
            | ManagedRuntimeEntityStatus::Interrupted
            | ManagedRuntimeEntityStatus::Lost
    )
}
