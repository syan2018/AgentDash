use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, UserInputBlock, UserInputSource, UserInputSubmissionKind,
};
use agentdash_application_agentrun::WorkflowApplicationError;
use agentdash_application_agentrun::agent_run as agent_run_boundary;
use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
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

pub fn agent_run_session_cancel_runtime(
    service: runtime_session::SessionRuntimeService,
) -> SessionCancelRuntimeBridge {
    SessionCancelRuntimeBridge { service }
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
        source: UserInputSource,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError> {
        self.service
            .emit_user_input_submitted(session_id, turn_id, event_id, kind, source, input)
            .await
            .map(|_| ())
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))
    }

    async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> std::io::Result<agent_run_boundary::RuntimeSessionEventSubscription> {
        let subscription = self.service.subscribe_after(session_id, after_seq).await?;
        Ok(agent_run_boundary::RuntimeSessionEventSubscription {
            snapshot_seq: subscription.snapshot_seq,
            backlog: subscription.backlog,
            ephemeral_backlog: subscription.ephemeral_backlog,
            rx: subscription.rx,
        })
    }

    fn ephemeral_epoch(&self) -> u64 {
        self.service.ephemeral_epoch()
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
        command: LaunchCommand,
        planning_input: LaunchPlanningInput,
    ) -> Result<String, WorkflowApplicationError> {
        self.service
            .launch_command_in_task(session_id, command, planning_input)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))
    }
}

pub struct SessionCancelRuntimeBridge {
    service: runtime_session::SessionRuntimeService,
}

#[async_trait]
impl agent_run_boundary::AgentRunCancelRuntimePort for SessionCancelRuntimeBridge {
    async fn cancel_runtime_session(&self, session_id: &str) -> Result<(), ConnectorError> {
        self.service.cancel(session_id).await
    }
}
