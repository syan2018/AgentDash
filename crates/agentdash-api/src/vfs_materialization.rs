use std::sync::Arc;

use agentdash_application::vfs::{RewriteJsonArgumentsInput, VfsMaterializationService};
use agentdash_application_ports::vfs_materialization::VfsMaterializationTransport;
use agentdash_relay::{RelayMessage, VfsMaterializePayload, VfsMaterializeResponse};
use agentdash_spi::ConnectorError;
use agentdash_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpToolInfo, RelayProbeResult,
};
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;

pub struct RelayVfsMaterializationTransport {
    backends: Arc<BackendRegistry>,
}

impl RelayVfsMaterializationTransport {
    pub fn new(backends: Arc<BackendRegistry>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl VfsMaterializationTransport for RelayVfsMaterializationTransport {
    async fn materialize(
        &self,
        backend_id: &str,
        payload: VfsMaterializePayload,
    ) -> Result<VfsMaterializeResponse, String> {
        let response = self
            .backends
            .send_command(
                backend_id,
                RelayMessage::CommandVfsMaterialize {
                    id: RelayMessage::new_id("vfs-materialize"),
                    payload: Box::new(payload),
                },
            )
            .await
            .map_err(|error| error.to_string())?;

        match response {
            RelayMessage::ResponseVfsMaterialize {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(payload),
            RelayMessage::ResponseVfsMaterialize {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("vfs.materialize 返回意外响应: {}", other.id())),
        }
    }
}

pub struct MaterializingMcpRelayProvider {
    backends: Arc<BackendRegistry>,
    materialization: Arc<VfsMaterializationService>,
}

impl MaterializingMcpRelayProvider {
    pub fn new(
        backends: Arc<BackendRegistry>,
        materialization: Arc<VfsMaterializationService>,
    ) -> Self {
        Self {
            backends,
            materialization,
        }
    }
}

#[async_trait]
impl McpRelayProvider for MaterializingMcpRelayProvider {
    async fn list_relay_tools(&self, requested_servers: &[String]) -> Vec<RelayMcpToolInfo> {
        self.backends.list_relay_tools(requested_servers).await
    }

    async fn call_relay_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        context: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, ConnectorError> {
        let backend_id = self
            .backends
            .find_backend_for_mcp_server(server_name)
            .await
            .ok_or_else(|| {
                ConnectorError::ConnectionFailed(format!(
                    "无在线 backend 提供 MCP server '{server_name}'"
                ))
            })?;

        let arguments = match (
            arguments,
            context.as_ref().and_then(|context| context.vfs.as_ref()),
        ) {
            (Some(arguments), Some(vfs)) => {
                let context_ref = context.as_ref().expect("context checked with vfs");
                let output = self
                    .materialization
                    .rewrite_json_arguments(RewriteJsonArgumentsInput {
                        vfs,
                        target_backend_id: &backend_id,
                        arguments: &arguments,
                        session_id: &context_ref.session_id,
                        turn_id: context_ref.turn_id.as_deref(),
                        tool_call_id: context_ref.tool_call_id.as_deref(),
                        overlay: None,
                        identity: context_ref.identity.as_ref(),
                    })
                    .await
                    .map_err(ConnectorError::Runtime)?;
                if !output.rewrites.is_empty() {
                    tracing::info!(
                        server = %server_name,
                        tool = %tool_name,
                        rewrite_count = output.rewrites.len(),
                        "relay MCP 参数中的 VFS URI 已物化并重写"
                    );
                }
                Some(output.arguments)
            }
            (arguments, None) => arguments,
            (None, Some(_)) => None,
        };

        self.backends
            .call_relay_tool(server_name, tool_name, arguments, context)
            .await
    }

    async fn probe_transport(
        &self,
        transport: &agentdash_domain::mcp_preset::McpTransportConfig,
    ) -> Result<RelayProbeResult, ConnectorError> {
        self.backends.probe_transport(transport).await
    }
}
