use crate::message::AgentMessage;
use crate::tool::ToolDefinition;

/// Agent 上下文 — 仅持有 schema 快照，用于 DTO 传递。
///
/// `tools` 字段为 `Vec<ToolDefinition>`（仅 schema），不持有可执行工具实例。
/// Agent loop 内部另持有 `HashMap<String, DynAgentTool>` 用于实际执行。
#[derive(Clone)]
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolDefinition>,
}

impl std::fmt::Debug for AgentContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentContext")
            .field("system_prompt", &self.system_prompt)
            .field("messages_count", &self.messages.len())
            .field("tools_count", &self.tools.len())
            .finish()
    }
}
