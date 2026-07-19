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
use async_trait::async_trait;

use super::LifecycleOrchestrator;

pub struct LifecycleRuntimeTurnTerminalObserver {
    product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    orchestrator: Arc<LifecycleOrchestrator>,
}

impl LifecycleRuntimeTurnTerminalObserver {
    pub fn new(
        product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        orchestrator: Arc<LifecycleOrchestrator>,
    ) -> Self {
        Self {
            product_projection,
            orchestrator,
        }
    }
}

#[async_trait]
impl AgentRunProductRuntimeChangeObserver for LifecycleRuntimeTurnTerminalObserver {
    fn consumer_name(&self) -> &'static str {
        "lifecycle_runtime_turn_terminal"
    }

    async fn observe_product_runtime_change(
        &self,
        input: &AgentRunProductRuntimeChange,
    ) -> Result<AgentRunProductRuntimeChangeOutcome, String> {
        if !change_can_contain_terminal_turn(&input.change.delta) {
            return Ok(AgentRunProductRuntimeChangeOutcome::Ignored);
        }

        let observation = self
            .product_projection
            .runtime_snapshot_observation(&input.binding.target)
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = match observation {
            AgentRunProductRuntimeSnapshotObservation::Current {
                product_binding,
                snapshot,
            } if product_binding == input.binding => snapshot,
            AgentRunProductRuntimeSnapshotObservation::Current { .. } => {
                return Err(
                    "Product Runtime binding changed while delivering terminal turn".into(),
                );
            }
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => {
                return Err("Product Runtime snapshot is absent for terminal turn".into());
            }
            AgentRunProductRuntimeSnapshotObservation::Stale(_) => {
                return Err("Product Runtime snapshot is stale for terminal turn".into());
            }
        };
        if snapshot.thread_id != input.change.thread_id
            || snapshot.revision < input.change.revision
            || snapshot.latest_change_sequence < input.change.sequence
        {
            return Err("Product Runtime snapshot is behind terminal turn change".into());
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
            applied |= self
                .orchestrator
                .converge_runtime_turn_terminal(&input.binding, &snapshot, &turn_id, status)
                .await?;
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
