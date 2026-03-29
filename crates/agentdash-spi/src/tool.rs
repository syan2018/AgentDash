// 全部类型已迁移到 agentdash-agent-types，这里仅 re-export 保持向后兼容。
pub use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolDefinition,
    ToolUpdateCallback,
};
