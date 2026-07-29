use std::sync::Arc;

use agentdash_agent_runtime::{
    RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant, RuntimeToolInvocation,
    RuntimeToolProductTarget, RuntimeToolProvenanceEvidence, RuntimeToolResolvedContext,
    RuntimeToolResourceGrant, ToolProtocolProjector,
};
use agentdash_agent_runtime_contract::{
    AgentEffectIdentity, AgentSurfaceRevision, AgentToolResult, AgentTurnId,
};
use agentdash_application::execution_authority::{
    ExecutionAuthority, ExecutionAuthorityRequest, ExecutionAuthorityResolver,
};
use agentdash_application_agentrun::agent_run::frame::runtime_backend_anchor_from_vfs;
use agentdash_application_operation_gateway::{
    OperationAuthorizationScope, OperationExecutionError, OperationMcpAccess, OperationMcpTool,
    OperationPlacement, OperationPrincipal, OperationPrincipalRef,
};
use agentdash_infrastructure::mcp::{RuntimeDynamicToolCatalog, RuntimeMcpToolCatalogRequest};
use agentdash_platform_spi::{RelayMcpCallContext, RuntimeMcpServer, RuntimeVfsAccessPolicy};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct ProductRuntimeMcpOperationAccess {
    execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
    catalog: Arc<dyn RuntimeDynamicToolCatalog>,
}

impl ProductRuntimeMcpOperationAccess {
    pub fn new(
        execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
        catalog: Arc<dyn RuntimeDynamicToolCatalog>,
    ) -> Self {
        Self {
            execution_authorities,
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
            .execution_authorities
            .resolve(ExecutionAuthorityRequest::for_target(target))
            .await
            .map_err(|error| OperationExecutionError::NotReady {
                code: error.code().to_string(),
                message: error.to_string(),
            })?;
        if agentdash_application_operation_gateway::scope_project_id(&scope.scope_ref)
            != Some(binding.project_id())
        {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["agent_run.project_scope".to_string()],
            });
        }
        if !scope.authority_revision.is_empty()
            && scope.authority_revision != binding.revision_token()
        {
            return Err(OperationExecutionError::NotReady {
                code: "stale_execution_authority".to_string(),
                message: "Operation surface authority changed during projection".to_string(),
            });
        }
        let servers = binding.mcp_servers().to_vec();
        let vfs = binding.resources().vfs(binding.project_id());
        let relay_context = (!vfs.mounts.is_empty())
            .then(|| {
                let backend_anchor = runtime_backend_anchor_from_vfs(
                    &vfs,
                    Some("operation_gateway_mcp".to_string()),
                )
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
                Ok(RelayMcpCallContext {
                    session_id: binding.runtime_thread_id().to_string(),
                    turn_id: None,
                    tool_call_id: None,
                    backend_anchor,
                    vfs: Some(vfs.clone()),
                    vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)),
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
                capability_state: binding.capability_state().clone(),
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
    binding: ExecutionAuthority,
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
        let revision = surface.binding.revision().max(1);
        let surface_digest = surface.binding.digest().to_string();
        let resources = surface.binding.resources();
        let evidence = surface.binding.evidence();
        let target = surface.binding.agent_run_target().ok_or_else(|| {
            OperationExecutionError::CapabilitiesDenied {
                missing: vec!["platform_tool.agent_run_principal".to_string()],
            }
        })?;
        let invocation = RuntimeToolInvocation {
            context: RuntimeToolResolvedContext {
                runtime_thread_id: surface.binding.runtime_thread_id().clone(),
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
                    project_id: surface.binding.project_id().to_string(),
                    run_id: target.run_id.to_string(),
                    agent_id: target.agent_id.to_string(),
                },
                applied_surface: RuntimeToolAppliedSurfaceEvidence {
                    agent_surface_revision: revision,
                    agent_surface_digest: surface_digest.clone(),
                    vfs_digest: resources.vfs_digest().to_string(),
                    vfs_provenance: RuntimeToolProvenanceEvidence {
                        source_kind: evidence.source_kind().to_string(),
                        source_id: evidence.source_id().to_string(),
                        source_revision: evidence.source_revision(),
                        projection_revision: evidence.projection_revision(),
                        captured_at_ms: evidence.captured_at_ms(),
                    },
                    task_digest: resources.task_digest().to_string(),
                    product_binding_digest: evidence.binding_digest().to_string(),
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
