//! Project Workspace Module HTTP 路由。
//!
//! 暴露 application workspace module projection 给项目设置页 UI；projection 本身使用
//! browser-facing contract DTO。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;
use agentdash_application::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_application_operation_gateway::{OperationDescriptor, OperationPrincipal};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModulePresentRequest, WorkspaceModulePresentation,
};
use agentdash_domain::interaction::{InteractionDefinitionStatus, InteractionOwner};
use agentdash_domain::operation::{OperationOriginRef, OperationScopeRef};
use agentdash_workspace_module::workspace_module::{
    WorkspaceModulePresentationError, build_workspace_module_presentation, build_workspace_modules,
};

#[derive(Debug, Deserialize)]
pub struct ProjectWorkspaceModulePath {
    pub project_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/workspace-modules",
            get(get_project_workspace_modules),
        )
        .route(
            "/projects/{project_id}/workspace-modules/present",
            post(present_workspace_module),
        )
}

/// GET `/api/projects/:project_id/workspace-modules`
///
/// 合并列出 enabled extension + visible canvas 贡献的 WorkspaceModule。
pub async fn get_project_workspace_modules(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectWorkspaceModulePath>,
) -> Result<Json<Vec<WorkspaceModuleDescriptor>>, ApiError> {
    let project_id = Uuid::parse_str(&path.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    let definitions =
        load_visible_canvas_revisions(state.as_ref(), &current_user.user_id, project_id).await?;
    let operations =
        load_user_workshop_operations(state.as_ref(), &current_user, project_id).await?;
    let modules = build_workspace_modules(&projection, &definitions, &operations);
    Ok(Json(modules))
}

/// POST `/api/projects/:project_id/workspace-modules/present`
///
/// 用户主动打开 workspace module UI，只返回 canonical presentation。运行时能力变更由
/// Agent turn 内的 `workspace_module_present` 工具路径负责。
pub async fn present_workspace_module(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectWorkspaceModulePath>,
    Json(request): Json<WorkspaceModulePresentRequest>,
) -> Result<Json<WorkspaceModulePresentation>, ApiError> {
    let project_id = Uuid::parse_str(&path.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let module_id = request.module_id.trim();
    let view_key = request.view_key.trim();
    if module_id.is_empty() || view_key.is_empty() {
        return Err(ApiError::BadRequest(
            "module_id 与 view_key 不能为空".to_string(),
        ));
    }
    let modules = load_project_workspace_modules(state.as_ref(), &current_user, project_id).await?;
    let module = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
        .ok_or_else(|| ApiError::NotFound(format!("workspace module not found: {module_id}")))?;
    let presentation = build_workspace_module_presentation(module, view_key, request.payload, None)
        .map_err(|error| match error {
            WorkspaceModulePresentationError::ViewNotFound { .. } => {
                ApiError::NotFound(error.to_string())
            }
            WorkspaceModulePresentationError::MissingPresentationUri { .. } => {
                ApiError::BadRequest(error.to_string())
            }
        })?;

    Ok(Json(presentation))
}

async fn load_project_workspace_modules(
    state: &AppState,
    current_user: &agentdash_spi::AuthIdentity,
    project_id: Uuid,
) -> Result<Vec<WorkspaceModuleDescriptor>, ApiError> {
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    let definitions =
        load_visible_canvas_revisions(state, &current_user.user_id, project_id).await?;
    let operations = load_user_workshop_operations(state, current_user, project_id).await?;
    Ok(build_workspace_modules(
        &projection,
        &definitions,
        &operations,
    ))
}

async fn load_user_workshop_operations(
    state: &AppState,
    current_user: &agentdash_spi::AuthIdentity,
    project_id: Uuid,
) -> Result<Vec<OperationDescriptor>, ApiError> {
    let surface = state
        .services
        .operation_gateway
        .surface_current(
            &OperationPrincipal::authenticated_user(current_user.clone()),
            &OperationScopeRef::Project { project_id },
            &OperationOriginRef::UserWorkshop,
            CancellationToken::new(),
        )
        .await
        .map_err(ApiError::from)?;
    Ok(surface.catalog.descriptors().into_iter().cloned().collect())
}

async fn load_visible_canvas_revisions(
    state: &AppState,
    current_user_id: &str,
    project_id: Uuid,
) -> Result<Vec<agentdash_domain::interaction::InteractionDefinitionRevision>, ApiError> {
    let definitions = state
        .repos
        .interaction_definition_repo
        .list_canvas_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let mut revisions = Vec::new();
    for definition in definitions {
        if definition.status != InteractionDefinitionStatus::Active {
            continue;
        }
        if matches!(&definition.owner, InteractionOwner::User(owner) if owner != current_user_id) {
            continue;
        }
        let revision = state
            .repos
            .interaction_definition_repo
            .get_revision(definition.current_revision_id)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| {
                ApiError::Internal("InteractionDefinition current revision 缺失".to_string())
            })?;
        revisions.push(revision);
    }
    Ok(revisions)
}
