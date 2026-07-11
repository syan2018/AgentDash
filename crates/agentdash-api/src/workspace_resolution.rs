use std::sync::Arc;

use crate::app_state::AppState;
use crate::relay::registry::{BackendCommandError, BackendRegistry};
use crate::rpc::ApiError;
pub use agentdash_application::workspace::ResolvedWorkspaceBinding;
use agentdash_application::workspace::{WorkspaceDetectionError, WorkspaceResolutionError};
use agentdash_application_ports::backend_transport::{
    BackendTransport, DirectoryBrowseInfo, DirectoryEntryInfo, GitRepoInfo, P4WorkspaceInfo,
    TransportError, WorkspaceIdentityDiscoveryCandidate, WorkspaceIdentityDiscoveryInfo,
    WorkspaceIdentityDiscoveryRequest, WorkspaceIdentityDiscoverySkipped, WorkspaceProbeInfo,
};
use agentdash_domain::workspace::{Workspace, WorkspaceIdentityKind};
use agentdash_relay::{
    CommandBrowseDirectoryPayload, CommandWorkspaceDetectPayload,
    CommandWorkspaceDiscoverByIdentityPayload, RelayMessage,
    WorkspaceIdentityDiscoveryWorkspaceRelay, WorkspaceIdentityKindRelay,
};
use async_trait::async_trait;
use uuid::Uuid;

pub use agentdash_application::workspace::WorkspaceDetectionResult;
pub use agentdash_application::workspace::resolve_workspace_binding_with_allowed_backends as resolve_workspace_binding_core;

fn map_backend_command_error(error: BackendCommandError) -> TransportError {
    match error {
        BackendCommandError::Offline { backend_id } => TransportError::BackendOffline(backend_id),
        BackendCommandError::Timeout { .. } => TransportError::Timeout,
        BackendCommandError::SendFailed { .. } | BackendCommandError::ResponseDropped { .. } => {
            TransportError::OperationFailed(error.to_string())
        }
    }
}

/// BackendRegistry 适配 BackendTransport trait —— API adapter 层
#[async_trait]
impl BackendTransport for BackendRegistry {
    async fn is_online(&self, backend_id: &str) -> bool {
        self.is_online(backend_id).await
    }

    async fn list_online_backend_ids(&self) -> Vec<String> {
        self.list_online_ids().await
    }

    async fn detect_workspace(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<WorkspaceProbeInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandWorkspaceDetect {
            id: RelayMessage::new_id("workspace-detect"),
            payload: CommandWorkspaceDetectPayload {
                path: root.to_string(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(map_backend_command_error)?;

        match resp {
            RelayMessage::ResponseWorkspaceDetect {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(WorkspaceProbeInfo {
                git: payload.git.map(|git| GitRepoInfo {
                    is_git_repo: true,
                    repo_root: Some(git.repo_root),
                    source_repo: git.remote_url,
                    default_branch: git.default_branch,
                    branch: git.current_branch,
                    commit_hash: git.commit_hash,
                }),
                p4: payload.p4.map(|p4| P4WorkspaceInfo {
                    is_p4_workspace: true,
                    workspace_root: Some(p4.workspace_root),
                    client_name: p4.client_name,
                    server_address: p4.server_address,
                    user_name: p4.user_name,
                    stream: p4.stream,
                }),
                warnings: payload.warnings,
            }),
            RelayMessage::ResponseWorkspaceDetect {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 workspace_detect 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 workspace_detect 返回了意外响应".into(),
            )),
        }
    }

    async fn browse_directory(
        &self,
        backend_id: &str,
        path: Option<&str>,
    ) -> Result<DirectoryBrowseInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandBrowseDirectory {
            id: RelayMessage::new_id("browse-dir"),
            payload: CommandBrowseDirectoryPayload {
                path: path.map(str::to_string),
            },
        };
        let resp =
            self.send_command(backend_id, cmd).await.map_err(
                |e| match map_backend_command_error(e) {
                    TransportError::OperationFailed(message) => TransportError::OperationFailed(
                        format!("relay browse_directory 失败: {message}"),
                    ),
                    other => other,
                },
            )?;

        match resp {
            RelayMessage::ResponseBrowseDirectory {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(DirectoryBrowseInfo {
                current_path: payload.current_path,
                entries: payload
                    .entries
                    .into_iter()
                    .map(|entry| DirectoryEntryInfo {
                        name: entry.name,
                        path: entry.path,
                        is_dir: entry.is_dir,
                    })
                    .collect(),
            }),
            RelayMessage::ResponseBrowseDirectory {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 browse_directory 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 browse_directory 返回了意外响应".into(),
            )),
        }
    }

    async fn discover_workspace_by_identity(
        &self,
        backend_id: &str,
        workspaces: Vec<WorkspaceIdentityDiscoveryRequest>,
    ) -> Result<WorkspaceIdentityDiscoveryInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandWorkspaceDiscoverByIdentity {
            id: RelayMessage::new_id("workspace-discover-identity"),
            payload: CommandWorkspaceDiscoverByIdentityPayload {
                workspaces: workspaces
                    .into_iter()
                    .map(|workspace| WorkspaceIdentityDiscoveryWorkspaceRelay {
                        workspace_id: workspace.workspace_id.to_string(),
                        identity_kind: identity_kind_to_relay(workspace.identity_kind),
                        identity_payload: workspace.identity_payload,
                    })
                    .collect(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(map_backend_command_error)?;

        match resp {
            RelayMessage::ResponseWorkspaceDiscoverByIdentity {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let candidates = payload
                    .candidates
                    .into_iter()
                    .map(|candidate| {
                        Ok::<_, TransportError>(WorkspaceIdentityDiscoveryCandidate {
                            workspace_id: parse_relay_uuid(
                                &candidate.workspace_id,
                                "workspace_id",
                            )?,
                            root_ref: candidate.root_ref,
                            identity_kind: identity_kind_from_relay(candidate.identity_kind),
                            identity_payload: candidate.identity_payload,
                            detected_facts: candidate.detected_facts,
                            confidence: candidate.confidence,
                            display_name: candidate.display_name,
                            client_name: candidate.client_name,
                            server_address: candidate.server_address,
                            stream: candidate.stream,
                            warnings: candidate.warnings,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let skipped = payload
                    .skipped
                    .into_iter()
                    .map(|skipped| {
                        Ok::<_, TransportError>(WorkspaceIdentityDiscoverySkipped {
                            workspace_id: parse_relay_uuid(&skipped.workspace_id, "workspace_id")?,
                            identity_kind: identity_kind_from_relay(skipped.identity_kind),
                            reason: skipped.reason,
                            message: skipped.message,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(WorkspaceIdentityDiscoveryInfo {
                    candidates,
                    skipped,
                    warnings: payload.warnings,
                })
            }
            RelayMessage::ResponseWorkspaceDiscoverByIdentity {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 workspace_discover_by_identity 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 workspace_discover_by_identity 返回了意外响应".into(),
            )),
        }
    }
}

fn identity_kind_to_relay(kind: WorkspaceIdentityKind) -> WorkspaceIdentityKindRelay {
    match kind {
        WorkspaceIdentityKind::GitRepo => WorkspaceIdentityKindRelay::GitRepo,
        WorkspaceIdentityKind::P4Workspace => WorkspaceIdentityKindRelay::P4Workspace,
        WorkspaceIdentityKind::LocalDir => WorkspaceIdentityKindRelay::LocalDir,
    }
}

fn identity_kind_from_relay(kind: WorkspaceIdentityKindRelay) -> WorkspaceIdentityKind {
    match kind {
        WorkspaceIdentityKindRelay::GitRepo => WorkspaceIdentityKind::GitRepo,
        WorkspaceIdentityKindRelay::P4Workspace => WorkspaceIdentityKind::P4Workspace,
        WorkspaceIdentityKindRelay::LocalDir => WorkspaceIdentityKind::LocalDir,
    }
}

fn parse_relay_uuid(raw: &str, field: &str) -> Result<Uuid, TransportError> {
    Uuid::parse_str(raw).map_err(|error| {
        TransportError::OperationFailed(format!(
            "远程 workspace_discover_by_identity 返回非法 {field}: {error}"
        ))
    })
}

/// 薄 API adapter：解析 workspace binding（错误映射到 ApiError）
pub async fn resolve_workspace_binding(
    state: &Arc<AppState>,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, ApiError> {
    let accesses = state
        .repos
        .project_backend_access_repo
        .list_active_by_project(workspace.project_id)
        .await?;
    let allowed_backend_ids = accesses
        .into_iter()
        .map(|access| access.backend_id)
        .collect::<std::collections::HashSet<_>>();
    resolve_workspace_binding_core(
        state.services.backend_registry.as_ref(),
        workspace,
        Some(&allowed_backend_ids),
    )
    .await
    .map_err(|err| match err {
        WorkspaceResolutionError::NoBindings(msg) | WorkspaceResolutionError::NoAvailable(msg) => {
            ApiError::Conflict(msg)
        }
    })
}

/// 薄 API adapter：探测远程 workspace（错误映射到 ApiError）
pub async fn detect_workspace_from_backend(
    state: &Arc<AppState>,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectionResult, ApiError> {
    agentdash_application::workspace::detect_workspace_from_backend(
        state.services.backend_registry.as_ref(),
        backend_id,
        root_ref,
    )
    .await
    .map_err(|err| match err {
        WorkspaceDetectionError::BadRequest(msg) => ApiError::BadRequest(msg),
        WorkspaceDetectionError::BackendOffline(msg) => ApiError::Conflict(msg),
        WorkspaceDetectionError::TransportFailed(msg) => ApiError::Internal(msg),
    })
}
