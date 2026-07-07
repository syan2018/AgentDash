use agentdash_agent_protocol::RuntimeTerminalDiagnostic;
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use std::sync::Arc;

use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::backend::ProjectBackendAccessRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, AgentRunDeliveryBinding,
    AgentRunDeliveryBindingRepository, DeliveryBindingStatus, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::{
    AgentRunDeliveryStateRepos, AgentRunDeliveryStateService, AgentRunMailboxScheduleTrigger,
    AgentRunMailboxService, AgentRunTerminalTransitionInput, SessionControlService,
    SessionCoreService, SessionEventingService, SessionLaunchService,
};

#[derive(Debug, Clone)]
pub struct AgentRunRuntimeTerminalCommand {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDeliveryTerminalEvent {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub turn_id: Option<String>,
    pub delivery_trace_ref: Option<String>,
}

#[derive(Clone)]
pub struct AgentRunTerminalConvergenceService {
    deps: AgentRunTerminalConvergenceDeps,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

#[derive(Clone)]
pub struct AgentRunTerminalConvergenceDeps {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub project_backend_access_repo: Arc<dyn ProjectBackendAccessRepository>,
    pub command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
    pub project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

impl AgentRunTerminalConvergenceService {
    pub fn new(
        deps: AgentRunTerminalConvergenceDeps,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            deps,
            session_core,
            session_control,
            session_eventing,
            session_launch,
        }
    }

    pub async fn converge_runtime_terminal(
        &self,
        command: AgentRunRuntimeTerminalCommand,
    ) -> Result<Option<AgentRunDeliveryTerminalEvent>, WorkflowApplicationError> {
        let Some(anchor) = self
            .deps
            .execution_anchor_repo
            .find_by_session(&command.runtime_session_id)
            .await?
        else {
            return Ok(None);
        };

        let frame_id = self
            .resolve_current_or_launch_frame(anchor.agent_id, anchor.launch_frame_id)
            .await?;
        let transition = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: self.deps.execution_anchor_repo.as_ref(),
            delivery_binding_repo: self.deps.delivery_binding_repo.as_ref(),
            lifecycle_agent_repo: self.deps.lifecycle_agent_repo.as_ref(),
            project_projection_notifications: self.deps.project_projection_notifications.as_deref(),
        })
        .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
            runtime_session_id: command.runtime_session_id.clone(),
            turn_id: command.turn_id.clone(),
            frame_id,
            terminal_state: command.terminal_state.clone(),
            terminal_message: command.terminal_message.clone(),
            terminal_diagnostic: command.terminal_diagnostic.clone(),
            observed_at: command.observed_at,
        })
        .await?;
        let Some(transition) = transition else {
            return Ok(None);
        };

        self.apply_mailbox_terminal_effect(
            transition.run_id,
            transition.agent_id,
            &transition.delivery_runtime_session_id,
            &command.terminal_state,
        )
        .await?;

        Ok(Some(AgentRunDeliveryTerminalEvent {
            run_id: transition.run_id,
            agent_id: transition.agent_id,
            frame_id: transition.frame_id,
            terminal_state: command.terminal_state,
            terminal_message: command.terminal_message,
            terminal_diagnostic: command.terminal_diagnostic,
            turn_id: Some(command.turn_id),
            delivery_trace_ref: Some(command.runtime_session_id),
        }))
    }

    pub async fn converge_terminal_binding(
        &self,
        binding: AgentRunDeliveryBinding,
    ) -> Result<Option<AgentRunDeliveryTerminalEvent>, WorkflowApplicationError> {
        if binding.status != DeliveryBindingStatus::Terminal {
            return Ok(None);
        }
        let Some(terminal_state) = binding.terminal_state.clone() else {
            return Ok(None);
        };
        self.apply_mailbox_terminal_effect(
            binding.run_id,
            binding.agent_id,
            &binding.runtime_session_id,
            &terminal_state,
        )
        .await?;
        let frame_id = self
            .resolve_current_or_launch_frame(binding.agent_id, binding.launch_frame_id)
            .await?;
        let transition = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: self.deps.execution_anchor_repo.as_ref(),
            delivery_binding_repo: self.deps.delivery_binding_repo.as_ref(),
            lifecycle_agent_repo: self.deps.lifecycle_agent_repo.as_ref(),
            project_projection_notifications: self.deps.project_projection_notifications.as_deref(),
        })
        .publish_terminal_binding_invalidation(&binding, frame_id)
        .await;
        Ok(Some(AgentRunDeliveryTerminalEvent {
            run_id: transition.run_id,
            agent_id: transition.agent_id,
            frame_id: transition.frame_id,
            terminal_state,
            terminal_message: binding.terminal_message,
            terminal_diagnostic: binding
                .terminal_diagnostic
                .and_then(|value| serde_json::from_value(value).ok()),
            turn_id: binding.last_turn_id,
            delivery_trace_ref: Some(binding.runtime_session_id),
        }))
    }

    async fn apply_mailbox_terminal_effect(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        terminal_state: &str,
    ) -> Result<(), WorkflowApplicationError> {
        match terminal_state {
            "completed" => {
                let _ = self
                    .mailbox_service()
                    .schedule(
                        run_id,
                        agent_id,
                        runtime_session_id,
                        AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary,
                        None,
                    )
                    .await?;
            }
            "failed" => {
                self.deps
                    .mailbox_repo
                    .pause_state(
                        run_id,
                        agent_id,
                        Some(runtime_session_id.to_string()),
                        "turn_failed".to_string(),
                        Some("上一轮失败，mailbox 已暂停。".to_string()),
                    )
                    .await?;
            }
            "interrupted" => {
                self.deps
                    .mailbox_repo
                    .pause_state(
                        run_id,
                        agent_id,
                        Some(runtime_session_id.to_string()),
                        "turn_interrupted".to_string(),
                        Some("上一轮已中断，mailbox 已暂停。".to_string()),
                    )
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn resolve_current_or_launch_frame(
        &self,
        agent_id: Uuid,
        launch_frame_id: Uuid,
    ) -> Result<Option<Uuid>, WorkflowApplicationError> {
        if let Some(frame) = self.deps.agent_frame_repo.get_current(agent_id).await? {
            return Ok(Some(frame.id));
        }
        Ok(self
            .deps
            .agent_frame_repo
            .get(launch_frame_id)
            .await?
            .map(|frame| frame.id))
    }

    fn mailbox_service(&self) -> AgentRunMailboxService<'_> {
        AgentRunMailboxService::new(
            self.deps.lifecycle_run_repo.as_ref(),
            self.deps.lifecycle_agent_repo.as_ref(),
            self.deps.project_agent_repo.as_ref(),
            self.deps.agent_frame_repo.as_ref(),
            self.deps.execution_anchor_repo.as_ref(),
            self.deps.delivery_binding_repo.as_ref(),
            self.deps.project_backend_access_repo.as_ref(),
            self.deps.command_receipt_repo.as_ref(),
            self.deps.mailbox_repo.as_ref(),
            self.session_core.clone(),
            self.session_control.clone(),
            self.session_eventing.clone(),
            self.session_launch.clone(),
        )
    }
}
