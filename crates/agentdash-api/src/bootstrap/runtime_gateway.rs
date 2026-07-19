use std::sync::Arc;

use agentdash_application::backend::{
    McpProbeBackendTarget, McpProbeBackendTargetResolutionError, resolve_mcp_probe_backend_target,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::workspace::WorkspaceDetectionError;
use agentdash_application_ports::backend_transport::{BackendTransport, TransportError};
use agentdash_application_ports::extension_runtime::ExtensionRuntimeActionTransport;
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeActionProvider, McpProbeSetupPort, McpProbeTarget, McpProbeToolOutput,
    McpProbeTransportInput, McpProbeTransportOutput, McpProbeTransportProvider, RuntimeGateway,
    RuntimeGatewaySetupError, WorkspaceBrowseDirectoryEntry, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput, WorkspaceBrowseDirectoryProvider,
    WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput, WorkspaceDetectGitOutput,
    WorkspaceDetectGitProvider, WorkspaceDetectGitSetupPort, WorkspaceDetectInput,
    WorkspaceDetectOutput, WorkspaceDetectProvider, WorkspaceDetectSetupPort,
    WorkspaceDiscoverByIdentityCandidateOutput, WorkspaceDiscoverByIdentityInput,
    WorkspaceDiscoverByIdentityOutput, WorkspaceDiscoverByIdentityProvider,
    WorkspaceDiscoverByIdentitySetupPort, WorkspaceDiscoverByIdentitySkippedOutput,
};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_platform_spi::AuthIdentity;
use agentdash_platform_spi::platform::mcp_probe::McpProbeTransport;
use agentdash_platform_spi::platform::mcp_relay::{McpRelayProvider, RelayProbeTarget};
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;

pub(crate) fn build_runtime_gateway(
    mcp_probe_relay: Arc<dyn agentdash_platform_spi::McpRelayProvider>,
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
    setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    >,
    extension_installations: Arc<dyn ProjectExtensionInstallationRepository>,
    extension_action_transport: Arc<dyn ExtensionRuntimeActionTransport>,
) -> Arc<RuntimeGateway> {
    let mcp_probe_setup = Arc::new(ApplicationMcpProbeSetupPort::new(
        Some(mcp_probe_relay),
        McpProbeBackendTargetResolver::new(repos, backend_registry),
        Arc::new(agentdash_infrastructure::RmcpProbeTransport::new()),
    ));
    let workspace_setup = Arc::new(ApplicationWorkspaceSetupPort::new(setup_action_transport));

    Arc::new(
        RuntimeGateway::new()
            .with_provider(Arc::new(McpProbeTransportProvider::new(mcp_probe_setup)))
            .with_provider(Arc::new(WorkspaceDetectProvider::new(
                workspace_setup.clone(),
            )))
            .with_provider(Arc::new(WorkspaceDetectGitProvider::new(
                workspace_setup.clone(),
            )))
            .with_provider(Arc::new(WorkspaceBrowseDirectoryProvider::new(
                workspace_setup.clone(),
            )))
            .with_provider(Arc::new(WorkspaceDiscoverByIdentityProvider::new(
                workspace_setup,
            )))
            .with_dynamic_provider(Arc::new(ExtensionRuntimeActionProvider::new(
                extension_installations,
                extension_action_transport,
            ))),
    )
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
