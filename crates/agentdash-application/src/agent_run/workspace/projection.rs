use crate::session::SessionExecutionState;

use super::types::{
    AgentRunWorkspaceProjectionInput, AgentRunWorkspaceProjectionModel,
    AgentRunWorkspaceRuntimeCommandStateModel, AgentRunWorkspaceRuntimeCommandStatus,
    AgentRunWorkspaceStateCode,
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

    pub fn runtime_command_state(
        execution_state: &SessionExecutionState,
    ) -> AgentRunWorkspaceRuntimeCommandStateModel {
        runtime_command_state(execution_state)
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
        | SessionExecutionState::Interrupted { turn_id, .. } => turn_id.clone(),
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
        SessionExecutionState::Idle if is_terminal_agent_status(agent_status) => {
            agent_status.to_string()
        }
        SessionExecutionState::Idle => "idle".to_string(),
    }
}

fn runtime_command_state(
    execution_state: &SessionExecutionState,
) -> AgentRunWorkspaceRuntimeCommandStateModel {
    match execution_state {
        SessionExecutionState::Idle => AgentRunWorkspaceRuntimeCommandStateModel {
            status: AgentRunWorkspaceRuntimeCommandStatus::Idle,
            turn_id: None,
            message: None,
        },
        SessionExecutionState::Running { turn_id } => AgentRunWorkspaceRuntimeCommandStateModel {
            status: AgentRunWorkspaceRuntimeCommandStatus::Running,
            turn_id: turn_id.clone(),
            message: None,
        },
        SessionExecutionState::Cancelling { turn_id } => {
            AgentRunWorkspaceRuntimeCommandStateModel {
                status: AgentRunWorkspaceRuntimeCommandStatus::Cancelling,
                turn_id: turn_id.clone(),
                message: Some("当前执行正在取消中。".to_string()),
            }
        }
        SessionExecutionState::Completed { turn_id } => AgentRunWorkspaceRuntimeCommandStateModel {
            status: AgentRunWorkspaceRuntimeCommandStatus::Completed,
            turn_id: Some(turn_id.clone()),
            message: None,
        },
        SessionExecutionState::Failed { turn_id, message } => {
            AgentRunWorkspaceRuntimeCommandStateModel {
                status: AgentRunWorkspaceRuntimeCommandStatus::Failed,
                turn_id: Some(turn_id.clone()),
                message: message.clone(),
            }
        }
        SessionExecutionState::Interrupted { turn_id, message } => {
            AgentRunWorkspaceRuntimeCommandStateModel {
                status: AgentRunWorkspaceRuntimeCommandStatus::Interrupted,
                turn_id: turn_id.clone(),
                message: message.clone(),
            }
        }
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
        assert_eq!(
            AgentRunWorkspaceProjection::runtime_command_state(&SessionExecutionState::Idle).status,
            AgentRunWorkspaceRuntimeCommandStatus::Idle
        );
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
        assert_eq!(
            AgentRunWorkspaceProjection::runtime_command_state(&SessionExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            })
            .status,
            AgentRunWorkspaceRuntimeCommandStatus::Running
        );
        assert_eq!(
            AgentRunWorkspaceProjection::runtime_command_state(&SessionExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            })
            .turn_id
            .as_deref(),
            Some("turn-1")
        );
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
        let command_state = AgentRunWorkspaceProjection::runtime_command_state(
            &SessionExecutionState::Cancelling {
                turn_id: Some("turn-1".to_string()),
            },
        );
        assert_eq!(
            command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Cancelling
        );
        assert_eq!(
            command_state.message.as_deref(),
            Some("当前执行正在取消中。")
        );
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
        assert_eq!(
            AgentRunWorkspaceProjection::runtime_command_state(&SessionExecutionState::Completed {
                turn_id: "turn-1".to_string(),
            })
            .status,
            AgentRunWorkspaceRuntimeCommandStatus::Completed
        );
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
        let command_state =
            AgentRunWorkspaceProjection::runtime_command_state(&SessionExecutionState::Failed {
                turn_id: "turn-1".to_string(),
                message: Some("provider failed".to_string()),
            });
        assert_eq!(
            command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Failed
        );
        assert_eq!(command_state.message.as_deref(), Some("provider failed"));
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
        let command_state = AgentRunWorkspaceProjection::runtime_command_state(
            &SessionExecutionState::Interrupted {
                turn_id: Some("turn-1".to_string()),
                message: Some("user interrupted".to_string()),
            },
        );
        assert_eq!(
            command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Interrupted
        );
        assert_eq!(command_state.message.as_deref(), Some("user interrupted"));
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
