//! Project Workspace Module HTTP 路由。
//!
//! 暴露 application workspace module read model 给项目设置页 UI；API 层负责映射为
//! browser-facing contract DTO。

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
use agentdash_application::workspace_module::{
    WorkspaceModuleDescriptor as WorkspaceModuleReadModel,
    WorkspaceModuleKind as WorkspaceModuleReadModelKind,
    WorkspaceModuleOperationDispatch as WorkspaceModuleReadModelOperationDispatch,
    WorkspaceModuleStatusKind as WorkspaceModuleReadModelStatusKind, build_workspace_modules,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperation,
    WorkspaceModuleOperationDispatch, WorkspaceModuleStatus, WorkspaceModuleStatusKind,
    WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};

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
    Ok(Json(
        modules
            .into_iter()
            .map(workspace_module_to_contract)
            .collect(),
    ))
}

fn workspace_module_to_contract(module: WorkspaceModuleReadModel) -> WorkspaceModuleDescriptor {
    WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: module.summary.module_id,
            kind: workspace_module_kind_to_contract(module.summary.kind),
            title: module.summary.title,
            description: module.summary.description,
            source: module.summary.source,
            ui_summary: module.summary.ui_summary,
            operation_summary: module.summary.operation_summary,
            permission_summary: module.summary.permission_summary,
            status: WorkspaceModuleStatus {
                kind: workspace_module_status_kind_to_contract(module.summary.status.kind),
                reason: module.summary.status.reason,
            },
        },
        ui_entries: module
            .ui_entries
            .into_iter()
            .map(|entry| WorkspaceModuleUiEntry {
                view_key: entry.view_key,
                renderer_kind: entry.renderer_kind,
                uri_scheme: entry.uri_scheme,
                title: entry.title,
            })
            .collect(),
        operations: module
            .operations
            .into_iter()
            .map(|operation| WorkspaceModuleOperation {
                operation_key: operation.operation_key,
                origin: operation.origin,
                description: operation.description,
                input_schema: operation.input_schema,
                output_schema: operation.output_schema,
                permission_summary: operation.permission_summary,
                dispatch: workspace_module_dispatch_to_contract(operation.dispatch),
            })
            .collect(),
        runtime_backing: module.runtime_backing,
    }
}

fn workspace_module_kind_to_contract(kind: WorkspaceModuleReadModelKind) -> WorkspaceModuleKind {
    match kind {
        WorkspaceModuleReadModelKind::Extension => WorkspaceModuleKind::Extension,
        WorkspaceModuleReadModelKind::Canvas => WorkspaceModuleKind::Canvas,
        WorkspaceModuleReadModelKind::Builtin => WorkspaceModuleKind::Builtin,
    }
}

fn workspace_module_status_kind_to_contract(
    kind: WorkspaceModuleReadModelStatusKind,
) -> WorkspaceModuleStatusKind {
    match kind {
        WorkspaceModuleReadModelStatusKind::Ready => WorkspaceModuleStatusKind::Ready,
        WorkspaceModuleReadModelStatusKind::Unavailable => WorkspaceModuleStatusKind::Unavailable,
    }
}

fn workspace_module_dispatch_to_contract(
    dispatch: WorkspaceModuleReadModelOperationDispatch,
) -> WorkspaceModuleOperationDispatch {
    match dispatch {
        WorkspaceModuleReadModelOperationDispatch::RuntimeAction { action_key } => {
            WorkspaceModuleOperationDispatch::RuntimeAction { action_key }
        }
        WorkspaceModuleReadModelOperationDispatch::ProtocolChannel {
            channel_key,
            method_name,
        } => WorkspaceModuleOperationDispatch::ProtocolChannel {
            channel_key,
            method_name,
        },
        WorkspaceModuleReadModelOperationDispatch::Canvas { canvas_action } => {
            WorkspaceModuleOperationDispatch::Canvas { canvas_action }
        }
        WorkspaceModuleReadModelOperationDispatch::Builtin { builtin_key } => {
            WorkspaceModuleOperationDispatch::Builtin { builtin_key }
        }
    }
}
