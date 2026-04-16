use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::workflow::WorkflowBindingKind;
use agentdash_spi::{AddressSpace, AddressSpaceContext, AddressSpaceDescriptor};

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_workspace_and_project_with_permission,
};
use crate::rpc::ApiError;
use agentdash_application::address_space::selected_workspace_binding;
use agentdash_application::address_space::{
    ListOptions, PROVIDER_INLINE_FS, ReadResult, ResourceRef, SessionMountTarget,
    normalize_mount_relative_path,
};

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
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<AddressSpacesQuery>,
) -> Result<Json<AddressSpacesResponse>, ApiError> {
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
    let ctx = AddressSpaceContext {
        workspace_available,
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
    pub owner_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
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
    CurrentUser(current_user): CurrentUser,
    Path(mount_id): Path<String>,
    Query(query): Query<ListMountEntriesQuery>,
) -> Result<Json<ListMountEntriesResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &current_user,
        &query.project_id,
        &query.story_id,
        &query.owner_type,
        &query.owner_id,
        ProjectPermission::View,
    )
    .await?;

    check_mount_available(&state, &address_space, &mount_id).await?;

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

    Ok(Json(ListMountEntriesResponse { mount_id, entries }))
}

// ─── Mount 级文件读取 ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReadMountFileRequest {
    pub project_id: Option<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub owner_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
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
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ReadMountFileRequest>,
) -> Result<Json<ReadMountFileResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &current_user,
        &req.project_id,
        &req.story_id,
        &req.owner_type,
        &req.owner_id,
        ProjectPermission::View,
    )
    .await?;

    check_mount_available(&state, &address_space, &req.mount_id).await?;

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
    #[serde(default)]
    pub owner_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
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
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<WriteMountFileRequest>,
) -> Result<Json<WriteMountFileResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &current_user,
        &req.project_id,
        &req.story_id,
        &req.owner_type,
        &req.owner_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mount = address_space
        .mounts
        .iter()
        .find(|m| m.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;

    if !mount.supports(agentdash_spi::MountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }

    let normalized_path =
        normalize_mount_relative_path(&req.path, false).map_err(ApiError::BadRequest)?;

    if mount.provider == PROVIDER_INLINE_FS {
        let persister =
            agentdash_application::address_space::inline_persistence::DbInlineContentPersister::new(
                state.repos.inline_file_repo.clone(),
            );
        let overlay =
            agentdash_application::address_space::inline_persistence::InlineContentOverlay::new(
                std::sync::Arc::new(persister),
            );
        overlay
            .write(mount, &normalized_path, &req.content)
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

    check_mount_available(&state, &address_space, &req.mount_id).await?;

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

// ─── Mount 级 apply_patch ──────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApplyMountPatchRequest {
    pub project_id: Option<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub owner_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    pub mount_id: String,
    pub patch: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyMountPatchResponse {
    pub mount_id: String,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

/// `POST /api/address-spaces/apply-patch`
///
/// 在指定 mount 上应用 Codex 风格的 apply_patch 文本。
pub async fn apply_mount_patch(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ApplyMountPatchRequest>,
) -> Result<Json<ApplyMountPatchResponse>, ApiError> {
    let address_space = resolve_address_space(
        &state,
        &current_user,
        &req.project_id,
        &req.story_id,
        &req.owner_type,
        &req.owner_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mount = address_space
        .mounts
        .iter()
        .find(|m| m.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;

    if !mount.supports(agentdash_spi::MountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }

    if mount.provider == PROVIDER_INLINE_FS {
        let persister =
            agentdash_application::address_space::inline_persistence::DbInlineContentPersister::new(
                state.repos.inline_file_repo.clone(),
            );
        let overlay =
            agentdash_application::address_space::inline_persistence::InlineContentOverlay::new(
                std::sync::Arc::new(persister),
            );
        let result = state
            .services
            .address_space_service
            .apply_patch(
                &address_space,
                &req.mount_id,
                &req.patch,
                Some(&overlay),
                None,
            )
            .await
            .map_err(ApiError::Internal)?;

        return Ok(Json(ApplyMountPatchResponse {
            mount_id: req.mount_id,
            added: result.added,
            modified: result.modified,
            deleted: result.deleted,
        }));
    }

    check_mount_available(&state, &address_space, &req.mount_id).await?;

    let result = state
        .services
        .address_space_service
        .apply_patch(&address_space, &req.mount_id, &req.patch, None, None)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(ApplyMountPatchResponse {
        mount_id: req.mount_id,
        added: result.added,
        modified: result.modified,
        deleted: result.deleted,
    }))
}

// ─── Address Space 预览 ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PreviewAddressSpaceRequest {
    pub project_id: String,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub owner_type: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
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
/// 根据 project_id + 可选 story_id 预览将要生成的 AddressSpace。
pub async fn preview_address_space(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<PreviewAddressSpaceRequest>,
) -> Result<Json<PreviewAddressSpaceResponse>, ApiError> {
    let project_id = Uuid::parse_str(&req.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 project_id".into()))?;
    let (project, story) = load_project_and_optional_story(
        &state,
        &current_user,
        project_id,
        &req.story_id,
        ProjectPermission::View,
    )
    .await?;

    let parsed_owner = parse_lifecycle_owner(&req.owner_type, &req.owner_id);
    let workspace = resolve_workspace_for_owner(&state, &project, parsed_owner).await?;

    let target = match parsed_owner {
        Some((WorkflowBindingKind::Task, _)) => SessionMountTarget::Task,
        _ => match req.target.as_str() {
            "story" => SessionMountTarget::Story,
            "task" => SessionMountTarget::Task,
            _ => SessionMountTarget::Project,
        },
    };

    let mut address_space = state
        .services
        .address_space_service
        .build_address_space(&project, story.as_ref(), workspace.as_ref(), target, None)
        .map_err(|e| ApiError::Internal(format!("构建 address space 失败: {e}")))?;
    if let Some((binding_kind, binding_id)) = parsed_owner {
        inject_lifecycle_mount(&state, binding_kind, binding_id, &mut address_space).await;
    }

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
            if let Ok((owner_kind, owner_id, container_id)) =
                agentdash_application::address_space::parse_inline_mount_owner(mount)
            {
                state
                    .repos
                    .inline_file_repo
                    .count_files(owner_kind, owner_id, &container_id)
                    .await
                    .ok()
                    .map(|c| c as usize)
            } else {
                None
            }
        } else {
            None
        };

        mounts.push(MountSummary {
            id: mount.id.clone(),
            provider: mount.provider.clone(),
            backend_id: mount.backend_id.clone(),
            root_ref: mount.root_ref.clone(),
            capabilities: mount
                .capabilities
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect(),
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

/// 通过已注册的 `MountProvider::is_available` 检查 mount 是否可用。
async fn check_mount_available(
    state: &Arc<AppState>,
    address_space: &AddressSpace,
    mount_id: &str,
) -> Result<(), ApiError> {
    if let Some(mount) = address_space.mounts.iter().find(|m| m.id == mount_id)
        && let Some(provider) = state.services.mount_provider_registry.get(&mount.provider)
        && !provider.is_available(mount).await
    {
        return Err(ApiError::ServiceUnavailable(format!(
            "挂载点 \"{}\" 的 Backend 当前不在线（{}），无法浏览文件。请确认 Backend 已连接。",
            mount.display_name, mount.backend_id,
        )));
    }
    Ok(())
}

/// 统一构建 address space：从 project_id + story_id + 可选 owner 信息推导完整的 mount 列表。
/// 当提供 owner_type + owner_id 时，自动注入活跃 lifecycle run 的 mount，并在 owner 为 task 时从 task 解析 workspace。
async fn resolve_address_space(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    project_id: &Option<String>,
    story_id: &Option<String>,
    owner_type: &Option<String>,
    owner_id: &Option<String>,
    permission: ProjectPermission,
) -> Result<AddressSpace, ApiError> {
    let pid_str = project_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("需要提供 project_id".into()))?;
    let pid =
        Uuid::parse_str(pid_str).map_err(|_| ApiError::BadRequest("无效的 project_id".into()))?;
    let (project, story) =
        load_project_and_optional_story(state, current_user, pid, story_id, permission).await?;

    let parsed_owner = parse_lifecycle_owner(owner_type, owner_id);
    let workspace = resolve_workspace_for_owner(state, &project, parsed_owner).await?;

    let target = match parsed_owner {
        Some((WorkflowBindingKind::Task, _)) => SessionMountTarget::Task,
        _ if story.is_some() => SessionMountTarget::Story,
        _ => SessionMountTarget::Project,
    };

    let mut address_space = state
        .services
        .address_space_service
        .build_address_space(&project, story.as_ref(), workspace.as_ref(), target, None)
        .map_err(|e| ApiError::Internal(format!("构建 address space 失败: {e}")))?;
    if let Some((binding_kind, binding_id)) = parsed_owner {
        inject_lifecycle_mount(state, binding_kind, binding_id, &mut address_space).await;
    }

    Ok(address_space)
}

fn parse_lifecycle_owner(
    owner_type: &Option<String>,
    owner_id: &Option<String>,
) -> Option<(WorkflowBindingKind, Uuid)> {
    let otype = owner_type.as_deref()?.trim();
    let oid = owner_id.as_deref()?.trim();
    let kind = WorkflowBindingKind::from_owner_type(otype)?;
    Uuid::parse_str(oid).ok().map(|id| (kind, id))
}

/// 按 owner 类型逐级解析 workspace：
/// - Task owner → task.workspace_id
/// - Story owner → story.default_workspace_id
/// - 最终兜底 → project.config.default_workspace_id → project 下第一个 workspace
async fn resolve_workspace_for_owner(
    state: &Arc<AppState>,
    project: &agentdash_domain::project::Project,
    owner: Option<(WorkflowBindingKind, Uuid)>,
) -> Result<Option<agentdash_domain::workspace::Workspace>, ApiError> {
    if let Some((kind, id)) = owner {
        match kind {
            WorkflowBindingKind::Task => {
                return resolve_task_workspace(state, id).await.map(Some);
            }
            WorkflowBindingKind::Story => {
                let story = state
                    .repos
                    .story_repo
                    .get_by_id(id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .ok_or_else(|| ApiError::NotFound(format!("Story {id} 不存在")))?;
                let ws_id = story.default_workspace_id.ok_or_else(|| {
                    ApiError::BadRequest(format!("Story {id} 未配置 default_workspace_id"))
                })?;
                let workspace = state
                    .repos
                    .workspace_repo
                    .get_by_id(ws_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        ApiError::NotFound(format!("Story {id} 绑定的 Workspace {ws_id} 不存在"))
                    })?;
                return Ok(Some(workspace));
            }
            WorkflowBindingKind::Project => {
                return super::project_agents::resolve_project_workspace(state, project)
                    .await?
                    .ok_or_else(|| {
                        ApiError::BadRequest(format!(
                            "Project {} 未配置 default_workspace_id",
                            project.id
                        ))
                    })
                    .map(Some);
            }
        }
    }

    super::project_agents::resolve_project_workspace(state, project).await
}

async fn resolve_task_workspace(
    state: &Arc<AppState>,
    task_id: Uuid,
) -> Result<agentdash_domain::workspace::Workspace, ApiError> {
    let task = state
        .repos
        .task_repo
        .get_by_id(task_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;
    let ws_id = task
        .workspace_id
        .ok_or_else(|| ApiError::BadRequest(format!("Task {task_id} 未绑定 workspace_id")))?;
    state
        .repos
        .workspace_repo
        .get_by_id(ws_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Task {task_id} 绑定的 Workspace {ws_id} 不存在"))
        })
}

/// 通过 owner 的 session binding 查找关联的活跃 lifecycle run 并注入 mount。
async fn inject_lifecycle_mount(
    state: &Arc<AppState>,
    binding_kind: WorkflowBindingKind,
    binding_id: Uuid,
    address_space: &mut AddressSpace,
) {
    use agentdash_application::address_space::build_lifecycle_mount;
    use agentdash_application::workflow::select_active_run;
    use agentdash_domain::session_binding::SessionOwnerType;

    let owner_type = match binding_kind {
        WorkflowBindingKind::Project => SessionOwnerType::Project,
        WorkflowBindingKind::Story => SessionOwnerType::Story,
        WorkflowBindingKind::Task => SessionOwnerType::Task,
    };

    let bindings = match state
        .repos
        .session_binding_repo
        .list_by_owner(owner_type, binding_id)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%binding_id, "inject_lifecycle_mount: session binding 查询失败 {e}");
            return;
        }
    };

    for binding in &bindings {
        let runs = match state
            .repos
            .lifecycle_run_repo
            .list_by_session(&binding.session_id)
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let Some(active_run) = select_active_run(runs) {
            let lifecycle_key = match state
                .repos
                .lifecycle_definition_repo
                .get_by_id(active_run.lifecycle_id)
                .await
            {
                Ok(Some(def)) => def.key,
                _ => "unknown".to_string(),
            };
            let runtime_mount = build_lifecycle_mount(active_run.id, &lifecycle_key);
            address_space.mounts.push(runtime_mount);
            return;
        }
    }
}

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

async fn load_project_and_optional_story(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    project_id: Uuid,
    story_id: &Option<String>,
    permission: ProjectPermission,
) -> Result<
    (
        agentdash_domain::project::Project,
        Option<agentdash_domain::story::Story>,
    ),
    ApiError,
> {
    let project =
        load_project_with_permission(state.as_ref(), current_user, project_id, permission).await?;
    if let Some(sid_str) = story_id {
        let sid =
            Uuid::parse_str(sid_str).map_err(|_| ApiError::BadRequest("无效的 story_id".into()))?;
        let (story, _) =
            load_story_and_project_with_permission(state.as_ref(), current_user, sid, permission)
                .await?;
        if story.project_id != project.id {
            return Err(ApiError::Conflict(
                "story_id 与 project_id 不属于同一 Project".into(),
            ));
        }
        Ok((project, Some(story)))
    } else {
        Ok((project, None))
    }
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
) -> Json<Vec<agentdash_application::address_space::ConfigurableProviderInfo>> {
    Json(
        state
            .services
            .mount_provider_registry
            .user_configurable_providers(),
    )
}
