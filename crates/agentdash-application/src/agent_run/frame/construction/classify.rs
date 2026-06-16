//! Compose 路径分类 — 决定 frame construction 走哪个 composer。
//!
//! 分类优先级：companion_hint → project_agent → lifecycle node → existing frame surface。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;
use crate::session::construction_provider::SessionConstructionProviderInput;

use super::{
    FrameConstructionService, build_envelope_from_frame, connector_internal, frame_surface_ready,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeRoute {
    ProjectAgent,
    LifecycleNode,
    ExistingSurface,
}

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

    let has_orchestration = if agent.project_agent_id.is_some() {
        false
    } else {
        has_orchestration_anchor(svc, input.session_id.as_str()).await?
    };
    match classify_primary_route(&agent, has_orchestration, frame_surface_ready(&frame)) {
        Some(ComposeRoute::ProjectAgent) => {
            return super::composer_project_agent::compose(svc, &frame, agent, run, &input).await;
        }
        Some(ComposeRoute::LifecycleNode) => {
            return super::composer_lifecycle_node::compose(svc, &frame, agent, run, &input).await;
        }
        Some(ComposeRoute::ExistingSurface) => {
            return build_envelope_from_frame(
                &frame,
                None,
                &input.command,
                None,
                &input.session_id,
                &input.requested_runtime_commands,
            );
        }
        None => {}
    }

    Err(ConnectorError::InvalidConfig(format!(
        "AgentFrame {} 缺少 launch surface，且无法从 lifecycle anchor 推导 compose 路径",
        frame.id
    )))
}

fn classify_primary_route(
    agent: &LifecycleAgent,
    has_orchestration_anchor: bool,
    has_frame_surface: bool,
) -> Option<ComposeRoute> {
    // ProjectAgent owner surface 优先于 orchestration anchor；active workflow 会在
    // ProjectAgent composer 内解析并挂载 lifecycle surface。
    if agent.project_agent_id.is_some() {
        return Some(ComposeRoute::ProjectAgent);
    }

    if has_orchestration_anchor {
        return Some(ComposeRoute::LifecycleNode);
    }

    if has_frame_surface {
        return Some(ComposeRoute::ExistingSurface);
    }

    None
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{ComposeRoute, classify_primary_route};
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent};

    #[test]
    fn project_agent_identity_wins_over_orchestration_anchor() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent)
            .with_project_agent(project_agent_id);

        assert_eq!(
            classify_primary_route(&agent, true, false),
            Some(ComposeRoute::ProjectAgent)
        );
    }

    #[test]
    fn lifecycle_node_routes_by_orchestration_anchor_without_project_agent_identity() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::WorkflowAgent);

        assert_eq!(
            classify_primary_route(&agent, true, false),
            Some(ComposeRoute::LifecycleNode)
        );
    }
}
