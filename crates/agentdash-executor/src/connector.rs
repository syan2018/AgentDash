pub use agentdash_connector_contract::connector::{
    AgentConnector, AgentDashExecutorConfig, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, ExecutorInfo, FlowCapabilities, PromptPayload, ThinkingLevel,
    content_block_to_text,
};

#[cfg(feature = "pi-agent")]
use agentdash_agent::DynAgentTool;
#[cfg(feature = "pi-agent")]
use async_trait::async_trait;

#[cfg(feature = "pi-agent")]
pub fn to_agent_runtime_thinking_level(level: ThinkingLevel) -> agentdash_agent::ThinkingLevel {
    match level {
        ThinkingLevel::Off => agentdash_agent::ThinkingLevel::Off,
        ThinkingLevel::Minimal => agentdash_agent::ThinkingLevel::Minimal,
        ThinkingLevel::Low => agentdash_agent::ThinkingLevel::Low,
        ThinkingLevel::Medium => agentdash_agent::ThinkingLevel::Medium,
        ThinkingLevel::High => agentdash_agent::ThinkingLevel::High,
        ThinkingLevel::Xhigh => agentdash_agent::ThinkingLevel::Xhigh,
    }
}

#[cfg(feature = "pi-agent")]
#[async_trait]
pub trait RuntimeToolProvider: Send + Sync {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
