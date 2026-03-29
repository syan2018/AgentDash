pub mod adapters;
pub mod connectors;
pub(crate) mod hook_events;

pub use agentdash_spi::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, AgentInfo, FlowCapabilities, PromptPayload, RuntimeToolProvider,
    content_block_to_text,
};
pub use agentdash_spi::{
    AddressSpace, AgentConfig, Mount, MountCapability, ThinkingLevel,
};
pub use agentdash_spi::tool::DynAgentTool;
pub use adapters::vibe_kanban_config::{is_native_agent, to_vibe_kanban_config};
