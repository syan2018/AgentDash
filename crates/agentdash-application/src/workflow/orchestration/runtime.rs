use std::collections::BTreeSet;

use agentdash_domain::workflow::{
    DispatchState, OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef,
    OrchestrationStatus, RuntimeNodeState, RuntimeNodeStatus, StateExchangeSnapshot,
};

pub const ROOT_ORCHESTRATION_ROLE: &str = "root";

pub fn activate_root_orchestration(
    source_ref: OrchestrationSourceRef,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> OrchestrationInstance {
    activate_orchestration(ROOT_ORCHESTRATION_ROLE, source_ref, plan_snapshot)
}

pub fn activate_orchestration(
    role: impl Into<String>,
    source_ref: OrchestrationSourceRef,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> OrchestrationInstance {
    let entry_node_ids = plan_snapshot.entry_node_ids.clone();
    let mut instance = OrchestrationInstance::new(role, source_ref, plan_snapshot);
    materialize_plan_activation(&mut instance, entry_node_ids);
    instance
}

pub fn materialize_plan_activation(
    instance: &mut OrchestrationInstance,
    ready_node_ids: Vec<String>,
) {
    let ready_set = ready_node_ids.iter().cloned().collect::<BTreeSet<_>>();

    instance.activation.ready_node_ids = ready_node_ids.clone();
    instance.dispatch = DispatchState {
        ready_node_ids,
        ..DispatchState::default()
    };
    instance.node_tree = instance
        .plan_snapshot
        .nodes
        .iter()
        .map(|node| RuntimeNodeState {
            node_id: node.node_id.clone(),
            node_path: node.node_path.clone(),
            kind: node.kind,
            status: if ready_set.contains(node.node_id.as_str()) {
                RuntimeNodeStatus::Ready
            } else {
                RuntimeNodeStatus::Pending
            },
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        })
        .collect();
    instance.state_snapshot = StateExchangeSnapshot::default();
    instance.status = if instance.activation.ready_node_ids.is_empty() {
        OrchestrationStatus::Pending
    } else {
        OrchestrationStatus::Running
    };
    instance.updated_at = chrono::Utc::now();
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::{
        ActivationRule, ExecutorSpec, OrchestrationLimits, OrchestrationSourceRef, PlanNode,
        PlanNodeKind, RuntimeNodeStatus,
    };
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn plan_node(node_id: &str, kind: PlanNodeKind) -> PlanNode {
        PlanNode {
            node_id: node_id.to_string(),
            node_path: node_id.to_string(),
            parent_node_id: None,
            kind,
            label: Some(node_id.to_string()),
            executor: None::<ExecutorSpec>,
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        }
    }

    fn plan_snapshot(source_ref: OrchestrationSourceRef) -> OrchestrationPlanSnapshot {
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:activation-fixture".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![
                plan_node("entry", PlanNodeKind::AgentCall),
                plan_node("follow_up", PlanNodeKind::Function),
            ],
            entry_node_ids: vec!["entry".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "entry".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn orchestration_runtime_activation_materializes_entry_ready_nodes() {
        let graph_id = Uuid::new_v4();
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id,
            graph_version: Some(7),
        };

        let orchestration =
            activate_root_orchestration(source_ref.clone(), plan_snapshot(source_ref.clone()));

        assert_eq!(orchestration.role, ROOT_ORCHESTRATION_ROLE);
        assert_eq!(orchestration.source_ref, source_ref);
        assert_eq!(
            orchestration.plan_snapshot.plan_digest,
            "sha256:activation-fixture"
        );
        assert_eq!(orchestration.activation.ready_node_ids, vec!["entry"]);
        assert_eq!(orchestration.dispatch.ready_node_ids, vec!["entry"]);
        assert_eq!(orchestration.status, OrchestrationStatus::Running);
        assert!(orchestration.state_snapshot.variables.is_empty());
        assert!(orchestration.state_snapshot.node_outputs.is_empty());

        let nodes = orchestration
            .node_tree
            .iter()
            .map(|node| (node.node_id.as_str(), node.status, node.attempt))
            .collect::<Vec<_>>();
        assert_eq!(
            nodes,
            vec![
                ("entry", RuntimeNodeStatus::Ready, 1),
                ("follow_up", RuntimeNodeStatus::Pending, 1),
            ]
        );
    }
}
