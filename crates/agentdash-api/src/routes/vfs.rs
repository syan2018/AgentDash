use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::vfs::{ListOptions, selected_workspace_binding};
use agentdash_spi::{VfsContext, VfsDescriptor};

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_workspace_and_project_with_permission,
    },
    rpc::ApiError,
};

const MAX_ENTRIES: usize = 200;

// ─── 能力发现 ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct VfssQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VfssResponse {
    pub spaces: Vec<VfsDescriptor>,
}

/// `GET /api/vfs` — 能力发现端点
pub async fn list_vfs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<VfssQuery>,
) -> Result<Json<VfssResponse>, ApiError> {
    let workspace_available = if let Some(ws_id_str) = &query.workspace_id {
        let ws_id = Uuid::parse_str(ws_id_str)
            .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
        let (workspace, _) = load_workspace_and_project_with_permission(
            state.as_ref(),
            &current_user,
            ws_id,
            ProjectPermission::View,
        )
        .await?;
        let _ = workspace;
        true
    } else {
        false
    };

    let has_mcp = state.config.mcp_base_url.is_some();
    let ctx = VfsContext {
        workspace_available,
        has_mcp,
    };

    let spaces = state.services.vfs_registry.available_spaces(&ctx);
    Ok(Json(VfssResponse { spaces }))
}

// ─── 条目检索 ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListEntriesQuery {
    #[serde(default)]
    pub query: Option<String>,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct VfsEntry {
    pub address: String,
    pub label: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dir: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListEntriesResponse {
    pub entries: Vec<VfsEntry>,
}

/// `GET /api/vfs/{space_id}/entries` — 条目搜索端点
pub async fn list_address_entries(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(space_id): Path<String>,
    Query(query): Query<ListEntriesQuery>,
) -> Result<Json<ListEntriesResponse>, ApiError> {
    match space_id.as_str() {
        "workspace_file" => {
            let workspace = load_workspace(
                &state,
                &current_user,
                &query.workspace_id,
                ProjectPermission::View,
            )
            .await?;
            require_backend_online(&state, &workspace).await?;

            let session = state
                .services
                .vfs_service
                .session_for_workspace(&workspace)
                .map_err(ApiError::BadRequest)?;

            let base_path = query.path.as_deref().unwrap_or(".");
            let recursive = query.recursive.unwrap_or(true);

            let listed = state
                .services
                .vfs_service
                .list(
                    &session,
                    "main",
                    ListOptions {
                        path: base_path.to_string(),
                        pattern: query.query.clone(),
                        recursive,
                    },
                    None,
                    None,
                )
                .await
                .map_err(ApiError::Internal)?;

            let entries = listed
                .entries
                .into_iter()
                .take(MAX_ENTRIES)
                .map(|entry| VfsEntry {
                    address: entry.path.clone(),
                    label: entry.path,
                    entry_type: if entry.is_dir {
                        "directory".to_string()
                    } else {
                        "file".to_string()
                    },
                    size: entry.size,
                    is_dir: Some(entry.is_dir),
                })
                .collect();

            Ok(Json(ListEntriesResponse { entries }))
        }
        _ => Err(ApiError::NotFound(format!(
            "寻址空间 '{space_id}' 不存在或不支持条目检索"
        ))),
    }
}

// ─── 辅助函数 ──────────────────────────────────────────────

async fn load_workspace(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    workspace_id: &Option<String>,
    permission: ProjectPermission,
) -> Result<agentdash_domain::workspace::Workspace, ApiError> {
    let ws_id_str = workspace_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("需要提供 workspace_id".into()))?;
    let ws_id = Uuid::parse_str(ws_id_str)
        .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
    let (workspace, _) =
        load_workspace_and_project_with_permission(state.as_ref(), current_user, ws_id, permission)
            .await?;
    Ok(workspace)
}

async fn require_backend_online(
    state: &Arc<AppState>,
    workspace: &agentdash_domain::workspace::Workspace,
) -> Result<(), ApiError> {
    let backend_id = selected_workspace_binding(workspace)
        .map(|binding| binding.backend_id.as_str())
        .unwrap_or("");
    let trimmed = backend_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(
            "Workspace 当前没有可用 binding.backend_id".to_string(),
        ));
    }
    if !state.services.backend_registry.is_online(trimmed).await {
        return Err(ApiError::Conflict(format!(
            "Workspace 所属 Backend 当前不在线: {trimmed}"
        )));
    }
    Ok(())
}

// ─── Mount Provider 发现 ──────────────────────────────────

/// `GET /api/mount-providers` — 返回所有可由用户配置的 mount provider。
///
/// 前端用于构建 ExternalService 容器的 provider 选择列表。
/// 数据直接来自 MountProviderRegistry 中各 provider 自身声明的元信息。
pub async fn list_configurable_mount_providers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<agentdash_application::vfs::ConfigurableProviderInfo>> {
    Json(
        state
            .services
            .mount_provider_registry
            .user_configurable_providers(),
    )
}
