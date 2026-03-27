pub use agentdash_connector_contract::connector::{
    AgentConnector, AgentDashExecutorConfig, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, ExecutorInfo, FlowCapabilities, PromptPayload, ThinkingLevel,
    content_block_to_text,
};
pub use agentdash_connector_contract::tool::DynAgentTool;

use async_trait::async_trait;

#[async_trait]
pub trait RuntimeToolProvider: Send + Sync {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
