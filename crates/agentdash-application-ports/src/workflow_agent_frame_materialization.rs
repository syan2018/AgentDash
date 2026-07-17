use async_trait::async_trait;
use std::collections::BTreeSet;
use uuid::Uuid;

use agentdash_domain::workflow::{ActivityDefinition, AgentProcedureContract};
use agentdash_spi::{AgentConfig, Vfs};

use crate::agent_frame_materialization::{
    AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
};

#[derive(Debug, Clone)]
pub struct WorkflowAgentNodeFrameMaterializationInput {
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: Option<String>,
    pub created_by_id: Option<String>,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub lifecycle_key: String,
    pub activity: ActivityDefinition,
    pub workflow_contract: Option<AgentProcedureContract>,
    pub base_vfs: Option<Vfs>,
    pub inherited_executor_config: Option<AgentConfig>,
    pub ready_port_keys: BTreeSet<String>,
}

#[async_trait]
pub trait WorkflowAgentNodeFrameMaterializationPort: Send + Sync {
    async fn materialize_workflow_agent_node_frame(
        &self,
        input: WorkflowAgentNodeFrameMaterializationInput,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}
