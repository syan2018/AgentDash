use std::sync::Arc;

use agentdash_agent_runtime::{
    RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant, RuntimeToolInvocation,
    RuntimeToolProductTarget, RuntimeToolProvenanceEvidence, RuntimeToolResolvedContext,
    RuntimeToolResourceGrant, ToolProtocolProjector,
};
use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentSurfaceRevision, AgentToolResult, AgentTurnId,
};
use agentdash_application_agentrun::agent_run::frame::runtime_backend_anchor_from_vfs;
use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingRepository,
};
use agentdash_application_operation_gateway::{
    OperationAuthorizationScope, OperationExecutionError, OperationMcpAccess, OperationMcpTool,
    OperationPlacement, OperationPrincipal, OperationPrincipalRef,
};
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_infrastructure::mcp::{RuntimeDynamicToolCatalog, RuntimeMcpToolCatalogRequest};
use agentdash_platform_spi::{RelayMcpCallContext, RuntimeMcpServer, RuntimeVfsAccessPolicy};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct ProductRuntimeMcpOperationAccess {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    catalog: Arc<dyn RuntimeDynamicToolCatalog>,
}

impl ProductRuntimeMcpOperationAccess {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        catalog: Arc<dyn RuntimeDynamicToolCatalog>,
    ) -> Self {
        Self {
            bindings,
            frames,
            catalog,
        }
    }

    async fn resolve(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
    ) -> Result<ResolvedMcpSurface, OperationExecutionError> {
        let OperationPrincipalRef::AgentRunAgent { run_id, agent_id } = principal.principal_ref()
        else {
            return Err(OperationExecutionError::NotReady {
                code: "mcp_agent_surface_required".to_string(),
                message: "MCP Operation 需要 AgentRun actor surface".to_string(),
            });
        };
        if agentdash_application_operation_gateway::scope_project_id(&scope.scope_ref).is_none() {
            return Err(OperationExecutionError::invalid_request(
                "MCP Operation 需要 Project scope",
            ));
        }
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id: *run_id,
            agent_id: *agent_id,
        };
        let binding = self
            .bindings
            .load_product_binding(&target)
            .await
            .map_err(OperationExecutionError::provider_failed)?
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "mcp_product_binding_missing".to_string(),
                message: "AgentRun Product runtime binding 不存在".to_string(),
            })?;
        let frame = self
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "mcp_agent_frame_missing".to_string(),
                message: "Product binding 对应 AgentFrame 不存在".to_string(),
            })?;
        let capability_state =
            frame
                .typed_capability_state()
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "mcp_capability_surface_missing".to_string(),
                    message: "AgentFrame 没有 typed capability surface".to_string(),
                })?;
        let servers = frame.typed_mcp_servers();
        let vfs = frame.typed_vfs();
        let relay_context = vfs
            .as_ref()
            .map(|vfs| {
                let backend_anchor =
                    runtime_backend_anchor_from_vfs(vfs, Some("operation_gateway_mcp".to_string()))
                        .map_err(|error| {
                            OperationExecutionError::provider_failed(error.to_string())
                        })?;
                Ok(RelayMcpCallContext {
                    session_id: binding.runtime_thread_id.to_string(),
                    turn_id: None,
                    tool_call_id: None,
                    backend_anchor,
                    vfs: Some(vfs.clone()),
                    vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs)),
                    identity: None,
                })
            })
            .transpose()?;
        let relay_backend_id = relay_context
            .as_ref()
            .and_then(|context| context.backend_anchor.as_ref())
            .map(|anchor| anchor.backend_id().to_string());
        let executors = self
            .catalog
            .resolve(RuntimeMcpToolCatalogRequest {
                servers: servers.clone(),
                capability_state,
                relay_context,
            })
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        Ok(ResolvedMcpSurface {
            binding,
            servers,
            executors,
            relay_backend_id,
        })
    }
}

struct ResolvedMcpSurface {
    binding: AgentRunProductRuntimeBinding,
    servers: Vec<RuntimeMcpServer>,
    executors: Vec<Arc<dyn agentdash_agent_runtime::RuntimeToolExecutor>>,
    relay_backend_id: Option<String>,
}

impl ResolvedMcpSurface {
    fn executor(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Option<Arc<dyn agentdash_agent_runtime::RuntimeToolExecutor>> {
        self.executors
            .iter()
            .find(|executor| {
                let definition = executor.definition();
                matches!(
                    definition.protocol_projector,
                    ToolProtocolProjector::Mcp { ref server_key } if server_key == server_name
                ) && definition
                    .provenance
                    .tool_path
                    .rsplit_once("::")
                    .is_some_and(|(_, candidate)| candidate == tool_name)
            })
            .cloned()
    }

    fn operation_tool(
        &self,
        executor: &Arc<dyn agentdash_agent_runtime::RuntimeToolExecutor>,
    ) -> Option<OperationMcpTool> {
        let definition = executor.definition();
        let ToolProtocolProjector::Mcp { server_key } = definition.protocol_projector else {
            return None;
        };
        let (_, tool_name) = definition.provenance.tool_path.rsplit_once("::")?;
        let server = self
            .servers
            .iter()
            .find(|server| server.name == server_key)?;
        let placement = if server.uses_relay {
            let backend_id = self.relay_backend_id.clone()?;
            OperationPlacement::LocalBackend { backend_id }
        } else {
            OperationPlacement::Cloud
        };
        Some(OperationMcpTool {
            server_name: server_key,
            tool_name: tool_name.to_string(),
            description: definition.description,
            input_schema: definition.parameters_schema,
            placement,
        })
    }
}

#[async_trait]
impl OperationMcpAccess for ProductRuntimeMcpOperationAccess {
    async fn discover_tools(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationMcpTool>, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let surface = self.resolve(principal, scope).await?;
        Ok(surface
            .executors
            .iter()
            .filter_map(|executor| surface.operation_tool(executor))
            .collect())
    }

    async fn invoke_tool(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let surface = self.resolve(principal, scope).await?;
        let executor = surface.executor(server_name, tool_name).ok_or_else(|| {
            OperationExecutionError::NotReady {
                code: "mcp_tool_missing".to_string(),
                message: format!("MCP tool 不在当前 surface: {server_name}.{tool_name}"),
            }
        })?;
        let definition = executor.definition();
        let revision = surface.binding.launch_frame.revision.max(1);
        let surface_digest = format!(
            "agent-frame:{}:{}",
            surface.binding.launch_frame.frame_id, revision
        );
        let invocation = RuntimeToolInvocation {
            context: RuntimeToolResolvedContext {
                runtime_thread_id: surface.binding.runtime_thread_id.clone(),
                host_binding_generation: None,
                applied_surface_revision: AgentSurfaceRevision(revision),
                turn_id: AgentTurnId::new("operation-gateway")
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?,
                item_id: None,
                effect_id: AgentEffectIdentity::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?,
                invocation_id: uuid::Uuid::new_v4().to_string(),
                deadline_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64 + 30_000,
            },
            tool: definition.name.clone(),
            arguments,
            grant: RuntimeToolAuthorizationGrant {
                permission: definition.permission,
                effect: definition.effect,
                target: RuntimeToolProductTarget {
                    project_id: agentdash_application_operation_gateway::scope_project_id(
                        &scope.scope_ref,
                    )
                    .expect("project scope checked")
                    .to_string(),
                    run_id: surface.binding.target.run_id.to_string(),
                    agent_id: surface.binding.target.agent_id.to_string(),
                },
                applied_surface: RuntimeToolAppliedSurfaceEvidence {
                    agent_surface_revision: revision,
                    agent_surface_digest: surface_digest.clone(),
                    vfs_digest: surface_digest.clone(),
                    vfs_provenance: RuntimeToolProvenanceEvidence {
                        source_kind: "agent_frame".to_string(),
                        source_id: surface.binding.launch_frame.frame_id.to_string(),
                        source_revision: revision,
                        projection_revision: revision,
                        captured_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
                    },
                    task_digest: surface_digest,
                    product_binding_digest: surface
                        .binding
                        .calculated_digest()
                        .map_err(OperationExecutionError::provider_failed)?,
                    host_binding_generation: None,
                },
                resources: RuntimeToolResourceGrant::Product,
            },
        };
        match executor.execute(invocation).await {
            AgentToolResult::Completed { output } => Ok(output),
            AgentToolResult::Rejected { code, message } => {
                Err(OperationExecutionError::NotReady { code, message })
            }
            AgentToolResult::Failed { code, message } => Err(
                OperationExecutionError::provider_failed(format!("{code}: {message}")),
            ),
        }
    }
}
