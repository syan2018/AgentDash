use std::sync::Arc;

use agentdash_agent_runtime::ToolProtocolProjector;
use agentdash_application::execution_authority::{
    ExecutionAuthority, ExecutionAuthorityRequest, ExecutionAuthorityResolver,
};
use agentdash_application::mcp_preset::{McpRuntimeBindingContext, resolve_preset_mcp_server};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application_agentrun::agent_run::frame::runtime_backend_anchor_from_vfs;
use agentdash_application_operation_gateway::{
    OperationAuthorizationScope, OperationExecutionError, OperationMcpAccess, OperationMcpTool,
    OperationPlacement, OperationPrincipal, OperationPrincipalRef,
};
use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_domain::common::{Mount, Vfs};
use agentdash_domain::operation::OperationScopeRef;
use agentdash_domain::workspace::WorkspaceBindingStatus;
use agentdash_infrastructure::mcp::{
    RuntimeDynamicToolCatalog, RuntimeMcpOperationInvocation, RuntimeMcpToolCatalogRequest,
    runtime_mcp_capability_key,
};
use agentdash_platform_spi::{
    CapabilityState, RelayMcpCallContext, RuntimeMcpServer, RuntimeVfsAccessPolicy, ToolCapability,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct ProductRuntimeMcpOperationAccess {
    execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
    catalog: Arc<dyn RuntimeDynamicToolCatalog>,
    repos: RepositorySet,
}

impl ProductRuntimeMcpOperationAccess {
    pub fn new(
        execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
        catalog: Arc<dyn RuntimeDynamicToolCatalog>,
        repos: RepositorySet,
    ) -> Self {
        Self {
            execution_authorities,
            catalog,
            repos,
        }
    }

    async fn resolve(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
    ) -> Result<ResolvedMcpSurface, OperationExecutionError> {
        let (servers, capability_state, relay_context, provenance_source) =
            match principal.principal_ref() {
                OperationPrincipalRef::AgentRunAgent { run_id, agent_id } => {
                    let binding = self
                        .resolve_agent_binding(*run_id, *agent_id, scope)
                        .await?;
                    let servers = binding.mcp_servers().to_vec();
                    let vfs = binding.resources().vfs(binding.project_id());
                    let relay_context = (!vfs.mounts.is_empty())
                        .then(|| {
                            let backend_anchor = runtime_backend_anchor_from_vfs(
                                &vfs,
                                Some("operation_gateway_mcp".to_string()),
                            )
                            .map_err(|error| {
                                OperationExecutionError::provider_failed(error.to_string())
                            })?;
                            Ok(RelayMcpCallContext {
                                session_id: binding.runtime_thread_id().to_string(),
                                turn_id: None,
                                tool_call_id: None,
                                backend_anchor,
                                vfs: Some(vfs.clone()),
                                vfs_access_policy: Some(
                                    RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
                                ),
                                identity: None,
                            })
                        })
                        .transpose()?;
                    (
                        servers,
                        binding.capability_state().clone(),
                        relay_context,
                        "agent_frame.mcp_surface",
                    )
                }
                OperationPrincipalRef::User { .. } => {
                    let project_id = self.scope_project_id(scope).await?;
                    let (servers, relay_context) = self
                        .resolve_user_surface(principal, scope, project_id)
                        .await?;
                    let mut capability_state = CapabilityState::default();
                    for server in &servers {
                        capability_state
                            .tool
                            .capabilities
                            .insert(ToolCapability::new(runtime_mcp_capability_key(
                                &server.name,
                            )));
                    }
                    (
                        servers,
                        capability_state,
                        relay_context,
                        "project.mcp_presets",
                    )
                }
                _ => {
                    return Err(OperationExecutionError::NotReady {
                        code: "mcp_principal_surface_unavailable".to_string(),
                        message: "当前 principal 没有 MCP Operation surface".to_string(),
                    });
                }
            };
        let relay_backend_id = relay_context
            .as_ref()
            .and_then(|context| context.backend_anchor.as_ref())
            .map(|anchor| anchor.backend_id().to_string());
        let request = RuntimeMcpToolCatalogRequest {
            servers: servers.clone(),
            capability_state,
            relay_context,
        };
        let executors = self
            .catalog
            .resolve(request.clone())
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        Ok(ResolvedMcpSurface {
            servers,
            executors,
            relay_backend_id,
            request,
            provenance_source,
        })
    }

    async fn resolve_agent_binding(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        scope: &OperationAuthorizationScope,
    ) -> Result<ExecutionAuthority, OperationExecutionError> {
        let binding = self
            .execution_authorities
            .resolve(ExecutionAuthorityRequest::for_target(
                agentdash_domain::agent_run_target::AgentRunTarget { run_id, agent_id },
            ))
            .await
            .map_err(|error| OperationExecutionError::NotReady {
                code: error.code().to_string(),
                message: error.to_string(),
            })?;
        if self.scope_project_id(scope).await? != binding.project_id() {
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
        Ok(binding)
    }

    async fn resolve_user_surface(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        project_id: uuid::Uuid,
    ) -> Result<(Vec<RuntimeMcpServer>, Option<RelayMcpCallContext>), OperationExecutionError> {
        let presets = self
            .repos
            .mcp_preset_repo
            .list_by_project(project_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        let (runtime_surface, relay_context) = match &scope.scope_ref {
            OperationScopeRef::WorkspaceBinding { workspace_id, .. } => {
                let (vfs, backend_anchor) = self
                    .resolve_workspace_runtime_surface(project_id, *workspace_id)
                    .await?;
                let context = McpRuntimeBindingContext {
                    vfs: Some(&vfs),
                    backend_anchor: Some(&backend_anchor),
                };
                let mut servers = Vec::with_capacity(presets.len());
                for preset in presets {
                    servers.push(resolve_preset_mcp_server(&preset, Some(&context)).map_err(
                        |error| OperationExecutionError::NotReady {
                            code: "mcp_runtime_binding_invalid".to_owned(),
                            message: error.to_string(),
                        },
                    )?);
                }
                let relay_context = RelayMcpCallContext {
                    session_id: format!("operation-workspace:{workspace_id}"),
                    turn_id: None,
                    tool_call_id: None,
                    backend_anchor: Some(backend_anchor),
                    vfs: Some(vfs.clone()),
                    vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)),
                    identity: principal.user_identity().cloned(),
                };
                (servers, Some(relay_context))
            }
            OperationScopeRef::Project { .. } | OperationScopeRef::InteractionInstance { .. } => {
                let mut servers = Vec::new();
                for preset in presets
                    .into_iter()
                    .filter(|preset| preset.runtime_binding.is_none())
                {
                    let server = resolve_preset_mcp_server(&preset, None).map_err(|error| {
                        OperationExecutionError::NotReady {
                            code: "mcp_preset_invalid".to_owned(),
                            message: error.to_string(),
                        }
                    })?;
                    if !server.uses_relay {
                        servers.push(server);
                    }
                }
                (servers, None)
            }
            OperationScopeRef::EnvironmentSetup { .. } => {
                return Err(OperationExecutionError::invalid_request(
                    "MCP Operation 不接受 Setup scope",
                ));
            }
        };
        let mut servers = runtime_surface;
        servers.sort_by(|left, right| left.name.cmp(&right.name));
        Ok((servers, relay_context))
    }

    async fn resolve_workspace_runtime_surface(
        &self,
        project_id: uuid::Uuid,
        workspace_id: uuid::Uuid,
    ) -> Result<(Vfs, RuntimeBackendAnchor), OperationExecutionError> {
        let workspace = self
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .filter(|workspace| workspace.project_id == project_id)
            .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                missing: vec!["workspace.project_scope".to_owned()],
            })?;
        let binding = workspace
            .default_binding_id
            .and_then(|binding_id| {
                workspace
                    .bindings
                    .iter()
                    .find(|binding| binding.id == binding_id)
            })
            .or_else(|| {
                workspace
                    .bindings
                    .iter()
                    .find(|binding| binding.status == WorkspaceBindingStatus::Ready)
            })
            .filter(|binding| binding.status == WorkspaceBindingStatus::Ready)
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "workspace_binding_unavailable".to_owned(),
                message: format!("Workspace 没有 active binding: {workspace_id}"),
            })?;
        let backend_anchor = RuntimeBackendAnchor::workspace_binding(
            binding.backend_id.clone(),
            workspace_id,
            binding.id,
            binding.root_ref.clone(),
        )
        .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        let mount_id = "main".to_owned();
        let vfs = Vfs {
            mounts: vec![Mount {
                id: mount_id.clone(),
                provider: "relay_fs".to_owned(),
                backend_id: binding.backend_id.clone(),
                root_ref: binding.root_ref.clone(),
                capabilities: workspace.mount_capabilities.clone(),
                default_write: false,
                display_name: workspace.name.clone(),
                metadata: serde_json::json!({
                    "workspace_id": workspace.id,
                    "workspace_binding_id": binding.id,
                    "workspace_identity_payload": workspace.identity_payload,
                    "workspace_detected_facts": binding.detected_facts,
                }),
            }],
            default_mount_id: Some(mount_id),
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        };
        Ok((vfs, backend_anchor))
    }

    async fn scope_project_id(
        &self,
        scope: &OperationAuthorizationScope,
    ) -> Result<uuid::Uuid, OperationExecutionError> {
        match &scope.scope_ref {
            OperationScopeRef::Project { project_id }
            | OperationScopeRef::WorkspaceBinding { project_id, .. } => Ok(*project_id),
            OperationScopeRef::InteractionInstance { instance_id } => {
                let instance = self
                    .repos
                    .interaction_instance_repo
                    .get(*instance_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "interaction_not_found".to_string(),
                        message: format!("InteractionInstance 不存在: {instance_id}"),
                    })?;
                let revision = self
                    .repos
                    .interaction_definition_repo
                    .get_revision(instance.definition_revision_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "interaction_revision_not_found".to_string(),
                        message: format!(
                            "Interaction definition revision 不存在: {}",
                            instance.definition_revision_id
                        ),
                    })?;
                Ok(revision.project_id)
            }
            OperationScopeRef::EnvironmentSetup { .. } => Err(
                OperationExecutionError::invalid_request("MCP Operation 不接受 Setup scope"),
            ),
        }
    }
}

struct ResolvedMcpSurface {
    servers: Vec<RuntimeMcpServer>,
    executors: Vec<Arc<dyn agentdash_agent_runtime::RuntimeToolExecutor>>,
    relay_backend_id: Option<String>,
    request: RuntimeMcpToolCatalogRequest,
    provenance_source: &'static str,
}

impl ResolvedMcpSurface {
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
            provenance_source: self.provenance_source.to_owned(),
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
        self.catalog
            .invoke_operation(
                surface.request,
                RuntimeMcpOperationInvocation {
                    server_name: server_name.to_owned(),
                    tool_name: tool_name.to_owned(),
                    arguments,
                },
            )
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))
    }
}
