use agentdash_agent_protocol::{ControlPlaneProjection, ControlPlaneProjectionChangeReason};
use agentdash_agent_runtime_contract::RuntimeThreadId;
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
    pub runtime_thread_id: Option<RuntimeThreadId>,
}

impl ProjectProjectionInvalidation {
    pub fn agent_run_list(
        project_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        reason: ControlPlaneProjectionChangeReason,
        runtime_thread_id: Option<RuntimeThreadId>,
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
            runtime_thread_id,
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
