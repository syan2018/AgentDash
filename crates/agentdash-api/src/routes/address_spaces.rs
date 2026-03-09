use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_injection::{AddressSpaceContext, AddressSpaceDescriptor};

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
    let workspace_root = if let Some(ws_id_str) = &query.workspace_id {
        let ws_id = Uuid::parse_str(ws_id_str)
            .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
        state
            .workspace_repo
            .get_by_id(ws_id)
            .await
            .ok()
            .flatten()
            .map(|ws| std::path::PathBuf::from(&ws.container_ref))
    } else {
        None
    };

    let has_mcp = state.mcp_base_url.is_some();
    let ctx = AddressSpaceContext {
        workspace_root: workspace_root.as_deref(),
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

            let root = std::path::Path::new(&workspace.container_ref);
            let search_query = query.query.as_deref().unwrap_or("");
            let entries = list_workspace_file_entries(root, search_query);

            Ok(Json(ListEntriesResponse { entries }))
        }
        _ => Err(ApiError::NotFound(format!(
            "寻址空间 '{space_id}' 不存在或不支持条目检索"
        ))),
    }
}

fn list_workspace_file_entries(root: &std::path::Path, query: &str) -> Vec<AddressEntry> {
    let query_lower = query.to_lowercase();
    let mut entries = Vec::new();

    let walker = walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                "node_modules" | "target" | ".git" | ".next" | "dist" | "__pycache__"
            )
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if entry.path() == root {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .display()
            .to_string()
            .replace('\\', "/");

        if !query_lower.is_empty() && !rel.to_lowercase().contains(&query_lower) {
            continue;
        }

        let entry_type = if entry.file_type().is_dir() {
            "directory"
        } else {
            "file"
        };

        entries.push(AddressEntry {
            label: rel.clone(),
            address: rel,
            entry_type: entry_type.to_string(),
        });

        if entries.len() >= 50 {
            break;
        }
    }

    entries
}
