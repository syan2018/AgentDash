//! Relay MCP 工具适配器 — 将远程 backend 上报的 MCP 工具包装为 AgentTool

use std::sync::Arc;

use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
    tools::sanitize_tool_schema,
};
use agentdash_spi::CapabilityState;
use agentdash_spi::platform::mcp_relay::{McpRelayProvider, RelayMcpCallContext, RelayMcpToolInfo};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::DiscoveredMcpTool;
use super::direct::{capability_key_for_mcp_server_name, namespaced_tool_name};

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
    call_context: Option<RelayMcpCallContext>,
}

impl RelayMcpToolAdapter {
    pub fn from_info(
        info: &RelayMcpToolInfo,
        provider: Arc<dyn McpRelayProvider>,
        call_context: Option<RelayMcpCallContext>,
    ) -> Self {
        let runtime_name = namespaced_tool_name(&info.server_name, &info.tool_name);
        let description = info.description.trim();
        let description = if description.is_empty() {
            "MCP 工具".to_string()
        } else {
            description.to_string()
        };
        let parameters_schema = sanitize_tool_schema(info.parameters_schema.clone());
        Self {
            runtime_name,
            original_tool_name: info.tool_name.clone(),
            server_name: info.server_name.clone(),
            description,
            parameters_schema,
            provider,
            call_context,
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
        tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let arguments = match args {
            serde_json::Value::Null => None,
            serde_json::Value::Object(map) => Some(map),
            other => {
                return Err(AgentToolError::InvalidArguments(format!(
                    "MCP 工具参数必须是 JSON object，实际为: {}",
                    other
                )));
            }
        };

        let result = self
            .provider
            .call_relay_tool(
                &self.server_name,
                &self.original_tool_name,
                arguments,
                self.call_context.clone().map(|mut context| {
                    context.tool_call_id = Some(tool_call_id.to_string());
                    context
                }),
            )
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(result.content)],
            is_error: result.is_error,
            details: None,
        })
    }
}

/// 从 relay provider 发现指定 server 的 MCP 工具并包装为 AgentTool。
/// `server_names` 是 Agent 配置中声明且匹配 backend 能力的 server name 列表。
pub async fn discover_relay_mcp_tools(
    provider: Arc<dyn McpRelayProvider>,
    server_names: &[String],
    capability_state: &CapabilityState,
    call_context: Option<RelayMcpCallContext>,
) -> Vec<DynAgentTool> {
    discover_relay_mcp_tool_entries(provider, server_names, capability_state, call_context)
        .await
        .into_iter()
        .map(|entry| entry.tool)
        .collect()
}

pub async fn discover_relay_mcp_tool_entries(
    provider: Arc<dyn McpRelayProvider>,
    server_names: &[String],
    capability_state: &CapabilityState,
    call_context: Option<RelayMcpCallContext>,
) -> Vec<DiscoveredMcpTool> {
    if server_names.is_empty() {
        return Vec::new();
    }
    let tools = provider.list_relay_tools(server_names).await;
    tools
        .iter()
        .filter(|info| {
            let capability_key = capability_key_for_mcp_server_name(&info.server_name);
            capability_state.is_capability_tool_enabled(&capability_key, &info.tool_name, None)
        })
        .map(|info| {
            let adapter = Arc::new(RelayMcpToolAdapter::from_info(
                info,
                provider.clone(),
                call_context.clone(),
            ));
            let tool = adapter.clone() as DynAgentTool;
            DiscoveredMcpTool {
                runtime_name: adapter.runtime_name.clone(),
                server_name: adapter.server_name.clone(),
                tool_name: adapter.original_tool_name.clone(),
                uses_relay: true,
                description: adapter.description.clone(),
                parameters_schema: adapter.parameters_schema.clone(),
                tool,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::platform::mcp_relay::{RelayMcpCallResult, RelayProbeResult};
    use agentdash_spi::{ConnectorError, ToolCapabilityFilter};
    use async_trait::async_trait;

    #[derive(Debug)]
    struct FakeRelayProvider {
        tools: Vec<RelayMcpToolInfo>,
    }

    #[async_trait]
    impl McpRelayProvider for FakeRelayProvider {
        async fn list_relay_tools(&self, _requested_servers: &[String]) -> Vec<RelayMcpToolInfo> {
            self.tools.clone()
        }

        async fn call_relay_tool(
            &self,
            _server_name: &str,
            _tool_name: &str,
            _arguments: Option<serde_json::Map<String, serde_json::Value>>,
            _context: Option<RelayMcpCallContext>,
        ) -> Result<RelayMcpCallResult, ConnectorError> {
            Ok(RelayMcpCallResult {
                content: String::new(),
                is_error: false,
            })
        }

        async fn probe_transport(
            &self,
            _transport: &agentdash_domain::mcp_preset::McpTransportConfig,
        ) -> Result<RelayProbeResult, ConnectorError> {
            Ok(RelayProbeResult {
                status: "ok".to_string(),
                latency_ms: None,
                tools: None,
                error: None,
            })
        }
    }

    #[tokio::test]
    async fn relay_discovery_filters_tools_by_capability_policy() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![
                RelayMcpToolInfo {
                    server_name: "agentdash-workflow-tools-123".to_string(),
                    tool_name: "get_workflow".to_string(),
                    description: String::new(),
                    parameters_schema: serde_json::json!({ "type": "object" }),
                },
                RelayMcpToolInfo {
                    server_name: "agentdash-workflow-tools-123".to_string(),
                    tool_name: "upsert_workflow_tool".to_string(),
                    description: String::new(),
                    parameters_schema: serde_json::json!({ "type": "object" }),
                },
            ],
        });
        let mut flow = CapabilityState::default();
        flow.tool
            .capabilities
            .insert(agentdash_spi::ToolCapability::new("workflow_management"));
        flow.tool.tool_policy.insert(
            "workflow_management".to_string(),
            ToolCapabilityFilter {
                include_only: Default::default(),
                exclude: ["upsert_workflow_tool".to_string()].into_iter().collect(),
            },
        );

        let tools = discover_relay_mcp_tools(
            provider,
            &["agentdash-workflow-tools-123".to_string()],
            &flow,
            None,
        )
        .await;
        let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();

        assert_eq!(names, vec!["mcp_agentdash_workflow_tools_get_workflow"]);
    }

    #[tokio::test]
    async fn relay_discovery_denies_mcp_tools_when_capability_state_is_empty() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![RelayMcpToolInfo {
                server_name: "agentdash-workflow-tools-123".to_string(),
                tool_name: "upsert_workflow_tool".to_string(),
                description: String::new(),
                parameters_schema: serde_json::json!({ "type": "object" }),
            }],
        });

        let tools = discover_relay_mcp_tools(
            provider,
            &["agentdash-workflow-tools-123".to_string()],
            &CapabilityState::default(),
            None,
        )
        .await;

        assert!(
            tools.is_empty(),
            "空 CapabilityState 不得因为 MCP server 已挂载而暴露 workflow 写入工具"
        );
    }
}
