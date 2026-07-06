use agentdash_application_ports::workspace_title::{WorkspaceTitleError, WorkspaceTitlePort};
use agentdash_domain::workflow::{
    LifecycleAgentRepository, RuntimeSessionExecutionAnchorRepository,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Implementation of WorkspaceTitlePort: resolves session → agent, writes title to LifecycleAgent.
pub struct AgentRunWorkspaceTitleAdapter {
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
}

impl AgentRunWorkspaceTitleAdapter {
    pub fn new(
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            anchor_repo,
            agent_repo,
        }
    }
}

#[async_trait]
impl WorkspaceTitlePort for AgentRunWorkspaceTitleAdapter {
    async fn update_workspace_title(
        &self,
        runtime_session_id: &str,
        title: String,
        title_source: &str,
    ) -> Result<bool, WorkspaceTitleError> {
        let anchor = self
            .anchor_repo
            .find_by_session(runtime_session_id)
            .await
            .map_err(|e| WorkspaceTitleError::Internal(e.to_string()))?
            .ok_or_else(|| {
                WorkspaceTitleError::SessionNotResolved(format!(
                    "no execution anchor for session {runtime_session_id}"
                ))
            })?;

        let mut agent = self
            .agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(|e| WorkspaceTitleError::Internal(e.to_string()))?
            .ok_or_else(|| {
                WorkspaceTitleError::Internal(format!("agent {} not found", anchor.agent_id))
            })?;

        let updated = agent.update_workspace_title(title, title_source);
        if updated {
            self.agent_repo
                .update(&agent)
                .await
                .map_err(|e| WorkspaceTitleError::Internal(e.to_string()))?;
        }

        Ok(updated)
    }
}
