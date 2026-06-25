//! ProjectAgent compose 路径 — 最简单的 owner bootstrap（无 workflow / story 依赖）。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun, SubjectRef};
use agentdash_spi::ConnectorError;

use crate::agent_run::frame::launch_envelope_provider::FrameLaunchEnvelopeConstructionInput;
use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::project_agent_context::{
    build_project_agent_context, resolve_project_workspace,
};

use super::subject_assignment::{SubjectContextAssignment, SubjectContextAssignmentResolver};
use super::workflow_projection::resolve_active_workflow_projection_from_message_stream_trace;
use super::{
    FrameConstructionService, OwnerBootstrapSpec, OwnerScope, connector_internal,
    frame_builder_from_existing, merge_user_executor_config, owner_prompt_launch_path,
    required_user_input,
};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    agent: LifecycleAgent,
    run: LifecycleRun,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let project_agent_id = agent.project_agent_id.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("LifecycleAgent {} 缺少 project_agent_id", agent.id))
    })?;
    let project = svc
        .repos
        .project_repo
        .get_by_id(run.project_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("Project {} 不存在", run.project_id))
        })?;
    let project_agent = svc
        .repos
        .project_agent_repo
        .get_by_project_and_id(project.id, project_agent_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("ProjectAgent {} 不存在", project_agent_id))
        })?;
    let agent_context = build_project_agent_context(&svc.repos, &project_agent)
        .await
        .map_err(connector_internal)?;
    let workspace = resolve_project_workspace(&svc.repos, &project)
        .await
        .map_err(connector_internal)?;
    let mut subject_assignment =
        resolve_project_agent_subject_assignment(svc, run.id, agent.id, run.project_id).await?;
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
    let executor_config = merge_user_executor_config(
        input.command.user_input().executor_config.clone(),
        &agent_context.executor_config,
    );
    let launch_path =
        owner_prompt_launch_path(svc.prompt_launch_path(Some(&executor_config), input));
    let user_input = required_user_input(input.command.user_input())?;
    let identity = input.command.identity();
    let active_workflow = resolve_active_workflow_projection_from_message_stream_trace(
        input.session_id.as_str(),
        &svc.repos,
    )
    .await
    .map_err(connector_internal)?;
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), input.session_id.as_str())?;
    let (builder, extras) = svc
        .owner_bootstrap_composer()
        .compose_owner_bootstrap_to_frame(
            builder,
            OwnerBootstrapSpec {
                owner: OwnerScope::Project {
                    project: &project,
                    workspace: workspace.as_ref(),
                    project_agent: Some(&project_agent),
                    agent_display_name: agent_context.display_name.clone(),
                    preset_name: agent_context.preset_name.clone(),
                },
                identity: identity.as_ref(),
                subject_context_contributions,
                subject_owner_ctx,
                subject_workspace,
                executor_config,
                user_input,
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
                agent_vfs_access_grants: agent_context
                    .preset_config
                    .vfs_access_grants
                    .clone()
                    .unwrap_or_default(),
                request_mcp_servers: input
                    .command
                    .local_relay_modifier()
                    .map(|payload| payload.mcp_servers.clone())
                    .unwrap_or_default(),
                existing_vfs: frame.typed_vfs(),
                visible_canvas_mount_ids: frame.visible_canvas_mount_ids(),
                // 三态直达：None/空集 → base mode=All；非空 → Allowlist（不再 unwrap_or_default 抹平）。
                visible_workspace_module_refs: agent_context
                    .preset_config
                    .visible_workspace_module_refs
                    .clone(),
                active_workflow,
                launch_path,
                audit_session_key: Some(input.session_id.clone()),
                caller_agent_id: Some(project_agent.id),
            },
        )
        .await
        .map_err(ConnectorError::InvalidConfig)?;

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

async fn resolve_project_agent_subject_assignment(
    svc: &FrameConstructionService,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    project_id: uuid::Uuid,
) -> Result<Option<SubjectContextAssignment>, ConnectorError> {
    let Some(subject_ref) = resolve_project_agent_subject_ref(svc, run_id, agent_id).await? else {
        return Ok(None);
    };
    let assignment = SubjectContextAssignmentResolver::new(
        &svc.repos,
        svc.availability.as_ref(),
        svc.vfs_service.as_ref(),
    )
    .resolve(project_id, subject_ref)
    .await
    .map_err(connector_internal)?;
    Ok(Some(assignment))
}

async fn resolve_project_agent_subject_ref(
    svc: &FrameConstructionService,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
) -> Result<Option<SubjectRef>, ConnectorError> {
    let agent_associations = svc
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
