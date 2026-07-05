use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunMailboxScheduleTrigger, AgentRunMailboxService, SessionControlService,
    SessionCoreService, SessionEventingService, SessionLaunchService,
};
use agentdash_application_runtime_session::session::SessionTerminalCallback;
use agentdash_application_runtime_session::session::SessionTerminalNotification;
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::backend::ProjectBackendAccessRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, AgentRunDeliveryBinding,
    AgentRunDeliveryBindingRepository, DeliveryBindingStatus, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};

#[derive(Clone)]
pub(crate) struct AgentRunMailboxTerminalCallbackDeps {
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
pub(crate) struct AgentRunMailboxTerminalCallback {
    deps: AgentRunMailboxTerminalCallbackDeps,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

impl AgentRunMailboxTerminalCallback {
    pub(crate) fn new(
        deps: AgentRunMailboxTerminalCallbackDeps,
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

    async fn record_agent_run_terminal_state(
        &self,
        notification: &SessionTerminalNotification,
    ) -> Result<(), agentdash_application_agentrun::WorkflowApplicationError> {
        let Some(anchor) = self
            .deps
            .execution_anchor_repo
            .find_by_session(&notification.session_id)
            .await?
        else {
            return Ok(());
        };
        let observed_at = chrono::Utc::now();
        let binding = match self
            .deps
            .delivery_binding_repo
            .get_current(anchor.run_id, anchor.agent_id)
            .await?
        {
            Some(binding) => binding,
            None => AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Terminal,
                observed_at,
            ),
        }
        .mark_terminal(
            notification.turn_id.clone(),
            notification.terminal_state.clone(),
            notification.terminal_message.clone(),
            observed_at,
        );
        self.deps.delivery_binding_repo.upsert(&binding).await?;
        Ok(())
    }
}

#[async_trait]
impl SessionTerminalCallback for AgentRunMailboxTerminalCallback {
    async fn on_session_terminal(&self, notification: SessionTerminalNotification) {
        if let Err(error) = self.record_agent_run_terminal_state(&notification).await {
            diag!(Warn, Subsystem::Api,

                runtime_session_id = %notification.session_id,
                turn_id = %notification.turn_id,
                terminal_state = %notification.terminal_state,
                error = %error,
                "AgentRun terminal state 写入失败"
            );
        }
        match notification.terminal_state.as_str() {
            "completed" => {
                if let Err(error) = self.schedule_turn_boundary(&notification.session_id).await {
                    diag!(Warn, Subsystem::Api,

                        runtime_session_id = %notification.session_id,
                        error = %error,
                        "AgentRun mailbox completed terminal fallback 调度失败"
                    );
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
                }
            }
            _ => {}
        }
    }
}
