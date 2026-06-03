use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared_library::InstalledAssetSource;

use super::validation::{validate_agent_procedure, validate_workflow_graph};
use super::value_objects::{
    ActivityDefinition, ActivityExecutionClaimStatus, ActivityLifecycleRunState, ActivityRunStatus,
    ActivityTransition, AgentProcedureContract, DefinitionSource, EffectiveSessionContract,
    ExecutorRunRef, LifecycleExecutionEntry, LifecycleRunStatus, ValidationIssue,
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
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: i32,
    pub status: super::value_objects::ActivityAttemptStatus,
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
    pub status: LifecycleRunStatus,
    /// Read-model-only：从 `WorkflowGraphInstance.activity_state` 派生的活跃 activity 集合。
    /// 业务逻辑应使用 `active_activity_refs()` 而非直接读取此字段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_node_keys: Vec<String>,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    /// 从 `active_node_keys` 解析出结构化 `ActiveActivityRef` 列表。
    pub fn active_activity_refs(&self) -> Vec<ActiveActivityRef> {
        self.active_node_keys
            .iter()
            .filter_map(|key| {
                let (gi_str, activity_key) = key.split_once(':')?;
                let graph_instance_id = Uuid::parse_str(gi_str).ok()?;
                Some(ActiveActivityRef {
                    graph_instance_id,
                    activity_key: activity_key.to_string(),
                    attempt: 0,
                    status: super::value_objects::ActivityAttemptStatus::Running,
                })
            })
            .collect()
    }

    /// 返回是否存在活跃 activity。业务代码应使用此方法代替 `current_activity_key`。
    pub fn has_active_activity(&self) -> bool {
        !self.active_node_keys.is_empty()
    }

    pub fn new_control(project_id: Uuid, root_graph_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::WorkflowGraph,
            root_graph_id: Some(root_graph_id),
            status: LifecycleRunStatus::Ready,
            active_node_keys: Vec::new(),
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
            status: LifecycleRunStatus::Ready,
            active_node_keys: Vec::new(),
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
        self.active_node_keys = states
            .iter()
            .flat_map(|(graph_instance_id, state)| {
                active_activity_keys(state)
                    .into_iter()
                    .map(|activity_key| format!("{graph_instance_id}:{activity_key}"))
                    .collect::<Vec<_>>()
            })
            .collect();
        let now = Utc::now();
        self.updated_at = now;
        self.last_activity_at = now;
    }

    pub fn append_execution_log(&mut self, entries: Vec<LifecycleExecutionEntry>) {
        if entries.is_empty() {
            return;
        }
        self.execution_log.extend(entries);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
    }
}

fn active_activity_keys(activity_state: &ActivityLifecycleRunState) -> Vec<String> {
    activity_state
        .attempts
        .iter()
        .filter(|attempt| {
            matches!(
                attempt.status,
                super::value_objects::ActivityAttemptStatus::Ready
                    | super::value_objects::ActivityAttemptStatus::Claiming
                    | super::value_objects::ActivityAttemptStatus::Running
            )
        })
        .map(|attempt| attempt.activity_key.clone())
        .collect()
}

fn aggregate_lifecycle_status<I>(statuses: I) -> LifecycleRunStatus
where
    I: IntoIterator<Item = ActivityRunStatus>,
{
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.is_empty() {
        return LifecycleRunStatus::Ready;
    }
    if statuses
        .iter()
        .any(|status| *status == ActivityRunStatus::Failed)
    {
        return LifecycleRunStatus::Failed;
    }
    if statuses
        .iter()
        .any(|status| *status == ActivityRunStatus::Running)
    {
        return LifecycleRunStatus::Running;
    }
    if statuses
        .iter()
        .any(|status| *status == ActivityRunStatus::Ready)
    {
        return LifecycleRunStatus::Ready;
    }
    if statuses
        .iter()
        .any(|status| *status == ActivityRunStatus::Blocked)
    {
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
    use crate::workflow::value_objects::{WorkflowContextBinding, WorkflowInjectionSpec};

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
                ..WorkflowInjectionSpec::default()
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
}
