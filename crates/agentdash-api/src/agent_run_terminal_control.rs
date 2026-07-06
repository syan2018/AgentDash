use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use std::sync::Arc;

use agentdash_application::companion::{
    AgentRunCompanionMailboxDelivery, CompanionGateControlRepos, CompanionGateControlService,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::SessionEventingService as ApplicationSessionEventingService;
use agentdash_application_agentrun::agent_run::{
    AgentRunDeliveryTerminalEvent, AgentRunRuntimeTerminalCommand, AgentRunTerminalConvergenceDeps,
    AgentRunTerminalConvergenceService, SessionControlService, SessionCoreService,
    SessionEventingService as AgentRunSessionEventingService, SessionLaunchService,
};
use agentdash_application_runtime_session::session::{
    SessionTerminalCallback, SessionTerminalNotification,
};
use agentdash_application_workflow::gate::WaitProducerTerminalEvent;
use agentdash_domain::workflow::WaitProducerRef;

#[derive(Clone)]
pub(crate) struct AgentRunTerminalControlCallbackDeps {
    pub(crate) repos: RepositorySet,
}

#[derive(Clone)]
pub(crate) struct AgentRunTerminalControlCallback {
    deps: AgentRunTerminalControlCallbackDeps,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    agent_run_eventing: AgentRunSessionEventingService,
    companion_eventing: ApplicationSessionEventingService,
    session_launch: SessionLaunchService,
}

impl AgentRunTerminalControlCallback {
    pub(crate) fn new(
        deps: AgentRunTerminalControlCallbackDeps,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        agent_run_eventing: AgentRunSessionEventingService,
        companion_eventing: ApplicationSessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            deps,
            session_core,
            session_control,
            agent_run_eventing,
            companion_eventing,
            session_launch,
        }
    }

    async fn converge_terminal(
        &self,
        notification: &SessionTerminalNotification,
    ) -> Result<
        Option<AgentRunDeliveryTerminalEvent>,
        agentdash_application_agentrun::WorkflowApplicationError,
    > {
        AgentRunTerminalConvergenceService::new(
            AgentRunTerminalConvergenceDeps {
                lifecycle_run_repo: self.deps.repos.lifecycle_run_repo.clone(),
                lifecycle_agent_repo: self.deps.repos.lifecycle_agent_repo.clone(),
                project_agent_repo: self.deps.repos.project_agent_repo.clone(),
                agent_frame_repo: self.deps.repos.agent_frame_repo.clone(),
                execution_anchor_repo: self.deps.repos.execution_anchor_repo.clone(),
                delivery_binding_repo: self.deps.repos.agent_run_delivery_binding_repo.clone(),
                project_backend_access_repo: self.deps.repos.project_backend_access_repo.clone(),
                command_receipt_repo: self.deps.repos.agent_run_command_receipt_repo.clone(),
                mailbox_repo: self.deps.repos.agent_run_mailbox_repo.clone(),
            },
            self.session_core.clone(),
            self.session_control.clone(),
            self.agent_run_eventing.clone(),
            self.session_launch.clone(),
        )
        .converge_runtime_terminal(AgentRunRuntimeTerminalCommand {
            runtime_session_id: notification.session_id.clone(),
            turn_id: notification.turn_id.clone(),
            terminal_state: notification.terminal_state.clone(),
            terminal_message: notification.terminal_message.clone(),
            observed_at: chrono::Utc::now(),
        })
        .await
    }

    async fn converge_wait_obligation_terminal(
        &self,
        event: AgentRunDeliveryTerminalEvent,
    ) -> Result<(), agentdash_application::ApplicationError> {
        let service = CompanionGateControlService::with_session_eventing(
            CompanionGateControlRepos {
                gate_repo: self.deps.repos.lifecycle_gate_repo.clone(),
                run_repo: self.deps.repos.lifecycle_run_repo.clone(),
                agent_repo: self.deps.repos.lifecycle_agent_repo.clone(),
                frame_repo: self.deps.repos.agent_frame_repo.clone(),
                anchor_repo: self.deps.repos.execution_anchor_repo.clone(),
                delivery_binding_repo: self.deps.repos.agent_run_delivery_binding_repo.clone(),
                lineage_repo: self.deps.repos.agent_lineage_repo.clone(),
            },
            self.companion_eventing.clone(),
        )
        .with_parent_mailbox_delivery(Arc::new(
            AgentRunCompanionMailboxDelivery::from_runtime_services(
                self.deps.repos.clone(),
                self.session_core.clone(),
                self.session_control.clone(),
                self.agent_run_eventing.clone(),
                self.session_launch.clone(),
            ),
        ));

        service
            .observe_wait_producer_terminal(wait_producer_terminal_event_from_agent_run(event))
            .await?;
        Ok(())
    }
}

fn wait_producer_terminal_event_from_agent_run(
    event: AgentRunDeliveryTerminalEvent,
) -> WaitProducerTerminalEvent {
    WaitProducerTerminalEvent {
        producer: WaitProducerRef::AgentRunDelivery {
            run_id: event.run_id,
            agent_id: event.agent_id,
            frame_id: event.frame_id,
        },
        terminal_state: event.terminal_state,
        terminal_message: event.terminal_message,
        source_turn_id: event.turn_id,
        trace_ref: event.delivery_trace_ref,
    }
}

#[async_trait]
impl SessionTerminalCallback for AgentRunTerminalControlCallback {
    async fn on_session_terminal(
        &self,
        notification: SessionTerminalNotification,
    ) -> Result<(), String> {
        let event = self
            .converge_terminal(&notification)
            .await
            .map_err(|error| error.to_string())?;

        if let Some(event) = event
            && let Err(error) = self.converge_wait_obligation_terminal(event).await
        {
            diag!(
                Warn,
                Subsystem::Api,
                runtime_session_id = %notification.session_id,
                terminal_state = %notification.terminal_state,
                error = %error,
                "AgentRun wait obligation terminal convergence 失败"
            );
            return Err(error.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn maps_agent_run_delivery_terminal_to_wait_producer_terminal_event() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();

        let event = wait_producer_terminal_event_from_agent_run(AgentRunDeliveryTerminalEvent {
            run_id,
            agent_id,
            frame_id: Some(frame_id),
            terminal_state: "failed".to_string(),
            terminal_message: Some("provider rejected model".to_string()),
            turn_id: Some("turn-42".to_string()),
            delivery_trace_ref: Some("delivery:trace".to_string()),
        });

        assert_eq!(
            event.producer,
            WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id,
                frame_id: Some(frame_id),
            }
        );
        assert_eq!(event.terminal_state, "failed");
        assert_eq!(
            event.terminal_message.as_deref(),
            Some("provider rejected model")
        );
        assert_eq!(event.source_turn_id.as_deref(), Some("turn-42"));
        assert_eq!(event.trace_ref.as_deref(), Some("delivery:trace"));
    }
}
