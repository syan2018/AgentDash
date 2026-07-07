use agentdash_agent_protocol::{ControlPlaneProjection, ControlPlaneProjectionChangeReason};
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ProjectProjectionInvalidation {
    pub project_id: Uuid,
    pub projection: ControlPlaneProjection,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub gate_id: Option<Uuid>,
    pub mailbox_message_id: Option<Uuid>,
    pub reason: ControlPlaneProjectionChangeReason,
    pub delivery_runtime_session_id: Option<String>,
}

impl ProjectProjectionInvalidation {
    pub fn agent_run_list(
        project_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        reason: ControlPlaneProjectionChangeReason,
        delivery_runtime_session_id: Option<String>,
    ) -> Self {
        Self {
            project_id,
            projection: ControlPlaneProjection::AgentRunList,
            run_id,
            agent_id,
            frame_id,
            gate_id: None,
            mailbox_message_id: None,
            reason,
            delivery_runtime_session_id,
        }
    }
}

#[async_trait]
pub trait ProjectProjectionNotificationPort: Send + Sync {
    async fn publish_project_projection_invalidated(
        &self,
        invalidation: ProjectProjectionInvalidation,
    ) -> Result<(), String>;
}
