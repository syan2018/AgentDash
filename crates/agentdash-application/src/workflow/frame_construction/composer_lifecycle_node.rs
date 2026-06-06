//! Lifecycle node compose 路径 — activity activation + lifecycle mount。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::{LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit};
use crate::workflow::frame_surface::AgentFrameSurfaceExt;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{FrameConstructionService, connector_internal, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    mut agent: LifecycleAgent,
    run: LifecycleRun,
    input: &SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let command = &input.command;
    let anchor = svc
        .repos
        .execution_anchor_repo
        .find_by_session(input.session_id.as_str())
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "RuntimeSession {} 缺少 orchestration anchor",
                input.session_id
            ))
        })?;
    let orchestration_id = anchor.orchestration_id.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!(
            "RuntimeSession {} anchor 缺少 orchestration_id",
            input.session_id
        ))
    })?;
    let node_path = anchor.node_path.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig(format!(
            "RuntimeSession {} anchor 缺少 node_path",
            input.session_id
        ))
    })?;
    let attempt = anchor.node_attempt.unwrap_or(1);
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "LifecycleRun {} 中不存在 orchestration {}",
                run.id, orchestration_id
            ))
        })?;
    let plan_node = orchestration
        .plan_snapshot
        .nodes
        .iter()
        .find(|item| item.node_path == node_path)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "Orchestration {} 中不存在 node_path `{}`",
                orchestration_id, node_path
            ))
        })?;
    let graph_id = run.root_graph_id.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("LifecycleRun {} 缺少 root_graph_id", run.id))
    })?;
    let lifecycle = svc
        .repos
        .workflow_graph_repo
        .get_by_id(graph_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| ConnectorError::InvalidConfig(format!("WorkflowGraph {graph_id} 不存在")))?;
    let activity = lifecycle
        .activities
        .iter()
        .find(|item| item.key == plan_node.node_id || item.key == plan_node.node_path)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "WorkflowGraph {} 中不存在 orchestration node `{}` 对应的 activity",
                lifecycle.id, plan_node.node_path
            ))
        })?;
    let workflow = match &activity.executor {
        agentdash_domain::workflow::ActivityExecutorSpec::Agent(spec) => svc
            .repos
            .agent_procedure_repo
            .get_by_project_and_key(run.project_id, &spec.procedure_key)
            .await
            .map_err(connector_internal)?,
        _ => None,
    };
    let inherited_executor_config = command
        .user_input()
        .executor_config
        .clone()
        .or_else(|| frame.typed_execution_profile());
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), command.reason_tag())?;
    let (builder, extras) = compose_lifecycle_node_to_frame_with_audit(
        builder,
        &svc.repos,
        svc.platform_config.as_ref(),
        LifecycleNodeSpec {
            run: &run,
            orchestration_id,
            node_path: &node_path,
            attempt,
            lifecycle: &lifecycle,
            activity,
            workflow: workflow.as_ref(),
            inherited_executor_config,
        },
        Some(svc.audit_bus.clone()),
        Some(input.session_id.as_str()),
    )
    .await
    .map_err(ConnectorError::InvalidConfig)?;

    svc.persist_composed_frame(
        builder,
        &mut agent,
        extras,
        command,
        input.session_id.as_str(),
        None,
    )
    .await
}
