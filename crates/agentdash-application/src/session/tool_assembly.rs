use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::mcp_discovery::{McpToolDiscovery, McpToolDiscoveryRequest};
use agentdash_spi::ExecutionContext;
use agentdash_spi::connector::RuntimeToolProvider;

pub(crate) async fn assemble_tools_for_execution_context(
    session_id: &str,
    context: &ExecutionContext,
    runtime_tool_provider: Option<&dyn RuntimeToolProvider>,
    mcp_tool_discovery: Option<&dyn McpToolDiscovery>,
) -> Vec<DynAgentTool> {
    let mut all_tools: Vec<DynAgentTool> = Vec::new();

    if let Some(provider) = runtime_tool_provider {
        match provider.build_tools(context).await {
            Ok(tools) => all_tools.extend(tools),
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
            Ok(entries) => all_tools.extend(entries.into_iter().map(|entry| entry.tool)),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "MCP 工具发现失败: {e}"
            ),
        }
    }

    all_tools
}
