use agentdash_domain::workflow::{AgentFrame, AgentRunLineage, LifecycleAgent, LifecycleRun};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct AgentRunForkGraph {
    pub child_run: LifecycleRun,
    pub child_agent: LifecycleAgent,
    pub child_frame: AgentFrame,
    pub lineage: AgentRunLineage,
}

#[async_trait]
pub trait AgentRunForkGraphStore: Send + Sync {
    async fn create_graph(&self, graph: &AgentRunForkGraph) -> Result<(), String>;
    async fn delete_graph(&self, graph: &AgentRunForkGraph) -> Result<(), String>;
}
