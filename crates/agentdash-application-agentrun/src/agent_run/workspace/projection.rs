use crate::agent_run::runtime_session_boundary::SessionExecutionState;

use super::types::{
    AgentRunWorkspaceProjectionInput, AgentRunWorkspaceProjectionModel, AgentRunWorkspaceStateCode,
};

pub struct AgentRunWorkspaceProjection;

impl AgentRunWorkspaceProjection {
    pub fn derive(input: AgentRunWorkspaceProjectionInput<'_>) -> AgentRunWorkspaceProjectionModel {
        let state_code = state_code(input.execution_state);
        let active_turn_id = active_turn_id(input.execution_state);
        let last_turn_id = last_turn_id(input.execution_state);
        let delivery_status = delivery_status(input.execution_state, input.agent_status);

        AgentRunWorkspaceProjectionModel {
            state_code,
            active_turn_id,
            last_turn_id,
            delivery_status,
        }
    }
}

pub fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn state_code(execution_state: &SessionExecutionState) -> AgentRunWorkspaceStateCode {
    match execution_state {
        SessionExecutionState::Idle => AgentRunWorkspaceStateCode::Ready,
        SessionExecutionState::Running { turn_id: None } => {
            AgentRunWorkspaceStateCode::StartingClaimed
        }
        SessionExecutionState::Running { turn_id: Some(_) } => {
            AgentRunWorkspaceStateCode::RunningActive
        }
        SessionExecutionState::Cancelling { .. } => AgentRunWorkspaceStateCode::Cancelling,
        SessionExecutionState::Completed { .. } => AgentRunWorkspaceStateCode::Completed,
        SessionExecutionState::Failed { .. } => AgentRunWorkspaceStateCode::Failed,
        SessionExecutionState::Interrupted { .. } => AgentRunWorkspaceStateCode::Interrupted,
        SessionExecutionState::Lost { .. } => AgentRunWorkspaceStateCode::Lost,
    }
}

fn active_turn_id(execution_state: &SessionExecutionState) -> Option<String> {
    match execution_state {
        SessionExecutionState::Running {
            turn_id: Some(turn_id),
        }
        | SessionExecutionState::Cancelling {
            turn_id: Some(turn_id),
        } => Some(turn_id.clone()),
        _ => None,
    }
}

fn last_turn_id(execution_state: &SessionExecutionState) -> Option<String> {
    match execution_state {
        SessionExecutionState::Running { turn_id }
        | SessionExecutionState::Cancelling { turn_id }
        | SessionExecutionState::Interrupted { turn_id, .. }
        | SessionExecutionState::Lost { turn_id, .. } => turn_id.clone(),
        SessionExecutionState::Completed { turn_id }
        | SessionExecutionState::Failed { turn_id, .. } => Some(turn_id.clone()),
        SessionExecutionState::Idle => None,
    }
}

fn delivery_status(execution_state: &SessionExecutionState, agent_status: &str) -> String {
    match execution_state {
        SessionExecutionState::Running { .. } => "running".to_string(),
        SessionExecutionState::Cancelling { .. } => "cancelling".to_string(),
        SessionExecutionState::Completed { .. } => "completed".to_string(),
        SessionExecutionState::Failed { .. } => "failed".to_string(),
        SessionExecutionState::Interrupted { .. } => "interrupted".to_string(),
        SessionExecutionState::Lost { .. } => "lost".to_string(),
        SessionExecutionState::Idle if is_terminal_agent_status(agent_status) => {
            agent_status.to_string()
        }
        SessionExecutionState::Idle => "idle".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(execution_state: &SessionExecutionState) -> AgentRunWorkspaceProjectionModel {
        project_with(execution_state, "active")
    }

    fn project_with(
        execution_state: &SessionExecutionState,
        agent_status: &str,
    ) -> AgentRunWorkspaceProjectionModel {
        AgentRunWorkspaceProjection::derive(AgentRunWorkspaceProjectionInput::new(
            execution_state,
            agent_status,
        ))
    }

    #[test]
    fn idle_workspace_is_ready() {
        let model = project(&SessionExecutionState::Idle);

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Ready);
        assert_eq!(model.state_code.as_str(), "ready");
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id, None);
        assert_eq!(model.delivery_status, "idle");
    }

    #[test]
    fn running_without_turn_is_starting_claimed() {
        let model = project(&SessionExecutionState::Running { turn_id: None });

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
        let model = project(&SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::RunningActive);
        assert_eq!(model.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "running");
    }

    #[test]
    fn cancelling_keeps_active_turn() {
        let model = project(&SessionExecutionState::Cancelling {
            turn_id: Some("turn-1".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Cancelling);
        assert_eq!(model.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "cancelling");
    }

    #[test]
    fn completed_turn_is_ready_with_last_turn() {
        let model = project(&SessionExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Completed);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "completed");
    }

    #[test]
    fn failed_turn_keeps_failure_message() {
        let model = project(&SessionExecutionState::Failed {
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
        let model = project(&SessionExecutionState::Interrupted {
            turn_id: Some("turn-1".to_string()),
            message: Some("user interrupted".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Interrupted);
        assert_eq!(model.active_turn_id, None);
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "interrupted");
    }

    #[test]
    fn lost_turn_projects_lost_delivery_status() {
        let model = project(&SessionExecutionState::Lost {
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
        let model = project_with(&SessionExecutionState::Idle, "completed");

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Ready);
        assert_eq!(model.delivery_status, "completed");
    }

    #[test]
    fn active_agent_idle_state_reports_idle_delivery() {
        let model = project_with(&SessionExecutionState::Idle, "active");

        assert_eq!(model.delivery_status, "idle");
    }
}
