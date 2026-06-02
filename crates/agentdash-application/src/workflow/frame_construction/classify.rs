//! Compose 路径分类 — 决定 frame construction 走哪个 composer。
//!
//! 分类优先级：companion_hint → story association → lifecycle node → task → project_agent。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{FrameConstructionService, build_envelope_from_frame, connector_internal, frame_surface_ready};

/// 根据 frame/agent/run 状态分类后路由到对应的 composer。
pub(super) async fn route_and_compose(
    svc: &FrameConstructionService,
    frame: AgentFrame,
    agent: LifecycleAgent,
    run: LifecycleRun,
    input: SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    // 1. companion hint 优先
    if let Some(companion) = input.command.companion_hint() {
        return super::composer_companion::compose(svc, &frame, agent, companion, &input).await;
    }

    // 2. story association
    if let Some(story_id) = resolve_story_association(svc, run.id, agent.id).await? {
        return super::composer_story::compose(svc, &frame, agent, run, story_id, &input).await;
    }

    // 3. lifecycle node (graph_instance + activity_key 同时存在)
    if frame.graph_instance_id.is_some() && frame.activity_key.is_some() {
        return super::composer_lifecycle_node::compose(svc, &frame, agent, run, &input).await;
    }

    // 4. task association
    if input.command.task_hint().is_some()
        || has_task_association(svc, run.id, agent.id).await?
    {
        return super::composer_task::compose(svc, &frame, agent, run, &input).await;
    }

    // 5. project_agent fallback
    if agent.project_agent_id.is_some() {
        return super::composer_project_agent::compose(svc, &frame, agent, run, &input).await;
    }

    // 6. 尝试直接使用已有 frame surface
    if frame_surface_ready(&frame) {
        return build_envelope_from_frame(
            &frame,
            None,
            &input.command,
            None,
            &input.session_id,
        );
    }

    Err(ConnectorError::InvalidConfig(format!(
        "AgentFrame {} 缺少 launch surface，且无法从 lifecycle anchor 推导 compose 路径",
        frame.id
    )))
}

async fn has_task_association(
    svc: &FrameConstructionService,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
) -> Result<bool, ConnectorError> {
    let associations = svc
        .repos
        .lifecycle_subject_association_repo
        .list_by_anchor(run_id, Some(agent_id))
        .await
        .map_err(connector_internal)?;
    Ok(associations
        .iter()
        .any(|assoc| assoc.subject_kind == "task"))
}

async fn resolve_story_association(
    svc: &FrameConstructionService,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
) -> Result<Option<uuid::Uuid>, ConnectorError> {
    let associations = svc
        .repos
        .lifecycle_subject_association_repo
        .list_by_anchor(run_id, Some(agent_id))
        .await
        .map_err(connector_internal)?;
    Ok(associations
        .iter()
        .find(|assoc| assoc.subject_kind == "story")
        .map(|assoc| assoc.subject_id))
}
