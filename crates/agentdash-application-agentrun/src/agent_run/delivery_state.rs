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
            Some(binding) => binding,
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
    }
}
