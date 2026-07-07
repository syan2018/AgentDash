use agentdash_agent_protocol::RuntimeTerminalDiagnostic;
use agentdash_domain::workflow::{
    AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository, DeliveryBindingStatus,
    RuntimeSessionExecutionAnchorRepository,
};
use chrono::{DateTime, Utc};

use crate::WorkflowApplicationError;

pub struct AgentRunDeliveryStateRepos<'a> {
    pub execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    pub delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
}

pub struct AgentRunDeliveryStateService<'a> {
    repos: AgentRunDeliveryStateRepos<'a>,
}

impl<'a> AgentRunDeliveryStateService<'a> {
    pub fn new(repos: AgentRunDeliveryStateRepos<'a>) -> Self {
        Self { repos }
    }

    pub async fn mark_terminal_from_runtime_session(
        &self,
        input: AgentRunTerminalTransitionInput,
    ) -> Result<bool, WorkflowApplicationError> {
        let Some(anchor) = self
            .repos
            .execution_anchor_repo
            .find_by_session(&input.runtime_session_id)
            .await?
        else {
            return Ok(false);
        };

        let binding = match self
            .repos
            .delivery_binding_repo
            .get_current(anchor.run_id, anchor.agent_id)
            .await?
        {
            Some(binding) if binding.runtime_session_id == input.runtime_session_id => binding,
            Some(_) => return Ok(false),
            None => AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Terminal,
                input.observed_at,
            ),
        }
        .mark_terminal(
            input.turn_id.clone(),
            input.terminal_state.clone(),
            input.terminal_message.clone(),
            input
                .terminal_diagnostic
                .as_ref()
                .and_then(|diagnostic| serde_json::to_value(diagnostic).ok()),
            input.observed_at,
        );
        self.repos.delivery_binding_repo.upsert(&binding).await?;

        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunTerminalTransitionInput {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub observed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::workflow_repositories::{
        MemoryAgentRunDeliveryBindingRepository, MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_domain::workflow::{
        AgentRunDeliveryBindingRepository, DeliveryBindingStatus, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository,
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn terminal_transition_updates_agent_run_binding() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let bindings = MemoryAgentRunDeliveryBindingRepository::default();
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        anchors.create_once(&anchor).await.expect("anchor");

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
        });
        let updated = service
            .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_state: "failed".to_string(),
                terminal_message: Some("provider failed".to_string()),
                terminal_diagnostic: Some(RuntimeTerminalDiagnostic {
                    kind: "provider".to_string(),
                    code: Some("invalid_request".to_string()),
                    http_status: Some(400),
                    provider: Some("Example LLM".to_string()),
                    model: Some("example-chat-large".to_string()),
                    message: "provider failed".to_string(),
                    retryable: false,
                }),
                observed_at: Utc::now(),
            })
            .await
            .expect("terminal transition");

        assert!(updated);
        let binding = bindings
            .get_current(anchor.run_id, anchor.agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.status, DeliveryBindingStatus::Terminal);
        assert_eq!(binding.active_turn_id, None);
        assert_eq!(binding.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.terminal_state.as_deref(), Some("failed"));
        assert_eq!(binding.terminal_message.as_deref(), Some("provider failed"));
        assert_eq!(
            binding
                .terminal_diagnostic
                .as_ref()
                .and_then(|value| value.get("code"))
                .and_then(serde_json::Value::as_str),
            Some("invalid_request")
        );
    }

    #[tokio::test]
    async fn terminal_transition_ignores_stale_runtime_when_current_binding_changed() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let bindings = MemoryAgentRunDeliveryBindingRepository::default();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let old_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-old",
            run_id,
            launch_frame_id,
            agent_id,
        );
        let current_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-current",
            run_id,
            launch_frame_id,
            agent_id,
        );
        anchors.create_once(&old_anchor).await.expect("old anchor");
        anchors
            .create_once(&current_anchor)
            .await
            .expect("current anchor");
        let current_binding = AgentRunDeliveryBinding::from_anchor(
            &current_anchor,
            DeliveryBindingStatus::Running,
            Utc::now(),
        )
        .mark_running("turn-current", Utc::now());
        bindings
            .upsert(&current_binding)
            .await
            .expect("current binding");

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
        });
        let updated = service
            .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
                runtime_session_id: "runtime-old".to_string(),
                turn_id: "turn-old".to_string(),
                terminal_state: "failed".to_string(),
                terminal_message: Some("old runtime failed late".to_string()),
                terminal_diagnostic: None,
                observed_at: Utc::now(),
            })
            .await
            .expect("stale terminal transition");

        assert!(!updated);
        let binding = bindings
            .get_current(run_id, agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.runtime_session_id, "runtime-current");
        assert_eq!(binding.status, DeliveryBindingStatus::Running);
        assert_eq!(binding.active_turn_id.as_deref(), Some("turn-current"));
        assert_eq!(binding.terminal_state, None);
    }
}
