use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::shared_library::InstalledAssetSource;

use super::validation::{validate_agent_procedure, validate_workflow_graph};
use super::value_objects::{
    ActivityAttemptStatus, ActivityDefinition, ActivityExecutionClaimStatus,
    ActivityLifecycleRunState, ActivityRunStatus, ActivityTransition, AgentProcedureContract,
    DefinitionSource, EffectiveSessionContract, ExecutorRunRef, LifecycleContext,
    LifecycleExecutionEntry, LifecycleRunStatus, OrchestrationInstance, ValidationIssue,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityExecutionClaim {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub claim_id: Uuid,
    pub executor_kind: String,
    pub status: ActivityExecutionClaimStatus,
    pub idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_run_ref: Option<ExecutorRunRef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ActivityExecutionClaim {
    pub fn new(
        run_id: Uuid,
        graph_instance_id: Uuid,
        activity_key: impl Into<String>,
        attempt: u32,
        executor_kind: impl Into<String>,
    ) -> Self {
        let activity_key = activity_key.into();
        let now = Utc::now();
        Self {
            run_id,
            graph_instance_id,
            activity_key: activity_key.clone(),
            attempt,
            claim_id: Uuid::new_v4(),
            executor_kind: executor_kind.into(),
            status: ActivityExecutionClaimStatus::Claiming,
            idempotency_key: format!("{run_id}:{graph_instance_id}:{activity_key}:{attempt}"),
            executor_run_ref: None,
            created_at: now,
            updated_at: now,
        }
    }
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

/// 结构化的活跃 Activity 引用。替代旧的 `"graph_instance_id:activity_key"` 字符串拼接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveActivityRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub status: ActivityAttemptStatus,
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
    /// 此 run 关联的 root WorkflowGraph ID；graphless run 不拥有 WorkflowGraph。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_graph_id: Option<Uuid>,
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
    pub fn new_control(project_id: Uuid, root_graph_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::WorkflowGraph,
            root_graph_id: Some(root_graph_id),
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
            root_graph_id: None,
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

    pub fn sync_graph_instance_activity_projections<'a, I>(&mut self, states: I)
    where
        I: IntoIterator<Item = (Uuid, &'a ActivityLifecycleRunState)>,
    {
        let states = states.into_iter().collect::<Vec<_>>();
        self.status = aggregate_lifecycle_status(states.iter().map(|(_, state)| state.status));
        let now = Utc::now();
        self.updated_at = now;
        self.last_activity_at = now;
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
}

pub fn active_activity_refs_from_states<'a, I>(run_id: Uuid, states: I) -> Vec<ActiveActivityRef>
where
    I: IntoIterator<Item = (Uuid, &'a ActivityLifecycleRunState)>,
{
    states
        .into_iter()
        .flat_map(|(graph_instance_id, activity_state)| {
            activity_state
                .attempts
                .iter()
                .filter(|attempt| {
                    matches!(
                        attempt.status,
                        ActivityAttemptStatus::Ready
                            | ActivityAttemptStatus::Claiming
                            | ActivityAttemptStatus::Running
                    )
                })
                .map(move |attempt| ActiveActivityRef {
                    run_id,
                    graph_instance_id,
                    activity_key: attempt.activity_key.clone(),
                    attempt: attempt.attempt,
                    status: attempt.status,
                })
        })
        .collect()
}

pub fn has_active_activity_state(activity_state: &ActivityLifecycleRunState) -> bool {
    activity_state.attempts.iter().any(|attempt| {
        matches!(
            attempt.status,
            ActivityAttemptStatus::Ready
                | ActivityAttemptStatus::Claiming
                | ActivityAttemptStatus::Running
        )
    })
}

fn aggregate_lifecycle_status<I>(statuses: I) -> LifecycleRunStatus
where
    I: IntoIterator<Item = ActivityRunStatus>,
{
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.is_empty() {
        return LifecycleRunStatus::Ready;
    }
    if statuses.contains(&ActivityRunStatus::Failed) {
        return LifecycleRunStatus::Failed;
    }
    if statuses.contains(&ActivityRunStatus::Running) {
        return LifecycleRunStatus::Running;
    }
    if statuses.contains(&ActivityRunStatus::Ready) {
        return LifecycleRunStatus::Ready;
    }
    if statuses.contains(&ActivityRunStatus::Blocked) {
        return LifecycleRunStatus::Blocked;
    }
    if statuses
        .iter()
        .all(|status| *status == ActivityRunStatus::Completed)
    {
        return LifecycleRunStatus::Completed;
    }
    if statuses
        .iter()
        .all(|status| *status == ActivityRunStatus::Cancelled)
    {
        return LifecycleRunStatus::Cancelled;
    }
    LifecycleRunStatus::Running
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_activity_key: &str,
    primary_workflow: Option<&AgentProcedure>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_activity_key: Some(active_activity_key.to_string()),
            injection: w.contract.injection.clone(),
            hook_rules: w.contract.hook_rules.clone(),
        },
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_activity_key: Some(active_activity_key.to_string()),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        AgentReusePolicy, BashExecExecutorSpec, ExecutorSpec, FunctionActivityExecutorSpec,
        HumanActivityExecutorSpec, HumanApprovalExecutorSpec, OrchestrationPlanSnapshot,
        OrchestrationSourceRef, OrchestrationStatus, PlanNode, PlanNodeKind, RuntimeSessionPolicy,
        WorkflowContextBinding, WorkflowInjectionSpec,
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

    #[test]
    fn activity_execution_claim_uses_attempt_idempotency_key() {
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let claim = ActivityExecutionClaim::new(run_id, graph_instance_id, "plan", 2, "agent");

        assert_eq!(claim.run_id, run_id);
        assert_eq!(claim.graph_instance_id, graph_instance_id);
        assert_eq!(claim.activity_key, "plan");
        assert_eq!(claim.attempt, 2);
        assert_eq!(claim.executor_kind, "agent");
        assert_eq!(claim.status, ActivityExecutionClaimStatus::Claiming);
        assert_eq!(
            claim.idempotency_key,
            format!("{run_id}:{graph_instance_id}:plan:2")
        );
        assert!(claim.status.is_active());
        assert!(!ActivityExecutionClaimStatus::Succeeded.is_active());
    }

    fn orchestration_instance(role: &str, executor: ExecutorSpec) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
            graph_instance_id: Some(Uuid::new_v4()),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_id: Uuid::new_v4(),
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
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![role.to_string()],
            activation_rules: Vec::new(),
            limits: Default::default(),
            created_at: Utc::now(),
        };
        OrchestrationInstance::new(role, source_ref, plan_snapshot)
    }

    fn agent_executor() -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure_key: "workflow.plan".to_string(),
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
        let control = LifecycleRun::new_control(Uuid::new_v4(), Uuid::new_v4());
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
        let mut run = LifecycleRun::new_control(Uuid::new_v4(), Uuid::new_v4());
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
        let mut run = LifecycleRun::new_control(Uuid::new_v4(), Uuid::new_v4());
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
