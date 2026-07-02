use agentdash_domain::workflow::{
    AgentFrame, AgentRunAcceptedRefs, AgentRunLineage, LifecycleAgent, LifecycleRun,
};
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AgentRunForkMaterializationInput {
    pub parent_run: LifecycleRun,
    pub parent_agent: LifecycleAgent,
    pub parent_frame: AgentFrame,
    pub parent_runtime_session_id: String,
    pub child_runtime_session_id: String,
    pub fork_point_event_seq: Option<u64>,
    pub fork_point_ref_json: Option<Value>,
    pub forked_by_user_id: String,
    pub metadata_json: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct AgentRunForkMaterializationResult {
    pub child_run: LifecycleRun,
    pub child_agent: LifecycleAgent,
    pub child_frame: AgentFrame,
    pub lineage: AgentRunLineage,
}

impl AgentRunForkMaterializationResult {
    pub fn accepted_refs(&self) -> AgentRunAcceptedRefs {
        AgentRunAcceptedRefs {
            run_id: self.child_run.id,
            agent_id: self.child_agent.id,
            frame_id: Some(self.child_frame.id),
            frame_revision: Some(self.child_frame.revision),
            runtime_session_id: Some(self.lineage.child_runtime_session_id.clone()),
            agent_run_turn_id: None,
            protocol_turn_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunForkMaterializationError {
    #[error("agent run fork materialization rejected: {message}")]
    Rejected { message: String },
    #[error("agent run fork materialization failed: {message}")]
    Internal { message: String },
}

#[async_trait]
pub trait AgentRunForkMaterializationPort: Send + Sync {
    async fn materialize_forked_agent_run(
        &self,
        input: AgentRunForkMaterializationInput,
    ) -> Result<AgentRunForkMaterializationResult, AgentRunForkMaterializationError>;
}
