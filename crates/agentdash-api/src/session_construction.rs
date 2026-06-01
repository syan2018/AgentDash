use std::sync::Arc;

use agentdash_application::session::construction::{
    OwnerResolutionTrace, ResolvedSessionOwner, RuntimeContextInspectionPlan,
};
use agentdash_application::session::construction_planner::RuntimeContextInspectionPlanner;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::CapabilityScope;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;
use crate::vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection;

pub(crate) async fn build_session_context_plan(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<Option<RuntimeContextInspectionPlan>, ApiError> {
    let frame = state
        .repos
        .agent_frame_repo
        .find_by_runtime_session(session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!("runtime_session 未附着到 AgentFrame: {session_id}"))
        })?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(frame.agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {}", frame.agent_id)))?;

    let session_meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;

    let project_id = Some(agent.project_id);

    let owner = ResolvedSessionOwner {
        owner_type: CapabilityScope::Project,
        project_id,
        trace: OwnerResolutionTrace {
            selected_reason: "context_query: runtime_session.agent_frame.lifecycle_agent"
                .to_string(),
        },
    };

    let mut plan = if let Some(pid) = project_id {
        let project = load_project_with_permission(
            state.as_ref(),
            current_user,
            pid,
            ProjectPermission::View,
        )
        .await?;
        let binding_label = agentdash_application::workflow::FREEFORM_SESSION_LABEL.to_string();
        RuntimeContextInspectionPlanner::plan_project_context_query(
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
        })?
    } else {
        return Ok(None);
    };
    attach_runtime_surface(state, session_id, &mut plan).await?;
    Ok(Some(plan))
}

async fn attach_runtime_surface(
    state: &Arc<AppState>,
    session_id: &str,
    plan: &mut RuntimeContextInspectionPlan,
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
    plan.surface.runtime_surface = Some(runtime_surface);
    Ok(())
}

fn runtime_surface_vfs(plan: &RuntimeContextInspectionPlan) -> Option<&agentdash_spi::Vfs> {
    plan.surface.vfs.as_ref()
}

#[cfg(test)]
mod tests {
    use agentdash_application::session::construction::{
        OwnerResolutionTrace, ResolvedSessionOwner, RuntimeContextInspectionPlan,
        SessionConstructionContextProjection,
    };
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::{CapabilityScope, Vfs};

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
            ..Default::default()
        }
    }

    #[test]
    fn runtime_surface_uses_final_surface_vfs_after_finalize() {
        let owner = ResolvedSessionOwner {
            owner_type: CapabilityScope::Project,
            project_id: None,
            trace: OwnerResolutionTrace {
                selected_reason: "test".to_string(),
            },
        };
        let mut plan = RuntimeContextInspectionPlan::new(
            "s1",
            owner,
            SessionConstructionContextProjection::default(),
        );
        plan.surface.vfs = Some(vfs_with_mount("surface"));
        plan.context_projection.vfs = Some(vfs_with_mount("context"));

        let vfs = runtime_surface_vfs(&plan).expect("surface vfs");
        assert_eq!(vfs.mounts[0].id, "surface");
    }
}
