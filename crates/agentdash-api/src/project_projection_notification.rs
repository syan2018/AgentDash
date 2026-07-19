use agentdash_application_ports::project_projection_notification::{
    ControlPlaneProjectionChanged, ProjectProjectionInvalidation,
    ProjectProjectionNotificationPort,
};
use agentdash_contracts::project::{
    ProjectControlPlaneProjectionChanged, ProjectEventStreamEnvelope,
};
use async_trait::async_trait;
use tokio::sync::broadcast;

#[derive(Clone)]
pub(crate) struct ProjectProjectionNotificationPublisher {
    sender: broadcast::Sender<ProjectEventStreamEnvelope>,
}

impl ProjectProjectionNotificationPublisher {
    pub(crate) fn new(sender: broadcast::Sender<ProjectEventStreamEnvelope>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl ProjectProjectionNotificationPort for ProjectProjectionNotificationPublisher {
    async fn publish_project_projection_invalidated(
        &self,
        invalidation: ProjectProjectionInvalidation,
    ) -> Result<(), String> {
        let change = ControlPlaneProjectionChanged {
            projection: invalidation.projection,
            reason: invalidation.reason,
            run_id: invalidation.run_id.to_string(),
            agent_id: invalidation.agent_id.to_string(),
            frame_id: invalidation.frame_id.map(|id| id.to_string()),
            gate_id: invalidation.gate_id.map(|id| id.to_string()),
            mailbox_message_id: invalidation.mailbox_message_id.map(|id| id.to_string()),
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
