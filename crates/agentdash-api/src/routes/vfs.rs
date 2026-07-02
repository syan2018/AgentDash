use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_application_vfs::{ListOptions, selected_workspace_binding};
use agentdash_contracts::vfs::{
    ConfigurableProviderInfo, ListEntriesResponse, ListVfssResponse, SelectorHint, VfsDescriptor,
    VfsEntry,
};
use agentdash_spi::VfsContext;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_workspace_and_project_with_permission},
    dto::{ListEntriesQuery, VfssQuery},
    rpc::ApiError,
};

const MAX_ENTRIES: usize = 200;

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/mount-providers",
            axum::routing::get(list_configurable_mount_providers),
        )
        .route("/vfs", axum::routing::get(list_vfs))
        .route(
            "/vfs/{space_id}/entries",
            axum::routing::get(list_address_entries),
        )
}

// ─── 能力发现 ──────────────────────────────────────────────

/// `GET /api/vfs` — 能力发现端点
pub async fn list_vfs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<VfssQuery>,
) -> Result<Json<ListVfssResponse>, ApiError> {
    let workspace_available = if let Some(ws_id_str) = &query.workspace_id {
        let ws_id = Uuid::parse_str(ws_id_str)
            .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
        let (workspace, _) = load_workspace_and_project_with_permission(
            state.as_ref(),
            &current_user,
            ws_id,
            ProjectPermission::Use,
        )
        .await?;
        let _ = workspace;
        true
    } else {
        false
    };

    let has_mcp = state.config.platform_config.mcp_base_url.is_some();
    let ctx = VfsContext {
        workspace_available,
        has_mcp,
    };

    let spaces = state
        .services
        .vfs_registry
        .available_spaces(&ctx)
        .into_iter()
        .map(vfs_descriptor_from_spi)
        .collect();
    Ok(Json(ListVfssResponse { spaces }))
}

// ─── 条目检索 ──────────────────────────────────────────────

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
                ProjectPermission::Use,
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
                    Some(&current_user),
                )
                .await
                .map_err(|e| ApiError::Internal(format!("VFS 条目检索失败: {e}")))?;

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
    current_user: &agentdash_integration_api::AuthIdentity,
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
) -> Json<Vec<ConfigurableProviderInfo>> {
    Json(
        state
            .services
            .mount_provider_registry
            .user_configurable_providers()
            .into_iter()
            .map(|provider| ConfigurableProviderInfo {
                service_id: provider.service_id,
                display_name: provider.display_name,
                root_ref_hint: provider.root_ref_hint,
                supported_capabilities: provider.supported_capabilities,
            })
            .collect(),
    )
}

fn vfs_descriptor_from_spi(descriptor: agentdash_spi::VfsDescriptor) -> VfsDescriptor {
    VfsDescriptor {
        id: descriptor.id,
        label: descriptor.label,
        kind: serde_json::to_value(descriptor.kind)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "file".to_string()),
        provider: descriptor.provider,
        supports: descriptor.supports,
        selector: descriptor.selector.map(|selector| SelectorHint {
            trigger: selector.trigger,
            placeholder: selector.placeholder,
            result_item_type: selector.result_item_type,
        }),
    }
}
