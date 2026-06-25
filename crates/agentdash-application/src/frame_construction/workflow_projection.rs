use agentdash_application_ports::lifecycle_surface_projection as ports_lifecycle_surface;
use agentdash_domain::workflow::{
    AgentProcedureRepository, ExecutorSpec, LifecycleRun, RuntimeNodeState, RuntimeNodeStatus,
};

use crate::repository_set::RepositorySet;

pub(super) async fn resolve_active_workflow_projection_from_message_stream_trace(
    session_id: &str,
    repos: &RepositorySet,
) -> Result<Option<ports_lifecycle_surface::ActiveWorkflowProjection>, String> {
    let Some(anchor) = repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let Some(orchestration_id) = anchor.orchestration_id else {
        return Ok(None);
    };
    let Some(node_path) = anchor.node_path.as_deref() else {
        return Ok(None);
    };
    let Some(run) = repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    active_workflow_projection_from_runtime_node(
        run,
        orchestration_id,
        node_path,
        anchor.node_attempt.unwrap_or(1),
        repos.agent_procedure_repo.as_ref(),
    )
    .await
}

async fn active_workflow_projection_from_runtime_node(
    run: LifecycleRun,
    orchestration_id: uuid::Uuid,
    node_path: &str,
    attempt: u32,
    definition_repo: &dyn AgentProcedureRepository,
) -> Result<Option<ports_lifecycle_surface::ActiveWorkflowProjection>, String> {
    let Some(orchestration) = run.orchestration_by_id(orchestration_id) else {
        return Ok(None);
    };
    let Some(active_attempt) =
        find_runtime_node(&orchestration.node_tree, node_path, attempt).cloned()
    else {
        return Ok(None);
    };
    let Some(plan_node) = orchestration.plan_snapshot.nodes.iter().find(|node| {
        node.node_path == active_attempt.node_path || node.node_id == active_attempt.node_id
    }) else {
        return Ok(None);
    };
    let lifecycle_identity =
        ports_lifecycle_surface::lifecycle_identity_from_orchestration(orchestration);
    let active_activity = ports_lifecycle_surface::activity_definition_from_plan_node(plan_node);
    let (active_procedure_key, active_node_type) =
        ports_lifecycle_surface::derive_agent_node_facts(plan_node);
    let snapshot_contract = match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure { procedure, .. }) => {
            procedure.snapshot_contract().cloned()
        }
        _ => None,
    };
    let primary_workflow = match active_procedure_key.as_deref() {
        Some(key) if !key.trim().is_empty() && snapshot_contract.is_none() => definition_repo
            .get_by_project_and_key(run.project_id, key)
            .await
            .map_err(|error| format!("查询 AgentProcedure 失败: {error}"))?,
        _ => None,
    };

    Ok(Some(ports_lifecycle_surface::ActiveWorkflowProjection {
        run,
        orchestration_id,
        node_path: node_path.to_string(),
        lifecycle_graph_id: lifecycle_identity.graph_id,
        lifecycle_key: lifecycle_identity.key,
        lifecycle_name: lifecycle_identity.name,
        active_activity,
        active_attempt,
        active_node_type,
        active_procedure_key,
        snapshot_contract,
        primary_workflow,
    }))
}

fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(child) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(child);
        }
    }
    None
}

#[allow(dead_code)]
fn find_first_active_node(nodes: &[RuntimeNodeState]) -> Option<&RuntimeNodeState> {
    for node in nodes {
        if matches!(
            node.status,
            RuntimeNodeStatus::Ready
                | RuntimeNodeStatus::Claiming
                | RuntimeNodeStatus::Running
                | RuntimeNodeStatus::Blocked
        ) {
            return Some(node);
        }
        if let Some(child) = find_first_active_node(&node.children) {
            return Some(child);
        }
    }
    None
}
