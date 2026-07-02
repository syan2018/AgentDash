use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use agentdash_contracts::context::VfsCapabilityDto as ContractMountCapability;
use agentdash_contracts::vfs::{
    CreateProjectVfsMountRequest, DeleteProjectVfsMountResponse, InstalledAssetSourceResponse,
    ProjectVfsMountContentDto as ContractProjectVfsMountContent, ProjectVfsMountResponse,
    UpdateProjectVfsMountRequest,
};
use agentdash_domain::common::MountCapability as DomainMountCapability;
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project_vfs_mount::{
    ProjectVfsMount, ProjectVfsMountContent as DomainProjectVfsMountContent,
};
use agentdash_domain::shared_library::InstalledAssetSource;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/vfs-mounts",
            axum::routing::get(list_vfs_mounts).post(create_vfs_mount),
        )
        .route(
            "/projects/{project_id}/vfs-mounts/{mount_id}",
            axum::routing::get(get_vfs_mount)
                .put(update_vfs_mount)
                .delete(delete_vfs_mount),
        )
}

#[derive(Debug, Deserialize)]
pub struct VfsMountPath {
    pub project_id: String,
    pub mount_id: String,
}

pub async fn list_vfs_mounts(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
) -> Result<Json<Vec<ProjectVfsMountResponse>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let mounts = state
        .repos
        .project_vfs_mount_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        mounts.into_iter().map(project_vfs_mount_response).collect(),
    ))
}

pub async fn create_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
    Json(req): Json<CreateProjectVfsMountRequest>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;

    let mount_id = normalize_identifier(&req.mount_id, "mount_id")?;
    let display_name = normalize_display_name(&req.display_name, &mount_id);
    let description = normalize_optional(req.description);
    let content = normalize_content(req.content)?;
    let capabilities = normalize_capabilities(req.capabilities);

    if state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, &mount_id)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "mount_id `{mount_id}` 已被占用"
        )));
    }

    let now = Utc::now();
    let mount = ProjectVfsMount {
        id: Uuid::new_v4(),
        project_id,
        mount_id,
        display_name,
        description,
        capabilities,
        installed_source: None,
        content,
        created_at: now,
        updated_at: now,
    };
    state.repos.project_vfs_mount_repo.create(&mount).await?;
    Ok(Json(project_vfs_mount_response(mount)))
}

pub async fn get_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let mount_id = path.mount_id.trim();
    let mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, mount_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
    Ok(Json(project_vfs_mount_response(mount)))
}

pub async fn update_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
    Json(req): Json<UpdateProjectVfsMountRequest>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let old_mount_id = path.mount_id.trim().to_string();
    let mut mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, &old_mount_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;

    let new_mount_id = normalize_identifier(&req.mount_id, "mount_id")?;
    if new_mount_id != old_mount_id
        && state
            .repos
            .project_vfs_mount_repo
            .get_by_project_and_mount_id(project_id, &new_mount_id)
            .await?
            .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "mount_id `{new_mount_id}` 已被占用"
        )));
    }

    let new_content = normalize_content(req.content)?;
    if std::mem::discriminant(&mount.content) != std::mem::discriminant(&new_content) {
        return Err(ApiError::BadRequest(
            "Project VFS Mount content kind 不可在更新中切换".into(),
        ));
    }

    mount.mount_id = new_mount_id;
    mount.display_name = normalize_display_name(&req.display_name, &mount.mount_id);
    mount.description = normalize_optional(req.description);
    mount.content = new_content;
    mount.capabilities = normalize_capabilities(req.capabilities);
    mount.updated_at = Utc::now();
    state.repos.project_vfs_mount_repo.update(&mount).await?;
    Ok(Json(project_vfs_mount_response(mount)))
}

pub async fn delete_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
) -> Result<Json<DeleteProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let mount_id = path.mount_id.trim();
    let mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, mount_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectVfsMount, mount.id)
        .await?;
    state
        .repos
        .project_vfs_mount_repo
        .delete(project_id, mount_id)
        .await?;
    Ok(Json(DeleteProjectVfsMountResponse { ok: true }))
}

fn project_vfs_mount_response(mount: ProjectVfsMount) -> ProjectVfsMountResponse {
    let surface_ref = format!("project-vfs-mount:{}:{}", mount.project_id, mount.mount_id);
    ProjectVfsMountResponse {
        project_id: mount.project_id.to_string(),
        mount_id: mount.mount_id,
        display_name: mount.display_name,
        description: mount.description,
        capabilities: mount
            .capabilities
            .into_iter()
            .map(contract_capability_from_domain)
            .collect(),
        installed_source: mount.installed_source.map(installed_source_response),
        content: contract_content_from_domain(mount.content),
        surface_ref,
        created_at: mount.created_at.to_rfc3339(),
        updated_at: mount.updated_at.to_rfc3339(),
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("{field} 非法")))
}

fn normalize_identifier(raw: &str, field: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    if trimmed.chars().any(char::is_whitespace)
        || trimmed.chars().any(|ch| matches!(ch, '/' | '\\' | ':'))
    {
        return Err(ApiError::BadRequest(format!(
            "{field} 不能包含空白、`/`、`\\` 或 `:`"
        )));
    }
    if trimmed.eq_ignore_ascii_case("main") {
        return Err(ApiError::BadRequest(format!("{field} 不能使用保留字 main")));
    }
    Ok(trimmed.to_string())
}

fn normalize_display_name(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_capabilities(
    capabilities: Vec<ContractMountCapability>,
) -> Vec<DomainMountCapability> {
    let mut normalized = Vec::new();
    for capability in capabilities {
        let capability = domain_capability_from_contract(capability);
        if !normalized.contains(&capability) {
            normalized.push(capability);
        }
    }
    if normalized.is_empty() {
        normalized.extend([
            DomainMountCapability::Read,
            DomainMountCapability::List,
            DomainMountCapability::Search,
        ]);
    }
    normalized
}

fn normalize_content(
    content: ContractProjectVfsMountContent,
) -> Result<DomainProjectVfsMountContent, ApiError> {
    match content {
        ContractProjectVfsMountContent::Inline => Ok(DomainProjectVfsMountContent::Inline),
        ContractProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        } => {
            let service_id = service_id.trim();
            let root_ref = root_ref.trim();
            if service_id.is_empty() {
                return Err(ApiError::BadRequest("service_id 不能为空".into()));
            }
            if root_ref.is_empty() {
                return Err(ApiError::BadRequest("root_ref 不能为空".into()));
            }
            Ok(DomainProjectVfsMountContent::ExternalService {
                service_id: service_id.to_string(),
                root_ref: root_ref.to_string(),
            })
        }
    }
}

fn domain_capability_from_contract(capability: ContractMountCapability) -> DomainMountCapability {
    match capability {
        ContractMountCapability::Read => DomainMountCapability::Read,
        ContractMountCapability::Write => DomainMountCapability::Write,
        ContractMountCapability::List => DomainMountCapability::List,
        ContractMountCapability::Search => DomainMountCapability::Search,
        ContractMountCapability::Exec => DomainMountCapability::Exec,
        ContractMountCapability::Watch => DomainMountCapability::Watch,
    }
}

fn contract_capability_from_domain(capability: DomainMountCapability) -> ContractMountCapability {
    match capability {
        DomainMountCapability::Read => ContractMountCapability::Read,
        DomainMountCapability::Write => ContractMountCapability::Write,
        DomainMountCapability::List => ContractMountCapability::List,
        DomainMountCapability::Search => ContractMountCapability::Search,
        DomainMountCapability::Exec => ContractMountCapability::Exec,
        DomainMountCapability::Watch => ContractMountCapability::Watch,
    }
}

fn contract_content_from_domain(
    content: DomainProjectVfsMountContent,
) -> ContractProjectVfsMountContent {
    match content {
        DomainProjectVfsMountContent::Inline => ContractProjectVfsMountContent::Inline,
        DomainProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        } => ContractProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        },
    }
}

fn installed_source_response(source: InstalledAssetSource) -> InstalledAssetSourceResponse {
    InstalledAssetSourceResponse {
        library_asset_id: source.library_asset_id.to_string(),
        source_ref: source.source_ref,
        source_version: source.source_version,
        source_digest: source.source_digest,
        installed_at: source.installed_at.to_rfc3339(),
    }
}
