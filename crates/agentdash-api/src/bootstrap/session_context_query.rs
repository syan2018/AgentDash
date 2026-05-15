//! Session context query use case.
//!
//! Route 层只负责权限与 DTO 投影；Task / Story / Project 的 context 查询分支
//! 集中在这里返回 `SessionConstructionPlan`。这一步仍复用现有 context builder，
//! 后续需要继续与 launch construction planner 合流为同一事实源。

use std::sync::Arc;

use agentdash_application::session::construction::{
    SessionConstructionContextProjection, SessionConstructionPlan,
};
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{
    ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
    load_task_story_project_with_permission,
};
use crate::routes::vfs_surfaces::build_surface_summary;
use crate::routes::{project_sessions, story_sessions, task_execution};
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
                    .session_hub
                    .get_session_meta(session_id)
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?
            } else {
                None
            };
            let built_context =
                agentdash_application::task::context_builder::build_task_session_context(
                    &state.repos,
                    &state.services.vfs_service,
                    &state.config.platform_config,
                    task_id,
                    session_meta.as_ref(),
                )
                .await;
            let resolved_vfs = built_context
                .as_ref()
                .and_then(|context| context.vfs.clone());
            let capabilities =
                try_build_session_capabilities(state, session_id, resolved_vfs.as_ref()).await;
            let runtime_surface = if let Some(space) = resolved_vfs.as_ref() {
                Some(
                    build_surface_summary(
                        state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.to_string(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            SessionConstructionPlan::new(
                session_id.to_string(),
                owner,
                SessionConstructionContextProjection {
                    workspace_id: task.workspace_id,
                    agent_binding: Some(result.agent_binding),
                    vfs: resolved_vfs,
                    runtime_surface,
                    context_snapshot: built_context.and_then(|context| context.context_snapshot),
                    session_capabilities: capabilities,
                },
            )
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
            let built_context =
                story_sessions::build_story_session_context_response(state, &story, session_id)
                    .await?;
            let resolved_vfs = built_context
                .as_ref()
                .and_then(|context| context.vfs.clone());
            let capabilities =
                try_build_session_capabilities(state, session_id, resolved_vfs.as_ref()).await;
            let runtime_surface = if let Some(space) = resolved_vfs.as_ref() {
                Some(
                    build_surface_summary(
                        state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.to_string(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            SessionConstructionPlan::new(
                session_id.to_string(),
                owner,
                SessionConstructionContextProjection {
                    workspace_id: None,
                    agent_binding: None,
                    vfs: resolved_vfs,
                    runtime_surface,
                    context_snapshot: built_context.and_then(|context| context.context_snapshot),
                    session_capabilities: capabilities,
                },
            )
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
            let built_context = project_sessions::build_project_session_context_response(
                state,
                &project,
                session_id,
                &owner.label,
            )
            .await?;
            let capabilities =
                try_build_session_capabilities(state, session_id, built_context.vfs.as_ref()).await;
            let runtime_surface = if let Some(space) = built_context.vfs.as_ref() {
                Some(
                    build_surface_summary(
                        state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.to_string(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            SessionConstructionPlan::new(
                session_id.to_string(),
                owner,
                SessionConstructionContextProjection {
                    workspace_id: None,
                    agent_binding: None,
                    vfs: built_context.vfs,
                    runtime_surface,
                    context_snapshot: built_context.context_snapshot,
                    session_capabilities: capabilities,
                },
            )
        }
    };

    Ok(Some(plan))
}

async fn try_build_session_capabilities(
    state: &AppState,
    _session_id: &str,
    vfs: Option<&agentdash_spi::Vfs>,
) -> Option<agentdash_spi::SessionBaselineCapabilities> {
    let skills = if let Some(space) = vfs {
        let result =
            agentdash_application::skill::load_skills_from_vfs(&state.services.vfs_service, space)
                .await;
        result.skills
    } else {
        Vec::new()
    };

    let caps =
        agentdash_application::session::baseline_capabilities::build_session_baseline_capabilities(
            &skills,
        );

    if caps.is_empty() { None } else { Some(caps) }
}
