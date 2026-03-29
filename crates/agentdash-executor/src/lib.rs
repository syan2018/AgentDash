pub mod adapters;
pub mod connectors;
pub(crate) mod hook_events;

pub use agentdash_connector_contract::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, AgentInfo, FlowCapabilities, PromptPayload, RuntimeToolProvider,
    content_block_to_text,
};
pub use agentdash_connector_contract::{
    AddressSpace, AgentConfig, Mount, MountCapability, ThinkingLevel,
};
pub use agentdash_connector_contract::tool::DynAgentTool;
pub use adapters::vibe_kanban_config::{is_native_agent, to_vibe_kanban_config};
