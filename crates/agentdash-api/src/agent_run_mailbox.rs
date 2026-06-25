use async_trait::async_trait;

use agentdash_application::repository_set::RepositorySet;
use agentdash_application_agentrun::agent_run::{
    AgentRunMailboxScheduleTrigger, AgentRunMailboxService,
};
use agentdash_application_lifecycle::WorkflowApplicationError;
use agentdash_application_runtime_session::session::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionLaunchService,
    SessionTerminalCallback,
};

#[derive(Clone)]
pub(crate) struct AgentRunMailboxTerminalCallback {
    repos: RepositorySet,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

impl AgentRunMailboxTerminalCallback {
    pub(crate) fn new(
        repos: RepositorySet,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            repos,
            session_core,
            session_control,
            session_eventing,
            session_launch,
        }
    }

    fn service(&self) -> AgentRunMailboxService<'_> {
        AgentRunMailboxService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.execution_anchor_repo.as_ref(),
            self.repos.agent_run_command_receipt_repo.as_ref(),
            self.repos.agent_run_mailbox_repo.as_ref(),
            self.session_core.clone(),
            self.session_control.clone(),
            self.session_eventing.clone(),
            self.session_launch.clone(),
        )
    }

    async fn schedule_turn_boundary(
        &self,
        session_id: &str,
    ) -> Result<(), WorkflowApplicationError> {
        let Some(anchor) = self
            .repos
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
}

#[async_trait]
impl SessionTerminalCallback for AgentRunMailboxTerminalCallback {
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str) {
        match terminal_state {
            "completed" => {
                if let Err(error) = self.schedule_turn_boundary(session_id).await {
                    tracing::warn!(
                        runtime_session_id = %session_id,
                        error = %error,
                        "AgentRun mailbox completed terminal fallback 调度失败"
                    );
                }
            }
            "failed" => {
                if let Err(error) = self
                    .service()
                    .pause_for_terminal(
                        session_id,
                        "turn_failed",
                        Some("上一轮失败，mailbox 已暂停。".to_string()),
                    )
                    .await
                {
                    tracing::warn!(
                        runtime_session_id = %session_id,
                        error = %error,
                        "AgentRun mailbox failed pause 写入失败"
                    );
                }
            }
            "interrupted" => {
                if let Err(error) = self
                    .service()
                    .pause_for_terminal(
                        session_id,
                        "turn_interrupted",
                        Some("上一轮已中断，mailbox 已暂停。".to_string()),
                    )
                    .await
                {
                    tracing::warn!(
                        runtime_session_id = %session_id,
                        error = %error,
                        "AgentRun mailbox interrupted pause 写入失败"
                    );
                }
            }
            _ => {}
        }
    }
}
