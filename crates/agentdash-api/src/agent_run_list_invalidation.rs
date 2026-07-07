use agentdash_agent_protocol::{ControlPlaneProjection, ControlPlaneProjectionChanged};
use agentdash_application_ports::agent_run_list_invalidation::{
    AgentRunListInvalidation, AgentRunListInvalidationPort,
};
use agentdash_contracts::project::{
    ProjectControlPlaneProjectionChanged, ProjectEventStreamEnvelope,
};
use async_trait::async_trait;
use tokio::sync::broadcast;

#[derive(Clone)]
pub(crate) struct ProjectAgentRunListInvalidationPublisher {
    sender: broadcast::Sender<ProjectEventStreamEnvelope>,
}

impl ProjectAgentRunListInvalidationPublisher {
    pub(crate) fn new(sender: broadcast::Sender<ProjectEventStreamEnvelope>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl AgentRunListInvalidationPort for ProjectAgentRunListInvalidationPublisher {
    async fn publish_agent_run_list_invalidated(
        &self,
        invalidation: AgentRunListInvalidation,
    ) -> Result<(), String> {
        let change = ControlPlaneProjectionChanged {
            projection: ControlPlaneProjection::AgentRunList,
            reason: invalidation.reason,
            run_id: invalidation.run_id.to_string(),
            agent_id: invalidation.agent_id.to_string(),
            frame_id: invalidation.frame_id.map(|id| id.to_string()),
            gate_id: None,
            mailbox_message_id: None,
            delivery_runtime_session_id: invalidation.delivery_runtime_session_id,
            workspace_module_presentation: None,
        };
        let event = ProjectEventStreamEnvelope::control_plane_projection_changed(
            ProjectControlPlaneProjectionChanged::new(invalidation.project_id.to_string(), change),
        );
        let _ = self.sender.send(event);
        Ok(())
    }
}

pub(crate) fn project_id_from_projection_event(event: &ProjectEventStreamEnvelope) -> Option<&str> {
    match event {
        ProjectEventStreamEnvelope::ControlPlaneProjectionChanged(data) => {
            Some(data.project_id.as_str())
        }
        _ => None,
    }
}
