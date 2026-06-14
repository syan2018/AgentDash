use crate::session::SessionExecutionState;

use super::types::{
    AgentRunWorkspaceActionAvailabilityModel, AgentRunWorkspaceActionSetModel,
    AgentRunWorkspaceControlPlaneModel, AgentRunWorkspaceControlPlaneStatus,
    AgentRunWorkspaceProjectionInput, AgentRunWorkspaceProjectionModel,
    AgentRunWorkspaceRuntimeCommandStateModel, AgentRunWorkspaceRuntimeCommandStatus,
    AgentRunWorkspaceStateCode,
};

pub struct AgentRunWorkspaceProjection;

impl AgentRunWorkspaceProjection {
    pub fn derive(input: AgentRunWorkspaceProjectionInput<'_>) -> AgentRunWorkspaceProjectionModel {
        let terminal_agent = is_terminal_agent_status(input.agent_status);
        let state_code = state_code(input.execution_state);
        let active_turn_id = active_turn_id(input.execution_state);
        let last_turn_id = last_turn_id(input.execution_state);
        let delivery_status = delivery_status(input.execution_state, input.agent_status);
        let control_plane = control_plane(input, terminal_agent);
        let actions = actions(input, terminal_agent);
        let runtime_command_state = runtime_command_state(input.execution_state);
        let replacement_command = replacement_command(terminal_agent);

        AgentRunWorkspaceProjectionModel {
            state_code,
            active_turn_id,
            last_turn_id,
            delivery_status,
            control_plane,
            actions,
            runtime_command_state,
            replacement_command,
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

fn control_plane(
    input: AgentRunWorkspaceProjectionInput<'_>,
    terminal_agent: bool,
) -> AgentRunWorkspaceControlPlaneModel {
    if terminal_agent {
        return AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::Terminal,
            reason: Some("当前 AgentRun 已结束。".to_string()),
        };
    }
    if !input.has_delivery_runtime {
        return AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::DeliveryMissing,
            reason: Some("当前 AgentRun 缺少可投递的 runtime 通道。".to_string()),
        };
    }
    if !input.has_frame {
        return AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::FrameMissing,
            reason: Some("当前 AgentRun 没有可投递的 runtime frame。".to_string()),
        };
    }
    match input.execution_state {
        SessionExecutionState::Cancelling { .. } => AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::Cancelling,
            reason: Some("当前 AgentRun 正在取消中，等待执行器收口。".to_string()),
        },
        SessionExecutionState::Running { turn_id } => AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::Running,
            reason: Some(if turn_id.is_none() {
                "当前 AgentRun 正在启动中，等待 active turn 建立。".to_string()
            } else {
                "当前 AgentRun 正在执行中。".to_string()
            }),
        },
        _ => AgentRunWorkspaceControlPlaneModel {
            status: AgentRunWorkspaceControlPlaneStatus::Ready,
            reason: None,
        },
    }
}

fn actions(
    input: AgentRunWorkspaceProjectionInput<'_>,
    terminal_agent: bool,
) -> AgentRunWorkspaceActionSetModel {
    let submit_message = if input.has_delivery_runtime && input.has_frame && !terminal_agent {
        AgentRunWorkspaceActionAvailabilityModel::enabled()
    } else if !input.has_delivery_runtime {
        AgentRunWorkspaceActionAvailabilityModel::disabled(
            "当前 AgentRun 缺少可投递的 runtime 通道。",
        )
    } else if terminal_agent {
        AgentRunWorkspaceActionAvailabilityModel::disabled(
            "当前 AgentRun 已结束，不能继续发送消息。",
        )
    } else {
        AgentRunWorkspaceActionAvailabilityModel::disabled(
            "当前 AgentRun 没有可投递的 runtime frame。",
        )
    };

    let cancel = match input.execution_state {
        SessionExecutionState::Running { .. } | SessionExecutionState::Cancelling { .. } => {
            AgentRunWorkspaceActionAvailabilityModel::enabled()
        }
        _ => AgentRunWorkspaceActionAvailabilityModel::disabled(
            "当前 AgentRun 没有正在执行的 turn。",
        ),
    };

    AgentRunWorkspaceActionSetModel {
        submit_message,
        cancel,
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

fn replacement_command(terminal_agent: bool) -> Option<String> {
    if terminal_agent {
        None
    } else {
        Some("submit_message".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(execution_state: &SessionExecutionState) -> AgentRunWorkspaceProjectionModel {
        project_with(execution_state, "active", true, true)
    }

    fn project_with(
        execution_state: &SessionExecutionState,
        agent_status: &str,
        has_delivery_runtime: bool,
        has_frame: bool,
    ) -> AgentRunWorkspaceProjectionModel {
        AgentRunWorkspaceProjection::derive(AgentRunWorkspaceProjectionInput::new(
            execution_state,
            agent_status,
            has_delivery_runtime,
            has_frame,
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
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Ready
        );
        assert_eq!(model.control_plane.reason, None);
        assert!(model.actions.submit_message.enabled);
        assert!(!model.actions.cancel.enabled);
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Idle
        );
        assert_eq!(model.replacement_command.as_deref(), Some("submit_message"));
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
        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Running
        );
        assert_eq!(
            model.control_plane.reason.as_deref(),
            Some("当前 AgentRun 正在启动中，等待 active turn 建立。")
        );
        assert!(model.actions.submit_message.enabled);
        assert!(model.actions.cancel.enabled);
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Running
        );
        assert_eq!(model.runtime_command_state.turn_id, None);
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
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Running
        );
        assert_eq!(
            model.control_plane.reason.as_deref(),
            Some("当前 AgentRun 正在执行中。")
        );
        assert!(model.actions.submit_message.enabled);
        assert!(model.actions.cancel.enabled);
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Running
        );
        assert_eq!(
            model.runtime_command_state.turn_id.as_deref(),
            Some("turn-1")
        );
    }

    #[test]
    fn cancelling_keeps_active_turn_and_cancel_available() {
        let model = project(&SessionExecutionState::Cancelling {
            turn_id: Some("turn-1".to_string()),
        });

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Cancelling);
        assert_eq!(model.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(model.delivery_status, "cancelling");
        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Cancelling
        );
        assert!(model.actions.submit_message.enabled);
        assert!(model.actions.cancel.enabled);
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Cancelling
        );
        assert_eq!(
            model.runtime_command_state.message.as_deref(),
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
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Ready
        );
        assert!(model.actions.submit_message.enabled);
        assert!(!model.actions.cancel.enabled);
        assert_eq!(
            model.runtime_command_state.status,
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
        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Ready
        );
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Failed
        );
        assert_eq!(
            model.runtime_command_state.message.as_deref(),
            Some("provider failed")
        );
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
        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Ready
        );
        assert_eq!(
            model.runtime_command_state.status,
            AgentRunWorkspaceRuntimeCommandStatus::Interrupted
        );
        assert_eq!(
            model.runtime_command_state.message.as_deref(),
            Some("user interrupted")
        );
    }

    #[test]
    fn terminal_agent_disables_submit_and_replacement() {
        let model = project_with(&SessionExecutionState::Idle, "completed", true, true);

        assert_eq!(model.state_code, AgentRunWorkspaceStateCode::Ready);
        assert_eq!(model.delivery_status, "completed");
        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::Terminal
        );
        assert_eq!(
            model.control_plane.reason.as_deref(),
            Some("当前 AgentRun 已结束。")
        );
        assert!(!model.actions.submit_message.enabled);
        assert_eq!(
            model.actions.submit_message.unavailable_reason.as_deref(),
            Some("当前 AgentRun 已结束，不能继续发送消息。")
        );
        assert!(!model.actions.cancel.enabled);
        assert_eq!(model.replacement_command, None);
    }

    #[test]
    fn missing_delivery_runtime_blocks_control_and_submit() {
        let model = project_with(&SessionExecutionState::Idle, "active", false, true);

        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::DeliveryMissing
        );
        assert_eq!(
            model.control_plane.reason.as_deref(),
            Some("当前 AgentRun 缺少可投递的 runtime 通道。")
        );
        assert_eq!(model.delivery_status, "idle");
        assert!(!model.actions.submit_message.enabled);
        assert_eq!(
            model.actions.submit_message.unavailable_reason.as_deref(),
            Some("当前 AgentRun 缺少可投递的 runtime 通道。")
        );
        assert_eq!(model.replacement_command.as_deref(), Some("submit_message"));
    }

    #[test]
    fn missing_frame_blocks_control_and_submit() {
        let model = project_with(&SessionExecutionState::Idle, "active", true, false);

        assert_eq!(
            model.control_plane.status,
            AgentRunWorkspaceControlPlaneStatus::FrameMissing
        );
        assert_eq!(
            model.control_plane.reason.as_deref(),
            Some("当前 AgentRun 没有可投递的 runtime frame。")
        );
        assert_eq!(model.delivery_status, "idle");
        assert!(!model.actions.submit_message.enabled);
        assert_eq!(
            model.actions.submit_message.unavailable_reason.as_deref(),
            Some("当前 AgentRun 没有可投递的 runtime frame。")
        );
        assert_eq!(model.replacement_command.as_deref(), Some("submit_message"));
    }
}
