use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_injection::{AddressSpaceContext, AddressSpaceDescriptor};
use agentdash_relay::{CommandWorkspaceFilesListPayload, RelayMessage};

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct AddressSpacesQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddressSpacesResponse {
    pub spaces: Vec<AddressSpaceDescriptor>,
}

/// `GET /api/address-spaces` — 能力发现端点
///
/// 返回当前环境下可用的寻址空间列表。
/// 前端据此决定显示哪些引用入口（文件选择、MCP 资源、实体引用等）。
pub async fn list_address_spaces(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddressSpacesQuery>,
) -> Result<Json<AddressSpacesResponse>, ApiError> {
    let workspace_available = if let Some(ws_id_str) = &query.workspace_id {
        let ws_id = Uuid::parse_str(ws_id_str)
            .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
        state
            .workspace_repo
            .get_by_id(ws_id)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };

    let has_mcp = state.mcp_base_url.is_some();
    let ctx = AddressSpaceContext {
        workspace_root: workspace_available.then_some(std::path::Path::new(".")),
        has_mcp,
    };

    let spaces = state.address_space_registry.available_spaces(&ctx);

    Ok(Json(AddressSpacesResponse { spaces }))
}

#[derive(Debug, Deserialize)]
pub struct ListEntriesQuery {
    #[serde(default)]
    pub query: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddressEntry {
    pub address: String,
    pub label: String,
    pub entry_type: String,
}

#[derive(Debug, Serialize)]
pub struct ListEntriesResponse {
    pub entries: Vec<AddressEntry>,
}

/// `GET /api/address-spaces/{space_id}/entries` — 条目搜索端点
///
/// 根据 space_id 和可选的搜索 query，返回匹配的引用候选条目。
/// 当前仅 `workspace_file` 空间支持条目检索（复用现有 workspace-files 能力）。
pub async fn list_address_entries(
    State(state): State<Arc<AppState>>,
    Path(space_id): Path<String>,
    Query(query): Query<ListEntriesQuery>,
) -> Result<Json<ListEntriesResponse>, ApiError> {
    match space_id.as_str() {
        "workspace_file" => {
            let ws_id_str = query.workspace_id.as_deref().ok_or_else(|| {
                ApiError::BadRequest("workspace_file 空间需要提供 workspace_id".into())
            })?;
            let ws_id = Uuid::parse_str(ws_id_str)
                .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
            let workspace = state
                .workspace_repo
                .get_by_id(ws_id)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))?;
            let backend_id = workspace.backend_id.trim();
            if backend_id.is_empty() {
                return Err(ApiError::BadRequest(
                    "Workspace.backend_id 不能为空".to_string(),
                ));
            }
            if !state.backend_registry.is_online(backend_id).await {
                return Err(ApiError::Conflict(format!(
                    "Workspace 所属 Backend 当前不在线: {backend_id}"
                )));
            }

            let cmd = RelayMessage::CommandWorkspaceFilesList {
                id: RelayMessage::new_id("address-space-list"),
                payload: CommandWorkspaceFilesListPayload {
                    workspace_id: ws_id_str.to_string(),
                    root_path: Some(workspace.container_ref.clone()),
                    path: None,
                    pattern: query.query.clone(),
                },
            };
            let resp = state
                .backend_registry
                .send_command(backend_id, cmd)
                .await
                .map_err(|e| {
                    ApiError::Internal(format!("relay workspace_file.entries 失败: {e}"))
                })?;

            let entries = match resp {
                RelayMessage::ResponseWorkspaceFilesList {
                    payload: Some(payload),
                    ..
                } => payload
                    .files
                    .into_iter()
                    .map(|entry| AddressEntry {
                        address: entry.path.clone(),
                        label: entry.path,
                        entry_type: if entry.is_dir {
                            "directory".to_string()
                        } else {
                            "file".to_string()
                        },
                    })
                    .take(50)
                    .collect(),
                RelayMessage::ResponseWorkspaceFilesList {
                    error: Some(err), ..
                } => {
                    return Err(ApiError::Internal(format!(
                        "远程 workspace_file.entries 错误: {}",
                        err.message
                    )));
                }
                _ => {
                    return Err(ApiError::Internal(
                        "远程 workspace_file.entries 返回了意外响应".into(),
                    ));
                }
            };

            Ok(Json(ListEntriesResponse { entries }))
        }
        _ => Err(ApiError::NotFound(format!(
            "寻址空间 '{space_id}' 不存在或不支持条目检索"
        ))),
    }
}
