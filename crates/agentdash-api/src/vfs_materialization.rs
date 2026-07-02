use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use agentdash_application_ports::vfs_materialization::{
    MaterializationAccessMode, MaterializationCacheScope, MaterializationPlanKind,
    MaterializationTargetKind, VfsMaterializationTransport, VfsMaterializeContent,
    VfsMaterializeEntry, VfsMaterializeRequest, VfsMaterializeResponse,
};
use agentdash_application_vfs::{RewriteJsonArgumentsInput, VfsMaterializationService};
use agentdash_relay::{RelayMessage, VfsMaterializePayload};
use agentdash_spi::ConnectorError;
use agentdash_spi::RuntimeMcpServer;
use agentdash_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome,
    RelayProbeResult,
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
        request: VfsMaterializeRequest,
    ) -> Result<VfsMaterializeResponse, String> {
        let response = self
            .backends
            .send_command(
                backend_id,
                RelayMessage::CommandVfsMaterialize {
                    id: RelayMessage::new_id("vfs-materialize"),
                    payload: Box::new(materialize_request_to_relay(request)),
                },
            )
            .await
            .map_err(|error| error.to_string())?;

        match response {
            RelayMessage::ResponseVfsMaterialize {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(materialize_response_from_relay(payload)),
            RelayMessage::ResponseVfsMaterialize {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("vfs.materialize 返回意外响应: {}", other.id())),
        }
    }
}

fn materialize_request_to_relay(request: VfsMaterializeRequest) -> VfsMaterializePayload {
    VfsMaterializePayload {
        session_id: request.session_id,
        turn_id: request.turn_id,
        tool_call_id: request.tool_call_id,
        plan_id: request.plan_id,
        plan_kind: plan_kind_to_relay(request.plan_kind),
        source_uri: request.source_uri,
        root_uri: request.root_uri,
        mount_id: request.mount_id,
        provider: request.provider,
        primary_relative_path: request.primary_relative_path,
        target_kind: target_kind_to_relay(request.target_kind),
        access_mode: access_mode_to_relay(request.access_mode),
        entries: request
            .entries
            .into_iter()
            .map(materialize_entry_to_relay)
            .collect(),
        cache_scope: cache_scope_to_relay(request.cache_scope),
        ttl_ms: request.ttl_ms,
    }
}

fn materialize_response_from_relay(
    response: agentdash_relay::VfsMaterializeResponse,
) -> VfsMaterializeResponse {
    VfsMaterializeResponse {
        source_uri: response.source_uri,
        local_root_path: response.local_root_path,
        primary_local_path: response.primary_local_path,
        primary_local_url: response.primary_local_url,
        access_mode: access_mode_from_relay(response.access_mode),
        manifest_digest: response.manifest_digest,
        total_size_bytes: response.total_size_bytes,
        entry_count: response.entry_count,
        dirty: response.dirty,
        cache_hit: response.cache_hit,
    }
}

fn materialize_entry_to_relay(entry: VfsMaterializeEntry) -> agentdash_relay::VfsMaterializeEntry {
    agentdash_relay::VfsMaterializeEntry {
        relative_path: entry.relative_path,
        content: match entry.content {
            VfsMaterializeContent::Utf8Text { text } => {
                agentdash_relay::VfsMaterializeContent::Utf8Text { text }
            }
            VfsMaterializeContent::Base64Bytes { data } => {
                agentdash_relay::VfsMaterializeContent::Base64Bytes { data }
            }
        },
        digest: entry.digest,
        size_bytes: entry.size_bytes,
        mime_hint: entry.mime_hint,
        executable_hint: entry.executable_hint,
    }
}

fn plan_kind_to_relay(kind: MaterializationPlanKind) -> agentdash_relay::MaterializationPlanKind {
    match kind {
        MaterializationPlanKind::SingleFile => agentdash_relay::MaterializationPlanKind::SingleFile,
        MaterializationPlanKind::DirectorySubtree => {
            agentdash_relay::MaterializationPlanKind::DirectorySubtree
        }
        MaterializationPlanKind::SkillResourceSet => {
            agentdash_relay::MaterializationPlanKind::SkillResourceSet
        }
        MaterializationPlanKind::WritableWorkingCopy => {
            agentdash_relay::MaterializationPlanKind::WritableWorkingCopy
        }
    }
}

fn target_kind_to_relay(
    kind: MaterializationTargetKind,
) -> agentdash_relay::MaterializationTargetKind {
    match kind {
        MaterializationTargetKind::File => agentdash_relay::MaterializationTargetKind::File,
        MaterializationTargetKind::Directory => {
            agentdash_relay::MaterializationTargetKind::Directory
        }
    }
}

fn access_mode_to_relay(
    mode: MaterializationAccessMode,
) -> agentdash_relay::MaterializationAccessMode {
    match mode {
        MaterializationAccessMode::ReadOnly => agentdash_relay::MaterializationAccessMode::ReadOnly,
        MaterializationAccessMode::WritableWorkdir => {
            agentdash_relay::MaterializationAccessMode::WritableWorkdir
        }
    }
}

fn access_mode_from_relay(
    mode: agentdash_relay::MaterializationAccessMode,
) -> MaterializationAccessMode {
    match mode {
        agentdash_relay::MaterializationAccessMode::ReadOnly => MaterializationAccessMode::ReadOnly,
        agentdash_relay::MaterializationAccessMode::WritableWorkdir => {
            MaterializationAccessMode::WritableWorkdir
        }
    }
}

fn cache_scope_to_relay(
    scope: MaterializationCacheScope,
) -> agentdash_relay::MaterializationCacheScope {
    match scope {
        MaterializationCacheScope::Public => agentdash_relay::MaterializationCacheScope::Public,
        MaterializationCacheScope::Session => agentdash_relay::MaterializationCacheScope::Session,
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
    async fn list_relay_tools(
        &self,
        requested_servers: &[RuntimeMcpServer],
        context: Option<RelayMcpCallContext>,
    ) -> RelayMcpListOutcome {
        self.backends
            .list_relay_tools(requested_servers, context)
            .await
    }

    async fn call_relay_tool(
        &self,
        server: &RuntimeMcpServer,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        context: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, ConnectorError> {
        let server_name = server.name.as_str();
        let backend_id = self
            .backends
            .resolve_backend_for_relay_mcp(server_name, context.as_ref())
            .await
            .map_err(|error| {
                ConnectorError::ConnectionFailed(format!(
                    "无法解析 relay MCP server '{server_name}' 的 runtime backend anchor: {error}"
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
                        access_policy: context_ref.vfs_access_policy.as_ref(),
                        target_backend_id: backend_id.as_str(),
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
                    diag!(Info, Subsystem::Vfs,

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
            .call_relay_tool(server, tool_name, arguments, context)
            .await
    }

    async fn probe_transport(
        &self,
        transport: &agentdash_domain::mcp_preset::McpTransportConfig,
    ) -> Result<RelayProbeResult, ConnectorError> {
        self.backends.probe_transport(transport).await
    }
}
