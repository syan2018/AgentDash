use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::agent_run_target::AgentRunTarget;

use super::run_state::ExecutorRunRef;
use super::{
    ActivityCompletionPolicy, ActivityIterationPolicy, ActivityJoinPolicy, AgentProcedureContract,
    AgentReusePolicy, ArtifactAliasPolicy, FunctionActivityExecutorSpec, HumanActivityExecutorSpec,
    InputPortDefinition, OutputPortDefinition, RuntimeThreadPolicy, TransitionCondition,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationInstance {
    pub orchestration_id: Uuid,
    pub role: String,
    pub source_ref: OrchestrationSourceRef,
    pub status: OrchestrationStatus,
    pub plan_snapshot: OrchestrationPlanSnapshot,
    #[serde(default)]
    pub activation: PlanActivation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_tree: Vec<RuntimeNodeState>,
    #[serde(default)]
    pub dispatch: DispatchState,
    #[serde(default)]
    pub state_snapshot: StateExchangeSnapshot,
    #[serde(default)]
    pub journal_cursor: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OrchestrationInstance {
    pub fn new(
        role: impl Into<String>,
        source_ref: OrchestrationSourceRef,
        plan_snapshot: OrchestrationPlanSnapshot,
    ) -> Self {
        let now = Utc::now();
        Self {
            orchestration_id: Uuid::new_v4(),
            role: role.into(),
            source_ref,
            status: OrchestrationStatus::Pending,
            plan_snapshot,
            activation: PlanActivation::default(),
            node_tree: Vec::new(),
            dispatch: DispatchState::default(),
            state_snapshot: StateExchangeSnapshot::default(),
            journal_cursor: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrchestrationSourceRef {
    WorkflowGraph {
        graph_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph_version: Option<i32>,
    },
    RunScriptArtifact {
        artifact_id: Uuid,
        revision: i32,
        source_digest: String,
    },
    WorkflowScript {
        script_id: Uuid,
        version: i32,
    },
    Inline {
        source_digest: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationPlanSnapshot {
    pub plan_digest: String,
    pub plan_version: u32,
    pub source_ref: OrchestrationSourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<PlanNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activation_rules: Vec<ActivationRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_exchange_rules: Vec<StateExchangeRule>,
    #[serde(default)]
    pub limits: OrchestrationLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanNode {
    pub node_id: String,
    pub node_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_node_id: Option<String>,
    pub kind: PlanNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<ExecutorSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_policy: Option<ActivityCompletionPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration_policy: Option<ActivityIterationPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_policy: Option<ActivityJoinPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_contract: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanNodeKind {
    Activity,
    AgentCall,
    Function,
    LocalEffect,
    ExtensionAction,
    HumanGate,
    Phase,
    ParallelGroup,
    Pipeline,
    Barrier,
    Subworkflow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentProcedureExecutionSpec {
    ByKey {
        procedure_key: String,
    },
    Snapshot {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        procedure_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        contract: Box<AgentProcedureContract>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_ref: Option<OrchestrationSourceRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contract_digest: Option<String>,
    },
}

impl AgentProcedureExecutionSpec {
    pub fn by_key(procedure_key: impl Into<String>) -> Self {
        Self::ByKey {
            procedure_key: procedure_key.into(),
        }
    }

    pub fn procedure_key(&self) -> Option<&str> {
        match self {
            Self::ByKey { procedure_key } => Some(procedure_key.as_str()),
            Self::Snapshot { procedure_key, .. } => procedure_key.as_deref(),
        }
    }

    pub fn snapshot_contract(&self) -> Option<&AgentProcedureContract> {
        match self {
            Self::Snapshot { contract, .. } => Some(contract.as_ref()),
            Self::ByKey { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutorSpec {
    AgentProcedure {
        procedure: AgentProcedureExecutionSpec,
        #[serde(default)]
        agent_reuse_policy: AgentReusePolicy,
        #[serde(default)]
        runtime_thread_policy: RuntimeThreadPolicy,
    },
    Function {
        spec: FunctionActivityExecutorSpec,
    },
    Human {
        spec: HumanActivityExecutorSpec,
    },
    LocalEffect {
        capability_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
    },
    ExtensionAction {
        extension_key: String,
        action_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivationRule {
    Entry {
        node_id: String,
    },
    Transition {
        rule_id: String,
        from_node_id: String,
        to_node_id: String,
        #[serde(default)]
        condition: TransitionCondition,
        #[serde(default)]
        join_policy: ActivityJoinPolicy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_traversals: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_path: Option<String>,
    },
    Dependency {
        node_id: String,
        depends_on_node_ids: Vec<String>,
    },
    Condition {
        node_id: String,
        expression: Value,
    },
    ArtifactBinding {
        from_node_id: String,
        from_port: String,
        to_node_id: String,
        to_port: String,
    },
    Join {
        node_id: String,
        policy: String,
    },
    Retry {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_attempts: Option<u32>,
    },
    Iteration {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_traversals: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateExchangeRule {
    pub rule_id: String,
    pub from_node_id: String,
    pub from_port: String,
    pub to_node_id: String,
    pub to_port: String,
    #[serde(default)]
    pub alias: ArtifactAliasPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_transition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_agent_runs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_effect_runs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_traversals: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PlanActivation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<OrchestrationLimits>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ready_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeNodeState {
    pub node_id: String,
    pub node_path: String,
    pub kind: PlanNodeKind,
    #[serde(default)]
    pub status: RuntimeNodeStatus,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<NodePortValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<NodePortValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_run_ref: Option<ExecutorRunRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_call: Option<WorkflowAgentCallRuntimeState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<RuntimeNodeState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phase_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RuntimeNodeError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace_refs: Vec<RuntimeTraceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<NodeCacheState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowAgentCallRuntimeState {
    pub request_id: String,
    pub payload_digest: String,
    pub target: AgentRunTarget,
    pub request: Value,
    pub prepared_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_binding: Option<WorkflowAgentCallSourceBindingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowAgentCallSourceBindingRef {
    pub source_ref: String,
    pub committed_at_revision: u64,
    pub applied_surface_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activated_at_revision: Option<u64>,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeNodeStatus {
    #[default]
    Pending,
    Ready,
    Claiming,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodePortValue {
    pub port_key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeNodeError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeTraceRef {
    RuntimeThread {
        thread_id: String,
    },
    AgentRun {
        run_id: Uuid,
        agent_id: Uuid,
    },
    FunctionRun {
        run_id: String,
    },
    HumanDecision {
        decision_id: String,
    },
    EffectInvocation {
        effect_id: String,
        effect_kind: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCacheState {
    pub cache_key: String,
    #[serde(default)]
    pub hit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DispatchState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ready_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leases: Vec<DispatchLeaseSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbox: Vec<DispatchOutboxItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchLeaseSnapshot {
    pub lease_id: String,
    pub node_path: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchOutboxItem {
    pub item_id: String,
    pub node_path: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StateExchangeSnapshot {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_outputs: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<StateArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_refs: Vec<NodeCacheRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateArtifactRef {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCacheRef {
    pub cache_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrchestrationJournalFact {
    PlanActivated {
        orchestration_id: Uuid,
        plan_digest: String,
        timestamp: DateTime<Utc>,
    },
    NodeReady {
        orchestration_id: Uuid,
        node_path: String,
        timestamp: DateTime<Utc>,
    },
    NodeClaimed {
        orchestration_id: Uuid,
        node_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        claim_id: Option<String>,
        timestamp: DateTime<Utc>,
    },
    NodeStarted {
        orchestration_id: Uuid,
        node_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        executor_run_ref: Option<ExecutorRunRef>,
        timestamp: DateTime<Utc>,
    },
    NodeCompleted {
        orchestration_id: Uuid,
        node_path: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        outputs: Vec<NodePortValue>,
        timestamp: DateTime<Utc>,
    },
    NodeFailed {
        orchestration_id: Uuid,
        node_path: String,
        error: RuntimeNodeError,
        timestamp: DateTime<Utc>,
    },
    NodeCancelled {
        orchestration_id: Uuid,
        node_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    HumanGateOpened {
        orchestration_id: Uuid,
        node_path: String,
        gate_id: String,
        timestamp: DateTime<Utc>,
    },
    HumanGateResolved {
        orchestration_id: Uuid,
        node_path: String,
        gate_id: String,
        decision: Value,
        timestamp: DateTime<Utc>,
    },
    SnapshotMaterialized {
        orchestration_id: Uuid,
        journal_cursor: u64,
        timestamp: DateTime<Utc>,
    },
    DispatchLeaseRecorded {
        orchestration_id: Uuid,
        node_path: String,
        lease_id: String,
        timestamp: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::super::{BashExecExecutorSpec, HumanApprovalExecutorSpec};
    use super::*;
    use serde_json::json;

    fn workflow_source() -> OrchestrationSourceRef {
        OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
        }
    }

    fn node(node_id: &str, kind: PlanNodeKind, executor: Option<ExecutorSpec>) -> PlanNode {
        PlanNode {
            node_id: node_id.to_string(),
            node_path: node_id.to_string(),
            parent_node_id: None,
            kind,
            label: Some(node_id.to_string()),
            executor,
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        }
    }

    #[test]
    fn orchestration_plan_snapshot_serializes_tagged_executor_shapes() {
        let source_ref = workflow_source();
        let snapshot = OrchestrationPlanSnapshot {
            plan_digest: "sha256:workflow-plan-v1".to_string(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![
                node(
                    "agent",
                    PlanNodeKind::AgentCall,
                    Some(ExecutorSpec::AgentProcedure {
                        procedure: AgentProcedureExecutionSpec::by_key("workflow.plan"),
                        agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
                        runtime_thread_policy: RuntimeThreadPolicy::CreateNew,
                    }),
                ),
                node(
                    "function",
                    PlanNodeKind::Function,
                    Some(ExecutorSpec::Function {
                        spec: FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
                            command: "pnpm".to_string(),
                            args: vec!["test".to_string()],
                            working_directory: None,
                        }),
                    }),
                ),
                node(
                    "human",
                    PlanNodeKind::HumanGate,
                    Some(ExecutorSpec::Human {
                        spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                            form_schema_key: "approval.plan_review".to_string(),
                            title: None,
                        }),
                    }),
                ),
                node("phase", PlanNodeKind::Phase, None),
                node("barrier", PlanNodeKind::Barrier, None),
            ],
            entry_node_ids: vec!["agent".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "agent".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits {
                max_concurrency: Some(2),
                ..OrchestrationLimits::default()
            },
            metadata: None,
            created_at: Utc::now(),
        };

        let value = serde_json::to_value(&snapshot).expect("serialize plan snapshot");
        assert_eq!(value["plan_digest"], "sha256:workflow-plan-v1");
        assert_eq!(value["source_ref"]["kind"], "workflow_graph");
        assert_eq!(value["nodes"][0]["executor"]["kind"], "agent_procedure");
        assert_eq!(value["nodes"][1]["executor"]["kind"], "function");
        assert_eq!(value["nodes"][1]["executor"]["spec"]["type"], "bash_exec");
        assert_eq!(value["nodes"][2]["executor"]["kind"], "human");

        let restored: OrchestrationPlanSnapshot =
            serde_json::from_value(value).expect("deserialize plan snapshot");
        assert_eq!(restored, snapshot);
        assert_eq!(restored.source_ref, source_ref);
    }

    #[test]
    fn orchestration_runtime_nodes_roundtrip_typed_executor_refs() {
        let nodes = vec![
            RuntimeNodeState {
                node_id: "agent".to_string(),
                node_path: "agent".to_string(),
                kind: PlanNodeKind::AgentCall,
                status: RuntimeNodeStatus::Running,
                attempt: 1,
                inputs: Vec::new(),
                outputs: Vec::new(),
                executor_run_ref: Some(ExecutorRunRef::AgentRun {
                    run_id: Uuid::nil(),
                    agent_id: Uuid::from_u128(1),
                }),
                agent_call: None,
                children: Vec::new(),
                phase_path: vec!["root".to_string()],
                started_at: Some(Utc::now()),
                completed_at: None,
                error: None,
                trace_refs: vec![RuntimeTraceRef::AgentRun {
                    run_id: Uuid::nil(),
                    agent_id: Uuid::from_u128(1),
                }],
                cache: None,
            },
            RuntimeNodeState {
                node_id: "function".to_string(),
                node_path: "function".to_string(),
                kind: PlanNodeKind::Function,
                status: RuntimeNodeStatus::Completed,
                attempt: 1,
                inputs: Vec::new(),
                outputs: vec![NodePortValue {
                    port_key: "result".to_string(),
                    value: json!({"ok": true}),
                }],
                executor_run_ref: Some(ExecutorRunRef::FunctionRun {
                    run_id: "function-run-1".to_string(),
                }),
                agent_call: None,
                children: Vec::new(),
                phase_path: Vec::new(),
                started_at: None,
                completed_at: Some(Utc::now()),
                error: None,
                trace_refs: vec![RuntimeTraceRef::FunctionRun {
                    run_id: "function-run-1".to_string(),
                }],
                cache: Some(NodeCacheState {
                    cache_key: "function:hash".to_string(),
                    hit: false,
                    source_node_path: None,
                    source_run_id: None,
                    digest: Some("sha256:function".to_string()),
                }),
            },
            RuntimeNodeState {
                node_id: "human".to_string(),
                node_path: "human".to_string(),
                kind: PlanNodeKind::HumanGate,
                status: RuntimeNodeStatus::Blocked,
                attempt: 1,
                inputs: Vec::new(),
                outputs: Vec::new(),
                executor_run_ref: Some(ExecutorRunRef::HumanDecision {
                    decision_id: "decision-1".to_string(),
                }),
                agent_call: None,
                children: Vec::new(),
                phase_path: Vec::new(),
                started_at: None,
                completed_at: None,
                error: None,
                trace_refs: vec![RuntimeTraceRef::HumanDecision {
                    decision_id: "decision-1".to_string(),
                }],
                cache: None,
            },
        ];

        let json = serde_json::to_string(&nodes).expect("serialize runtime nodes");
        assert!(json.contains(r#""kind":"runtime_thread""#));
        assert!(json.contains(r#""kind":"function_run""#));
        assert!(json.contains(r#""kind":"human_decision""#));
        let restored: Vec<RuntimeNodeState> =
            serde_json::from_str(&json).expect("deserialize runtime nodes");
        assert_eq!(restored, nodes);
    }

    #[test]
    fn orchestration_journal_facts_roundtrip() {
        let orchestration_id = Uuid::new_v4();
        let facts = vec![
            OrchestrationJournalFact::PlanActivated {
                orchestration_id,
                plan_digest: "sha256:activated-plan".to_string(),
                timestamp: Utc::now(),
            },
            OrchestrationJournalFact::NodeCompleted {
                orchestration_id,
                node_path: "agent".to_string(),
                outputs: vec![NodePortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"text": "done"}),
                }],
                timestamp: Utc::now(),
            },
            OrchestrationJournalFact::HumanGateResolved {
                orchestration_id,
                node_path: "review".to_string(),
                gate_id: "gate-1".to_string(),
                decision: json!({"approved": true}),
                timestamp: Utc::now(),
            },
        ];

        let value = serde_json::to_value(&facts).expect("serialize journal facts");
        assert_eq!(value[0]["kind"], "plan_activated");
        assert_eq!(value[0]["plan_digest"], "sha256:activated-plan");
        assert_eq!(value[1]["kind"], "node_completed");
        assert_eq!(value[2]["kind"], "human_gate_resolved");
        let restored: Vec<OrchestrationJournalFact> =
            serde_json::from_value(value).expect("deserialize journal facts");
        assert_eq!(restored, facts);
    }
}
