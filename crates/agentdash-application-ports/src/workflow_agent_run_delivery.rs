use std::sync::Arc;

use agentdash_agent_runtime_contract::{PresentationThreadId, RuntimeActor, RuntimeInput};
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent_run_runtime::AgentRunRuntimeTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowAgentRunDeliveryCommand {
    pub target: AgentRunRuntimeTarget,
    pub presentation_thread_id: PresentationThreadId,
    pub client_command_id: String,
    pub input: Vec<RuntimeInput>,
    pub presentation_content: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub actor: RuntimeActor,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAgentRunDeliveryReceipt {
    pub mailbox_message_id: Uuid,
    pub runtime_operation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowAgentRunDeliveryError {
    #[error("workflow AgentRun delivery is not composed")]
    Unavailable,
    #[error("workflow AgentRun delivery failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait WorkflowAgentRunDeliveryPort: Send + Sync {
    async fn deliver(
        &self,
        command: WorkflowAgentRunDeliveryCommand,
    ) -> Result<WorkflowAgentRunDeliveryReceipt, WorkflowAgentRunDeliveryError>;
}

#[derive(Clone, Default)]
pub struct SharedWorkflowAgentRunDeliveryHandle {
    inner: Arc<RwLock<Option<Arc<dyn WorkflowAgentRunDeliveryPort>>>>,
}

impl SharedWorkflowAgentRunDeliveryHandle {
    pub async fn set(&self, delivery: Arc<dyn WorkflowAgentRunDeliveryPort>) {
        *self.inner.write().await = Some(delivery);
    }
}

#[async_trait]
impl WorkflowAgentRunDeliveryPort for SharedWorkflowAgentRunDeliveryHandle {
    async fn deliver(
        &self,
        command: WorkflowAgentRunDeliveryCommand,
    ) -> Result<WorkflowAgentRunDeliveryReceipt, WorkflowAgentRunDeliveryError> {
        let delivery = self
            .inner
            .read()
            .await
            .clone()
            .ok_or(WorkflowAgentRunDeliveryError::Unavailable)?;
        delivery.deliver(command).await
    }
}
