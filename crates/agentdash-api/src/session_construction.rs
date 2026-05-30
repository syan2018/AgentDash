use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application::session::construction::{
    OwnerResolutionTrace, ResolvedSessionOwner, SessionConstructionPlan,
};
use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::session::construction_provider::{
    CompanionLaunchSource, SessionConstructionProviderInput, TaskLaunchSource,
};
use agentdash_application::session::construction_use_case::{
    SessionConstructionConfigDeps, SessionConstructionServiceDeps, SessionConstructionUseCaseDeps,
};
use agentdash_application::session::{UserPromptInput, construction_use_case};
use agentdash_application::workspace::BackendAvailability;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::CapabilityScope;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;
use crate::vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection;

pub(crate) async fn build_session_construction_for_launch(
    state: &Arc<AppState>,
    session_id: &str,
    user_input: &UserPromptInput,
    task_input: Option<TaskLaunchSource>,
    companion_input: Option<CompanionLaunchSource>,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: SessionConstructionProviderInput,
) -> Result<SessionConstructionPlan, ApiError> {
    let deps = session_construction_deps(state);
    construction_use_case::build_session_construction_for_launch(
        &deps,
        session_id,
        user_input,
        task_input,
        companion_input,
        source_mcp_declarations,
        local_relay_workspace_root,
        facts,
    )
    .await
    .map_err(ApiError::from)
}

pub(crate) fn session_construction_deps<'a>(
    state: &'a Arc<AppState>,
) -> SessionConstructionUseCaseDeps<'a> {
    let backend_registry: Arc<dyn BackendAvailability> = state.services.backend_registry.clone();
    SessionConstructionUseCaseDeps {
        repos: &state.repos,
        services: SessionConstructionServiceDeps {
            connector: state.services.connector.clone(),
            vfs_service: state.services.vfs_service.clone(),
            extra_skill_dirs: &state.services.extra_skill_dirs,
            backend_registry,
            audit_bus: state.services.audit_bus.clone(),
            session_capability: &state.services.session_capability,
            session_eventing: &state.services.session_eventing,
        },
        config: SessionConstructionConfigDeps {
            platform_config: state.config.platform_config.clone(),
        },
    }
}

pub(crate) async fn build_session_context_plan(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<Option<SessionConstructionPlan>, ApiError> {
    let session_meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;

    let project_id = session_meta
        .project_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());

    let owner = ResolvedSessionOwner {
        owner_type: CapabilityScope::Project,
        project_id,
        trace: OwnerResolutionTrace {
            selected_reason: "context_query: meta.project_id".to_string(),
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
        SessionConstructionPlanner::plan_project_context_query(
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
    plan.surface.runtime_surface = Some(runtime_surface);
    Ok(())
}

fn runtime_surface_vfs(plan: &SessionConstructionPlan) -> Option<&agentdash_spi::Vfs> {
    plan.surface.vfs.as_ref()
}

#[cfg(test)]
mod tests {
    use agentdash_application::session::construction::{
        OwnerResolutionTrace, ResolvedSessionOwner, SessionConstructionContextProjection,
        SessionConstructionPlan,
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
        let mut plan = SessionConstructionPlan::new(
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
