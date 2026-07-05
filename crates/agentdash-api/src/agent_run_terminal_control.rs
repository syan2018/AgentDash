use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunDeliveryStateRepos, AgentRunDeliveryStateService, AgentRunMailboxScheduleTrigger,
    AgentRunMailboxService, AgentRunTerminalTransitionInput, SessionControlService,
    SessionCoreService, SessionEventingService, SessionLaunchService,
};
use agentdash_application_runtime_session::session::SessionTerminalCallback;
use agentdash_application_runtime_session::session::SessionTerminalNotification;
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::backend::ProjectBackendAccessRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, AgentRunDeliveryBindingRepository,
    LifecycleAgentRepository, LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};

#[derive(Clone)]
pub(crate) struct AgentRunTerminalControlCallbackDeps {
    pub(crate) lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub(crate) lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub(crate) project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub(crate) agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub(crate) execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub(crate) delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub(crate) project_backend_access_repo: Arc<dyn ProjectBackendAccessRepository>,
    pub(crate) command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub(crate) mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
}

#[derive(Clone)]
pub(crate) struct AgentRunTerminalControlCallback {
    deps: AgentRunTerminalControlCallbackDeps,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

impl AgentRunTerminalControlCallback {
    pub(crate) fn new(
        deps: AgentRunTerminalControlCallbackDeps,
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

    fn service(&self) -> AgentRunMailboxService<'_> {
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

    async fn schedule_turn_boundary(
        &self,
        session_id: &str,
    ) -> Result<(), agentdash_application_agentrun::WorkflowApplicationError> {
        let Some(anchor) = self
            .deps
            .execution_anchor_repo
            .find_by_session(session_id)
            .await?
        else {
            return Ok(());
        };
        let _ = self
            .service()
            .schedule(
                anchor.run_id,
                anchor.agent_id,
                session_id,
                AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary,
                None,
            )
            .await?;
        Ok(())
    }

    async fn sync_terminal_delivery_state(
        &self,
        notification: &SessionTerminalNotification,
    ) -> Result<(), agentdash_application_agentrun::WorkflowApplicationError> {
        AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: self.deps.execution_anchor_repo.as_ref(),
            delivery_binding_repo: self.deps.delivery_binding_repo.as_ref(),
        })
        .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
            runtime_session_id: notification.session_id.clone(),
            turn_id: notification.turn_id.clone(),
            terminal_state: notification.terminal_state.clone(),
            terminal_message: notification.terminal_message.clone(),
            observed_at: chrono::Utc::now(),
        })
        .await?;
        Ok(())
    }
}

#[async_trait]
impl SessionTerminalCallback for AgentRunTerminalControlCallback {
    async fn on_session_terminal(
        &self,
        notification: SessionTerminalNotification,
    ) -> Result<(), String> {
        self.sync_terminal_delivery_state(&notification)
            .await
            .map_err(|error| error.to_string())?;
        match notification.terminal_state.as_str() {
            "completed" => {
                if let Err(error) = self.schedule_turn_boundary(&notification.session_id).await {
                    diag!(Warn, Subsystem::Api,

                        runtime_session_id = %notification.session_id,
                        error = %error,
                        "AgentRun mailbox completed terminal fallback 调度失败"
                    );
                    return Err(error.to_string());
                }
            }
            "failed" => {
                if let Err(error) = self
                    .service()
                    .pause_for_terminal(
                        &notification.session_id,
                        "turn_failed",
                        Some("上一轮失败，mailbox 已暂停。".to_string()),
                    )
                    .await
                {
                    diag!(Warn, Subsystem::Api,

                        runtime_session_id = %notification.session_id,
                        error = %error,
                        "AgentRun mailbox failed pause 写入失败"
                    );
                    return Err(error.to_string());
                }
            }
            "interrupted" => {
                if let Err(error) = self
                    .service()
                    .pause_for_terminal(
                        &notification.session_id,
                        "turn_interrupted",
                        Some("上一轮已中断，mailbox 已暂停。".to_string()),
                    )
                    .await
                {
                    diag!(Warn, Subsystem::Api,

                        runtime_session_id = %notification.session_id,
                        error = %error,
                        "AgentRun mailbox interrupted pause 写入失败"
                    );
                    return Err(error.to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}
