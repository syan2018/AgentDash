//! Shared Library HTTP 路由——公共资产查询与 builtin seed 入口。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::shared_library::{
    AgentTemplateDependencyMode as ApplicationAgentTemplateDependencyMode,
    InstallLibraryAssetInput, InstallLibraryAssetOptions as ApplicationInstallLibraryAssetOptions,
    InstallLibraryAssetOutput, ProjectAssetPublishKind, ProjectAssetSourceStatus,
    ProjectAssetSourceStatusItem, PublishLibraryAssetInput, SeedBuiltinLibraryAssetsInput,
    SharedLibraryService, install_library_asset_to_project, list_project_asset_source_status,
    publish_project_asset_to_library,
};
use agentdash_domain::extension_package::ExtensionPackageArtifactOwner;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetPayload, LibraryAssetScope, LibraryAssetType,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    InstallLibraryAssetRequest, InstallLibraryAssetResponse, LibraryAssetDto,
    ListLibraryAssetsQuery, ProjectAssetSourceStatusDto, ProjectAssetSourceStatusItemDto,
    PublishLibraryAssetRequest, SeedBuiltinLibraryAssetsRequest, library_asset_response,
    library_asset_response_with_extension_package, parse_asset_scope, parse_asset_type,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct LibraryAssetPath {
    pub id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/shared-library/assets",
            axum::routing::get(list_library_assets),
        )
        .route(
            "/shared-library/assets/seed-builtin",
            axum::routing::post(seed_builtin_library_assets),
        )
        .route(
            "/shared-library/assets/{id}",
            axum::routing::get(get_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/install",
            axum::routing::post(install_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/publish",
            axum::routing::post(publish_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/source-status",
            axum::routing::get(get_project_asset_source_status),
        )
}

#[derive(Debug, Deserialize)]
pub struct ProjectSharedLibraryPath {
    pub project_id: String,
}

/// GET `/api/shared-library/assets`
pub async fn list_library_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<ListLibraryAssetsQuery>,
) -> Result<Json<Vec<LibraryAssetDto>>, ApiError> {
    let filter = LibraryAssetListFilter {
        asset_type: parse_optional_asset_type(query.asset_type)?,
        scope: parse_optional_scope(query.scope)?,
        owner_id: query.owner_id.filter(|value| !value.trim().is_empty()),
        include_deprecated: query.include_deprecated,
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.list(filter).await?;
    let mut response = Vec::with_capacity(assets.len());
    for asset in assets {
        response.push(library_asset_response_for_api(state.as_ref(), asset).await?);
    }
    Ok(Json(response))
}

/// GET `/api/shared-library/assets/:id`
pub async fn get_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(path): Path<LibraryAssetPath>,
) -> Result<Json<LibraryAssetDto>, ApiError> {
    let id = parse_library_asset_id(&path.id)?;
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let asset = service.get(id).await?;
    Ok(Json(
        library_asset_response_for_api(state.as_ref(), asset).await?,
    ))
}

/// POST `/api/shared-library/assets/seed-builtin`
pub async fn seed_builtin_library_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(req): Json<SeedBuiltinLibraryAssetsRequest>,
) -> Result<Json<Vec<LibraryAssetDto>>, ApiError> {
    let input = SeedBuiltinLibraryAssetsInput {
        asset_type: parse_optional_asset_type(req.asset_type)?,
        key: req.key.filter(|value| !value.trim().is_empty()),
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.seed_builtin_assets(input).await?;
    let mut response = Vec::with_capacity(assets.len());
    for asset in assets {
        response.push(library_asset_response_for_api(state.as_ref(), asset).await?);
    }
    Ok(Json(response))
}

/// POST `/api/projects/:project_id/shared-library/install`
pub async fn install_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSharedLibraryPath>,
    Json(req): Json<InstallLibraryAssetRequest>,
) -> Result<Json<InstallLibraryAssetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let output = install_library_asset_to_project(
        &state.repos,
        InstallLibraryAssetInput {
            project_id,
            library_asset_id: parse_library_asset_id(&req.library_asset_id)?,
            target_key: req.target_key,
            overwrite: req.overwrite,
            install_options: req.install_options.map(install_options_input),
        },
    )
    .await?;
    Ok(Json(install_output_response(output)))
}

fn install_options_input(
    options: agentdash_contracts::shared_library::InstallLibraryAssetOptions,
) -> ApplicationInstallLibraryAssetOptions {
    match options {
        agentdash_contracts::shared_library::InstallLibraryAssetOptions::McpServerTemplate {
            parameters,
        } => ApplicationInstallLibraryAssetOptions::McpServerTemplate { parameters },
        agentdash_contracts::shared_library::InstallLibraryAssetOptions::AgentTemplate {
            dependency_mode,
            dependency_parameters,
            overwrite_dependencies,
        } => ApplicationInstallLibraryAssetOptions::AgentTemplate {
            dependency_mode: match dependency_mode {
                agentdash_contracts::shared_library::AgentTemplateDependencyMode::Required => {
                    ApplicationAgentTemplateDependencyMode::Required
                }
                agentdash_contracts::shared_library::AgentTemplateDependencyMode::All => {
                    ApplicationAgentTemplateDependencyMode::All
                }
                agentdash_contracts::shared_library::AgentTemplateDependencyMode::Skip => {
                    ApplicationAgentTemplateDependencyMode::Skip
                }
            },
            dependency_parameters,
            overwrite_dependencies,
        },
    }
}

/// POST `/api/projects/:project_id/shared-library/publish`
pub async fn publish_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSharedLibraryPath>,
    Json(req): Json<PublishLibraryAssetRequest>,
) -> Result<Json<LibraryAssetDto>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let asset_kind = parse_publish_asset_kind(&req.asset_kind)?;
    let project_asset_id =
        resolve_project_asset_id(&state, project_id, asset_kind, &req.project_asset_id).await?;
    let input = PublishLibraryAssetInput {
        project_id,
        project_asset_id,
        asset_kind,
        owner_id: current_user.user_id.clone(),
        scope: parse_asset_scope(&req.scope).map_err(ApiError::BadRequest)?,
        key: req.key,
        display_name: req.display_name,
        description: req.description,
        version: req.version,
        overwrite: req.overwrite,
    };
    let asset = publish_project_asset_to_library(&state.repos, input).await?;
    Ok(Json(
        library_asset_response_for_api(state.as_ref(), asset).await?,
    ))
}

/// GET `/api/projects/:project_id/shared-library/source-status`
pub async fn get_project_asset_source_status(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSharedLibraryPath>,
) -> Result<Json<ProjectAssetSourceStatusDto>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let status = list_project_asset_source_status(&state.repos, project_id).await?;
    Ok(Json(project_source_status_response(status)))
}

fn parse_library_asset_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 LibraryAsset ID".into()))
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

async fn library_asset_response_for_api(
    state: &AppState,
    asset: LibraryAsset,
) -> Result<LibraryAssetDto, ApiError> {
    if !matches!(asset.asset_type, LibraryAssetType::ExtensionTemplate) {
        return Ok(library_asset_response(asset));
    }

    let owner = ExtensionPackageArtifactOwner::library_asset(asset.id);
    let payload = match asset.typed_payload()? {
        LibraryAssetPayload::ExtensionTemplate(payload) => payload,
        _ => return Ok(library_asset_response(asset)),
    };
    let extension_package_artifact = state
        .repos
        .extension_package_artifact_repo
        .list_by_owner(&owner)
        .await?
        .into_iter()
        .find(|artifact| artifact.matches_extension_template(&payload));

    Ok(library_asset_response_with_extension_package(
        asset,
        extension_package_artifact,
    ))
}

fn parse_optional_asset_type(raw: Option<String>) -> Result<Option<LibraryAssetType>, ApiError> {
    raw.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_asset_type)
        .transpose()
        .map_err(ApiError::BadRequest)
}

fn parse_optional_scope(raw: Option<String>) -> Result<Option<LibraryAssetScope>, ApiError> {
    raw.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_asset_scope)
        .transpose()
        .map_err(ApiError::BadRequest)
}

async fn resolve_project_asset_id(
    state: &Arc<AppState>,
    project_id: Uuid,
    asset_kind: ProjectAssetPublishKind,
    raw: &str,
) -> Result<Uuid, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("project_asset_id 不能为空".into()));
    }
    if matches!(asset_kind, ProjectAssetPublishKind::VfsMount) {
        if let Ok(uuid) = Uuid::parse_str(trimmed) {
            return Ok(uuid);
        }
        let mount = state
            .repos
            .project_vfs_mount_repo
            .get_by_project_and_mount_id(project_id, trimmed)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
        return Ok(mount.id);
    }
    Uuid::parse_str(trimmed).map_err(|_| ApiError::BadRequest("project_asset_id 非法".into()))
}

fn parse_publish_asset_kind(raw: &str) -> Result<ProjectAssetPublishKind, ApiError> {
    match raw.trim() {
        "project_agent" => Ok(ProjectAssetPublishKind::ProjectAgent),
        "mcp_preset" => Ok(ProjectAssetPublishKind::McpPreset),
        "workflow_bundle" => Ok(ProjectAssetPublishKind::WorkflowBundle),
        "skill_asset" => Ok(ProjectAssetPublishKind::SkillAsset),
        "vfs_mount" | "project_vfs_mount" => Ok(ProjectAssetPublishKind::VfsMount),
        "extension_installation" => Ok(ProjectAssetPublishKind::ExtensionInstallation),
        other => Err(ApiError::BadRequest(format!(
            "未知 publish asset_kind: {other}"
        ))),
    }
}

fn install_output_response(output: InstallLibraryAssetOutput) -> InstallLibraryAssetResponse {
    match output {
        InstallLibraryAssetOutput::ProjectAgent { project_agent_id } => {
            InstallLibraryAssetResponse::ProjectAgent {
                project_agent_id: project_agent_id.to_string(),
            }
        }
        InstallLibraryAssetOutput::McpPreset { id } => {
            InstallLibraryAssetResponse::McpPreset { id: id.to_string() }
        }
        InstallLibraryAssetOutput::WorkflowTemplate {
            workflow_ids,
            lifecycle_id,
        } => InstallLibraryAssetResponse::WorkflowTemplate {
            workflow_ids: workflow_ids.into_iter().map(|id| id.to_string()).collect(),
            lifecycle_id: lifecycle_id.to_string(),
        },
        InstallLibraryAssetOutput::SkillAsset { id } => {
            InstallLibraryAssetResponse::SkillAsset { id: id.to_string() }
        }
        InstallLibraryAssetOutput::VfsMount { id, mount_id } => {
            InstallLibraryAssetResponse::VfsMount {
                id: id.to_string(),
                mount_id,
            }
        }
        InstallLibraryAssetOutput::ExtensionInstallation { id } => {
            InstallLibraryAssetResponse::ExtensionInstallation { id: id.to_string() }
        }
    }
}

fn project_source_status_response(status: ProjectAssetSourceStatus) -> ProjectAssetSourceStatusDto {
    crate::dto::project_source_status_response(
        status
            .project_agents
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .mcp_presets
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .skill_assets
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .vfs_mounts
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .agent_procedures
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .workflow_graphs
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        status
            .extension_installations
            .into_iter()
            .map(source_status_item_response)
            .collect(),
    )
}

fn source_status_item_response(
    item: ProjectAssetSourceStatusItem,
) -> ProjectAssetSourceStatusItemDto {
    crate::dto::source_status_item_response(
        item.asset_kind,
        item.project_asset_id,
        item.project_asset_key,
        item.installed_source,
        item.source_status,
        item.current_source_version,
        item.current_source_digest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_library_asset_id_rejects_invalid() {
        let err = parse_library_asset_id("bad").expect_err("invalid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn parse_optional_asset_type_accepts_known_type() {
        let parsed = parse_optional_asset_type(Some("agent_template".to_string()))
            .expect("parse")
            .expect("some");
        assert_eq!(parsed, LibraryAssetType::AgentTemplate);
    }

    #[test]
    fn parse_optional_asset_type_rejects_unknown_type() {
        let err = parse_optional_asset_type(Some("catalog".to_string())).expect_err("invalid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn parse_publish_asset_kind_accepts_known_type() {
        let kind = parse_publish_asset_kind("skill_asset").expect("parse");
        assert_eq!(kind, ProjectAssetPublishKind::SkillAsset);
    }
}
