use agentdash_agent_protocol::ControlPlaneProjectionChangeReason;
use agentdash_application_ports::agent_run_list_invalidation::{
    AgentRunListInvalidation, AgentRunListInvalidationPort,
};
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
    agent_run_list_invalidation: Option<Arc<dyn AgentRunListInvalidationPort>>,
}

impl AgentRunWorkspaceTitleAdapter {
    pub fn new(
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            anchor_repo,
            agent_repo,
            agent_run_list_invalidation: None,
        }
    }

    pub fn with_agent_run_list_invalidation(
        mut self,
        port: Option<Arc<dyn AgentRunListInvalidationPort>>,
    ) -> Self {
        self.agent_run_list_invalidation = port;
        self
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
            if let Some(port) = self.agent_run_list_invalidation.as_ref() {
                let _ = port
                    .publish_agent_run_list_invalidated(AgentRunListInvalidation {
                        project_id: agent.project_id,
                        run_id: anchor.run_id,
                        agent_id: agent.id,
                        frame_id: Some(anchor.launch_frame_id),
                        reason: ControlPlaneProjectionChangeReason::TitleChanged,
                        delivery_runtime_session_id: Some(runtime_session_id.to_string()),
                    })
                    .await;
            }
        }

        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        MemoryLifecycleAgentRepository, MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_domain::workflow::{
        AgentSource, LifecycleAgent, LifecycleAgentRepository, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository,
    };
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[derive(Default)]
    struct RecordingInvalidationPort {
        items: Mutex<Vec<AgentRunListInvalidation>>,
    }

    #[async_trait::async_trait]
    impl AgentRunListInvalidationPort for RecordingInvalidationPort {
        async fn publish_agent_run_list_invalidated(
            &self,
            invalidation: AgentRunListInvalidation,
        ) -> Result<(), String> {
            self.items.lock().unwrap().push(invalidation);
            Ok(())
        }
    }

    #[tokio::test]
    async fn title_update_emits_agent_run_list_invalidation() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let frame_id = Uuid::new_v4();
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-title",
            run_id,
            frame_id,
            agent.id,
        );
        let anchors = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
        let agents = Arc::new(MemoryLifecycleAgentRepository::default());
        let invalidations = Arc::new(RecordingInvalidationPort::default());
        anchors.create_once(&anchor).await.expect("anchor");
        agents.create(&agent).await.expect("agent");

        let updated = AgentRunWorkspaceTitleAdapter::new(anchors, agents)
            .with_agent_run_list_invalidation(Some(invalidations.clone()))
            .update_workspace_title("runtime-title", "新标题".to_string(), "source")
            .await
            .expect("title update");

        assert!(updated);
        let recorded = invalidations.items.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].project_id, project_id);
        assert_eq!(recorded[0].run_id, run_id);
        assert_eq!(recorded[0].agent_id, agent.id);
        assert_eq!(recorded[0].frame_id, Some(frame_id));
        assert_eq!(
            recorded[0].reason,
            ControlPlaneProjectionChangeReason::TitleChanged
        );
        assert_eq!(
            recorded[0].delivery_runtime_session_id.as_deref(),
            Some("runtime-title")
        );
    }
}
