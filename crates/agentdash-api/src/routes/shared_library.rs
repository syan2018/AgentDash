//! Shared Library HTTP 路由——公共资产查询与 builtin seed 入口。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::shared_library::{
    InstallLibraryAssetInput, InstallLibraryAssetOutput, ProjectAssetPublishKind,
    ProjectAssetSourceStatus, ProjectAssetSourceStatusItem, PublishLibraryAssetInput,
    SeedBuiltinLibraryAssetsInput, SharedLibraryService, install_library_asset_to_project,
    list_project_asset_source_status, publish_project_asset_to_library,
};
use agentdash_domain::shared_library::{
    LibraryAssetListFilter, LibraryAssetScope, LibraryAssetType,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    InstallLibraryAssetRequest, InstallLibraryAssetResponse, InstalledAssetSourceResponse,
    LibraryAssetResponse, ListLibraryAssetsQuery, ProjectAssetSourceStatusItemResponse,
    ProjectAssetSourceStatusResponse, PublishLibraryAssetRequest, SeedBuiltinLibraryAssetsRequest,
    parse_asset_scope, parse_asset_type, source_status_tag,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct LibraryAssetPath {
    pub id: String,
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
) -> Result<Json<Vec<LibraryAssetResponse>>, ApiError> {
    let filter = LibraryAssetListFilter {
        asset_type: parse_optional_asset_type(query.asset_type)?,
        scope: parse_optional_scope(query.scope)?,
        owner_id: query.owner_id.filter(|value| !value.trim().is_empty()),
        include_deprecated: query.include_deprecated,
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.list(filter).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

/// GET `/api/shared-library/assets/:id`
pub async fn get_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(path): Path<LibraryAssetPath>,
) -> Result<Json<LibraryAssetResponse>, ApiError> {
    let id = parse_library_asset_id(&path.id)?;
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    Ok(Json(service.get(id).await?.into()))
}

/// POST `/api/shared-library/assets/seed-builtin`
pub async fn seed_builtin_library_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(req): Json<SeedBuiltinLibraryAssetsRequest>,
) -> Result<Json<Vec<LibraryAssetResponse>>, ApiError> {
    let input = SeedBuiltinLibraryAssetsInput {
        asset_type: parse_optional_asset_type(req.asset_type)?,
        key: req.key.filter(|value| !value.trim().is_empty()),
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.seed_builtin_assets(input).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
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
            library_asset_id: req.library_asset_id,
            target_key: req.target_key,
            overwrite: req.overwrite,
        },
    )
    .await?;
    Ok(Json(install_output_response(output)))
}

/// POST `/api/projects/:project_id/shared-library/publish`
pub async fn publish_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSharedLibraryPath>,
    Json(req): Json<PublishLibraryAssetRequest>,
) -> Result<Json<LibraryAssetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let input = PublishLibraryAssetInput {
        project_id,
        project_asset_id: req.project_asset_id,
        asset_kind: parse_publish_asset_kind(&req.asset_kind)?,
        owner_id: current_user.user_id.clone(),
        scope: parse_asset_scope(&req.scope).map_err(ApiError::BadRequest)?,
        key: req.key,
        display_name: req.display_name,
        description: req.description,
        version: req.version,
        overwrite: req.overwrite,
    };
    let asset = publish_project_asset_to_library(&state.repos, input).await?;
    Ok(Json(asset.into()))
}

/// GET `/api/projects/:project_id/shared-library/source-status`
pub async fn get_project_asset_source_status(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSharedLibraryPath>,
) -> Result<Json<ProjectAssetSourceStatusResponse>, ApiError> {
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

fn parse_publish_asset_kind(raw: &str) -> Result<ProjectAssetPublishKind, ApiError> {
    match raw.trim() {
        "project_agent" => Ok(ProjectAssetPublishKind::ProjectAgent),
        "mcp_preset" => Ok(ProjectAssetPublishKind::McpPreset),
        "workflow_bundle" => Ok(ProjectAssetPublishKind::WorkflowBundle),
        "skill_asset" => Ok(ProjectAssetPublishKind::SkillAsset),
        "filespace" | "project_filespace" => Ok(ProjectAssetPublishKind::Filespace),
        other => Err(ApiError::BadRequest(format!(
            "未知 publish asset_kind: {other}"
        ))),
    }
}

fn install_output_response(output: InstallLibraryAssetOutput) -> InstallLibraryAssetResponse {
    match output {
        InstallLibraryAssetOutput::ProjectAgent { project_agent_id } => {
            InstallLibraryAssetResponse::ProjectAgent { project_agent_id }
        }
        InstallLibraryAssetOutput::McpPreset { id } => {
            InstallLibraryAssetResponse::McpPreset { id }
        }
        InstallLibraryAssetOutput::WorkflowTemplate {
            workflow_ids,
            lifecycle_id,
        } => InstallLibraryAssetResponse::WorkflowTemplate {
            workflow_ids,
            lifecycle_id,
        },
        InstallLibraryAssetOutput::SkillAsset { id } => {
            InstallLibraryAssetResponse::SkillAsset { id }
        }
        InstallLibraryAssetOutput::Filespace { id } => {
            InstallLibraryAssetResponse::Filespace { id }
        }
        InstallLibraryAssetOutput::ExtensionInstallation { id } => {
            InstallLibraryAssetResponse::ExtensionInstallation { id }
        }
    }
}

fn project_source_status_response(
    status: ProjectAssetSourceStatus,
) -> ProjectAssetSourceStatusResponse {
    ProjectAssetSourceStatusResponse {
        project_agents: status
            .project_agents
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        mcp_presets: status
            .mcp_presets
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        skill_assets: status
            .skill_assets
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        filespaces: status
            .filespaces
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        workflow_definitions: status
            .workflow_definitions
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        activity_lifecycle_definitions: status
            .activity_lifecycle_definitions
            .into_iter()
            .map(source_status_item_response)
            .collect(),
        extension_installations: status
            .extension_installations
            .into_iter()
            .map(source_status_item_response)
            .collect(),
    }
}

fn source_status_item_response(
    item: ProjectAssetSourceStatusItem,
) -> ProjectAssetSourceStatusItemResponse {
    ProjectAssetSourceStatusItemResponse {
        asset_kind: item.asset_kind,
        project_asset_id: item.project_asset_id,
        project_asset_key: item.project_asset_key,
        installed_source: InstalledAssetSourceResponse::from(item.installed_source),
        source_status: source_status_tag(item.source_status),
        current_source_version: item.current_source_version,
        current_source_digest: item.current_source_digest,
    }
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
