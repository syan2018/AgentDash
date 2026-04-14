//! Relay MCP 工具适配器 — 将远程 backend 上报的 MCP 工具包装为 AgentTool

use std::sync::Arc;

use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
    tools::sanitize_tool_schema,
};
use agentdash_spi::mcp_relay::{McpRelayProvider, RelayMcpToolInfo};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::pi_agent_mcp::namespaced_tool_name;

/// 将 relay MCP 工具适配为 Pi Agent 可调用的 AgentTool。
///
/// 每个实例对应一个远程 MCP server 上的一个具体工具，
/// 调用时通过 `McpRelayProvider` 走 relay 信道转发到本机执行。
#[derive(Clone)]
pub struct RelayMcpToolAdapter {
    runtime_name: String,
    original_tool_name: String,
    server_name: String,
    description: String,
    parameters_schema: serde_json::Value,
    provider: Arc<dyn McpRelayProvider>,
}

impl RelayMcpToolAdapter {
    pub fn from_info(info: &RelayMcpToolInfo, provider: Arc<dyn McpRelayProvider>) -> Self {
        let runtime_name = namespaced_tool_name(&info.server_name, &info.tool_name);
        let description = format!(
            "MCP relay 工具（server={}, tool={}）: {}",
            info.server_name, info.tool_name, info.description
        );
        let parameters_schema = sanitize_tool_schema(info.parameters_schema.clone());
        Self {
            runtime_name,
            original_tool_name: info.tool_name.clone(),
            server_name: info.server_name.clone(),
            description,
            parameters_schema,
            provider,
        }
    }
}

#[async_trait]
impl AgentTool for RelayMcpToolAdapter {
    fn name(&self) -> &str {
        &self.runtime_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let arguments = match args {
            serde_json::Value::Null => None,
            serde_json::Value::Object(map) => Some(map),
            other => {
                return Err(AgentToolError::InvalidArguments(format!(
                    "MCP relay 工具参数必须是 JSON object，实际为: {}",
                    other
                )));
            }
        };

        let result = self
            .provider
            .call_relay_tool(&self.server_name, &self.original_tool_name, arguments)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "MCP relay server: {}\nMCP tool: {}\n\n{}",
                self.server_name, self.original_tool_name, result.content
            ))],
            is_error: result.is_error,
            details: None,
        })
    }
}

/// 从 relay provider 发现所有远程 MCP 工具并包装为 AgentTool
pub async fn discover_relay_mcp_tools(
    provider: Arc<dyn McpRelayProvider>,
) -> Vec<DynAgentTool> {
    let tools = provider.list_relay_tools().await;
    tools
        .iter()
        .map(|info| {
            Arc::new(RelayMcpToolAdapter::from_info(info, provider.clone())) as DynAgentTool
        })
        .collect()
}
