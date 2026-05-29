use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application::session::construction::SessionConstructionPlan;
use agentdash_application::session::construction_provider::{
    CompanionLaunchSource, SessionConstructionProviderInput, TaskLaunchSource,
};
use agentdash_application::session::construction_use_case::{
    SessionConstructionConfigDeps, SessionConstructionServiceDeps, SessionConstructionUseCaseDeps,
};
use agentdash_application::session::context_query_use_case::{
    SessionContextQueryInput, SessionContextQueryOwnerFacts,
};
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_application::session::{UserPromptInput, construction_use_case};
use agentdash_application::workspace::BackendAvailability;
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{
    ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
    load_task_story_project_with_permission,
};
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
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;

    let owner_facts = match owner.owner_type {
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
            SessionContextQueryOwnerFacts::Task {
                task_id,
                workspace_id: task.workspace_id,
                agent_binding: result.agent_binding,
            }
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
            SessionContextQueryOwnerFacts::Story { story }
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
            SessionContextQueryOwnerFacts::Project {
                project,
                binding_label: owner.label.clone(),
            }
        }
    };

    let had_existing_runtime = state.services.connector.has_live_session(session_id).await;
    let requested_runtime_commands = state
        .services
        .session_capability
        .list_requested_runtime_commands(session_id)
        .await
        .map_err(ApiError::from)?;
    let deps = session_construction_deps(state);
    let input = SessionContextQueryInput {
        session_id: session_id.to_string(),
        owner,
        owner_facts,
        session_meta,
        identity: Some(current_user.clone()),
        had_existing_runtime,
        requested_runtime_commands,
    };
    let mut plan =
        agentdash_application::session::context_query_use_case::build_session_context_plan(
            &deps, input,
        )
        .await
        .map_err(ApiError::from)?;
    if let Some(plan) = plan.as_mut() {
        attach_runtime_surface(state, session_id, plan).await?;
    }
    Ok(plan)
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
        SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use agentdash_application::session::ownership::SessionOwnerResolver;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
    use agentdash_spi::Vfs;
    use uuid::Uuid;

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

    fn test_owner() -> agentdash_application::session::ownership::ResolvedSessionOwner {
        let binding = SessionBinding::new(
            Uuid::new_v4(),
            "s1".to_string(),
            SessionOwnerType::Project,
            Uuid::new_v4(),
            "project",
        );
        SessionOwnerResolver::resolve_primary(&[binding]).expect("owner")
    }

    #[test]
    fn runtime_surface_uses_surface_vfs() {
        let mut plan = SessionConstructionPlan::new(
            "s1",
            test_owner(),
            SessionConstructionContextProjection::default(),
        );
        plan.surface.vfs = Some(vfs_with_mount("surface"));
        plan.context_projection.vfs = Some(vfs_with_mount("context"));

        let vfs = runtime_surface_vfs(&plan).expect("surface vfs");
        assert_eq!(vfs.mounts[0].id, "surface");
    }
}
