pub use agentdash_connector_contract::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, AgentInfo, FlowCapabilities, PromptPayload,
    content_block_to_text,
};
pub use agentdash_connector_contract::{AgentConfig, ThinkingLevel};
pub use agentdash_connector_contract::tool::DynAgentTool;
pub use crate::adapters::vibe_kanban_config::{is_native_agent, to_vibe_kanban_config};

use async_trait::async_trait;

#[async_trait]
pub trait RuntimeToolProvider: Send + Sync {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
