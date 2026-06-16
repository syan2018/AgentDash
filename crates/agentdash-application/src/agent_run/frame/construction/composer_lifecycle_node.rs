//! Lifecycle node compose 路径 — activity activation + lifecycle mount。

use agentdash_domain::workflow::{
    AgentFrame, AgentProcedure, AgentProcedureContract, AgentProcedureExecutionSpec, ExecutorSpec,
    LifecycleAgent, LifecycleRun,
};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::{LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit};
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::lifecycle::projection::{
    activity_definition_from_plan_node, lifecycle_identity_from_orchestration,
};
use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;

use super::{FrameConstructionService, connector_internal, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    _agent: LifecycleAgent,
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
    let lifecycle_identity = lifecycle_identity_from_orchestration(orchestration);
    let activity = activity_definition_from_plan_node(plan_node);
    let loaded_workflow = load_workflow_for_plan_node(svc, run.project_id, plan_node)
        .await
        .map_err(connector_internal)?;
    let snapshot_contract = snapshot_contract_for_plan_node(plan_node);
    let workflow_contract =
        snapshot_contract.or_else(|| loaded_workflow.as_ref().map(|workflow| &workflow.contract));
    let snapshot_label = snapshot_label_for_plan_node(plan_node);
    let workflow_label = loaded_workflow
        .as_ref()
        .map(|workflow| format!("`{}` ({})", workflow.key, workflow.name))
        .or(snapshot_label);
    let inherited_executor_config = command
        .user_input()
        .executor_config
        .clone()
        .or_else(|| frame.typed_execution_profile());
    let base_vfs = frame.typed_vfs();
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
            lifecycle_key: &lifecycle_identity.key,
            activity: &activity,
            workflow_contract,
            base_vfs: base_vfs.as_ref(),
            workflow_label: workflow_label.as_deref(),
            inherited_executor_config,
        },
        Some(svc.audit_bus.clone()),
        Some(input.session_id.as_str()),
    )
    .await
    .map_err(ConnectorError::InvalidConfig)?;

    svc.compose_pending_frame(
        builder,
        extras,
        command,
        input.session_id.as_str(),
        None,
        &input.requested_runtime_commands,
    )
    .await
}

async fn load_workflow_for_plan_node(
    svc: &FrameConstructionService,
    project_id: uuid::Uuid,
    plan_node: &agentdash_domain::workflow::PlanNode,
) -> Result<Option<AgentProcedure>, agentdash_domain::DomainError> {
    let Some(ExecutorSpec::AgentProcedure {
        procedure: AgentProcedureExecutionSpec::ByKey { procedure_key },
        ..
    }) = &plan_node.executor
    else {
        return Ok(None);
    };

    svc.repos
        .agent_procedure_repo
        .get_by_project_and_key(project_id, procedure_key)
        .await
}

fn snapshot_contract_for_plan_node(
    plan_node: &agentdash_domain::workflow::PlanNode,
) -> Option<&AgentProcedureContract> {
    match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure { procedure, .. }) => procedure.snapshot_contract(),
        _ => None,
    }
}

fn snapshot_label_for_plan_node(
    plan_node: &agentdash_domain::workflow::PlanNode,
) -> Option<String> {
    match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure:
                AgentProcedureExecutionSpec::Snapshot {
                    procedure_key,
                    name,
                    ..
                },
            ..
        }) => Some(match (procedure_key.as_deref(), name.as_deref()) {
            (Some(key), Some(name)) => format!("`{key}` ({name})"),
            (Some(key), None) => format!("`{key}`"),
            (None, Some(name)) => name.to_string(),
            (None, None) => "inline workflow".to_string(),
        }),
        _ => None,
    }
}
