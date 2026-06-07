use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::shared_library::InstalledAssetSource;

use super::validation::{validate_agent_procedure, validate_workflow_graph};
use super::value_objects::{
    ActivityDefinition, ActivityTransition, AgentProcedureContract, DefinitionSource,
    EffectiveSessionContract, LifecycleContext, LifecycleExecutionEntry, LifecycleRunStatus,
    OrchestrationInstance, OrchestrationStatus, RuntimeNodeState, RuntimeNodeStatus,
    ValidationIssue,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProcedure {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub version: i32,
    pub contract: AgentProcedureContract,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentProcedure {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        source: DefinitionSource,
        contract: AgentProcedureContract,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        validate_agent_procedure(&key, &name, &contract)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            source,
            installed_source: None,
            version: 1,
            contract,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_agent_procedure(&self.key, &self.name, &self.contract) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "agent_procedure_invalid",
                error,
                "contract",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub version: i32,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<ActivityTransition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowGraph {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        source: DefinitionSource,
        entry_activity_key: impl Into<String>,
        activities: Vec<ActivityDefinition>,
        transitions: Vec<ActivityTransition>,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        let entry_activity_key = entry_activity_key.into();
        validate_workflow_graph(&key, &name, &entry_activity_key, &activities, &transitions)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            source,
            installed_source: None,
            version: 1,
            entry_activity_key,
            activities,
            transitions,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_workflow_graph(
            &self.key,
            &self.name,
            &self.entry_activity_key,
            &self.activities,
            &self.transitions,
        ) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "workflow_graph_invalid",
                error,
                "activities",
            )],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunTopology {
    Graphless,
    WorkflowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub topology: LifecycleRunTopology,
    #[serde(default)]
    pub context: LifecycleContext,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orchestrations: Vec<OrchestrationInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_projection: Option<Value>,
    pub status: LifecycleRunStatus,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    pub fn new_control(project_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::WorkflowGraph,
            context: LifecycleContext::default(),
            orchestrations: Vec::new(),
            view_projection: None,
            status: LifecycleRunStatus::Ready,
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    pub fn new_graphless(project_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::Graphless,
            context: LifecycleContext::default(),
            orchestrations: Vec::new(),
            view_projection: None,
            status: LifecycleRunStatus::Ready,
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    pub fn set_lifecycle_context(&mut self, context: LifecycleContext) {
        self.context = context;
        self.touch_activity();
    }

    pub fn add_orchestration(&mut self, orchestration: OrchestrationInstance) -> bool {
        let orchestration_id = orchestration.orchestration_id;
        if self.orchestration_by_id(orchestration_id).is_some() {
            return false;
        }
        self.orchestrations.push(orchestration);
        self.refresh_status_from_orchestrations();
        self.touch_activity();
        true
    }

    pub fn replace_orchestration(
        &mut self,
        orchestration: OrchestrationInstance,
    ) -> Option<OrchestrationInstance> {
        let orchestration_id = orchestration.orchestration_id;
        let position = self
            .orchestrations
            .iter()
            .position(|existing| existing.orchestration_id == orchestration_id)?;
        let previous = std::mem::replace(&mut self.orchestrations[position], orchestration);
        self.refresh_status_from_orchestrations();
        self.touch_activity();
        Some(previous)
    }

    pub fn orchestration_by_id(&self, orchestration_id: Uuid) -> Option<&OrchestrationInstance> {
        self.orchestrations
            .iter()
            .find(|orchestration| orchestration.orchestration_id == orchestration_id)
    }

    pub fn append_execution_log(&mut self, entries: Vec<LifecycleExecutionEntry>) {
        if entries.is_empty() {
            return;
        }
        self.execution_log.extend(entries);
        self.touch_activity();
    }

    fn touch_activity(&mut self) {
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
    }

    fn refresh_status_from_orchestrations(&mut self) {
        self.status = aggregate_orchestration_status(&self.orchestrations);
    }
}

fn aggregate_orchestration_status(orchestrations: &[OrchestrationInstance]) -> LifecycleRunStatus {
    if orchestrations.is_empty() {
        return LifecycleRunStatus::Ready;
    }
    if orchestrations
        .iter()
        .any(|instance| instance.status == OrchestrationStatus::Failed)
    {
        return LifecycleRunStatus::Failed;
    }
    if orchestrations
        .iter()
        .all(|instance| instance.status == OrchestrationStatus::Cancelled)
    {
        return LifecycleRunStatus::Cancelled;
    }
    if orchestrations
        .iter()
        .all(|instance| instance.status == OrchestrationStatus::Completed)
    {
        return LifecycleRunStatus::Completed;
    }
    if orchestration_nodes(orchestrations)
        .iter()
        .any(|node| node.status == RuntimeNodeStatus::Blocked)
    {
        return LifecycleRunStatus::Blocked;
    }
    if orchestrations.iter().any(|instance| {
        matches!(
            instance.status,
            OrchestrationStatus::Running | OrchestrationStatus::Paused
        ) || orchestration_nodes(std::slice::from_ref(instance))
            .iter()
            .any(|node| {
                matches!(
                    node.status,
                    RuntimeNodeStatus::Claiming | RuntimeNodeStatus::Running
                )
            })
    }) {
        return LifecycleRunStatus::Running;
    }
    LifecycleRunStatus::Ready
}

fn orchestration_nodes(orchestrations: &[OrchestrationInstance]) -> Vec<&RuntimeNodeState> {
    fn collect<'a>(node: &'a RuntimeNodeState, nodes: &mut Vec<&'a RuntimeNodeState>) {
        nodes.push(node);
        for child in &node.children {
            collect(child, nodes);
        }
    }

    let mut nodes = Vec::new();
    for instance in orchestrations {
        for node in &instance.node_tree {
            collect(node, &mut nodes);
        }
    }
    nodes
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_activity_key: &str,
    primary_workflow: Option<&AgentProcedure>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => {
            build_effective_contract_from_contract(lifecycle_key, active_activity_key, &w.contract)
        }
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_activity_key: Some(active_activity_key.to_string()),
            ..Default::default()
        },
    }
}

pub fn build_effective_contract_from_contract(
    lifecycle_key: &str,
    active_activity_key: &str,
    contract: &AgentProcedureContract,
) -> EffectiveSessionContract {
    EffectiveSessionContract {
        lifecycle_key: Some(lifecycle_key.to_string()),
        active_activity_key: Some(active_activity_key.to_string()),
        injection: contract.injection.clone(),
        hook_rules: contract.hook_rules.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        AgentProcedureExecutionSpec, AgentReusePolicy, BashExecExecutorSpec, ExecutorSpec,
        FunctionActivityExecutorSpec, HumanActivityExecutorSpec, HumanApprovalExecutorSpec,
        OrchestrationPlanSnapshot, OrchestrationSourceRef, OrchestrationStatus, PlanNode,
        PlanNodeKind, RuntimeSessionPolicy, WorkflowContextBinding, WorkflowInjectionSpec,
    };

    fn contract() -> AgentProcedureContract {
        AgentProcedureContract {
            injection: WorkflowInjectionSpec {
                guidance: Some("follow the workflow".to_string()),
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
            },
            ..AgentProcedureContract::default()
        }
    }

    #[test]
    fn effective_contract_matches_primary_workflow() {
        let primary = AgentProcedure::new(
            Uuid::new_v4(),
            "wf_primary",
            "Primary",
            "desc",
            DefinitionSource::BuiltinSeed,
            contract(),
        )
        .expect("primary");

        let effective = build_effective_contract("lc", "step", Some(&primary));
        assert_eq!(effective.hook_rules.len(), 0);
    }

    fn orchestration_instance(role: &str, executor: ExecutorSpec) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: format!("sha256:{role}"),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: role.to_string(),
                node_path: role.to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::Activity,
                label: None,
                executor: Some(executor),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![role.to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        OrchestrationInstance::new(role, source_ref, plan_snapshot)
    }

    fn agent_executor() -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure: AgentProcedureExecutionSpec::by_key("workflow.plan"),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn function_executor() -> ExecutorSpec {
        ExecutorSpec::Function {
            spec: FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: None,
            }),
        }
    }

    fn human_executor() -> ExecutorSpec {
        ExecutorSpec::Human {
            spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: None,
            }),
        }
    }

    #[test]
    fn lifecycle_run_orchestration_contract_defaults_empty() {
        let control = LifecycleRun::new_control(Uuid::new_v4());
        assert_eq!(control.context, LifecycleContext::default());
        assert!(control.orchestrations.is_empty());
        assert!(control.view_projection.is_none());

        let graphless = LifecycleRun::new_graphless(Uuid::new_v4());
        assert_eq!(graphless.context, LifecycleContext::default());
        assert!(graphless.orchestrations.is_empty());
        assert!(graphless.view_projection.is_none());
    }

    #[test]
    fn lifecycle_run_orchestration_aggregate_adds_replaces_and_finds_one_instance() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let context = LifecycleContext {
            main_agent_run_id: Some(Uuid::new_v4()),
            ..LifecycleContext::default()
        };
        run.set_lifecycle_context(context.clone());
        assert_eq!(run.context, context);

        let orchestration = orchestration_instance("root", agent_executor());
        let orchestration_id = orchestration.orchestration_id;
        assert!(run.add_orchestration(orchestration.clone()));
        assert!(!run.add_orchestration(orchestration));
        assert_eq!(
            run.orchestration_by_id(orchestration_id)
                .expect("orchestration")
                .role,
            "root"
        );

        let mut replacement = orchestration_instance("root_replacement", function_executor());
        replacement.orchestration_id = orchestration_id;
        replacement.status = OrchestrationStatus::Running;
        let previous = run
            .replace_orchestration(replacement)
            .expect("previous orchestration");
        assert_eq!(previous.role, "root");
        assert_eq!(
            run.orchestration_by_id(orchestration_id)
                .expect("orchestration")
                .status,
            OrchestrationStatus::Running
        );
    }

    #[test]
    fn lifecycle_run_orchestration_aggregate_keeps_multiple_executor_instances() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        assert!(run.add_orchestration(orchestration_instance("agent", agent_executor())));
        assert!(run.add_orchestration(orchestration_instance("function", function_executor())));
        assert!(run.add_orchestration(orchestration_instance("human", human_executor())));

        assert_eq!(run.orchestrations.len(), 3);
        assert!(matches!(
            run.orchestrations[0].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("agent executor"),
            ExecutorSpec::AgentProcedure { .. }
        ));
        assert!(matches!(
            run.orchestrations[1].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("function executor"),
            ExecutorSpec::Function { .. }
        ));
        assert!(matches!(
            run.orchestrations[2].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("human executor"),
            ExecutorSpec::Human { .. }
        ));
    }
}
