//! Project Workspace Module HTTP 路由。
//!
//! 暴露 application workspace module projection 给项目设置页 UI；projection 本身使用
//! browser-facing contract DTO。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;
use agentdash_application_operation_gateway::UserWorkshopOperationHost;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModulePresentRequest, WorkspaceModulePresentation,
};
use agentdash_platform_spi::WorkspaceModuleDimension;
use agentdash_workspace_module::product::{
    WorkspaceModuleActor, WorkspaceModulePresentationError, WorkspaceModulePresentationRequest,
    WorkspaceModuleProviderContext, build_workspace_module_presentation,
};
use tokio_util::sync::CancellationToken;

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
/// 通过统一 provider registry 列出当前用户可见的 Workspace Module。
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

    let modules = load_project_workspace_modules(state.as_ref(), &current_user, project_id).await?;
    Ok(Json(modules))
}

/// POST `/api/projects/:project_id/workspace-modules/present`
///
/// 用户主动打开 Workspace Module UI，只返回 provider 准备后的 canonical presentation。
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
    let (modules, provider_context) =
        load_project_workspace_module_surface(state.as_ref(), &current_user, project_id).await?;
    let module = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
        .ok_or_else(|| ApiError::NotFound(format!("workspace module not found: {module_id}")))?;
    let preparation = state
        .services
        .workspace_module_providers
        .prepare_presentation(WorkspaceModulePresentationRequest {
            context: &provider_context,
            module,
            view_key,
            payload: request.payload.clone(),
        })
        .await
        .map_err(product_tool_outcome_to_api)?;
    let mut presentation = build_workspace_module_presentation(
        module,
        view_key,
        request.payload,
        preparation.diagnostics,
    )
    .map_err(|error| match error {
        WorkspaceModulePresentationError::ViewNotFound { .. } => {
            ApiError::NotFound(error.to_string())
        }
        WorkspaceModulePresentationError::MissingPresentationUri { .. } => {
            ApiError::BadRequest(error.to_string())
        }
    })?;
    if let Some(uri) = preparation.presentation_uri {
        presentation.presentation_uri = uri;
    }

    Ok(Json(presentation))
}

pub(crate) async fn load_project_workspace_modules(
    state: &AppState,
    current_user: &agentdash_platform_spi::AuthIdentity,
    project_id: Uuid,
) -> Result<Vec<WorkspaceModuleDescriptor>, ApiError> {
    load_project_workspace_module_surface(state, current_user, project_id)
        .await
        .map(|(modules, _)| modules)
}

async fn load_project_workspace_module_surface(
    state: &AppState,
    current_user: &agentdash_platform_spi::AuthIdentity,
    project_id: Uuid,
) -> Result<
    (
        Vec<WorkspaceModuleDescriptor>,
        WorkspaceModuleProviderContext,
    ),
    ApiError,
> {
    let host = UserWorkshopOperationHost::project(
        state.services.operation_gateway.clone(),
        current_user.clone(),
        project_id,
    )
    .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let surface = host
        .discover(CancellationToken::new())
        .await
        .map_err(|error| ApiError::Forbidden(error.to_string()))?;
    let operations = surface
        .catalog
        .descriptors()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let context = WorkspaceModuleProviderContext {
        project_id,
        actor: WorkspaceModuleActor::User {
            user_id: current_user.user_id.clone(),
        },
        invocation_id: Uuid::new_v4().to_string(),
        visibility: WorkspaceModuleDimension::all(),
        operations,
    };
    let modules = state
        .services
        .workspace_module_providers
        .modules(&context)
        .await
        .map_err(product_tool_outcome_to_api)?;
    Ok((modules, context))
}

fn product_tool_outcome_to_api(
    outcome: agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome,
) -> ApiError {
    match outcome {
        agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome::Rejected {
            message,
            ..
        } => ApiError::BadRequest(message),
        agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome::Failed {
            message,
            ..
        } => ApiError::Internal(message),
        agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome::Completed {
            ..
        } => ApiError::Internal("Workspace Module provider 返回了无效 outcome".into()),
    }
}
