//! Session context query use case.
//!
//! Route 层只负责权限与 DTO 投影；Task / Story / Project 的 context projection
//! 由 application 层 `SessionConstructionPlanner` 产出。

use std::sync::Arc;

use agentdash_application::session::construction::SessionConstructionPlan;
use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::session::construction_provider::SessionConstructionProviderInput;
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_application::session::{LaunchCommand, UserPromptInput};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{
    ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
    load_task_story_project_with_permission,
};
use crate::rpc::ApiError;
use crate::session_use_cases::construction::{
    SessionConstructionProjectionMode, finalize_session_construction_projection,
};
use crate::vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection;

pub(crate) async fn build_session_context_plan(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    bindings: &[SessionBinding],
) -> Result<Option<SessionConstructionPlan>, ApiError> {
    let Some(owner) = SessionOwnerResolver::resolve_primary(bindings) else {
        return Ok(None);
    };
    let session_meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;

    let mut plan = match owner.owner_type {
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
                .map_err(ApiError::from)?;
            let plan = SessionConstructionPlanner::plan_task_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.services.extra_skill_dirs,
                &state.config.platform_config,
                session_id.to_string(),
                owner,
                task_id,
                task.workspace_id,
                result.agent_binding,
                Some(&session_meta),
            )
            .await;
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
            let Some(plan) = SessionConstructionPlanner::plan_story_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.services.extra_skill_dirs,
                &state.config.platform_config,
                session_id.to_string(),
                owner,
                &story,
                Some(&session_meta),
            )
            .await
            .map_err(ApiError::Internal)?
            else {
                return Ok(None);
            };
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
            let binding_label = owner.label.clone();
            let plan = SessionConstructionPlanner::plan_project_context_query(
                &state.repos,
                &state.services.vfs_service,
                &state.services.extra_skill_dirs,
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
            plan
        }
    };

    let user_input = UserPromptInput {
        prompt_blocks: None,
        env: Default::default(),
        executor_config: session_meta.executor_config.clone(),
    };
    let had_existing_runtime = state.services.connector.has_live_session(session_id).await;
    let requested_runtime_commands = state
        .services
        .session_capability
        .list_requested_runtime_commands(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let facts = SessionConstructionProviderInput {
        session_id: session_id.to_string(),
        command: LaunchCommand::http_prompt_input(user_input, Some(current_user.clone())),
        session_meta,
        had_existing_runtime,
        requested_runtime_commands,
    };
    plan = finalize_session_construction_projection(
        state,
        plan,
        Vec::new(),
        None,
        &facts,
        SessionConstructionProjectionMode::Inspect,
    )
    .await?;
    attach_runtime_surface(state, session_id, &mut plan).await?;

    Ok(Some(plan))
}

async fn attach_runtime_surface(
    state: &Arc<AppState>,
    session_id: &str,
    plan: &mut SessionConstructionPlan,
) -> Result<(), ApiError> {
    let Some(vfs) = runtime_surface_vfs(plan) else {
        return Ok(());
    };
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let runtime_surface = agentdash_application::vfs::build_surface_summary(
        state.repos.inline_file_repo.as_ref(),
        &runtime_projection,
        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
            session_id: session_id.to_string(),
        },
        vfs,
    )
    .await;
    plan.context_projection.runtime_surface = Some(runtime_surface.clone());
    plan.projections.context.runtime_surface = Some(runtime_surface.clone());
    plan.surface.runtime_surface = Some(runtime_surface);
    Ok(())
}

fn runtime_surface_vfs(plan: &SessionConstructionPlan) -> Option<&agentdash_spi::Vfs> {
    plan.surface.vfs.as_ref()
}

#[cfg(test)]
mod tests {
    use agentdash_application::session::construction::{
        SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use agentdash_application::session::ownership::SessionOwnerResolver;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
    use agentdash_spi::Vfs;

    use super::*;

    fn vfs_with_mount(mount_id: &str) -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: mount_id.to_string(),
                provider: "inline_fs".to_string(),
                backend_id: String::new(),
                root_ref: format!("inline://{mount_id}"),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: mount_id.to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some(mount_id.to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn runtime_surface_uses_final_surface_vfs_after_finalize() {
        let binding = SessionBinding::new(
            uuid::Uuid::new_v4(),
            "sess-final-vfs".to_string(),
            SessionOwnerType::Project,
            uuid::Uuid::new_v4(),
            "project_agent:test",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let initial_vfs = vfs_with_mount("initial");
        let final_vfs = vfs_with_mount("final");
        let mut plan = SessionConstructionPlan::new(
            "sess-final-vfs",
            owner,
            SessionConstructionContextProjection {
                vfs: Some(initial_vfs),
                ..Default::default()
            },
        );
        plan.surface.vfs = Some(final_vfs);

        let selected = runtime_surface_vfs(&plan).expect("final vfs");

        assert_eq!(selected.mounts[0].id, "final");
    }
}
