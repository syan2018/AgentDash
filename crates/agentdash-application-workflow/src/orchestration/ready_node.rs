use agentdash_domain::workflow::{
    LifecycleRun, OrchestrationInstance, OrchestrationStatus, PlanNode, RuntimeNodeState,
    RuntimeNodeStatus, StateExchangeSnapshot,
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::WorkflowApplicationError;

const ORCHESTRATION_NODE_COORDINATE_CONTRACT: &str = "orchestration_node_coordinate.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeNodeCoordinate {
    pub(super) run_id: Uuid,
    pub(super) orchestration_id: Uuid,
    pub(super) node_path: String,
    pub(super) attempt: u32,
}

impl RuntimeNodeCoordinate {
    pub(super) fn new(
        run_id: Uuid,
        orchestration_id: Uuid,
        node_path: impl Into<String>,
        attempt: u32,
    ) -> Self {
        Self {
            run_id,
            orchestration_id,
            node_path: node_path.into(),
            attempt,
        }
    }

    pub(super) fn detail(&self) -> Value {
        json!({
            "contract": ORCHESTRATION_NODE_COORDINATE_CONTRACT,
            "run_id": self.run_id,
            "orchestration_id": self.orchestration_id,
            "node_path": self.node_path,
            "attempt": self.attempt,
        })
    }

    pub(super) fn detail_with<I, K>(&self, fields: I) -> Value
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let mut detail = match self.detail() {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        for (key, value) in fields {
            detail.insert(key.into(), value);
        }
        Value::Object(detail)
    }
}

pub(super) struct ReadyNodeView<'a> {
    pub(super) coordinate: RuntimeNodeCoordinate,
    pub(super) plan_node: &'a PlanNode,
}

impl<'a> ReadyNodeView<'a> {
    pub(super) fn next(run: &'a LifecycleRun) -> Option<Self> {
        for orchestration in &run.orchestrations {
            if matches!(
                orchestration.status,
                OrchestrationStatus::Completed
                    | OrchestrationStatus::Failed
                    | OrchestrationStatus::Cancelled
            ) {
                continue;
            }
            for ready_node_id in &orchestration.dispatch.ready_node_ids {
                if let Some(view) = Self::from_ready_node_id(run, orchestration, ready_node_id) {
                    return Some(view);
                }
            }
        }
        None
    }

    pub(super) fn for_coordinate(
        run: &'a LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<Self, WorkflowApplicationError> {
        let (orchestration, runtime_node) = runtime_node_for_coordinate(run, coordinate)?;
        if runtime_node.status != RuntimeNodeStatus::Ready {
            return Err(WorkflowApplicationError::Conflict(format!(
                "runtime node {} 当前不是 Ready",
                runtime_node.node_path
            )));
        }
        let plan_node = plan_node_for_runtime(orchestration, runtime_node)?;
        Ok(Self {
            coordinate: coordinate.clone(),
            plan_node,
        })
    }

    fn from_ready_node_id(
        run: &'a LifecycleRun,
        orchestration: &'a OrchestrationInstance,
        node_id: &str,
    ) -> Option<Self> {
        let runtime_node = find_runtime_node_by_id(&orchestration.node_tree, node_id)?;
        if runtime_node.status != RuntimeNodeStatus::Ready {
            return None;
        }
        let plan_node = orchestration
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == runtime_node.node_id)?;
        Some(Self {
            coordinate: RuntimeNodeCoordinate::new(
                run.id,
                orchestration.orchestration_id,
                runtime_node.node_path.clone(),
                runtime_node.attempt,
            ),
            plan_node,
        })
    }
}

pub(super) struct RunningNodeView<'a> {
    pub(super) coordinate: RuntimeNodeCoordinate,
    pub(super) plan_node: &'a PlanNode,
    pub(super) runtime_node: &'a RuntimeNodeState,
    pub(super) state_snapshot: &'a StateExchangeSnapshot,
}

impl<'a> RunningNodeView<'a> {
    pub(super) fn for_coordinate(
        run: &'a LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<Self, WorkflowApplicationError> {
        let (orchestration, runtime_node) = runtime_node_for_coordinate(run, coordinate)?;
        if runtime_node.status != RuntimeNodeStatus::Running {
            return Err(WorkflowApplicationError::Conflict(format!(
                "runtime node {} 当前不是 Running",
                runtime_node.node_path
            )));
        }
        let plan_node = plan_node_for_runtime(orchestration, runtime_node)?;
        Ok(Self {
            coordinate: coordinate.clone(),
            plan_node,
            runtime_node,
            state_snapshot: &orchestration.state_snapshot,
        })
    }
}

fn runtime_node_for_coordinate<'a>(
    run: &'a LifecycleRun,
    coordinate: &RuntimeNodeCoordinate,
) -> Result<(&'a OrchestrationInstance, &'a RuntimeNodeState), WorkflowApplicationError> {
    if run.id != coordinate.run_id {
        return Err(WorkflowApplicationError::Internal(format!(
            "runtime coordinate run_id 不匹配: expected={}, actual={}",
            coordinate.run_id, run.id
        )));
    }
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == coordinate.orchestration_id)
        .ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "orchestration 不存在: {}",
                coordinate.orchestration_id
            ))
        })?;
    let runtime_node = find_runtime_node(
        &orchestration.node_tree,
        &coordinate.node_path,
        coordinate.attempt,
    )
    .ok_or_else(|| {
        WorkflowApplicationError::NotFound(format!(
            "runtime node 不存在: {}#{}",
            coordinate.node_path, coordinate.attempt
        ))
    })?;
    Ok((orchestration, runtime_node))
}

fn plan_node_for_runtime<'a>(
    orchestration: &'a OrchestrationInstance,
    runtime_node: &RuntimeNodeState,
) -> Result<&'a PlanNode, WorkflowApplicationError> {
    orchestration
        .plan_snapshot
        .nodes
        .iter()
        .find(|node| node.node_id == runtime_node.node_id)
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "plan node 不存在: {}",
                runtime_node.node_id
            ))
        })
}

fn find_runtime_node_by_id<'a>(
    nodes: &'a [RuntimeNodeState],
    node_id: &str,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_id == node_id {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_by_id(&node.children, node_id) {
            return Some(found);
        }
    }
    None
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
        if let Some(found) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}
