//! Session context query use case.
//!
//! Route 层只负责权限与 DTO 投影；Task / Story / Project 的 context projection
//! 由 application 层 `SessionConstructionPlanner` 产出。

use std::sync::Arc;

use agentdash_application::session::construction::SessionConstructionPlan;
use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{
    ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
    load_task_story_project_with_permission,
};
use crate::routes::task_execution;
use crate::routes::vfs_surfaces::build_surface_summary;
use crate::rpc::ApiError;

pub(crate) async fn build_session_context_plan(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    bindings: &[SessionBinding],
) -> Result<Option<SessionConstructionPlan>, ApiError> {
    let Some(owner) = SessionOwnerResolver::resolve_primary(bindings) else {
        return Ok(None);
    };

    let plan = match owner.owner_type {
        SessionOwnerType::Task => {
            let task_id = owner.owner_id;
            let (task, _, _) = load_task_story_project_with_permission(
                state.as_ref(),
                current_user,
                task_id,
                ProjectPermission::View,
            )
            .await?;
            let result = state
                .services
                .story_step_activation_service
                .get_task_session(task_id)
                .await
                .map_err(task_execution::map_task_execution_error)?;
            let session_meta = if let Some(session_id) = result.session_id.as_deref() {
                state
                    .services
                    .session_core
                    .get_session_meta(session_id)
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?
            } else {
                None
            };
            let mut plan = SessionConstructionPlanner::plan_task_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.config.platform_config,
                session_id.to_string(),
                owner,
                task_id,
                task.workspace_id,
                result.agent_binding,
                session_meta.as_ref(),
            )
            .await;
            attach_runtime_surface(state, session_id, &mut plan).await?;
            plan
        }
        SessionOwnerType::Story => {
            let story_id = owner.owner_id;
            let (story, _) = load_story_and_project_with_permission(
                state.as_ref(),
                current_user,
                story_id,
                ProjectPermission::View,
            )
            .await?;
            let session_meta = state
                .services
                .session_core
                .get_session_meta(session_id)
                .await
                .map_err(|error| {
                    ApiError::Internal(format!("读取 story session meta 失败: {error}"))
                })?;
            let Some(mut plan) = SessionConstructionPlanner::plan_story_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.config.platform_config,
                session_id.to_string(),
                owner,
                &story,
                session_meta.as_ref(),
            )
            .await
            .map_err(ApiError::Internal)?
            else {
                return Ok(None);
            };
            attach_runtime_surface(state, session_id, &mut plan).await?;
            plan
        }
        SessionOwnerType::Project => {
            let project_id = owner.owner_id;
            let project = load_project_with_permission(
                state.as_ref(),
                current_user,
                project_id,
                ProjectPermission::View,
            )
            .await?;
            let session_meta = state
                .services
                .session_core
                .get_session_meta(session_id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?
                .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;
            let binding_label = owner.label.clone();
            let mut plan = SessionConstructionPlanner::plan_project_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.config.platform_config,
                session_id.to_string(),
                owner,
                &project,
                &binding_label,
                &session_meta,
            )
            .await
            .map_err(|error| {
                if error.starts_with("无效的项目 Agent session label")
                    || error.starts_with("Project Agent `")
                {
                    ApiError::NotFound(error)
                } else {
                    ApiError::Internal(error)
                }
            })?;
            attach_runtime_surface(state, session_id, &mut plan).await?;
            plan
        }
    };

    Ok(Some(plan))
}

async fn attach_runtime_surface(
    state: &Arc<AppState>,
    session_id: &str,
    plan: &mut SessionConstructionPlan,
) -> Result<(), ApiError> {
    let Some(vfs) = plan.context_projection.vfs.as_ref() else {
        return Ok(());
    };
    let runtime_surface = build_surface_summary(
        state,
        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
            session_id: session_id.to_string(),
        },
        vfs,
    )
    .await?;
    plan.context_projection.runtime_surface = Some(runtime_surface.clone());
    plan.projections.context.runtime_surface = Some(runtime_surface.clone());
    plan.surface.runtime_surface = Some(runtime_surface);
    Ok(())
}
