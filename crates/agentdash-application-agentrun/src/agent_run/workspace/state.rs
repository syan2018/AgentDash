use crate::agent_run::AgentRunExecutionState;

use super::types::{AgentRunWorkspaceStateCode, AgentRunWorkspaceStateModel};

pub fn derive_workspace_state(
    execution_state: &AgentRunExecutionState,
    agent_status: &str,
) -> AgentRunWorkspaceStateModel {
    AgentRunWorkspaceStateModel {
        state_code: state_code(execution_state),
        active_turn_id: active_turn_id(execution_state),
        last_turn_id: last_turn_id(execution_state),
        delivery_status: delivery_status(execution_state, agent_status),
    }
}

pub fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn state_code(execution_state: &AgentRunExecutionState) -> AgentRunWorkspaceStateCode {
    match execution_state {
        AgentRunExecutionState::Idle => AgentRunWorkspaceStateCode::Ready,
        AgentRunExecutionState::Running { turn_id: None } => {
            AgentRunWorkspaceStateCode::StartingClaimed
        }
        AgentRunExecutionState::Running { turn_id: Some(_) } => {
            AgentRunWorkspaceStateCode::RunningActive
        }
        AgentRunExecutionState::Cancelling { .. } => AgentRunWorkspaceStateCode::Cancelling,
        AgentRunExecutionState::Completed { .. } => AgentRunWorkspaceStateCode::Completed,
        AgentRunExecutionState::Failed { .. } => AgentRunWorkspaceStateCode::Failed,
        AgentRunExecutionState::Interrupted { .. } => AgentRunWorkspaceStateCode::Interrupted,
        AgentRunExecutionState::Lost { .. } => AgentRunWorkspaceStateCode::Lost,
    }
}

fn active_turn_id(execution_state: &AgentRunExecutionState) -> Option<String> {
    match execution_state {
        AgentRunExecutionState::Running {
            turn_id: Some(turn_id),
        }
        | AgentRunExecutionState::Cancelling {
            turn_id: Some(turn_id),
        } => Some(turn_id.clone()),
        _ => None,
    }
}

fn last_turn_id(execution_state: &AgentRunExecutionState) -> Option<String> {
    match execution_state {
        AgentRunExecutionState::Running { turn_id }
        | AgentRunExecutionState::Cancelling { turn_id }
        | AgentRunExecutionState::Interrupted { turn_id, .. }
        | AgentRunExecutionState::Lost { turn_id, .. } => turn_id.clone(),
        AgentRunExecutionState::Completed { turn_id }
        | AgentRunExecutionState::Failed { turn_id, .. } => Some(turn_id.clone()),
        AgentRunExecutionState::Idle => None,
    }
}

fn delivery_status(execution_state: &AgentRunExecutionState, agent_status: &str) -> String {
    match execution_state {
        AgentRunExecutionState::Running { .. } => "running".to_string(),
        AgentRunExecutionState::Cancelling { .. } => "cancelling".to_string(),
        AgentRunExecutionState::Completed { .. } => "completed".to_string(),
        AgentRunExecutionState::Failed { .. } => "failed".to_string(),
        AgentRunExecutionState::Interrupted { .. } => "interrupted".to_string(),
        AgentRunExecutionState::Lost { .. } => "lost".to_string(),
        AgentRunExecutionState::Idle if is_terminal_agent_status(agent_status) => {
            agent_status.to_string()
        }
        AgentRunExecutionState::Idle => "idle".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(execution_state: &AgentRunExecutionState) -> AgentRunWorkspaceStateModel {
        state_with(execution_state, "active")
    }

    fn state_with(
        execution_state: &AgentRunExecutionState,
        agent_status: &str,
    ) -> AgentRunWorkspaceStateModel {
        derive_workspace_state(execution_state, agent_status)
    }

    #[test]
    fn idle_workspace_is_ready() {
        let model = state(&AgentRunExecutionState::Idle);

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Ready);
        assert_eq!(model.state_code.as_str(), "ready");
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id, None);
        assert_eq!(model.delivery_status, "idle");
    }

    #[test]
    fn running_without_turn_is_starting_claimed() {
        let model = state(&AgentRunExecutionState::Running { turn_id: None });

        assert_eq!(
            model.state_code,
            AgentRunWorkspaceStateCode::StartingClaimed
        );
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id, None);
        assert_eq!(model.delivery_status, "running");
    }

    #[test]
    fn running_with_turn_is_running_active() {
        let model = state(&AgentRunExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::RunningActive);
        assert_eq!(model.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "running");
    }

    #[test]
    fn cancelling_keeps_active_turn() {
        let model = state(&AgentRunExecutionState::Cancelling {
            turn_id: Some("turn-1".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Cancelling);
        assert_eq!(model.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "cancelling");
    }

    #[test]
    fn completed_turn_is_ready_with_last_turn() {
        let model = state(&AgentRunExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Completed);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "completed");
    }

    #[test]
    fn failed_turn_keeps_failure_message() {
        let model = state(&AgentRunExecutionState::Failed {
            turn_id: "turn-1".to_string(),
            message: Some("provider failed".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Failed);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "failed");
    }

    #[test]
    fn interrupted_turn_keeps_last_turn_without_active_turn() {
        let model = state(&AgentRunExecutionState::Interrupted {
            turn_id: Some("turn-1".to_string()),
            message: Some("user interrupted".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Interrupted);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "interrupted");
    }

    #[test]
    fn lost_turn_reports_lost_delivery_status() {
        let model = state(&AgentRunExecutionState::Lost {
            turn_id: Some("turn-1".to_string()),
            message: Some("backend disconnected".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Lost);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "lost");
    }

    #[test]
    fn terminal_agent_status_sets_delivery_status() {
        let model = state_with(&AgentRunExecutionState::Idle, "completed");

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Ready);
        assert_eq!(model.delivery_status, "completed");
    }

    #[test]
    fn active_agent_idle_state_reports_idle_delivery() {
        let model = state_with(&AgentRunExecutionState::Idle, "active");

        assert_eq!(model.delivery_status, "idle");
    }
}
