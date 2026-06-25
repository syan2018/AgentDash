use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, UserInputBlock, UserInputSubmissionKind};
use agentdash_application_agentrun::WorkflowApplicationError;
use agentdash_application_agentrun::agent_run as agent_run_boundary;
use agentdash_application_runtime_session::session as runtime_session;
use agentdash_spi::ConnectorError;
use async_trait::async_trait;

pub fn agent_run_session_core(
    service: runtime_session::SessionCoreService,
) -> agent_run_boundary::SessionCoreService {
    agent_run_boundary::SessionCoreService::new(Arc::new(SessionCoreBridge { service }))
}

pub fn agent_run_session_control(
    service: runtime_session::SessionControlService,
) -> agent_run_boundary::SessionControlService {
    agent_run_boundary::SessionControlService::new(Arc::new(SessionControlBridge { service }))
}

pub fn agent_run_session_eventing(
    service: runtime_session::SessionEventingService,
) -> agent_run_boundary::SessionEventingService {
    agent_run_boundary::SessionEventingService::new(Arc::new(SessionEventingBridge { service }))
}

pub fn agent_run_session_launch(
    service: runtime_session::SessionLaunchService,
) -> agent_run_boundary::SessionLaunchService {
    agent_run_boundary::SessionLaunchService::new(Arc::new(SessionLaunchBridge { service }))
}

struct SessionCoreBridge {
    service: runtime_session::SessionCoreService,
}

#[async_trait]
impl agent_run_boundary::RuntimeSessionCorePort for SessionCoreBridge {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<agent_run_boundary::SessionExecutionState, WorkflowApplicationError> {
        self.service
            .inspect_session_execution_state(session_id)
            .await
            .map(map_execution_state)
            .map_err(WorkflowApplicationError::from)
    }

    async fn get_session_meta(
        &self,
        session_id: &str,
    ) -> Result<Option<agent_run_boundary::SessionMeta>, WorkflowApplicationError> {
        self.service
            .get_session_meta(session_id)
            .await
            .map_err(WorkflowApplicationError::from)
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), WorkflowApplicationError> {
        self.service
            .delete_session(session_id)
            .await
            .map_err(WorkflowApplicationError::from)
    }
}

fn map_execution_state(
    state: runtime_session::SessionExecutionState,
) -> agent_run_boundary::SessionExecutionState {
    match state {
        runtime_session::SessionExecutionState::Idle => {
            agent_run_boundary::SessionExecutionState::Idle
        }
        runtime_session::SessionExecutionState::Running { turn_id } => {
            agent_run_boundary::SessionExecutionState::Running { turn_id }
        }
        runtime_session::SessionExecutionState::Cancelling { turn_id } => {
            agent_run_boundary::SessionExecutionState::Cancelling { turn_id }
        }
        runtime_session::SessionExecutionState::Completed { turn_id } => {
            agent_run_boundary::SessionExecutionState::Completed { turn_id }
        }
        runtime_session::SessionExecutionState::Failed { turn_id, message } => {
            agent_run_boundary::SessionExecutionState::Failed { turn_id, message }
        }
        runtime_session::SessionExecutionState::Interrupted { turn_id, message } => {
            agent_run_boundary::SessionExecutionState::Interrupted { turn_id, message }
        }
        runtime_session::SessionExecutionState::Lost { turn_id, message } => {
            agent_run_boundary::SessionExecutionState::Lost { turn_id, message }
        }
    }
}

struct SessionControlBridge {
    service: runtime_session::SessionControlService,
}

#[async_trait]
impl agent_run_boundary::RuntimeSessionControlPort for SessionControlBridge {
    async fn supports_session_steering(&self, session_id: &str) -> bool {
        self.service.supports_session_steering(session_id).await
    }

    async fn steer_session(
        &self,
        command: agent_run_boundary::SessionTurnSteerCommand,
    ) -> Result<(), ConnectorError> {
        self.service
            .steer_session(runtime_session::SessionTurnSteerCommand {
                session_id: command.session_id,
                expected_turn_id: command.expected_turn_id,
                input: command.input,
            })
            .await
    }
}

struct SessionEventingBridge {
    service: runtime_session::SessionEventingService,
}

#[async_trait]
impl agent_run_boundary::RuntimeSessionEventingPort for SessionEventingBridge {
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> std::io::Result<agent_run_boundary::SessionEventPage> {
        self.service
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> Result<(), WorkflowApplicationError> {
        self.service
            .persist_notification(session_id, envelope)
            .await
            .map(|_| ())
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))
    }

    async fn emit_user_input_submitted(
        &self,
        session_id: &str,
        turn_id: &str,
        event_id: &str,
        kind: UserInputSubmissionKind,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError> {
        self.service
            .emit_user_input_submitted(session_id, turn_id, event_id, kind, input)
            .await
            .map(|_| ())
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))
    }
}

struct SessionLaunchBridge {
    service: runtime_session::SessionLaunchService,
}

#[async_trait]
impl agent_run_boundary::RuntimeSessionLaunchPort for SessionLaunchBridge {
    async fn launch_command_in_task(
        &self,
        session_id: String,
        command: agent_run_boundary::LaunchCommand,
    ) -> Result<String, WorkflowApplicationError> {
        let command = runtime_launch_command(command)?;
        self.service
            .launch_command_in_task(session_id, command)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))
    }
}

fn runtime_launch_command(
    command: agent_run_boundary::LaunchCommand,
) -> Result<runtime_session::LaunchCommand, WorkflowApplicationError> {
    let backend_selection = command
        .user_input()
        .backend_selection
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApplicationError::BadRequest(format!("backend_selection 格式无效: {error}"))
        })?;
    let input = runtime_session::UserPromptInput {
        input: command.user_input().input.clone(),
        env: command.user_input().env.clone(),
        executor_config: command.user_input().executor_config.clone(),
        backend_selection,
    };
    let mut runtime_command = match command.source() {
        agent_run_boundary::LaunchSource::HttpPrompt => {
            runtime_session::LaunchCommand::http_prompt_input(input, command.identity())
        }
        agent_run_boundary::LaunchSource::LifecycleAgentUserMessage => {
            runtime_session::LaunchCommand::lifecycle_agent_user_message_input(
                input,
                command.identity(),
            )
        }
        agent_run_boundary::LaunchSource::HookAutoResume => {
            runtime_session::LaunchCommand::hook_auto_resume_input(input)
        }
        agent_run_boundary::LaunchSource::CompanionDispatch => {
            let companion = command.companion_modifier().ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "CompanionDispatch 缺少 companion launch modifier".to_string(),
                )
            })?;
            runtime_session::LaunchCommand::companion_dispatch_input(
                input,
                command.identity(),
                companion,
            )
        }
        agent_run_boundary::LaunchSource::CompanionParentResume => {
            runtime_session::LaunchCommand::companion_parent_resume_input(input)
        }
        agent_run_boundary::LaunchSource::WorkflowOrchestrator => {
            runtime_session::LaunchCommand::workflow_orchestrator_input(input)
        }
        agent_run_boundary::LaunchSource::RoutineExecutor => {
            let routine = command.routine_modifier().ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "RoutineExecutor 缺少 routine launch modifier".to_string(),
                )
            })?;
            runtime_session::LaunchCommand::routine_executor_input(
                input,
                command.identity(),
                routine,
            )
        }
        agent_run_boundary::LaunchSource::LocalRelayPrompt => {
            let local_relay = command.local_relay_modifier().ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "LocalRelayPrompt 缺少 local relay launch modifier".to_string(),
                )
            })?;
            runtime_session::LaunchCommand::local_relay_prompt_input(
                input,
                local_relay.mcp_servers.clone(),
                local_relay.workspace_root.clone(),
            )
        }
    };
    runtime_command =
        runtime_command.with_follow_up(command.follow_up_session_id().map(ToString::to_string));
    Ok(runtime_command)
}
