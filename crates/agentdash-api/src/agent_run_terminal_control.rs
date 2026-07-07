use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use std::sync::Arc;

use agentdash_application::companion::AgentRunCompanionMailboxDelivery;
use agentdash_application::gate_wait_policy::{
    CompanionGateMailboxWakeDelivery, GateProducerTerminalConvergencePort,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application_agentrun::agent_run::{
    SessionControlService, SessionCoreService,
    SessionEventingService as AgentRunSessionEventingService, SessionLaunchService,
};
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunLifecycleTerminalConvergencePort, AgentRunWaitProducerTerminalConvergencePort,
    AgentRunWaitProducerTerminalEvent,
};
use agentdash_application_workflow::gate::GateProducerTerminalEvent;
use agentdash_domain::workflow::WaitProducerRef;

#[derive(Clone)]
pub(crate) struct ApiWaitProducerTerminalConvergenceAdapter {
    repos: RepositorySet,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    agent_run_eventing: AgentRunSessionEventingService,
    session_launch: SessionLaunchService,
}

impl ApiWaitProducerTerminalConvergenceAdapter {
    pub(crate) fn new(
        repos: RepositorySet,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        agent_run_eventing: AgentRunSessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            repos,
            session_core,
            session_control,
            agent_run_eventing,
            session_launch,
        }
    }
}

#[async_trait]
impl AgentRunWaitProducerTerminalConvergencePort for ApiWaitProducerTerminalConvergenceAdapter {
    async fn observe_agent_run_wait_producer_terminal(
        &self,
        event: AgentRunWaitProducerTerminalEvent,
    ) -> Result<(), String> {
        let parent_mailbox_delivery =
            Arc::new(AgentRunCompanionMailboxDelivery::from_runtime_services(
                self.repos.clone(),
                self.session_core.clone(),
                self.session_control.clone(),
                self.agent_run_eventing.clone(),
                self.session_launch.clone(),
            ));
        let service =
            agentdash_application::gate_wait_policy::GateProducerTerminalConvergenceServiceAdapter::with_mailbox_wake_delivery(
                self.repos.lifecycle_gate_repo.clone(),
                self.repos.agent_run_delivery_binding_repo.clone(),
                Arc::new(CompanionGateMailboxWakeDelivery::new(parent_mailbox_delivery)),
            );

        service
            .observe_gate_producer_terminal(wait_producer_terminal_event_from_agent_run(event))
            .await
            .map_err(|error| {
                diag!(
                    Warn,
                    Subsystem::Api,
                    error = %error,
                    "AgentRun gate producer terminal fallback 失败"
                );
                error.to_string()
            })?;
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct ApiLifecycleTerminalConvergenceAdapter {
    inner: Arc<agentdash_application_lifecycle::LifecycleOrchestrator>,
}

impl ApiLifecycleTerminalConvergenceAdapter {
    pub(crate) fn new(inner: Arc<agentdash_application_lifecycle::LifecycleOrchestrator>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AgentRunLifecycleTerminalConvergencePort for ApiLifecycleTerminalConvergenceAdapter {
    async fn observe_lifecycle_terminal(
        &self,
        delivery_runtime_session_id: &str,
        terminal_state: &str,
    ) -> Result<(), String> {
        self.inner
            .on_session_terminal(delivery_runtime_session_id, terminal_state)
            .await
            .map(|_| ())
    }
}

fn wait_producer_terminal_event_from_agent_run(
    event: AgentRunWaitProducerTerminalEvent,
) -> GateProducerTerminalEvent {
    GateProducerTerminalEvent {
        producer: WaitProducerRef::AgentRunDelivery {
            run_id: event.run_id,
            agent_id: event.agent_id,
            frame_id: event.frame_id,
        },
        terminal_state: event.terminal_state,
        terminal_message: event.terminal_message,
        terminal_diagnostic: event.terminal_diagnostic,
        source_turn_id: event.source_turn_id,
        trace_ref: event.delivery_trace_ref,
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

        let event =
            wait_producer_terminal_event_from_agent_run(AgentRunWaitProducerTerminalEvent {
                run_id,
                agent_id,
                frame_id: Some(frame_id),
                terminal_state: "failed".to_string(),
                terminal_message: Some("provider rejected model".to_string()),
                terminal_diagnostic: None,
                source_turn_id: Some("turn-42".to_string()),
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
