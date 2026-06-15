use chrono::{DateTime, Utc};
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleAgent, LifecycleRun, RuntimeNodeState, RuntimeNodeStatus,
    RuntimeSessionExecutionAnchor,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskRuntimeCoordinate {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub trace_session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskRuntimeProjection {
    pub coordinate: TaskRuntimeCoordinate,
    pub node_status: RuntimeNodeStatus,
    pub observed_at: DateTime<Utc>,
}

pub(crate) fn task_runtime_projection_from_anchor(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    frame_id: Uuid,
    anchor: &RuntimeSessionExecutionAnchor,
) -> Option<TaskRuntimeProjection> {
    if anchor.run_id != run.id || anchor.agent_id != agent.id || anchor.launch_frame_id != frame_id
    {
        return None;
    }

    let orchestration_id = anchor.orchestration_id?;
    let node_path = anchor.node_path.as_deref()?;
    let attempt = anchor.node_attempt.unwrap_or(1);
    let orchestration = run.orchestration_by_id(orchestration_id)?;
    let node = find_runtime_node(&orchestration.node_tree, node_path, attempt)?;
    let observed_at = node
        .completed_at
        .or(node.started_at)
        .unwrap_or(anchor.updated_at);

    Some(TaskRuntimeProjection {
        coordinate: TaskRuntimeCoordinate {
            run_id: run.id,
            agent_id: agent.id,
            frame_id,
            orchestration_id,
            node_path: node.node_path.clone(),
            attempt,
            trace_session_id: Some(anchor.runtime_session_id.clone()),
        },
        node_status: node.status,
        observed_at,
    })
}

pub(crate) fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn runtime_node_status_code(status: RuntimeNodeStatus) -> &'static str {
    match status {
        RuntimeNodeStatus::Pending => "pending",
        RuntimeNodeStatus::Ready => "ready",
        RuntimeNodeStatus::Claiming => "claiming",
        RuntimeNodeStatus::Running => "running",
        RuntimeNodeStatus::Blocked => "blocked",
        RuntimeNodeStatus::Completed => "completed",
        RuntimeNodeStatus::Failed => "failed",
        RuntimeNodeStatus::Cancelled => "cancelled",
        RuntimeNodeStatus::Skipped => "skipped",
    }
}
