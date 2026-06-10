//! Task compose 路径 — task + story step phase bootstrap。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::{StoryStepPhase, StoryStepSpec, TaskLaunchPhase};
use crate::task::gateway::resolve_effective_task_workspace;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{FrameConstructionService, connector_internal, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    mut agent: LifecycleAgent,
    run: LifecycleRun,
    input: &SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let mut associations = svc
        .repos
        .lifecycle_subject_association_repo
        .list_by_anchor(run.id, Some(agent.id))
        .await
        .map_err(connector_internal)?;
    if associations.is_empty() {
        associations = svc
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(connector_internal)?;
    }
    let task_id = associations
        .iter()
        .find(|assoc| assoc.subject_kind == "task")
        .map(|assoc| assoc.subject_id)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "LifecycleRun {} / Agent {} 缺少 task subject association",
                run.id, agent.id
            ))
        })?;
    let story = svc
        .repos
        .story_repo
        .find_by_task_id(task_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| ConnectorError::InvalidConfig(format!("Task {task_id} 不存在")))?;
    let task = story.find_task(task_id).cloned().ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("Story {} 中不存在 Task {task_id}", story.id))
    })?;
    let project = svc
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("Project {} 不存在", story.project_id))
        })?;
    let workspace = resolve_effective_task_workspace(&svc.repos, &task, &story, &project)
        .await
        .map_err(connector_internal)?;
    let task_hint = input.command.task_hint();
    let phase = match task_hint.as_ref().and_then(|hint| hint.phase) {
        Some(TaskLaunchPhase::Start) => StoryStepPhase::Start,
        _ => StoryStepPhase::Continue,
    };
    let explicit_executor_config = input.command.user_input().executor_config.clone();
    let identity = input.command.identity();
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), input.session_id.as_str())?;
    let (builder, extras, hook_binding) = svc
        .assembler()
        .compose_story_step_to_frame(
            builder,
            StoryStepSpec {
                task: &task,
                story: &story,
                project: &project,
                workspace: workspace.as_ref(),
                identity: identity.as_ref(),
                phase,
                override_prompt: task_hint
                    .as_ref()
                    .and_then(|hint| hint.override_prompt.as_deref()),
                additional_prompt: task_hint
                    .as_ref()
                    .and_then(|hint| hint.additional_prompt.as_deref()),
                request_mcp_servers: input.command.local_relay_mcp_declarations(),
                explicit_executor_config,
                strict_config_resolution: true,
                active_workflow: None,
                audit_session_key: Some(input.session_id.clone()),
            },
        )
        .await
        .map_err(|error| ConnectorError::InvalidConfig(error.to_string()))?;

    svc.persist_composed_frame(
        builder,
        &mut agent,
        extras,
        &input.command,
        input.session_id.as_str(),
        hook_binding,
    )
    .await
}
