use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use std::sync::Arc;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{AgentDashThreadItem, BackboneEvent};
use agentdash_application::companion::AgentRunCompanionMailboxDelivery;
use agentdash_application::gate_wait_policy::{
    CompanionGateMailboxWakeDelivery, GateProducerTerminalConvergencePort,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application_agentrun::agent_run::{
    AgentRunJournalEvent, AgentRunJournalQuery, AgentRunJournalService, SessionControlService,
    SessionCoreService, SessionEventingService as AgentRunSessionEventingService,
    SessionLaunchService, agent_run_journal_session_id,
};
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    SessionItemProjection, SessionItemView, filter_session_items, item_summary_for_view,
    session_item_projections,
};
use agentdash_application_lifecycle::surface::journey::session_items::SessionItemContent;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunLifecycleTerminalConvergencePort, AgentRunWaitProducerTerminalConvergencePort,
    AgentRunWaitProducerTerminalEvent, ProducerLastMessageEvidence,
};
use agentdash_application_runtime_session::session::SessionBranchingService;
use agentdash_application_workflow::gate::GateProducerTerminalEvent;
use agentdash_domain::workflow::WaitProducerRef;

const FALLBACK_MESSAGE_PREVIEW_LIMIT: usize = 2_000;

#[derive(Clone)]
pub(crate) struct ApiWaitProducerTerminalConvergenceAdapter {
    repos: RepositorySet,
    session_branching: SessionBranchingService,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    agent_run_eventing: AgentRunSessionEventingService,
    session_launch: SessionLaunchService,
}

impl ApiWaitProducerTerminalConvergenceAdapter {
    pub(crate) fn new(
        repos: RepositorySet,
        session_branching: SessionBranchingService,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        agent_run_eventing: AgentRunSessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            repos,
            session_branching,
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
        let mut gate_event = wait_producer_terminal_event_from_agent_run(event);
        if gate_event.producer_last_message.is_none() {
            gate_event.producer_last_message =
                self.producer_last_message_evidence(&gate_event).await;
        }

        service
            .observe_gate_producer_terminal(gate_event)
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

impl ApiWaitProducerTerminalConvergenceAdapter {
    async fn producer_last_message_evidence(
        &self,
        event: &GateProducerTerminalEvent,
    ) -> Option<ProducerLastMessageEvidence> {
        let WaitProducerRef::AgentRunDelivery {
            run_id, agent_id, ..
        } = &event.producer;
        let run_id = *run_id;
        let agent_id = *agent_id;
        let delivery_runtime_session_id = event.trace_ref.as_deref()?;
        let journal_service = AgentRunJournalService::new(
            self.session_branching.clone(),
            self.agent_run_eventing.clone(),
        );
        let journal = match journal_service
            .load_visible_journal(AgentRunJournalQuery {
                run_id,
                agent_id,
                delivery_runtime_session_id: Some(delivery_runtime_session_id.to_string()),
            })
            .await
        {
            Ok(journal) => journal,
            Err(error) => {
                diag!(
                    Warn,
                    Subsystem::Api,
                    run_id = %run_id,
                    agent_id = %agent_id,
                    delivery_runtime_session_id = %delivery_runtime_session_id,
                    error = %error,
                    "读取 AgentRun journal last message fallback 失败"
                );
                return None;
            }
        };
        let persisted_events = journal
            .events
            .iter()
            .map(|event| event.event.clone())
            .collect::<Vec<_>>();
        let projections = session_item_projections(&persisted_events);
        let messages = filter_session_items(&projections, SessionItemView::Messages);
        let selected = messages
            .iter()
            .filter(|projection| {
                matches!(
                    &projection.content,
                    SessionItemContent::Message { role: "agent", .. }
                )
            })
            .filter_map(|projection| {
                let event_ref =
                    last_journal_event_for_item(&journal.events, &projection.summary.item_id)?;
                Some((
                    projection,
                    event_ref.journal_seq,
                    event_ref.source_event_seq,
                ))
            })
            .max_by_key(|(_, journal_seq, _)| *journal_seq);
        let (projection, _, source_event_seq) = selected?;
        producer_last_message_from_projection(run_id, agent_id, projection, source_event_seq)
    }
}

fn producer_last_message_from_projection(
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    projection: &SessionItemProjection,
    source_event_seq: u64,
) -> Option<ProducerLastMessageEvidence> {
    let SessionItemContent::Message {
        role: "agent",
        text,
        ..
    } = &projection.content
    else {
        return None;
    };
    let summary = bounded_message_preview(text, FALLBACK_MESSAGE_PREVIEW_LIMIT);
    if summary.is_empty() {
        return None;
    }
    let message_summary = item_summary_for_view(projection, SessionItemView::Messages);
    Some(ProducerLastMessageEvidence {
        summary,
        message_path: child_message_uri(agent_id, &message_summary.path),
        journal_session_id: agent_run_journal_session_id(run_id, agent_id),
        source_event_seq,
    })
}

fn child_message_uri(agent_id: uuid::Uuid, session_message_path: &str) -> String {
    let file_path = session_message_path
        .strip_prefix("session/messages/")
        .unwrap_or(session_message_path);
    format!("lifecycle://agent-runs/{agent_id}/sessions/messages/{file_path}")
}

fn last_journal_event_for_item<'a>(
    events: &'a [AgentRunJournalEvent],
    item_id: &str,
) -> Option<&'a AgentRunJournalEvent> {
    events
        .iter()
        .filter(|event| journal_event_item_id(event) == Some(item_id))
        .max_by_key(|event| event.journal_seq)
}

fn journal_event_item_id(event: &AgentRunJournalEvent) -> Option<&str> {
    match &event.event.notification.event {
        BackboneEvent::AgentMessageDelta(delta) => Some(delta.item_id.as_str()),
        BackboneEvent::ItemStarted(notification) => agent_message_item_id(&notification.item),
        BackboneEvent::ItemUpdated(notification) => agent_message_item_id(&notification.item),
        BackboneEvent::ItemCompleted(notification) => agent_message_item_id(&notification.item),
        _ => None,
    }
}

fn agent_message_item_id(item: &AgentDashThreadItem) -> Option<&str> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { id, .. }) => Some(id.as_str()),
        _ => None,
    }
}

fn bounded_message_preview(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut preview = trimmed.chars().take(limit).collect::<String>();
    preview.push_str("...");
    preview
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
        producer_last_message: event.producer_last_message,
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
        let fallback_message = ProducerLastMessageEvidence {
            summary: "child agent final note".to_string(),
            message_path: format!(
                "lifecycle://agent-runs/{agent_id}/sessions/messages/0002__agent.md"
            ),
            journal_session_id: format!("agentrun:{run_id}:{agent_id}"),
            source_event_seq: 12,
        };

        let event =
            wait_producer_terminal_event_from_agent_run(AgentRunWaitProducerTerminalEvent {
                run_id,
                agent_id,
                frame_id: Some(frame_id),
                terminal_state: "failed".to_string(),
                terminal_message: Some("provider rejected model".to_string()),
                terminal_diagnostic: None,
                producer_last_message: Some(fallback_message.clone()),
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
        assert_eq!(event.producer_last_message, Some(fallback_message));
        assert_eq!(event.source_turn_id.as_deref(), Some("turn-42"));
        assert_eq!(event.trace_ref.as_deref(), Some("delivery:trace"));
    }
}
