use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub use agentdash_application::workspace::ResolvedWorkspaceBinding;
use agentdash_application::workspace::{
    BackendAvailability, WorkspaceResolutionError,
};
use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
};
use async_trait::async_trait;

use crate::app_state::AppState;
use crate::rpc::ApiError;

pub use agentdash_application::workspace::resolve_workspace_binding as resolve_workspace_binding_core;

pub(crate) struct AppStateBackendAvailability {
    state: Arc<AppState>,
}

impl AppStateBackendAvailability {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl BackendAvailability for AppStateBackendAvailability {
    async fn is_online(&self, backend_id: &str) -> bool {
        self.state.services.backend_registry.is_online(backend_id).await
    }
}

pub async fn resolve_workspace_binding(
    state: &Arc<AppState>,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, ApiError> {
    let availability = AppStateBackendAvailability::new(state.clone());
    resolve_workspace_binding_core(&availability, workspace)
        .await
        .map_err(|err| match err {
            WorkspaceResolutionError::NoBindings(msg) | WorkspaceResolutionError::NoAvailable(msg) => {
                ApiError::Conflict(msg)
            }
        })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceDetectionResult {
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub binding: WorkspaceBinding,
    pub confidence: String,
    pub warnings: Vec<String>,
}

pub async fn detect_workspace_from_backend(
    state: &Arc<AppState>,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectionResult, ApiError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("backend_id 不能为空".into()));
    }
    let root_ref = root_ref.trim();
    if root_ref.is_empty() {
        return Err(ApiError::BadRequest("root_ref 不能为空".into()));
    }
    if !state.services.backend_registry.is_online(backend_id).await {
        return Err(ApiError::Conflict(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }

    let git =
        crate::routes::workspaces::detect_git_via_backend(state, backend_id, root_ref).await?;
    let mut warnings = Vec::new();
    let (identity_kind, identity_payload, confidence) = if git.is_git_repo {
        (
            WorkspaceIdentityKind::GitRepo,
            json!({
                "remote_url": git.source_repo.clone(),
                "branch": git.branch.clone(),
                "root_hint": root_ref,
            }),
            "high".to_string(),
        )
    } else {
        warnings
            .push("当前未识别为 Git 仓库，已按 local_dir 处理。P4 自动识别尚未接入。".to_string());
        (
            WorkspaceIdentityKind::LocalDir,
            json!({ "root_hint": root_ref }),
            "medium".to_string(),
        )
    };

    let mut binding = WorkspaceBinding::new(
        Uuid::nil(),
        backend_id.to_string(),
        root_ref.to_string(),
        json!({
            "git": {
                "is_repo": git.is_git_repo,
                "source_repo": git.source_repo,
                "branch": git.branch,
                "commit_hash": git.commit_hash,
            },
        }),
    );
    binding.status = WorkspaceBindingStatus::Ready;
    binding.last_verified_at = Some(Utc::now());

    Ok(WorkspaceDetectionResult {
        identity_kind,
        identity_payload,
        binding,
        confidence,
        warnings,
    })
}
