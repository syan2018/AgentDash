use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_executor::ExecutionAddressSpace;
use agentdash_injection::{AddressSpaceContext, AddressSpaceDescriptor};

use crate::address_space_access::{
    ListOptions, ReadResult, ResourceRef, SessionMountTarget, inline_files_from_mount,
    normalize_mount_relative_path, PROVIDER_INLINE_FS,
};
use crate::app_state::AppState;
use crate::rpc::ApiError;

const MAX_ENTRIES: usize = 200;

// ─── 能力发现 ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddressSpacesQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddressSpacesResponse {
    pub spaces: Vec<AddressSpaceDescriptor>,
}

/// `GET /api/address-spaces` — 能力发现端点
pub async fn list_address_spaces(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddressSpacesQuery>,
) -> Result<Json<AddressSpacesResponse>, ApiError> {
    let workspace_available = if let Some(ws_id_str) = &query.workspace_id {
        let ws_id = Uuid::parse_str(ws_id_str)
            .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
        state
            .repos
            .workspace_repo
            .get_by_id(ws_id)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };

    let has_mcp = state.config.mcp_base_url.is_some();
    let ctx = AddressSpaceContext {
        workspace_root: workspace_available.then_some(std::path::Path::new(".")),
        has_mcp,
    };

    let spaces = state.services.address_space_registry.available_spaces(&ctx);

    Ok(Json(AddressSpacesResponse { spaces }))
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
pub struct AddressEntry {
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
    pub entries: Vec<AddressEntry>,
}

/// `GET /api/address-spaces/{space_id}/entries` — 条目搜索端点
pub async fn list_address_entries(
    State(state): State<Arc<AppState>>,
    Path(space_id): Path<String>,
    Query(query): Query<ListEntriesQuery>,
) -> Result<Json<ListEntriesResponse>, ApiError> {
    match space_id.as_str() {
        "workspace_file" => {
            let workspace = load_workspace(&state, &query.workspace_id).await?;
            require_backend_online(&state, &workspace.backend_id).await?;

            let session = state
                .services
                .address_space_service
                .session_for_workspace(&workspace)
                .map_err(ApiError::BadRequest)?;

            let base_path = query.path.as_deref().unwrap_or(".");
            let recursive = query.recursive.unwrap_or(true);

            let listed = state
                .services
                .address_space_service
                .list(
                    &session,
                    "main",
                    ListOptions {
                        path: base_path.to_string(),
                        pattern: query.query.clone(),
                        recursive,
                    },
                    None,
                )
                .await
                .map_err(ApiError::Internal)?;

            let entries = listed
                .entries
                .into_iter()
                .take(MAX_ENTRIES)
                .map(|entry| AddressEntry {
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

// ─── Mount 级条目列表 ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListMountEntriesQuery {
    pub project_id: Option<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MountEntry {
    pub path: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct ListMountEntriesResponse {
    pub mount_id: String,
    pub entries: Vec<MountEntry>,
}

/// `GET /api/address-spaces/mounts/{mount_id}/entries`
///
/// 通过 project_id + story_id 构建完整 address space，然后列出指定 mount 下的条目。
pub async fn list_mount_entries(
    State(state): State<Arc<AppState>>,
    Path(mount_id): Path<String>,
    Query(query): Query<ListMountEntriesQuery>,
) -> Result<Json<ListMountEntriesResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &query.project_id,
        &query.story_id,
    )
    .await?;

    check_mount_backend_online(&state, &address_space, &mount_id).await?;

    let base_path = query.path.as_deref().unwrap_or(".");
    let recursive = query.recursive.unwrap_or(false);

    let listed = state
        .services
        .address_space_service
        .list(
            &address_space,
            &mount_id,
            ListOptions {
                path: base_path.to_string(),
                pattern: query.pattern.clone(),
                recursive,
            },
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    let entries = listed
        .entries
        .into_iter()
        .take(MAX_ENTRIES)
        .map(|entry| MountEntry {
            path: entry.path,
            entry_type: if entry.is_dir {
                "directory".to_string()
            } else {
                "file".to_string()
            },
            size: entry.size,
            is_dir: entry.is_dir,
        })
        .collect();

    Ok(Json(ListMountEntriesResponse {
        mount_id,
        entries,
    }))
}

// ─── Mount 级文件读取 ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReadMountFileRequest {
    pub project_id: Option<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ReadMountFileResponse {
    pub mount_id: String,
    pub path: String,
    pub content: String,
    pub size: u64,
}

/// `POST /api/address-spaces/read-file`
///
/// 通过 project_id + story_id 构建完整 address space，然后读取指定 mount 下的文件。
pub async fn read_mount_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReadMountFileRequest>,
) -> Result<Json<ReadMountFileResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &req.project_id,
        &req.story_id,
    )
    .await?;

    check_mount_backend_online(&state, &address_space, &req.mount_id).await?;

    let result: ReadResult = state
        .services
        .address_space_service
        .read_text(
            &address_space,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    let size = result.content.len() as u64;
    Ok(Json(ReadMountFileResponse {
        mount_id: req.mount_id,
        path: result.path,
        content: result.content,
        size,
    }))
}

// ─── Mount 级文件写入 ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WriteMountFileRequest {
    pub project_id: Option<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct WriteMountFileResponse {
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub persisted: bool,
}

/// `POST /api/address-spaces/write-file`
///
/// 写入文件到指定 mount。relay_fs 走远端 relay；inline_fs 持久化到 project/story 的容器配置。
pub async fn write_mount_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WriteMountFileRequest>,
) -> Result<Json<WriteMountFileResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &req.project_id,
        &req.story_id,
    )
    .await?;

    let mount = address_space
        .mounts
        .iter()
        .find(|m| m.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;

    if !mount.supports(agentdash_executor::ExecutionMountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }

    let normalized_path = normalize_mount_relative_path(&req.path, false)
        .map_err(ApiError::BadRequest)?;

    if mount.provider == PROVIDER_INLINE_FS {
        let persister = crate::address_space_access::DbInlineContentPersister::new(
            state.repos.project_repo.clone(),
            state.repos.story_repo.clone(),
        );
        let overlay = crate::address_space_access::InlineContentOverlay::new(std::sync::Arc::new(persister));
        overlay
            .write(&address_space, mount, &normalized_path, &req.content)
            .await
            .map_err(ApiError::Internal)?;

        let size = req.content.len() as u64;
        return Ok(Json(WriteMountFileResponse {
            mount_id: req.mount_id,
            path: normalized_path,
            size,
            persisted: true,
        }));
    }

    check_mount_backend_online(&state, &address_space, &req.mount_id).await?;

    state
        .services
        .address_space_service
        .write_text(
            &address_space,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            &req.content,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    let size = req.content.len() as u64;
    Ok(Json(WriteMountFileResponse {
        mount_id: req.mount_id,
        path: normalized_path,
        size,
        persisted: false,
    }))
}

// ─── Address Space 预览 ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PreviewAddressSpaceRequest {
    pub project_id: String,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default = "default_preview_target")]
    pub target: String,
}

fn default_preview_target() -> String {
    "project".to_string()
}

#[derive(Debug, Serialize)]
pub struct MountSummary {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: Vec<String>,
    pub default_write: bool,
    pub display_name: String,
    pub backend_online: Option<bool>,
    pub file_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PreviewAddressSpaceResponse {
    pub mounts: Vec<MountSummary>,
    pub default_mount_id: Option<String>,
}

/// `POST /api/address-spaces/preview`
///
/// 根据 project_id + 可选 story_id 预览将要生成的 ExecutionAddressSpace。
pub async fn preview_address_space(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PreviewAddressSpaceRequest>,
) -> Result<Json<PreviewAddressSpaceResponse>, ApiError> {
    let project_id = Uuid::parse_str(&req.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 project_id".into()))?;
    let project = state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;

    let story = load_story_optional(&state, &req.story_id).await?;
    let workspace = resolve_project_workspace(&state, &project).await;

    let target = match req.target.as_str() {
        "story" => SessionMountTarget::Story,
        "task" => SessionMountTarget::Task,
        _ => SessionMountTarget::Project,
    };

    let address_space = state
        .services
        .address_space_service
        .build_preview_address_space(&project, story.as_ref(), workspace.as_ref(), target)
        .map_err(|e| ApiError::Internal(format!("构建 address space 失败: {e}")))?;

    let mut mounts = Vec::new();
    for mount in &address_space.mounts {
        let backend_online = if !mount.backend_id.is_empty() {
            Some(
                state
                    .services
                    .backend_registry
                    .is_online(&mount.backend_id)
                    .await,
            )
        } else {
            None
        };

        let file_count = if mount.provider == PROVIDER_INLINE_FS {
            inline_files_from_mount(mount)
                .ok()
                .map(|files| files.len())
        } else {
            None
        };

        mounts.push(MountSummary {
            id: mount.id.clone(),
            provider: mount.provider.clone(),
            backend_id: mount.backend_id.clone(),
            root_ref: mount.root_ref.clone(),
            capabilities: mount.capabilities.iter().map(|c| format!("{c:?}").to_lowercase()).collect(),
            default_write: mount.default_write,
            display_name: mount.display_name.clone(),
            backend_online,
            file_count,
        });
    }

    Ok(Json(PreviewAddressSpaceResponse {
        mounts,
        default_mount_id: address_space.default_mount_id,
    }))
}

// ─── 辅助函数 ──────────────────────────────────────────────

/// 检查指定 mount 的 backend 是否在线。inline_fs 等无 backend 的 mount 直接通过。
async fn check_mount_backend_online(
    state: &Arc<AppState>,
    address_space: &ExecutionAddressSpace,
    mount_id: &str,
) -> Result<(), ApiError> {
    if let Some(mount) = address_space.mounts.iter().find(|m| m.id == mount_id) {
        if mount.provider != PROVIDER_INLINE_FS && !mount.backend_id.is_empty() {
            if !state.services.backend_registry.is_online(&mount.backend_id).await {
                return Err(ApiError::Conflict(format!(
                    "挂载点 \"{}\" 的 Backend 当前不在线（{}），无法浏览文件。请确认 Backend 已连接。",
                    mount.display_name, mount.backend_id,
                )));
            }
        }
    }
    Ok(())
}

/// 统一构建 address space：从 project_id + story_id 推导完整的 mount 列表。
/// 这确保 inline_fs / relay_fs 等所有 mount 都能被正确解析。
async fn resolve_address_space(
    state: &Arc<AppState>,
    project_id: &Option<String>,
    story_id: &Option<String>,
) -> Result<ExecutionAddressSpace, ApiError> {
    let pid_str = project_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("需要提供 project_id".into()))?;
    let pid = Uuid::parse_str(pid_str)
        .map_err(|_| ApiError::BadRequest("无效的 project_id".into()))?;
    let project = state
        .repos
        .project_repo
        .get_by_id(pid)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {pid} 不存在")))?;

    let story = load_story_optional(state, story_id).await?;
    let workspace = resolve_project_workspace(state, &project).await;

    let target = if story.is_some() {
        SessionMountTarget::Story
    } else {
        SessionMountTarget::Project
    };

    state
        .services
        .address_space_service
        .build_preview_address_space(&project, story.as_ref(), workspace.as_ref(), target)
        .map_err(|e| ApiError::Internal(format!("构建 address space 失败: {e}")))
}

async fn load_workspace(
    state: &Arc<AppState>,
    workspace_id: &Option<String>,
) -> Result<agentdash_domain::workspace::Workspace, ApiError> {
    let ws_id_str = workspace_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("需要提供 workspace_id".into()))?;
    let ws_id = Uuid::parse_str(ws_id_str)
        .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
    state
        .repos
        .workspace_repo
        .get_by_id(ws_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))
}

async fn load_story_optional(
    state: &Arc<AppState>,
    story_id: &Option<String>,
) -> Result<Option<agentdash_domain::story::Story>, ApiError> {
    if let Some(sid_str) = story_id {
        let sid = Uuid::parse_str(sid_str)
            .map_err(|_| ApiError::BadRequest("无效的 story_id".into()))?;
        Ok(Some(
            state
                .repos
                .story_repo
                .get_by_id(sid)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Story {sid} 不存在")))?,
        ))
    } else {
        Ok(None)
    }
}

async fn require_backend_online(
    state: &Arc<AppState>,
    backend_id: &str,
) -> Result<(), ApiError> {
    let trimmed = backend_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(
            "Workspace.backend_id 不能为空".to_string(),
        ));
    }
    if !state.services.backend_registry.is_online(trimmed).await {
        return Err(ApiError::Conflict(format!(
            "Workspace 所属 Backend 当前不在线: {trimmed}"
        )));
    }
    Ok(())
}

async fn resolve_project_workspace(
    state: &Arc<AppState>,
    project: &agentdash_domain::project::Project,
) -> Option<agentdash_domain::workspace::Workspace> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return state
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .ok()
            .flatten();
    }
    state
        .repos
        .workspace_repo
        .list_by_project(project.id)
        .await
        .ok()
        .and_then(|ws| ws.into_iter().next())
}
