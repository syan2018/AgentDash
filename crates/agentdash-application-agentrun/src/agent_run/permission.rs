use agentdash_application_ports::agent_run_permission::{
    AgentRunPermissionDecision, AgentRunPermissionError, AgentRunPermissionFacade,
    AgentRunPermissionRequest,
};
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct AllowAllAgentRunPermissionFacade;

#[async_trait]
impl AgentRunPermissionFacade for AllowAllAgentRunPermissionFacade {
    async fn authorize(
        &self,
        _request: AgentRunPermissionRequest,
    ) -> Result<AgentRunPermissionDecision, AgentRunPermissionError> {
        Ok(AgentRunPermissionDecision::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn current_permission_fact_is_allow_all() {
        let decision = AllowAllAgentRunPermissionFacade
            .authorize(AgentRunPermissionRequest {
                run_id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                runtime_session_id: "runtime-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-1".to_string(),
                capability_key: "canvas".to_string(),
                tool_name: "canvas_create".to_string(),
            })
            .await
            .expect("allow-all facade");

        assert_eq!(decision, AgentRunPermissionDecision::Allowed);
    }
}
