//! Story compose 路径 — story + workspace + project agent context bootstrap。

use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::ConnectorError;

use crate::session::construction_planner::{build_project_agent_context, resolve_project_workspace};
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
    story_id: uuid::Uuid,
    input: &SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let story = load_story_for_run(svc, story_id, &run).await?;
    let project = load_project_for_story(svc, &story).await?;
    let workspace = resolve_story_owner_workspace(svc, &story, &project).await?;
    let project_agent = resolve_story_project_agent(svc, &agent, project.id).await?;
    let agent_context =
        build_project_agent_context(&svc.repos, &project_agent)
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
    agent.project_agent_id = Some(project_agent.id);

    let (builder, extras) = svc
        .assembler()
        .compose_owner_bootstrap_to_frame(
            builder,
            OwnerBootstrapSpec {
                owner: OwnerScope::Story {
                    story: &story,
                    project: &project,
                    workspace: workspace.as_ref(),
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

// ─── Story-specific helpers ───

async fn load_story_for_run(
    svc: &FrameConstructionService,
    story_id: uuid::Uuid,
    run: &LifecycleRun,
) -> Result<Story, ConnectorError> {
    let story = svc
        .repos
        .story_repo
        .get_by_id(story_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| ConnectorError::InvalidConfig(format!("Story {story_id} 不存在")))?;
    if story.project_id != run.project_id {
        return Err(ConnectorError::InvalidConfig(format!(
            "Story {story_id} 不属于 LifecycleRun {} 的 Project {}",
            run.id, run.project_id
        )));
    }
    Ok(story)
}

async fn load_project_for_story(
    svc: &FrameConstructionService,
    story: &Story,
) -> Result<Project, ConnectorError> {
    svc.repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("Project {} 不存在", story.project_id))
        })
}

async fn resolve_story_owner_workspace(
    svc: &FrameConstructionService,
    story: &Story,
    project: &Project,
) -> Result<Option<Workspace>, ConnectorError> {
    if let Some(workspace_id) = story.default_workspace_id {
        return svc
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "Story 默认 Workspace {workspace_id} 不存在"
                ))
            })
            .map(Some);
    }

    resolve_project_workspace(&svc.repos, project)
        .await
        .map_err(connector_internal)
}

async fn resolve_story_project_agent(
    svc: &FrameConstructionService,
    agent: &LifecycleAgent,
    project_id: uuid::Uuid,
) -> Result<ProjectAgent, ConnectorError> {
    if let Some(project_agent_id) = agent.project_agent_id {
        return svc
            .repos
            .project_agent_repo
            .get_by_project_and_id(project_id, project_agent_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("ProjectAgent {project_agent_id} 不存在"))
            });
    }

    crate::story::resolve_story_root_project_agent(&svc.repos, project_id)
        .await
        .map_err(connector_internal)
}
