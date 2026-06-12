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
use agentdash_application::canvas::{expose_existing_canvas_for_session, list_project_canvases};
use agentdash_application::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_application::vfs::tools::{
    SessionToolServices, SharedRuntimeVfs, SharedSessionToolServicesHandle,
};
use agentdash_application::workspace_module::{
    build_workspace_module_presentation, build_workspace_modules,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModulePresentRequest, WorkspaceModulePresentation,
};
use agentdash_spi::AgentToolError;

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
        ProjectPermission::View,
    )
    .await?;

    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    let canvases = list_project_canvases(&state.repos, project_id)
        .await
        .map_err(ApiError::from)?;
    let modules = build_workspace_modules(&projection, &canvases);
    Ok(Json(modules))
}

/// POST `/api/projects/:project_id/workspace-modules/present`
///
/// 用户主动打开 workspace module UI。带 `runtime_session_id` 时，Canvas 会先暴露到该
/// RuntimeSession 对应的当前 AgentFrame，再返回 canonical presentation。
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
        ProjectPermission::Edit,
    )
    .await?;

    let module_id = request.module_id.trim();
    let view_key = request.view_key.trim();
    if module_id.is_empty() || view_key.is_empty() {
        return Err(ApiError::BadRequest(
            "module_id 与 view_key 不能为空".to_string(),
        ));
    }
    let runtime_session_id = request.runtime_session_id.clone();

    let modules = load_project_workspace_modules(state.as_ref(), project_id).await?;
    let module = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
        .ok_or_else(|| ApiError::NotFound(format!("workspace module not found: {module_id}")))?;
    let presentation = build_workspace_module_presentation(module, view_key, request.payload, None)
        .map_err(|error| match error {
            agentdash_application::workspace_module::WorkspaceModulePresentationError::ViewNotFound {
                ..
            } => ApiError::NotFound(error.to_string()),
            agentdash_application::workspace_module::WorkspaceModulePresentationError::MissingPresentationUri {
                ..
            } => ApiError::BadRequest(error.to_string()),
        })?;

    if let Some(runtime_session_id) = runtime_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        ensure_runtime_session_belongs_to_project(state.as_ref(), runtime_session_id, project_id)
            .await?;
        if presentation.renderer_kind == "canvas" {
            let handle = session_tool_services_handle(state.as_ref()).await;
            let active_vfs = state
                .services
                .session_capability
                .get_latest_capability_state(runtime_session_id)
                .await
                .and_then(|state| state.vfs.active)
                .unwrap_or_default();
            let shared_vfs = SharedRuntimeVfs::new(active_vfs);
            expose_existing_canvas_for_session(
                state.repos.canvas_repo.as_ref(),
                project_id,
                &module.summary.source,
                &shared_vfs,
                &handle,
                Some(runtime_session_id),
            )
            .await
            .map_err(agent_tool_error_to_api)?;
        }
    }

    Ok(Json(presentation))
}

async fn load_project_workspace_modules(
    state: &AppState,
    project_id: Uuid,
) -> Result<Vec<WorkspaceModuleDescriptor>, ApiError> {
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    let canvases = list_project_canvases(&state.repos, project_id)
        .await
        .map_err(ApiError::from)?;
    Ok(build_workspace_modules(&projection, &canvases))
}

async fn session_tool_services_handle(state: &AppState) -> SharedSessionToolServicesHandle {
    let handle = SharedSessionToolServicesHandle::default();
    handle
        .set(SessionToolServices {
            core: state.services.session_core.clone(),
            eventing: state.services.session_eventing.clone(),
            control: state.services.session_control.clone(),
            launch: state.services.session_launch.clone(),
            hooks: state.services.session_hooks.clone(),
            capability: state.services.session_capability.clone(),
        })
        .await;
    handle
}

async fn ensure_runtime_session_belongs_to_project(
    state: &AppState,
    runtime_session_id: &str,
    expected_project_id: Uuid,
) -> Result<(), ApiError> {
    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(runtime_session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "runtime session 缺少 execution anchor: {runtime_session_id}"
            ))
        })?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {}", anchor.run_id)))?;
    if run.project_id != expected_project_id {
        return Err(ApiError::Conflict(format!(
            "runtime session `{runtime_session_id}` 不属于 Project `{expected_project_id}`"
        )));
    }
    Ok(())
}

fn agent_tool_error_to_api(error: AgentToolError) -> ApiError {
    match error {
        AgentToolError::InvalidArguments(message) => ApiError::BadRequest(message),
        AgentToolError::ExecutionFailed(message) if message.contains("不存在") => {
            ApiError::NotFound(message)
        }
        AgentToolError::ExecutionFailed(message) => ApiError::Conflict(message),
        AgentToolError::Other(error) => ApiError::Internal(error.to_string()),
    }
}
