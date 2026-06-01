//! Runtime context inspection projection use case.
//!
//! 这个模块承接 owner/context/capability 查询逻辑。它只返回 runtime/context inspection projection；
//! route 层不承载 runtime launch composition 主分支。

use std::path::PathBuf;
use std::sync::Arc;

use crate::extension_runtime::{
    ExtensionRuntimeProjection, extension_runtime_projection_from_installations,
};
use crate::session::construction::{
    ConstructionResolutionPlan, RuntimeContextInspectionPlan, SessionConstructionTraceEntry,
};
use crate::session::construction_provider::{
    RoutineLaunchSource, SessionConstructionProviderInput,
};
use crate::session::local_workspace_vfs;
use crate::session::replay_runtime_capability_transitions;
use crate::session::{
    SessionCapabilityProjectionInput, derive_session_capability_projection,
    normalize_capability_state_dimensions,
};
use crate::skill_asset::SkillAssetService;
use agentdash_domain::routine::ROUTINE_MEMORY_SKILL_NAME;

use crate::context::SharedContextAuditBus;
use crate::error::ApplicationError;
use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::{SessionCapabilityService, SessionEventingService};
use crate::vfs::VfsService;
use crate::workspace::BackendAvailability;
use agentdash_executor::AgentConnector;

pub struct SessionConstructionUseCaseDeps<'a> {
    pub repos: &'a RepositorySet,
    pub services: SessionConstructionServiceDeps<'a>,
    pub config: SessionConstructionConfigDeps,
}

pub struct SessionConstructionServiceDeps<'a> {
    pub connector: Arc<dyn AgentConnector>,
    pub vfs_service: Arc<VfsService>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub backend_registry: Arc<dyn BackendAvailability>,
    pub audit_bus: SharedContextAuditBus,
    pub session_capability: &'a SessionCapabilityService,
    pub session_eventing: &'a SessionEventingService,
}

pub struct SessionConstructionConfigDeps {
    pub platform_config: SharedPlatformConfig,
}

pub async fn finalize_session_construction_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    mut plan: RuntimeContextInspectionPlan,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: &SessionConstructionProviderInput,
) -> Result<RuntimeContextInspectionPlan, ApplicationError> {
    plan.source.launch_source = Some(facts.command.reason_tag().to_string());
    if plan.identity.identity.is_none() {
        plan.identity.identity = facts.command.identity();
    }

    let (mut base_vfs, mut vfs_source) = if let Some(vfs) = plan.surface.vfs.clone() {
        (vfs, "construction.surface.vfs".to_string())
    } else if let Some(root) = local_relay_workspace_root.as_ref() {
        (
            local_workspace_vfs(root),
            "source.local_relay_workspace_root".to_string(),
        )
    } else {
        return Err(ApplicationError::BadRequest(
            "construction 未产出 VFS，且来源事实中没有可解析 workspace root".to_string(),
        ));
    };

    if let Some(routine_source) = facts.command.routine_hint() {
        if let Some(pid) = plan.owner.project_id {
            append_routine_projection(state, &mut base_vfs, pid, &routine_source).await?;
        }
        vfs_source = format!("{vfs_source}+routine_source");
    }

    let (base_mcp_servers, base_mcp_source) = if !plan.projections.mcp_servers.is_empty() {
        (
            plan.projections.mcp_servers.clone(),
            "construction.projections.mcp_servers".to_string(),
        )
    } else if !source_mcp_declarations.is_empty() {
        (
            source_mcp_declarations,
            "source.mcp_declarations".to_string(),
        )
    } else {
        (Vec::new(), "empty".to_string())
    };

    let mut base_capability_state = plan
        .projections
        .capability_state
        .clone()
        .unwrap_or_default();
    base_capability_state.vfs.active = Some(base_vfs.clone());
    base_capability_state.tool.mcp_servers = base_mcp_servers.clone();

    let requested_transitions = facts
        .requested_runtime_commands
        .iter()
        .map(|command| command.pending_capability_state_transition())
        .collect::<Vec<_>>();
    let replay = if requested_transitions.is_empty() {
        None
    } else {
        Some(
            replay_runtime_capability_transitions(&base_capability_state, &requested_transitions)
                .map_err(ApplicationError::BadRequest)?,
        )
    };
    let effective_vfs = replay
        .as_ref()
        .and_then(|replay| replay.effective_vfs.clone())
        .unwrap_or_else(|| base_vfs.clone());
    let pending_overlay_applied = requested_transitions.iter().any(|transition| {
        transition
            .transition
            .effects
            .iter()
            .any(|effect| effect.dimension.as_str() == "vfs")
    });
    let (mcp_servers, mcp_source) = if let Some(replay) = replay.as_ref() {
        (
            replay
                .effective_mcp_servers
                .clone()
                .unwrap_or_else(|| replay.capability_state.tool.mcp_servers.clone()),
            "runtime_command.pending_transition".to_string(),
        )
    } else {
        (base_mcp_servers.clone(), base_mcp_source)
    };

    let working_directory = effective_vfs
        .default_mount()
        .map(|mount| PathBuf::from(mount.root_ref.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            ApplicationError::BadRequest("vfs 缺少 default_mount 或 root_ref 无效".to_string())
        })?;

    let projection = derive_session_capability_projection(SessionCapabilityProjectionInput {
        vfs_service: Some(&state.services.vfs_service),
        active_vfs: Some(&effective_vfs),
        extra_skill_dirs: state.services.extra_skill_dirs,
        diagnostics_label: "session_construction_finalize",
    })
    .await;
    let session_capabilities = projection.session_capabilities;
    let discovered_guidelines = projection.discovered_guidelines;

    let executor_source = if plan.execution_profile.executor_config.is_some() {
        "construction.execution_profile.executor_config"
    } else if facts.command.user_input().executor_config.is_some() {
        "source.user_input.executor_config"
    } else if facts.session_meta.executor_config.is_some() {
        "session.meta.executor_config"
    } else {
        "unresolved.inspect"
    };
    let executor_config = plan
        .execution_profile
        .executor_config
        .clone()
        .or_else(|| facts.command.user_input().executor_config.clone())
        .or_else(|| facts.session_meta.executor_config.clone());
    normalize_capability_state_dimensions(
        &mut base_capability_state,
        Some(base_vfs),
        base_mcp_servers,
        &session_capabilities,
    );

    let mut final_capability_state = replay
        .map(|replay| replay.capability_state)
        .unwrap_or_else(|| base_capability_state.clone());
    normalize_capability_state_dimensions(
        &mut final_capability_state,
        Some(effective_vfs.clone()),
        mcp_servers.clone(),
        &session_capabilities,
    );
    let extension_runtime = if let Some(pid) = plan.owner.project_id {
        build_extension_runtime_projection(state, pid).await?
    } else {
        ExtensionRuntimeProjection::default()
    };

    plan.workspace.working_directory = Some(working_directory);
    plan.execution_profile.executor_config = executor_config;
    plan.context_projection.session_capabilities = Some(session_capabilities.clone());
    plan.projections.mcp_servers = mcp_servers;
    plan.projections.capability_state = Some(final_capability_state);
    plan.set_active_vfs(effective_vfs);
    plan.projections.session_capabilities = Some(session_capabilities);
    plan.projections.discovered_guidelines = discovered_guidelines;
    plan.projections.extension_runtime = Some(extension_runtime);
    plan.resolution = ConstructionResolutionPlan {
        vfs_source: Some(if pending_overlay_applied {
            "runtime_command.pending_vfs_overlay".to_string()
        } else {
            vfs_source
        }),
        mcp_source: Some(mcp_source),
        capability_source: Some(if facts.requested_runtime_commands.is_empty() {
            "construction.base_capability_state".to_string()
        } else {
            "runtime_command.pending_transition".to_string()
        }),
        executor_source: Some(executor_source.to_string()),
        working_directory_source: Some("vfs.default_mount.root_ref".to_string()),
        pending_overlay_applied,
        runtime_base_capability_state: Some(base_capability_state),
    };
    plan.trace.entries.extend([
        SessionConstructionTraceEntry {
            stage: "vfs_source",
            source: plan.resolution.vfs_source.clone().unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "mcp_source",
            source: plan.resolution.mcp_source.clone().unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "capability_source",
            source: plan
                .resolution
                .capability_source
                .clone()
                .unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "working_directory_source",
            source: plan
                .resolution
                .working_directory_source
                .clone()
                .unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "extension_runtime",
            source: "project.extension_installations".to_string(),
        },
    ]);
    Ok(plan)
}

async fn build_extension_runtime_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    project_id: uuid::Uuid,
) -> Result<ExtensionRuntimeProjection, ApplicationError> {
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok(extension_runtime_projection_from_installations(
        installations,
    )?)
}

async fn append_routine_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    vfs: &mut agentdash_spi::Vfs,
    project_id: uuid::Uuid,
    source: &RoutineLaunchSource,
) -> Result<(), ApplicationError> {
    SkillAssetService::new(state.repos.skill_asset_repo.as_ref())
        .bootstrap_builtins(project_id, Some(ROUTINE_MEMORY_SKILL_NAME))
        .await
        .map_err(ApplicationError::from)?;

    let routine_mount = crate::vfs::build_routine_mount(
        source.routine_id,
        source.execution_id,
        &source.trigger_source,
        source.entity_key.as_deref(),
    );
    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|candidate| candidate.id == routine_mount.id)
    {
        *existing = routine_mount;
    } else {
        vfs.mounts.push(routine_mount);
    }

    crate::vfs::append_skill_asset_projection(
        vfs,
        project_id,
        &[ROUTINE_MEMORY_SKILL_NAME.to_string()],
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionCommandDefinition,
        ExtensionCommandHandler, ExtensionFlagDefinition, ExtensionFlagType,
        ExtensionMessageRendererDefinition, ExtensionPermissionAccess,
        ExtensionPermissionDeclaration, ExtensionRendererDeclaration,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
        ExtensionWorkspaceTabDefinition, ExtensionWorkspaceTabRendererDeclaration,
        InstalledAssetSource, ProjectExtensionInstallation,
    };

    use super::*;

    #[test]
    fn extension_runtime_projection_flattens_enabled_installations() {
        let source = InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "plugin:test:extension_template:demo",
            "0.1.0",
            "digest",
        );
        let manifest = ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "demo".to_string(),
            package: ExtensionPackageMetadata {
                name: "demo".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![ExtensionCommandDefinition {
                name: "demo:run".to_string(),
                description: "run demo".to_string(),
                handler: ExtensionCommandHandler::InjectMessage {
                    content: "run".to_string(),
                },
            }],
            flags: vec![ExtensionFlagDefinition {
                name: "demo.verbose".to_string(),
                flag_type: ExtensionFlagType::Bool,
                default: serde_json::Value::Bool(false),
                description: "verbose".to_string(),
            }],
            message_renderers: vec![ExtensionMessageRendererDefinition {
                custom_type: "demo.card".to_string(),
                renderer: ExtensionRendererDeclaration::JsonCard,
            }],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: "demo.profile".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                permissions: vec!["local.profile.read".to_string()],
            }],
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
                type_id: "demo.profile-panel".to_string(),
                label: "Profile".to_string(),
                uri_scheme: "demo".to_string(),
                renderer: ExtensionWorkspaceTabRendererDeclaration::Webview {
                    entry: "dist/panel/index.html".to_string(),
                },
            }],
            permissions: vec![ExtensionPermissionDeclaration::LocalProfile {
                access: ExtensionPermissionAccess::Read,
            }],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
            capability_directives: vec![],
            asset_refs: vec![],
        };
        let installation = ProjectExtensionInstallation::new(
            uuid::Uuid::new_v4(),
            "demo",
            "Demo Extension",
            manifest,
            source,
        )
        .expect("valid installation");

        let projection = extension_runtime_projection_from_installations(vec![installation])
            .expect("projection");

        assert_eq!(projection.installations.len(), 1);
        assert_eq!(projection.commands[0].name, "demo:run");
        assert_eq!(projection.flags[0].name, "demo.verbose");
        assert_eq!(projection.message_renderers[0].custom_type, "demo.card");
        assert_eq!(projection.runtime_actions[0].action_key, "demo.profile");
        assert_eq!(projection.workspace_tabs[0].type_id, "demo.profile-panel");
        assert_eq!(projection.permissions.len(), 1);
        assert_eq!(projection.bundles[0].entry, "dist/extension.js");
    }
}
