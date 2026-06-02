//! Lifecycle node compose 路径 — activity activation + lifecycle mount。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::{LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit};
use crate::workflow::frame_surface::AgentFrameSurfaceExt;
use crate::workflow::runtime_launch::RuntimeLaunchRequest;

use super::{FrameConstructionService, connector_internal, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    mut agent: LifecycleAgent,
    run: LifecycleRun,
    input: &SessionConstructionProviderInput,
) -> Result<RuntimeLaunchRequest, ConnectorError> {
    let command = &input.command;
    let graph_instance_id = frame.graph_instance_id.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("AgentFrame {} 缺少 graph_instance_id", frame.id))
    })?;
    let graph_instance = svc
        .repos
        .workflow_graph_instance_repo
        .get_by_run_and_id(run.id, graph_instance_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "WorkflowGraphInstance {graph_instance_id} 不属于 run {}",
                run.id
            ))
        })?;
    let lifecycle = svc
        .repos
        .workflow_graph_repo
        .get_by_id(graph_instance.graph_id)
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "WorkflowGraph {} 不存在",
                graph_instance.graph_id
            ))
        })?;
    let activity_key = frame.activity_key.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("AgentFrame {} 缺少 activity_key", frame.id))
    })?;
    let activity = lifecycle
        .activities
        .iter()
        .find(|item| item.key == activity_key)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "WorkflowGraph {} 中不存在 activity `{activity_key}`",
                lifecycle.id
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
            graph_instance_id,
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
