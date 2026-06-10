//! Project Workspace Module HTTP 路由。
//!
//! 暴露 Child 1 的 canonical projection（`build_workspace_modules`）给项目设置页 UI，
//! 与 Agent 工具复用同一聚合函数（单一 canonical，无第二份 DTO）。
//! `WorkspaceModuleDescriptor` 本身即 contract 类型，handler 直接序列化，无需 mapper。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;
use agentdash_application::canvas::list_project_canvases;
use agentdash_application::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_application::workspace_module::build_workspace_modules;
use agentdash_contracts::workspace_module::WorkspaceModuleDescriptor;

#[derive(Debug, Deserialize)]
pub struct ProjectWorkspaceModulePath {
    pub project_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new().route(
        "/projects/{project_id}/workspace-modules",
        axum::routing::get(get_project_workspace_modules),
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
