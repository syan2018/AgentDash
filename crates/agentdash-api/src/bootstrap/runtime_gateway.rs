use std::sync::Arc;

use agentdash_application::backend::{
    McpProbeBackendTarget, McpProbeBackendTargetResolutionError, resolve_mcp_probe_backend_target,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::workspace::WorkspaceDetectionError;
use agentdash_application_ports::backend_transport::{BackendTransport, TransportError};
use agentdash_application_ports::extension_runtime::ExtensionRuntimeActionTransport;
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeActionProvider, InMemoryOperationResultStore, McpCallToolProvider,
    McpListToolsProvider, McpProbeSetupPort, McpProbeTarget, McpProbeToolOutput,
    McpProbeTransportInput, McpProbeTransportOutput, OperationGateway, RuntimeGateway,
    RuntimeGatewaySetupError, RuntimeSessionMcpAccess, SetupOperationAccessPort,
    SetupOperationAuthorityResolver, SetupOperationProvider, TracingOperationAuditSink,
    WorkspaceBrowseDirectoryEntry, WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectoryOutput,
    WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput, WorkspaceDetectGitOutput,
    WorkspaceDetectGitSetupPort, WorkspaceDetectInput, WorkspaceDetectOutput,
    WorkspaceDetectSetupPort, WorkspaceDiscoverByIdentityCandidateOutput,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentityOutput,
    WorkspaceDiscoverByIdentitySetupPort, WorkspaceDiscoverByIdentitySkippedOutput,
};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::AuthIdentity;
use agentdash_spi::platform::mcp_probe::McpProbeTransport;
use agentdash_spi::platform::mcp_relay::{McpRelayProvider, RelayProbeTarget};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::relay::registry::BackendRegistry;

pub(crate) fn build_runtime_gateway(
    session_mcp_access: Arc<dyn RuntimeSessionMcpAccess>,
    extension_installations: Arc<dyn ProjectExtensionInstallationRepository>,
    extension_action_transport: Arc<dyn ExtensionRuntimeActionTransport>,
) -> Arc<RuntimeGateway> {
    Arc::new(
        RuntimeGateway::new()
            .with_provider(Arc::new(McpListToolsProvider::new(
                session_mcp_access.clone(),
            )))
            .with_provider(Arc::new(McpCallToolProvider::new(session_mcp_access)))
            .with_dynamic_provider(Arc::new(ExtensionRuntimeActionProvider::new(
                extension_installations,
                extension_action_transport,
            ))),
    )
}

pub(crate) fn build_operation_gateway(
    mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider>,
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
    setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    >,
) -> Result<Arc<OperationGateway>, agentdash_application_runtime_gateway::OperationExecutionError> {
    let mcp_probe_setup = Arc::new(ApplicationMcpProbeSetupPort::new(
        Some(mcp_probe_relay),
        McpProbeBackendTargetResolver::new(repos.clone(), backend_registry),
        Arc::new(agentdash_infrastructure::RmcpProbeTransport::new()),
    ));
    let workspace_setup = Arc::new(ApplicationWorkspaceSetupPort::new(setup_action_transport));

    let setup_provider = Arc::new(SetupOperationProvider::new(
        mcp_probe_setup,
        workspace_setup.clone(),
        workspace_setup.clone(),
        workspace_setup.clone(),
        workspace_setup,
    ));
    OperationGateway::try_new(
        Arc::new(SetupOperationAuthorityResolver::new(Arc::new(
            ApplicationSetupOperationAccess { repos },
        ))),
        [setup_provider as Arc<dyn agentdash_application_runtime_gateway::OperationProvider>],
        Arc::new(InMemoryOperationResultStore::default()),
        Arc::new(TracingOperationAuditSink),
    )
    .map(Arc::new)
}

struct ApplicationSetupOperationAccess {
    repos: RepositorySet,
}

#[async_trait]
impl SetupOperationAccessPort for ApplicationSetupOperationAccess {
    async fn resolve_access(
        &self,
        identity: &AuthIdentity,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<
        agentdash_application_runtime_gateway::OperationAuthorityGrant,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application::backend::{BackendAuthorizationService, BackendPermission};
        use agentdash_application::project::{
            ProjectAuthorizationService, ProjectPermission,
            project_authorization_context_from_identity,
        };
        use agentdash_application_runtime_gateway::{OperationExecutionError, OperationScopeRef};

        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let OperationScopeRef::EnvironmentSetup {
            project_id,
            workspace_id,
            backend_id,
        } = &scope.scope_ref
        else {
            return Err(OperationExecutionError::invalid_request(
                "Setup authority 需要 EnvironmentSetup scope",
            ));
        };

        let mut revision_facts = vec![format!("user:{}", identity.user_id)];
        if let Some(project_id) = project_id {
            let project = self
                .repos
                .project_repo
                .get_by_id(*project_id)
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "project_not_found".to_string(),
                    message: format!("Project 不存在: {project_id}"),
                })?;
            let allowed = ProjectAuthorizationService::new(self.repos.project_repo.as_ref())
                .can_access_project(
                    &project_authorization_context_from_identity(identity),
                    &project,
                    ProjectPermission::Use,
                )
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
            if !allowed {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["project.use".to_string()],
                });
            }
            revision_facts.push(format!("project:{project_id}:{}", project.updated_at));
        }

        if let Some(workspace_id) = workspace_id {
            let workspace = self
                .repos
                .workspace_repo
                .get_by_id(*workspace_id)
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "workspace_not_found".to_string(),
                    message: format!("Workspace 不存在: {workspace_id}"),
                })?;
            if project_id.is_some_and(|project_id| workspace.project_id != project_id) {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["workspace.project_scope".to_string()],
                });
            }
            revision_facts.push(format!("workspace:{workspace_id}:{}", workspace.updated_at));
        }

        let mut capabilities = std::collections::BTreeSet::from(["setup.mcp_probe".to_string()]);
        if let Some(backend_id) = backend_id {
            let backend = BackendAuthorizationService::new(
                self.repos.backend_repo.as_ref(),
                self.repos.project_repo.as_ref(),
                self.repos.project_backend_access_repo.as_ref(),
            )
            .require_backend(identity, backend_id, BackendPermission::View)
            .await
            .map_err(|error| OperationExecutionError::CapabilitiesDenied {
                missing: vec![error.to_string()],
            })?;
            revision_facts.push(format!(
                "backend:{}:{}:{:?}:{:?}:{:?}",
                backend.id,
                backend.enabled,
                backend.owner_user_id,
                backend.visibility,
                backend.share_scope_id
            ));
            if let Some(project_id) = project_id {
                let grant = self
                    .repos
                    .project_backend_access_repo
                    .get_active_for_project_backend(*project_id, backend_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["project.backend.use".to_string()],
                    })?;
                revision_facts.push(format!("grant:{}:{}", grant.id, grant.updated_at));
            }
            capabilities.insert("setup.workspace".to_string());
        }

        revision_facts.sort();
        let mut digest = Sha256::new();
        for fact in revision_facts {
            digest.update(fact.as_bytes());
            digest.update([0]);
        }
        Ok(
            agentdash_application_runtime_gateway::OperationAuthorityGrant {
                authority_revision: format!("sha256:{:x}", digest.finalize()),
                capabilities,
            },
        )
    }
}

struct ApplicationMcpProbeSetupPort {
    relay: Option<Arc<dyn McpRelayProvider>>,
    target_resolver: McpProbeBackendTargetResolver,
    http_probe: Arc<dyn McpProbeTransport>,
}

impl ApplicationMcpProbeSetupPort {
    fn new(
        relay: Option<Arc<dyn McpRelayProvider>>,
        target_resolver: McpProbeBackendTargetResolver,
        http_probe: Arc<dyn McpProbeTransport>,
    ) -> Self {
        Self {
            relay,
            target_resolver,
            http_probe,
        }
    }
}

#[async_trait]
impl McpProbeSetupPort for ApplicationMcpProbeSetupPort {
    async fn probe_transport(
        &self,
        input: McpProbeTransportInput,
    ) -> Result<McpProbeTransportOutput, RuntimeGatewaySetupError> {
        let relay_target = if input.route_policy.uses_relay(&input.transport) {
            match self
                .target_resolver
                .resolve(&input.current_user, &input.probe_target)
                .await
            {
                Ok(target) => Some(target),
                Err(McpProbeBackendTargetResolutionError::Unavailable(message)) => {
                    return Ok(McpProbeTransportOutput::Unsupported { reason: message });
                }
                Err(McpProbeBackendTargetResolutionError::Failed(message)) => {
                    return Err(RuntimeGatewaySetupError::ProviderFailed(message));
                }
            }
        } else {
            None
        };
        let result = agentdash_application::mcp_preset::probe_transport_without_runtime_context(
            &input.transport,
            input.route_policy,
            input.runtime_binding.as_ref(),
            relay_target,
            self.relay.as_deref(),
            self.http_probe.as_ref(),
        )
        .await;
        Ok(match result {
            agentdash_application::mcp_preset::ProbeResult::Ok { latency_ms, tools } => {
                McpProbeTransportOutput::Ok {
                    latency_ms,
                    tools: tools
                        .into_iter()
                        .map(|tool| McpProbeToolOutput {
                            name: tool.name,
                            description: tool.description,
                        })
                        .collect(),
                }
            }
            agentdash_application::mcp_preset::ProbeResult::Error { error } => {
                McpProbeTransportOutput::Error { error }
            }
            agentdash_application::mcp_preset::ProbeResult::Unsupported { reason } => {
                McpProbeTransportOutput::Unsupported { reason }
            }
        })
    }
}

#[derive(Clone)]
struct McpProbeBackendTargetResolver {
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
}

impl McpProbeBackendTargetResolver {
    fn new(repos: RepositorySet, backend_registry: Arc<BackendRegistry>) -> Self {
        Self {
            repos,
            backend_registry,
        }
    }

    async fn resolve(
        &self,
        identity: &AuthIdentity,
        target: &McpProbeTarget,
    ) -> Result<RelayProbeTarget, McpProbeBackendTargetResolutionError> {
        let online_backend_ids = self.backend_registry.list_online_ids().await;
        let resolved = resolve_mcp_probe_backend_target(
            self.repos.backend_repo.as_ref(),
            self.repos.project_repo.as_ref(),
            self.repos.project_backend_access_repo.as_ref(),
            identity,
            &mcp_probe_backend_target(target),
            &online_backend_ids,
        )
        .await?;
        Ok(RelayProbeTarget {
            backend_id: resolved.backend_id,
        })
    }
}

fn mcp_probe_backend_target(target: &McpProbeTarget) -> McpProbeBackendTarget {
    match target {
        McpProbeTarget::DefaultUserLocal => McpProbeBackendTarget::DefaultUserLocal,
        McpProbeTarget::Backend { backend_id } => McpProbeBackendTarget::Backend {
            backend_id: backend_id.clone(),
        },
    }
}

struct ApplicationWorkspaceSetupPort {
    transport: Arc<dyn BackendTransport>,
}

impl ApplicationWorkspaceSetupPort {
    fn new(transport: Arc<dyn BackendTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl WorkspaceDetectSetupPort for ApplicationWorkspaceSetupPort {
    async fn detect_workspace(
        &self,
        input: WorkspaceDetectInput,
    ) -> Result<WorkspaceDetectOutput, RuntimeGatewaySetupError> {
        let result = agentdash_application::workspace::detect_workspace_from_backend(
            self.transport.as_ref(),
            &input.backend_id,
            &input.root_ref,
        )
        .await
        .map_err(|error| match error {
            WorkspaceDetectionError::BadRequest(message) => {
                RuntimeGatewaySetupError::BadRequest(message)
            }
            WorkspaceDetectionError::BackendOffline(message) => {
                RuntimeGatewaySetupError::BackendOffline(message)
            }
            WorkspaceDetectionError::TransportFailed(message) => {
                RuntimeGatewaySetupError::TransportFailed(message)
            }
        })?;

        Ok(WorkspaceDetectOutput {
            identity_kind: result.identity_kind,
            identity_payload: result.identity_payload,
            binding: result.binding,
            confidence: result.confidence,
            warnings: result.warnings,
        })
    }
}

#[async_trait]
impl WorkspaceDetectGitSetupPort for ApplicationWorkspaceSetupPort {
    async fn detect_git(
        &self,
        input: WorkspaceDetectGitInput,
    ) -> Result<WorkspaceDetectGitOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        let root_ref = require_root_ref(&input.root_ref)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let info = self
            .transport
            .detect_git_repo(backend_id, root_ref)
            .await
            .map_err(setup_error_from_transport)?;
        Ok(WorkspaceDetectGitOutput {
            resolved_root_ref: root_ref.to_string(),
            is_git_repo: info.is_git_repo,
            source_repo: info.source_repo,
            branch: info.branch,
            commit_hash: info.commit_hash,
        })
    }
}

#[async_trait]
impl WorkspaceBrowseDirectorySetupPort for ApplicationWorkspaceSetupPort {
    async fn browse_directory(
        &self,
        input: WorkspaceBrowseDirectoryInput,
    ) -> Result<WorkspaceBrowseDirectoryOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let result = self
            .transport
            .browse_directory(backend_id, input.path.as_deref())
            .await
            .map_err(setup_error_from_transport)?;
        Ok(WorkspaceBrowseDirectoryOutput {
            current_path: result.current_path,
            entries: result
                .entries
                .into_iter()
                .map(|entry| WorkspaceBrowseDirectoryEntry {
                    name: entry.name,
                    path: entry.path,
                    is_dir: entry.is_dir,
                })
                .collect(),
        })
    }
}

#[async_trait]
impl WorkspaceDiscoverByIdentitySetupPort for ApplicationWorkspaceSetupPort {
    async fn discover_by_identity(
        &self,
        input: WorkspaceDiscoverByIdentityInput,
    ) -> Result<WorkspaceDiscoverByIdentityOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let result = self
            .transport
            .discover_workspace_by_identity(
                backend_id,
                input
                    .workspaces
                    .into_iter()
                    .map(|workspace| {
                        agentdash_application_ports::backend_transport::WorkspaceIdentityDiscoveryRequest {
                            workspace_id: workspace.workspace_id,
                            identity_kind: workspace.identity_kind,
                            identity_payload: workspace.identity_payload,
                        }
                    })
                    .collect(),
            )
            .await
            .map_err(setup_error_from_transport)?;

        Ok(WorkspaceDiscoverByIdentityOutput {
            candidates: result
                .candidates
                .into_iter()
                .map(|candidate| WorkspaceDiscoverByIdentityCandidateOutput {
                    workspace_id: candidate.workspace_id,
                    root_ref: candidate.root_ref,
                    identity_kind: candidate.identity_kind,
                    identity_payload: candidate.identity_payload,
                    detected_facts: candidate.detected_facts,
                    confidence: candidate.confidence,
                    display_name: candidate.display_name,
                    client_name: candidate.client_name,
                    server_address: candidate.server_address,
                    stream: candidate.stream,
                    warnings: candidate.warnings,
                })
                .collect(),
            skipped: result
                .skipped
                .into_iter()
                .map(|skipped| WorkspaceDiscoverByIdentitySkippedOutput {
                    workspace_id: skipped.workspace_id,
                    identity_kind: skipped.identity_kind,
                    reason: skipped.reason,
                    message: skipped.message,
                })
                .collect(),
            warnings: result.warnings,
        })
    }
}

fn require_backend_id(raw: &str) -> Result<&str, RuntimeGatewaySetupError> {
    let backend_id = raw.trim();
    if backend_id.is_empty() {
        return Err(RuntimeGatewaySetupError::BadRequest(
            "backend_id 不能为空".to_string(),
        ));
    }
    Ok(backend_id)
}

fn require_root_ref(raw: &str) -> Result<&str, RuntimeGatewaySetupError> {
    let root_ref = raw.trim();
    if root_ref.is_empty() {
        return Err(RuntimeGatewaySetupError::BadRequest(
            "root_ref 不能为空".to_string(),
        ));
    }
    Ok(root_ref)
}

async fn ensure_backend_online(
    transport: &dyn BackendTransport,
    backend_id: &str,
) -> Result<(), RuntimeGatewaySetupError> {
    if !transport.is_online(backend_id).await {
        return Err(RuntimeGatewaySetupError::BackendOffline(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }
    Ok(())
}

fn setup_error_from_transport(error: TransportError) -> RuntimeGatewaySetupError {
    match error {
        TransportError::BackendOffline(message) => {
            RuntimeGatewaySetupError::BackendOffline(message)
        }
        TransportError::OperationFailed(message) => {
            RuntimeGatewaySetupError::TransportFailed(message)
        }
        TransportError::Timeout => RuntimeGatewaySetupError::Timeout,
    }
}
