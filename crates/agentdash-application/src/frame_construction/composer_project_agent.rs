//! ProjectAgent compose 路径 — 最简单的 owner bootstrap（无 workflow / story 依赖）。

use agentdash_domain::common::AgentBackendRequirement;
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun, SubjectRef};
use agentdash_domain::workspace::{Workspace, WorkspaceBinding};
use agentdash_platform_spi::{AgentConfig, PlatformRuntimeError, Vfs};

use crate::agent_run::frame::{
    AgentFrameBuilder, AgentFrameSurfaceExt, FrameLaunchEnvelope,
    FrameLaunchEnvelopeConstructionInput,
};
use crate::agent_run::{build_project_agent_context, resolve_project_workspace};
use crate::context::SharedContextAuditBus;
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::workspace::BackendAvailability;
use crate::workspace::resolve_workspace_binding_with_allowed_backends;
use agentdash_application_ports::lifecycle_surface_projection::{
    ActiveWorkflowProjection, LifecycleSurfaceProjectionPort,
};
use agentdash_application_vfs::VfsService;

use super::subject_assignment::{SubjectContextAssignment, SubjectContextAssignmentResolver};
use super::workflow_projection::resolve_active_workflow_projection_from_message_stream_trace;
use super::{
    FrameConstructionService, OwnerBootstrapSpec, OwnerScope, connector_internal,
    frame_builder_from_existing, merge_user_executor_config, owner_prompt_launch_path,
    required_user_input,
};

pub(super) struct ProjectAgentOwnerCompositionContext<'a> {
    pub repos: &'a RepositorySet,
    pub vfs_service: &'a VfsService,
    pub availability: &'a dyn BackendAvailability,
    pub platform_config: &'a PlatformConfig,
    pub lifecycle_surface_projection: &'a dyn LifecycleSurfaceProjectionPort,
    pub audit_bus: Option<SharedContextAuditBus>,
}

impl<'a> ProjectAgentOwnerCompositionContext<'a> {
    fn from_service(svc: &'a FrameConstructionService) -> Self {
        Self {
            repos: &svc.repos,
            vfs_service: svc.vfs_service.as_ref(),
            availability: svc.availability.as_ref(),
            platform_config: svc.platform_config.as_ref(),
            lifecycle_surface_projection: svc.lifecycle_surface_projection.as_ref(),
            audit_bus: Some(svc.audit_bus.clone()),
        }
    }

    fn owner_bootstrap_composer(&self) -> super::OwnerBootstrapComposer<'_> {
        let composer = super::OwnerBootstrapComposer::new(
            self.vfs_service,
            self.availability,
            self.repos,
            self.platform_config,
            self.lifecycle_surface_projection,
        );
        match self.audit_bus.as_ref() {
            Some(audit_bus) => composer.with_audit_bus(audit_bus.clone()),
            None => composer,
        }
    }
}

pub(super) struct ProjectAgentOwnerCompositionInput<'a> {
    pub builder: AgentFrameBuilder,
    pub agent: &'a LifecycleAgent,
    pub run: &'a LifecycleRun,
    pub subject_ref: Option<SubjectRef>,
    pub executor_config_override: Option<AgentConfig>,
    pub user_input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub existing_vfs: Option<Vfs>,
    pub active_workflow: Option<ActiveWorkflowProjection>,
    pub launch_path: super::OwnerPromptLaunchPath,
    pub runtime_session_id: String,
}

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    agent: LifecycleAgent,
    run: LifecycleRun,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
    let launch_path = owner_prompt_launch_path(
        svc.prompt_launch_path(frame.typed_execution_profile().as_ref(), input),
    );
    let user_input = required_user_input(input.command.prompt())?;
    let active_workflow = resolve_active_workflow_projection_from_message_stream_trace(
        input.session_id.as_str(),
        &svc.repos,
    )
    .await
    .map_err(connector_internal)?;
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), input.session_id.as_str())?;
    let (builder, extras) = compose_project_agent_owner_frame(
        &ProjectAgentOwnerCompositionContext::from_service(svc),
        ProjectAgentOwnerCompositionInput {
            builder,
            agent: &agent,
            run: &run,
            subject_ref: None,
            executor_config_override: input.command.prompt().executor_config.clone(),
            user_input,
            existing_vfs: frame.typed_vfs(),
            active_workflow,
            launch_path,
            runtime_session_id: input.session_id.clone(),
        },
    )
    .await?;

    svc.compose_pending_frame(
        builder,
        extras,
        &input.command,
        input.session_id.as_str(),
        None,
        &input.requested_runtime_commands,
    )
    .await
}

pub(super) async fn compose_project_agent_owner_frame(
    context: &ProjectAgentOwnerCompositionContext<'_>,
    input: ProjectAgentOwnerCompositionInput<'_>,
) -> Result<(AgentFrameBuilder, super::FrameAssemblyLaunchExtras), PlatformRuntimeError> {
    let project_agent_id = input.agent.project_agent_id.ok_or_else(|| {
        PlatformRuntimeError::InvalidConfig(format!(
            "LifecycleAgent {} 缺少 project_agent_id",
            input.agent.id
        ))
    })?;
    let project = context
        .repos
        .project_repo
        .get_by_id(input.run.project_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(format!("Project {} 不存在", input.run.project_id))
        })?;
    let project_agent = context
        .repos
        .project_agent_repo
        .get_by_project_and_id(project.id, project_agent_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(format!("ProjectAgent {} 不存在", project_agent_id))
        })?;
    let agent_context = build_project_agent_context(&project_agent)
        .await
        .map_err(connector_internal)?;
    let executor_config = merge_user_executor_config(
        input.executor_config_override,
        &agent_context.executor_config,
    );
    let backend_requirement = agent_context.preset_config.backend_requirement_or_default();
    let workspace = resolve_project_agent_workspace(context, &project, backend_requirement).await?;
    let mut subject_assignment = resolve_project_agent_subject_assignment(
        context,
        input.run.id,
        input.agent.id,
        input.run.project_id,
        input.subject_ref.as_ref(),
    )
    .await?;
    let subject_owner_ctx = subject_assignment
        .as_ref()
        .map(|assignment| assignment.capability_scope.clone());
    let subject_context_contributions = subject_assignment
        .as_mut()
        .map(|assignment| std::mem::take(&mut assignment.contributions))
        .unwrap_or_default();
    let subject_workspace = subject_assignment
        .as_ref()
        .and_then(|assignment| assignment.workspace.as_ref());
    let lifecycle_address =
        agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress {
            run_id: input.run.id,
            agent_id: input.agent.id,
            frame_id: input.builder.frame_id(),
        };
    let lifecycle_message_stream =
        agentdash_application_ports::lifecycle_surface_projection::MessageStreamProjectionRef {
            runtime_session_id: input.runtime_session_id,
            trace_kind: agentdash_application_ports::lifecycle_surface_projection::MessageStreamTraceKind::ConnectorRuntimeSession,
        };

    context
        .owner_bootstrap_composer()
        .compose_owner_bootstrap_to_frame(
            input.builder,
            OwnerBootstrapSpec {
                owner: OwnerScope::Project {
                    project: &project,
                    workspace: workspace.as_ref(),
                    project_agent: Some(&project_agent),
                    agent_display_name: agent_context.display_name.clone(),
                    preset_name: agent_context.preset_name.clone(),
                },
                subject_context_contributions,
                subject_owner_ctx,
                subject_workspace,
                executor_config,
                user_input: input.user_input,
                agent_tool_directives: agent_context
                    .preset_config
                    .capability_directives
                    .clone()
                    .unwrap_or_default(),
                agent_skill_asset_keys: agent_context
                    .preset_config
                    .skill_asset_keys
                    .clone()
                    .unwrap_or_default(),
                project_vfs_mount_exposure_grants: agent_context
                    .preset_config
                    .project_vfs_mount_exposure_grants
                    .clone()
                    .unwrap_or_default(),
                request_mcp_servers: Vec::new(),
                existing_vfs: input.existing_vfs,
                workspace_module_policy_refs: agent_context
                    .preset_config
                    .visible_workspace_module_refs
                    .clone(),
                active_workflow: input.active_workflow,
                launch_path: input.launch_path,
                lifecycle_address,
                lifecycle_message_stream,
                audit_run_id: Some(input.run.id.to_string()),
                audit_agent_id: Some(input.agent.id.to_string()),
                caller_agent_id: Some(project_agent.id),
            },
        )
        .await
        .map_err(PlatformRuntimeError::InvalidConfig)
}

async fn resolve_project_agent_workspace(
    context: &ProjectAgentOwnerCompositionContext<'_>,
    project: &agentdash_domain::project::Project,
    backend_requirement: AgentBackendRequirement,
) -> Result<Option<Workspace>, PlatformRuntimeError> {
    let Some(workspace) = resolve_project_workspace(context.repos.workspace_repo.as_ref(), project)
        .await
        .map_err(connector_internal)?
    else {
        return Ok(None);
    };
    let active_accesses = context
        .repos
        .project_backend_access_repo
        .list_active_by_project(project.id)
        .await
        .map_err(connector_internal)?;
    let allowed_backend_ids = active_accesses
        .into_iter()
        .map(|access| access.backend_id.trim().to_string())
        .filter(|backend_id| !backend_id.is_empty())
        .collect::<std::collections::HashSet<_>>();
    let resolved = resolve_workspace_binding_with_allowed_backends(
        context.availability,
        &workspace,
        Some(&allowed_backend_ids),
    )
    .await;
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(error) => {
            return match backend_requirement {
                AgentBackendRequirement::Required => {
                    Err(PlatformRuntimeError::ConnectionFailed(error.to_string()))
                }
                AgentBackendRequirement::Optional => Ok(None),
            };
        }
    };
    if !context.availability.is_online(&resolved.backend_id).await {
        return match backend_requirement {
            AgentBackendRequirement::Required => Err(PlatformRuntimeError::ConnectionFailed(format!(
                "Workspace `{}` 的 backend `{}` 当前不在线",
                workspace.name, resolved.backend_id
            ))),
            AgentBackendRequirement::Optional => Ok(None),
        };
    }
    Ok(Some(workspace_with_selected_binding(
        workspace,
        resolved.binding_id,
    )))
}

fn workspace_with_selected_binding(mut workspace: Workspace, binding_id: uuid::Uuid) -> Workspace {
    let selected = workspace
        .bindings
        .iter()
        .find(|binding| binding.id == binding_id)
        .cloned();
    if let Some(binding) = selected {
        workspace.bindings = vec![WorkspaceBinding {
            workspace_id: workspace.id,
            ..binding
        }];
        workspace.default_binding_id = Some(binding_id);
    }
    workspace
}

async fn resolve_project_agent_subject_assignment(
    context: &ProjectAgentOwnerCompositionContext<'_>,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    project_id: uuid::Uuid,
    explicit_subject_ref: Option<&SubjectRef>,
) -> Result<Option<SubjectContextAssignment>, PlatformRuntimeError> {
    let subject_ref = match explicit_subject_ref {
        Some(subject_ref) if subject_ref.kind != "project" => Some(subject_ref.clone()),
        Some(_) => None,
        None => resolve_project_agent_subject_ref(context, run_id, agent_id).await?,
    };
    let Some(subject_ref) = subject_ref else {
        return Ok(None);
    };
    let assignment = SubjectContextAssignmentResolver::new(
        context.repos,
        context.availability,
        context.vfs_service,
    )
    .resolve(project_id, subject_ref)
    .await
    .map_err(connector_internal)?;
    Ok(Some(assignment))
}

async fn resolve_project_agent_subject_ref(
    context: &ProjectAgentOwnerCompositionContext<'_>,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
) -> Result<Option<SubjectRef>, PlatformRuntimeError> {
    let agent_associations = context
        .repos
        .lifecycle_subject_association_repo
        .list_by_anchor(run_id, Some(agent_id))
        .await
        .map_err(connector_internal)?;
    Ok(agent_associations
        .iter()
        .find(|assoc| assoc.role == "subject" && assoc.subject_kind != "project")
        .map(|assoc| SubjectRef::new(assoc.subject_kind.clone(), assoc.subject_id)))
}
