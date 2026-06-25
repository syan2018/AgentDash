//! Compose owner route 分类 — 先决定 frame construction 的 owner surface。
//!
//! Companion 不是顶层互斥 route；它只能在 owner route 已经明确后作为 modifier 应用。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::agent_run::frame::FrameLaunchEnvelope;
use crate::agent_run::frame::FrameLaunchEnvelopeConstructionInput;

use super::{
    FrameConstructionService, build_envelope_from_frame, connector_internal, frame_surface_ready,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeRoute {
    ProjectAgent,
    LifecycleNode,
    ExistingSurface,
}

/// 根据 frame/agent/run 状态先解析 owner route，再按 launch modifier 路由到对应 composer。
pub(super) async fn route_and_compose(
    svc: &FrameConstructionService,
    frame: AgentFrame,
    agent: LifecycleAgent,
    run: LifecycleRun,
    input: FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let has_orchestration = if agent.project_agent_id.is_some() {
        false
    } else {
        has_orchestration_anchor(svc, input.session_id.as_str()).await?
    };
    let owner_route = classify_owner_route(&agent, has_orchestration, frame_surface_ready(&frame));
    let companion_modifier = input.command.companion_modifier();

    match (owner_route, companion_modifier) {
        (Some(ComposeRoute::ProjectAgent), Some(companion)) => {
            return super::composer_companion::compose_project_agent_owner_modifier(
                svc, &frame, companion, &input,
            )
            .await;
        }
        (Some(ComposeRoute::ProjectAgent), None) => {
            return super::composer_project_agent::compose(svc, &frame, agent, run, &input).await;
        }
        (Some(ComposeRoute::LifecycleNode), Some(companion)) if companion.workflow.is_some() => {
            return super::composer_companion::compose_lifecycle_node_owner_modifier(
                svc, &frame, companion, &input,
            )
            .await;
        }
        (Some(ComposeRoute::LifecycleNode), Some(_)) => {
            return Err(ConnectorError::InvalidConfig(format!(
                "RuntimeSession {} 的 LifecycleNode owner 收到 companion modifier，但缺少 companion.workflow owner facts",
                input.session_id
            )));
        }
        (Some(ComposeRoute::LifecycleNode), None) => {
            return super::composer_workflow_node::compose(svc, &frame, agent, run, &input).await;
        }
        (Some(ComposeRoute::ExistingSurface), Some(_)) => {
            return Err(ConnectorError::InvalidConfig(format!(
                "RuntimeSession {} 的 ExistingSurface owner 不支持 companion modifier：缺少 ProjectAgent 或 LifecycleNode owner facts",
                input.session_id
            )));
        }
        (Some(ComposeRoute::ExistingSurface), None) => {
            return build_envelope_from_frame(
                &frame,
                None,
                &input.command,
                None,
                &input.session_id,
                &input.requested_runtime_commands,
            );
        }
        (None, Some(_)) => {
            return Err(ConnectorError::InvalidConfig(format!(
                "AgentFrame {} 无法判定 owner route，拒绝仅凭 companion modifier 启动",
                frame.id
            )));
        }
        (None, None) => {}
    }

    Err(ConnectorError::InvalidConfig(format!(
        "AgentFrame {} 缺少 launch surface，且无法从 lifecycle anchor 推导 compose 路径",
        frame.id
    )))
}

fn classify_owner_route(
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

    use super::{ComposeRoute, classify_owner_route};
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent};

    #[test]
    fn project_agent_identity_wins_over_orchestration_anchor() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent)
            .with_project_agent(project_agent_id);

        assert_eq!(
            classify_owner_route(&agent, true, false),
            Some(ComposeRoute::ProjectAgent)
        );
    }

    #[test]
    fn lifecycle_node_routes_by_orchestration_anchor_without_project_agent_identity() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::WorkflowAgent);

        assert_eq!(
            classify_owner_route(&agent, true, false),
            Some(ComposeRoute::LifecycleNode)
        );
    }

    #[test]
    fn existing_surface_routes_only_after_owner_facts_are_absent() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::Unknown);

        assert_eq!(
            classify_owner_route(&agent, false, true),
            Some(ComposeRoute::ExistingSurface)
        );
    }

    #[test]
    fn companion_modifier_does_not_participate_in_owner_classification() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();
        let project_agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent)
            .with_project_agent(project_agent_id);
        let workflow_agent =
            LifecycleAgent::new_root(run_id, project_id, AgentSource::WorkflowAgent);

        assert_eq!(
            classify_owner_route(&project_agent, true, false),
            Some(ComposeRoute::ProjectAgent)
        );
        assert_eq!(
            classify_owner_route(&workflow_agent, true, false),
            Some(ComposeRoute::LifecycleNode)
        );
        assert_eq!(classify_owner_route(&workflow_agent, false, false), None);
    }
}
