use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::mcp_discovery::{McpToolDiscovery, McpToolDiscoveryRequest};
use agentdash_spi::ExecutionContext;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::hooks::RuntimeToolSchemaEntry;

use crate::session::dimension::tool_schema::{
    runtime_tool_schema_entries_from_mcp_tools, runtime_tool_schema_entries_from_tools,
};

#[derive(Clone, Default)]
pub(crate) struct AssembledToolSurface {
    pub tools: Vec<DynAgentTool>,
    pub schemas: Vec<RuntimeToolSchemaEntry>,
}

pub(crate) async fn assemble_tool_surface_for_execution_context(
    session_id: &str,
    context: &ExecutionContext,
    runtime_tool_provider: Option<&dyn RuntimeToolProvider>,
    mcp_tool_discovery: Option<&dyn McpToolDiscovery>,
) -> AssembledToolSurface {
    let mut all_tools: Vec<DynAgentTool> = Vec::new();
    let mut all_schemas: Vec<RuntimeToolSchemaEntry> = Vec::new();

    if let Some(provider) = runtime_tool_provider {
        match provider.build_tools(context).await {
            Ok(tools) => {
                all_schemas.extend(runtime_tool_schema_entries_from_tools(&tools));
                all_tools.extend(tools);
            }
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "runtime tool 构建失败: {e}"
            ),
        }
    }

    if let Some(discovery) = mcp_tool_discovery {
        let call_context = agentdash_spi::RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: Some(context.session.turn_id.clone()),
            tool_call_id: None,
            vfs: context.session.vfs.clone(),
            identity: context.session.identity.clone(),
        };
        match discovery
            .discover_tool_entries(McpToolDiscoveryRequest {
                servers: context.session.mcp_servers.clone(),
                capability_state: context.turn.capability_state.clone(),
                call_context: Some(call_context),
            })
            .await
        {
            Ok(entries) => {
                all_schemas.extend(runtime_tool_schema_entries_from_mcp_tools(&entries));
                all_tools.extend(entries.into_iter().map(|entry| entry.tool));
            }
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "MCP 工具发现失败: {e}"
            ),
        }
    }

    dedupe_tool_schemas(&mut all_schemas);
    AssembledToolSurface {
        tools: all_tools,
        schemas: all_schemas,
    }
}

fn dedupe_tool_schemas(schemas: &mut Vec<RuntimeToolSchemaEntry>) {
    schemas.sort_by_key(schema_entry_key);
    schemas.dedup_by(|left, right| schema_entry_key(left) == schema_entry_key(right));
}

fn schema_entry_key(entry: &RuntimeToolSchemaEntry) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        entry.source.as_deref().unwrap_or_default(),
        entry.tool_path.as_deref().unwrap_or_default(),
        entry.name
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use agentdash_application_ports::mcp_discovery::{
        DiscoveredMcpTool, McpToolDiscovery, McpToolDiscoveryRequest,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct StubMcpDiscovery;

    #[async_trait]
    impl McpToolDiscovery for StubMcpDiscovery {
        async fn discover_tool_entries(
            &self,
            _request: McpToolDiscoveryRequest,
        ) -> Result<Vec<DiscoveredMcpTool>, agentdash_spi::ConnectorError> {
            Ok(vec![DiscoveredMcpTool {
                runtime_name: "mcp_code_analyzer_scan_repo".to_string(),
                server_name: "code-analyzer".to_string(),
                tool_name: "scan_repo".to_string(),
                uses_relay: false,
                description: "Scan repository structure".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "root": {
                            "type": "string",
                            "description": "Repository root"
                        }
                    },
                    "required": ["root"]
                }),
                tool: Arc::new(StubTool),
            }])
        }
    }

    struct StubTool;

    #[async_trait]
    impl AgentTool for StubTool {
        fn name(&self) -> &str {
            "mcp_code_analyzer_scan_repo"
        }

        fn description(&self) -> &str {
            "Scan repository structure"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
        }

        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: Value,
            _cancel: CancellationToken,
            _on_update: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(AgentToolResult {
                content: vec![ContentPart::text("ok")],
                is_error: false,
                details: None,
            })
        }
    }

    #[tokio::test]
    async fn assembly_surface_preserves_project_mcp_schema_provenance() {
        let context = ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: std::path::PathBuf::from("."),
                environment_variables: std::collections::HashMap::new(),
                executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: vec![agentdash_spi::RuntimeMcpServer {
                    name: "code-analyzer".to_string(),
                    transport: agentdash_spi::McpTransportConfig::Http {
                        url: "http://localhost:18000/mcp".to_string(),
                        headers: Default::default(),
                    },
                    uses_relay: false,
                }],
                vfs: None,
                backend_execution: None,
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame::default(),
        };

        let surface = assemble_tool_surface_for_execution_context(
            "session-1",
            &context,
            None,
            Some(&StubMcpDiscovery),
        )
        .await;

        assert_eq!(surface.tools.len(), 1);
        assert_eq!(surface.schemas.len(), 1);
        let schema = &surface.schemas[0];
        assert_eq!(schema.name, "mcp_code_analyzer_scan_repo");
        assert_eq!(schema.capability_key.as_deref(), Some("mcp:code-analyzer"));
        assert_eq!(schema.source.as_deref(), Some("mcp:code-analyzer"));
        assert_eq!(
            schema.tool_path.as_deref(),
            Some("mcp:code-analyzer::scan_repo")
        );
        assert_eq!(
            schema.context_usage_kind.as_deref(),
            Some(agentdash_spi::context_usage_kind::MCP_TOOLS)
        );
        assert_eq!(schema.parameters_schema["required"][0], "root");
    }
}
