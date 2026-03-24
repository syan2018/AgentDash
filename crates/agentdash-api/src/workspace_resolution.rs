use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
    WorkspaceResolutionPolicy,
};

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolvedWorkspaceBinding {
    pub workspace_id: Uuid,
    pub binding_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub resolution_reason: String,
    pub warnings: Vec<String>,
    pub detected_facts: Value,
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

pub async fn resolve_workspace_binding(
    state: &Arc<AppState>,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, ApiError> {
    if workspace.bindings.is_empty() {
        return Err(ApiError::Conflict(format!(
            "Workspace `{}` 当前还没有任何可解析 binding",
            workspace.name
        )));
    }

    let mut warnings = Vec::new();
    let mut online_candidates = Vec::new();
    for binding in &workspace.bindings {
        let backend_id = binding.backend_id.trim();
        if backend_id.is_empty() {
            warnings.push(format!("binding `{}` 缺少 backend_id", binding.id));
            continue;
        }
        let is_online = state.services.backend_registry.is_online(backend_id).await;
        if !is_online {
            warnings.push(format!("backend `{backend_id}` 当前离线"));
        }
        online_candidates.push((binding, is_online));
    }

    let selected = match workspace.resolution_policy {
        WorkspaceResolutionPolicy::PreferDefaultBinding => {
            select_default_binding(workspace, &online_candidates)
                .or_else(|| select_first_online(&online_candidates))
                .or_else(|| online_candidates.first().map(|(binding, _)| *binding))
        }
        WorkspaceResolutionPolicy::PreferOnline => {
            select_first_online(&online_candidates)
                .or_else(|| select_default_binding(workspace, &online_candidates))
                .or_else(|| online_candidates.first().map(|(binding, _)| *binding))
        }
    };

    let Some(binding) = selected else {
        return Err(ApiError::Conflict(format!(
            "Workspace `{}` 没有可用 binding",
            workspace.name
        )));
    };

    Ok(ResolvedWorkspaceBinding {
        workspace_id: workspace.id,
        binding_id: binding.id,
        backend_id: binding.backend_id.trim().to_string(),
        root_ref: binding.root_ref.trim().to_string(),
        resolution_reason: build_resolution_reason(workspace, binding),
        warnings,
        detected_facts: binding.detected_facts.clone(),
    })
}

fn select_default_binding<'a>(
    workspace: &Workspace,
    bindings: &'a [(&'a WorkspaceBinding, bool)],
) -> Option<&'a WorkspaceBinding> {
    let default_binding_id = workspace.default_binding_id?;
    bindings
        .iter()
        .find(|(binding, _)| binding.id == default_binding_id)
        .map(|(binding, _)| *binding)
}

fn select_first_online<'a>(
    bindings: &'a [(&'a WorkspaceBinding, bool)],
) -> Option<&'a WorkspaceBinding> {
    bindings
        .iter()
        .filter(|(_, online)| *online)
        .map(|(binding, _)| *binding)
        .max_by_key(|binding| binding.priority)
}

fn build_resolution_reason(workspace: &Workspace, binding: &WorkspaceBinding) -> String {
    if workspace.default_binding_id == Some(binding.id) {
        return "命中默认 binding".to_string();
    }
    match workspace.resolution_policy {
        WorkspaceResolutionPolicy::PreferDefaultBinding => "默认 binding 不可用，回退到候选 binding".to_string(),
        WorkspaceResolutionPolicy::PreferOnline => "根据在线 backend 选择候选 binding".to_string(),
    }
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

    let git = crate::routes::workspaces::detect_git_via_backend(state, backend_id, root_ref).await?;
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
        warnings.push("当前未识别为 Git 仓库，已按 local_dir 处理。P4 自动识别尚未接入。".to_string());
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
