use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::mcp_discovery::{McpToolDiscovery, McpToolDiscoveryRequest};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::ExecutionContext;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::hooks::RuntimeToolSchemaEntry;
use std::collections::BTreeMap;

use crate::session::dimension::tool_schema::{
    runtime_tool_schema_entries_from_mcp_tools, runtime_tool_schema_entries_from_tools,
};

#[derive(Clone, Default)]
pub(crate) struct AssembledToolSurface {
    pub tools: Vec<DynAgentTool>,
    pub schemas: Vec<RuntimeToolSchemaEntry>,
    pub mcp_failures: Vec<McpDiscoveryFailure>,
}

#[derive(Clone)]
pub(crate) struct McpDiscoveryFailure {
    pub summary: String,
}

pub(crate) async fn assemble_tool_surface_for_execution_context(
    session_id: &str,
    context: &ExecutionContext,
    runtime_tool_provider: Option<&dyn RuntimeToolProvider>,
    mcp_tool_discovery: Option<&dyn McpToolDiscovery>,
) -> AssembledToolSurface {
    let mut all_tools: Vec<DynAgentTool> = Vec::new();
    let mut all_schemas: Vec<RuntimeToolSchemaEntry> = Vec::new();
    let mut mcp_failures: Vec<McpDiscoveryFailure> = Vec::new();

    if let Some(provider) = runtime_tool_provider {
        match provider.build_tools(context).await {
            Ok(tools) => {
                all_schemas.extend(runtime_tool_schema_entries_from_tools(&tools));
                all_tools.extend(tools);
            }
            Err(e) => diag!(Warn, Subsystem::AgentRun,

                session_id = %session_id,
                "runtime tool 构建失败: {e}"
            ),
        }
    }

    if let Some(discovery) = mcp_tool_discovery {
        if let Err(error) = context
            .session
            .require_runtime_backend_anchor("tool_assembly", Some(session_id))
        {
            diag!(Warn, Subsystem::AgentRun,

                session_id = %session_id,
                error = %error,
                "MCP 工具发现跳过：缺少 runtime backend anchor"
            );
            return finalize_tool_surface(session_id, all_tools, all_schemas);
        }
        let call_context = agentdash_spi::RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: Some(context.session.turn_id.clone()),
            tool_call_id: None,
            backend_anchor: context.session.runtime_backend_anchor.clone(),
            vfs: context.session.vfs.clone(),
            vfs_access_policy: context.session.vfs_access_policy.clone(),
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
            Err(e) => {
                diag!(Warn, Subsystem::AgentRun,
                    session_id = %session_id,
                    "MCP 工具发现失败: {e}"
                );
                mcp_failures.push(McpDiscoveryFailure {
                    summary: format!("{e}"),
                });
            }
        }
    }

    let mut surface = finalize_tool_surface(session_id, all_tools, all_schemas);
    surface.mcp_failures = mcp_failures;
    surface
}

fn finalize_tool_surface(
    session_id: &str,
    tools: Vec<DynAgentTool>,
    mut schemas: Vec<RuntimeToolSchemaEntry>,
) -> AssembledToolSurface {
    dedupe_tool_schemas(&mut schemas);
    if let Some(message) = duplicate_callable_tool_diagnostic(&tools, &schemas) {
        diag!(Warn, Subsystem::AgentRun,
            session_id = %session_id,
            "runtime callable tool name 冲突，跳过本轮工具 surface: {message}"
        );
        return AssembledToolSurface::default();
    }

    AssembledToolSurface {
        tools,
        schemas,
        mcp_failures: Vec::new(),
    }
}

fn dedupe_tool_schemas(schemas: &mut Vec<RuntimeToolSchemaEntry>) {
    schemas.sort_by_key(schema_entry_key);
    schemas.dedup_by(|left, right| schema_entry_key(left) == schema_entry_key(right));
}

fn duplicate_callable_tool_diagnostic(
    tools: &[DynAgentTool],
    schemas: &[RuntimeToolSchemaEntry],
) -> Option<String> {
    let mut first_index_by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, tool) in tools.iter().enumerate() {
        let name = tool.name();
        if let Some(first_index) = first_index_by_name.get(name) {
            let provenance = schemas
                .iter()
                .filter(|schema| schema.name == name)
                .map(|schema| {
                    format!(
                        "source=`{}` path=`{}`",
                        schema.source.as_deref().unwrap_or("<unknown>"),
                        schema.tool_path.as_deref().unwrap_or("<unknown>")
                    )
                })
                .collect::<Vec<_>>();
            let provenance = if provenance.is_empty() {
                "schema provenance unavailable".to_string()
            } else {
                provenance.join(", ")
            };
            return Some(format!(
                "tool `{name}` appears at callable indexes {first_index} and {index}; {provenance}"
            ));
        }
        first_index_by_name.insert(name, index);
    }
    None
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
    use std::sync::Mutex;

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

    struct StubMcpDiscovery {
        captured_anchor: Arc<Mutex<Option<agentdash_spi::RuntimeBackendAnchor>>>,
    }

    #[async_trait]
    impl McpToolDiscovery for StubMcpDiscovery {
        async fn discover_tool_entries(
            &self,
            request: McpToolDiscoveryRequest,
        ) -> Result<Vec<DiscoveredMcpTool>, agentdash_spi::ConnectorError> {
            let mut captured = self
                .captured_anchor
                .lock()
                .expect("captured anchor mutex poisoned");
            *captured = request
                .call_context
                .and_then(|context| context.backend_anchor);
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

    struct NamedStubTool {
        name: String,
    }

    #[async_trait]
    impl AgentTool for NamedStubTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "MCP test tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({ "type": "object" })
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

    struct FilteringMcpDiscovery;

    #[async_trait]
    impl McpToolDiscovery for FilteringMcpDiscovery {
        async fn discover_tool_entries(
            &self,
            request: McpToolDiscoveryRequest,
        ) -> Result<Vec<DiscoveredMcpTool>, agentdash_spi::ConnectorError> {
            let entries = ["allowed_tool", "blocked_tool"]
                .into_iter()
                .map(|tool_name| {
                    let runtime_name = format!("mcp_code_analyzer_{tool_name}");
                    DiscoveredMcpTool {
                        runtime_name: runtime_name.clone(),
                        server_name: "code-analyzer".to_string(),
                        tool_name: tool_name.to_string(),
                        uses_relay: false,
                        description: format!("{tool_name} description"),
                        parameters_schema: serde_json::json!({ "type": "object" }),
                        tool: Arc::new(NamedStubTool { name: runtime_name }),
                    }
                })
                .filter(|entry| {
                    request.capability_state.is_capability_tool_enabled(
                        "mcp:code-analyzer",
                        &entry.tool_name,
                        None,
                    )
                })
                .collect();
            Ok(entries)
        }
    }

    struct StaticRuntimeToolProvider {
        tools: Vec<DynAgentTool>,
    }

    #[async_trait]
    impl RuntimeToolProvider for StaticRuntimeToolProvider {
        async fn build_tools(
            &self,
            _context: &ExecutionContext,
        ) -> Result<Vec<DynAgentTool>, agentdash_spi::ConnectorError> {
            Ok(self.tools.clone())
        }
    }

    #[tokio::test]
    async fn assembly_surface_preserves_project_mcp_schema_provenance() {
        let captured_anchor = Arc::new(Mutex::new(None));
        let runtime_backend_anchor = agentdash_spi::RuntimeBackendAnchor::new(
            "backend-1",
            agentdash_spi::RuntimeBackendAnchorSource::System,
        )
        .expect("anchor");
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
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: Some(runtime_backend_anchor.clone()),
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame::default(),
        };

        let surface = assemble_tool_surface_for_execution_context(
            "session-1",
            &context,
            None,
            Some(&StubMcpDiscovery {
                captured_anchor: captured_anchor.clone(),
            }),
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
        assert_eq!(
            captured_anchor
                .lock()
                .expect("captured anchor mutex poisoned")
                .as_ref()
                .map(|anchor| anchor.backend_id()),
            Some(runtime_backend_anchor.backend_id())
        );
    }

    #[tokio::test]
    async fn assembly_surface_uses_filtered_mcp_entries_for_schema_and_callable_tools() {
        let runtime_backend_anchor = agentdash_spi::RuntimeBackendAnchor::new(
            "backend-1",
            agentdash_spi::RuntimeBackendAnchorSource::System,
        )
        .expect("anchor");
        let mut capability_state = agentdash_spi::CapabilityState::default();
        capability_state
            .tool
            .capabilities
            .insert(agentdash_spi::ToolCapability::custom_mcp("code-analyzer"));
        capability_state.tool.tool_policy.insert(
            "mcp:code-analyzer".to_string(),
            agentdash_spi::ToolCapabilityFilter {
                include_only: Default::default(),
                exclude: ["blocked_tool".to_string()].into_iter().collect(),
            },
        );
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
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: Some(runtime_backend_anchor),
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame {
                capability_state,
                ..Default::default()
            },
        };

        let surface = assemble_tool_surface_for_execution_context(
            "session-1",
            &context,
            None,
            Some(&FilteringMcpDiscovery),
        )
        .await;

        let tool_names = surface
            .tools
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>();
        let schema_names = surface
            .schemas
            .iter()
            .map(|schema| schema.name.as_str())
            .collect::<Vec<_>>();
        let schema_paths = surface
            .schemas
            .iter()
            .filter_map(|schema| schema.tool_path.as_deref())
            .collect::<Vec<_>>();

        assert_eq!(tool_names, vec!["mcp_code_analyzer_allowed_tool"]);
        assert_eq!(schema_names, vec!["mcp_code_analyzer_allowed_tool"]);
        assert_eq!(schema_paths, vec!["mcp:code-analyzer::allowed_tool"]);
    }

    #[tokio::test]
    async fn assembly_surface_rejects_duplicate_callable_tool_names() {
        let context = ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: std::path::PathBuf::from("."),
                environment_variables: std::collections::HashMap::new(),
                executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: None,
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame::default(),
        };
        let provider = StaticRuntimeToolProvider {
            tools: vec![
                Arc::new(NamedStubTool {
                    name: "duplicate_tool".to_string(),
                }),
                Arc::new(NamedStubTool {
                    name: "duplicate_tool".to_string(),
                }),
            ],
        };

        let surface = assemble_tool_surface_for_execution_context(
            "session-1",
            &context,
            Some(&provider),
            None,
        )
        .await;

        assert!(surface.tools.is_empty());
        assert!(surface.schemas.is_empty());
    }
}
