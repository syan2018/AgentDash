use std::sync::Arc;

pub use agentdash_application::workspace::ResolvedWorkspaceBinding;
use agentdash_application::workspace::{
    WorkspaceDetectionError, WorkspaceResolutionError,
};
use agentdash_application::backend_transport::{BackendTransport, GitRepoInfo, TransportError};
use agentdash_domain::workspace::Workspace;
use agentdash_relay::{CommandWorkspaceDetectGitPayload, RelayMessage};
use async_trait::async_trait;

use crate::app_state::AppState;
use crate::relay::registry::BackendRegistry;
use crate::rpc::ApiError;

pub use agentdash_application::workspace::resolve_workspace_binding as resolve_workspace_binding_core;
pub use agentdash_application::workspace::WorkspaceDetectionResult;

/// BackendRegistry 适配 BackendTransport trait —— API adapter 层
#[async_trait]
impl BackendTransport for BackendRegistry {
    async fn is_online(&self, backend_id: &str) -> bool {
        self.is_online(backend_id).await
    }

    async fn list_online_backend_ids(&self) -> Vec<String> {
        self.list_online_ids().await
    }

    async fn detect_git_repo(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<GitRepoInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandWorkspaceDetectGit {
            id: RelayMessage::new_id("workspace-detect-git"),
            payload: CommandWorkspaceDetectGitPayload {
                path: root.to_string(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(|e| TransportError::OperationFailed(e.to_string()))?;

        match resp {
            RelayMessage::ResponseWorkspaceDetectGit {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(GitRepoInfo {
                is_git_repo: payload.is_git,
                source_repo: payload
                    .remote_url
                    .clone()
                    .or_else(|| payload.is_git.then(|| root.to_string())),
                branch: payload.current_branch.or(payload.default_branch),
                commit_hash: None,
            }),
            RelayMessage::ResponseWorkspaceDetectGit {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 workspace_detect_git 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 workspace_detect_git 返回了意外响应".into(),
            )),
        }
    }
}

/// 薄 API adapter：解析 workspace binding（错误映射到 ApiError）
pub async fn resolve_workspace_binding(
    state: &Arc<AppState>,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, ApiError> {
    resolve_workspace_binding_core(state.services.backend_registry.as_ref(), workspace)
        .await
        .map_err(|err| match err {
            WorkspaceResolutionError::NoBindings(msg)
            | WorkspaceResolutionError::NoAvailable(msg) => ApiError::Conflict(msg),
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
