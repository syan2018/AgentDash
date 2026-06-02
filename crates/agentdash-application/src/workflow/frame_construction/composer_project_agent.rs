//! ProjectAgent compose 路径 — 最简单的 owner bootstrap（无 workflow / story 依赖）。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_planner::RuntimeContextInspectionPlanner;
use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::{AgentLevelMcp, OwnerBootstrapSpec, OwnerScope};
use crate::workflow::frame_surface::AgentFrameSurfaceExt;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{
    FrameConstructionService, connector_internal, frame_builder_from_existing,
    merge_user_executor_config, owner_prompt_lifecycle, required_prompt_blocks,
};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    mut agent: LifecycleAgent,
    run: LifecycleRun,
    input: &SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let project_agent_id = agent.project_agent_id.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!(
            "LifecycleAgent {} 缺少 project_agent_id",
            agent.id
        ))
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
    let agent_context =
        RuntimeContextInspectionPlanner::build_project_agent_context(&svc.repos, &project_agent)
            .await
            .map_err(connector_internal)?;
    let workspace =
        RuntimeContextInspectionPlanner::resolve_project_workspace(&svc.repos, &project)
            .await
            .map_err(connector_internal)?;
    let executor_config = merge_user_executor_config(
        input.command.user_input().executor_config.clone(),
        &agent_context.executor_config,
    );
    let lifecycle = owner_prompt_lifecycle(svc.prompt_lifecycle(Some(&executor_config), input));
    let user_prompt_blocks = required_prompt_blocks(input.command.user_input())?;
    let builder = frame_builder_from_existing(
        frame,
        input.session_id.as_str(),
        input.session_id.as_str(),
    )?;
    let (builder, extras) = svc
        .assembler()
        .compose_owner_bootstrap_to_frame(
            builder,
            OwnerBootstrapSpec {
                owner: OwnerScope::Project {
                    project: &project,
                    workspace: workspace.as_ref(),
                    agent_id: Some(project_agent.id),
                    agent_display_name: agent_context.display_name.clone(),
                    preset_name: agent_context.preset_name.clone(),
                },
                executor_config,
                user_prompt_blocks,
                agent_mcp: AgentLevelMcp {
                    preset_mcp_servers: agent_context.preset_mcp_servers.clone(),
                },
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
                request_mcp_servers: input.command.local_relay_mcp_declarations().to_vec(),
                existing_vfs: frame.typed_vfs(),
                visible_canvas_mount_ids: frame.visible_canvas_mount_ids(),
                active_workflow: None,
                lifecycle,
                audit_session_key: Some(input.session_id.clone()),
                caller_agent_id: Some(project_agent.id),
            },
        )
        .await
        .map_err(ConnectorError::InvalidConfig)?;

    svc.persist_composed_frame(
        builder,
        &mut agent,
        extras,
        &input.command,
        input.session_id.as_str(),
        None,
    )
    .await
}
