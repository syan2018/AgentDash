//! Compose 路径分类 — 决定 frame construction 走哪个 composer。
//!
//! 分类优先级：companion_hint → lifecycle node → project_agent → existing frame surface。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{
    FrameConstructionService, build_envelope_from_frame, connector_internal, frame_surface_ready,
};

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

    // 2. lifecycle node (由 RuntimeSessionExecutionAnchor 的 orchestration binding 决定)
    if has_orchestration_anchor(svc, input.session_id.as_str()).await? {
        return super::composer_lifecycle_node::compose(svc, &frame, agent, run, &input).await;
    }

    // 3. ProjectAgent 入口消费 Story/Task subject context，不再让 subject association 抢走 composer。
    if agent.project_agent_id.is_some() {
        return super::composer_project_agent::compose(svc, &frame, agent, run, &input).await;
    }

    // 4. 尝试直接使用已有 frame surface
    if frame_surface_ready(&frame) {
        return build_envelope_from_frame(&frame, None, &input.command, None, &input.session_id);
    }

    Err(ConnectorError::InvalidConfig(format!(
        "AgentFrame {} 缺少 launch surface，且无法从 lifecycle anchor 推导 compose 路径",
        frame.id
    )))
}

async fn has_orchestration_anchor(
    svc: &FrameConstructionService,
    runtime_session_id: &str,
) -> Result<bool, ConnectorError> {
    let anchor = svc
        .repos
        .execution_anchor_repo
        .find_by_session(runtime_session_id)
        .await
        .map_err(connector_internal)?;
    Ok(
        anchor
            .is_some_and(|anchor| anchor.orchestration_id.is_some() && anchor.node_path.is_some()),
    )
}
