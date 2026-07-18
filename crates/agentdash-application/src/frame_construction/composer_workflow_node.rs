//! Workflow node compose path — activity activation plus lifecycle mount.

use agentdash_application_ports::lifecycle_surface_projection::{
    activity_definition_from_plan_node, lifecycle_identity_from_orchestration,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentProcedure, AgentProcedureContract, AgentProcedureExecutionSpec, ExecutorSpec,
    LifecycleAgent, LifecycleRun,
};
use agentdash_platform_spi::PlatformRuntimeError;

use crate::agent_run::frame::AgentFrameSurfaceExt;
use crate::agent_run::frame::FrameLaunchEnvelope;
use crate::agent_run::frame::FrameLaunchEnvelopeConstructionInput;

use super::{
    FrameConstructionService, LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit,
    connector_internal, frame_builder_from_existing,
};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    agent: LifecycleAgent,
    run: LifecycleRun,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
    let command = &input.command;
    let association = agentdash_application_lifecycle::resolve_activity_runtime_association_from_message_stream_trace(
        input.session_id.as_str(),
        svc.repos.agent_frame_repo.as_ref(),
        svc.repos.lifecycle_agent_repo.as_ref(),
        svc.repos.lifecycle_run_repo.as_ref(),
        Some(svc.repos.agent_run_runtime_binding_repo.as_ref()),
    )
        .await
        .map_err(connector_internal)?
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(format!(
                "RuntimeSession {} 缺少 lifecycle runtime association",
                input.session_id
            ))
        })?;
    let orchestration_id = association.orchestration_id;
    let node_path = association.node_path;
    let attempt = association.attempt;
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(format!(
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
            PlatformRuntimeError::InvalidConfig(format!(
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
        .prompt()
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
        svc.lifecycle_surface_projection.as_ref(),
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
        Some(&run.id.to_string()),
        Some(&agent.id.to_string()),
    )
    .await
    .map_err(PlatformRuntimeError::InvalidConfig)?;

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
