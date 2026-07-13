//! Relay MCP 工具适配器 — 将远程 backend 上报的 MCP 工具包装为 AgentTool

use std::{collections::BTreeSet, sync::Arc};

use agentdash_application_ports::mcp_discovery::{McpToolDiscoveryOutcome, McpToolSourceOutcome};
use agentdash_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpListOutcome, RelayMcpToolInfo,
};
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, CapabilityState, ContentPart, DynAgentTool,
    RuntimeMcpServer, ToolUpdateCallback,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{
    DiscoveredMcpTool,
    common::{McpToolSurface, build_discovered_entry, normalize_args_object},
    naming::capability_key_for_mcp_server_name,
};

/// 将 relay MCP 工具适配为 Pi Agent 可调用的 AgentTool。
///
/// 每个实例对应一个远程 MCP server 上的一个具体工具，
/// 调用时通过 `McpRelayProvider` 走 relay 信道转发到本机执行。
#[derive(Clone)]
pub struct RelayMcpToolAdapter {
    surface: McpToolSurface,
    server: RuntimeMcpServer,
    provider: Arc<dyn McpRelayProvider>,
    call_context: Option<RelayMcpCallContext>,
}

impl RelayMcpToolAdapter {
    pub fn from_info(
        info: &RelayMcpToolInfo,
        provider: Arc<dyn McpRelayProvider>,
        call_context: Option<RelayMcpCallContext>,
    ) -> Self {
        let surface = McpToolSurface::new(
            info.server_name.clone(),
            info.tool_name.clone(),
            Some(&info.description),
            info.parameters_schema.clone(),
        );
        Self {
            surface,
            server: info.server.clone(),
            provider,
            call_context,
        }
    }
}

#[async_trait]
impl AgentTool for RelayMcpToolAdapter {
    fn name(&self) -> &str {
        &self.surface.runtime_name
    }

    fn description(&self) -> &str {
        &self.surface.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.surface.parameters_schema.clone()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some(format!("main_tool_mcp_relay_{}_lifecycle", self.name()))
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let arguments = normalize_args_object(args)?;

        let result = self
            .provider
            .call_relay_tool(
                &self.server,
                &self.surface.tool_name,
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
/// `servers` 是 Agent 配置中声明且匹配 backend 能力的 resolved server 列表。
pub async fn discover_relay_mcp_tools(
    provider: Arc<dyn McpRelayProvider>,
    servers: &[RuntimeMcpServer],
    capability_state: &CapabilityState,
    call_context: Option<RelayMcpCallContext>,
) -> Vec<DynAgentTool> {
    discover_relay_mcp_tool_outcome(provider, servers, capability_state, call_context)
        .await
        .tools
        .into_iter()
        .map(|entry| entry.tool)
        .collect()
}

pub async fn discover_relay_mcp_tool_entries(
    provider: Arc<dyn McpRelayProvider>,
    servers: &[RuntimeMcpServer],
    capability_state: &CapabilityState,
    call_context: Option<RelayMcpCallContext>,
) -> Vec<DiscoveredMcpTool> {
    discover_relay_mcp_tool_outcome(provider, servers, capability_state, call_context)
        .await
        .tools
}

pub async fn discover_relay_mcp_tool_outcome(
    provider: Arc<dyn McpRelayProvider>,
    servers: &[RuntimeMcpServer],
    capability_state: &CapabilityState,
    call_context: Option<RelayMcpCallContext>,
) -> McpToolDiscoveryOutcome {
    if servers.is_empty() {
        return McpToolDiscoveryOutcome::default();
    }
    let requested_names = servers
        .iter()
        .map(|server| server.name.as_str())
        .collect::<BTreeSet<_>>();
    let RelayMcpListOutcome { tools, sources } = provider
        .list_relay_tools(servers, call_context.clone())
        .await;
    let tools = tools
        .iter()
        .filter(|info| {
            if !requested_names.contains(info.server_name.as_str()) {
                return false;
            }
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
            build_discovered_entry(&adapter.surface, true, tool)
        })
        .collect();
    McpToolDiscoveryOutcome {
        tools,
        sources: sources
            .into_iter()
            .map(|source| McpToolSourceOutcome {
                server: source.server,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::platform::mcp_relay::{
        RelayMcpCallResult, RelayMcpSourceOutcome, RelayProbeResult,
    };
    use agentdash_spi::{ConnectorError, ToolCapability, ToolCapabilityFilter};
    use async_trait::async_trait;

    #[derive(Debug)]
    struct FakeRelayProvider {
        tools: Vec<RelayMcpToolInfo>,
    }

    #[async_trait]
    impl McpRelayProvider for FakeRelayProvider {
        async fn list_relay_tools(
            &self,
            _requested_servers: &[RuntimeMcpServer],
            _context: Option<RelayMcpCallContext>,
        ) -> RelayMcpListOutcome {
            RelayMcpListOutcome {
                tools: self.tools.clone(),
                sources: _requested_servers
                    .iter()
                    .map(|server| RelayMcpSourceOutcome::ready(server.clone(), self.tools.len()))
                    .collect(),
            }
        }

        async fn call_relay_tool(
            &self,
            _server: &RuntimeMcpServer,
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
            _target: agentdash_spi::platform::mcp_relay::RelayProbeTarget,
        ) -> Result<RelayProbeResult, ConnectorError> {
            Ok(RelayProbeResult {
                status: "ok".to_string(),
                latency_ms: None,
                tools: None,
                error: None,
            })
        }
    }

    fn relay_server(name: &str) -> RuntimeMcpServer {
        RuntimeMcpServer {
            name: name.to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: format!("http://localhost/{name}"),
                headers: vec![],
            },
            uses_relay: true,
            readiness: Default::default(),
        }
    }

    #[tokio::test]
    async fn relay_discovery_filters_tools_by_capability_policy() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![
                RelayMcpToolInfo {
                    server_name: "agentdash-workflow-tools-123".to_string(),
                    server: relay_server("agentdash-workflow-tools-123"),
                    tool_name: "get_workflow".to_string(),
                    description: String::new(),
                    parameters_schema: serde_json::json!({ "type": "object" }),
                },
                RelayMcpToolInfo {
                    server_name: "agentdash-workflow-tools-123".to_string(),
                    server: relay_server("agentdash-workflow-tools-123"),
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
            &[relay_server("agentdash-workflow-tools-123")],
            &flow,
            None,
        )
        .await;
        let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();

        assert_eq!(names, vec!["mcp_agentdash_workflow_tools_get_workflow"]);
    }

    #[tokio::test]
    async fn relay_discovery_filters_custom_mcp_raw_tool_policy_from_entries_and_callables() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![
                RelayMcpToolInfo {
                    server_name: "code-analyzer".to_string(),
                    server: relay_server("code-analyzer"),
                    tool_name: "allowed_tool".to_string(),
                    description: String::new(),
                    parameters_schema: serde_json::json!({ "type": "object" }),
                },
                RelayMcpToolInfo {
                    server_name: "code-analyzer".to_string(),
                    server: relay_server("code-analyzer"),
                    tool_name: "blocked_tool".to_string(),
                    description: String::new(),
                    parameters_schema: serde_json::json!({ "type": "object" }),
                },
            ],
        });
        let mut flow = CapabilityState::default();
        flow.tool
            .capabilities
            .insert(ToolCapability::custom_mcp("code-analyzer"));
        flow.tool.tool_policy.insert(
            "mcp:code-analyzer".to_string(),
            ToolCapabilityFilter {
                include_only: Default::default(),
                exclude: ["blocked_tool".to_string()].into_iter().collect(),
            },
        );

        let entries = discover_relay_mcp_tool_entries(
            provider.clone(),
            &[relay_server("code-analyzer")],
            &flow,
            None,
        )
        .await;
        let raw_tool_names = entries
            .iter()
            .map(|entry| entry.tool_name.as_str())
            .collect::<Vec<_>>();
        let callable_names =
            discover_relay_mcp_tools(provider, &[relay_server("code-analyzer")], &flow, None)
                .await
                .iter()
                .map(|tool| tool.name().to_string())
                .collect::<Vec<_>>();

        assert_eq!(raw_tool_names, vec!["allowed_tool"]);
        assert_eq!(callable_names, vec!["mcp_code_analyzer_allowed_tool"]);
    }

    #[test]
    fn relay_mcp_owner_contract_matches_main_lifecycle_fixture_identity() {
        let info = RelayMcpToolInfo {
            server_name: "agentdash-workflow-tools-123".to_string(),
            server: relay_server("agentdash-workflow-tools-123"),
            tool_name: "upsert_workflow_tool".to_string(),
            description: String::new(),
            parameters_schema: serde_json::json!({"type":"object"}),
        };
        let adapter = RelayMcpToolAdapter::from_info(
            &info,
            Arc::new(FakeRelayProvider { tools: vec![] }),
            None,
        );
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime/fixtures/main-mcp-tool-lifecycle.json"
        ))
        .expect("valid Main MCP fixture");
        let relay = fixture["scenarios"]
            .as_array()
            .expect("MCP scenarios")
            .iter()
            .find(|scenario| scenario["id"] == "relay")
            .expect("relay MCP scenario");

        assert_eq!(adapter.name(), relay["runtime_name"]);
        assert_eq!(
            adapter.protocol_fixture_id().as_deref(),
            relay["fixture_id"].as_str()
        );
        assert!(matches!(
            adapter.protocol_projector(),
            Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
        ));
    }

    #[tokio::test]
    async fn relay_discovery_denies_mcp_tools_when_capability_state_is_empty() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![RelayMcpToolInfo {
                server_name: "agentdash-workflow-tools-123".to_string(),
                server: relay_server("agentdash-workflow-tools-123"),
                tool_name: "upsert_workflow_tool".to_string(),
                description: String::new(),
                parameters_schema: serde_json::json!({ "type": "object" }),
            }],
        });

        let tools = discover_relay_mcp_tools(
            provider,
            &[relay_server("agentdash-workflow-tools-123")],
            &CapabilityState::default(),
            None,
        )
        .await;

        assert!(
            tools.is_empty(),
            "空 CapabilityState 不得因为 MCP server 已挂载而暴露 workflow 写入工具"
        );
    }

    #[tokio::test]
    async fn relay_discovery_ignores_unrequested_provider_tools() {
        let provider = Arc::new(FakeRelayProvider {
            tools: vec![RelayMcpToolInfo {
                server_name: "other-tools".to_string(),
                server: relay_server("other-tools"),
                tool_name: "read".to_string(),
                description: String::new(),
                parameters_schema: serde_json::json!({ "type": "object" }),
            }],
        });
        let mut flow = CapabilityState::default();
        flow.tool
            .capabilities
            .insert(agentdash_spi::ToolCapability::new("mcp:other-tools"));

        let tools =
            discover_relay_mcp_tools(provider, &[relay_server("requested-tools")], &flow, None)
                .await;

        assert!(tools.is_empty());
    }
}
