use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared_library::InstalledAssetSource;

use super::validation::{
    validate_activity_lifecycle_definition, validate_workflow_definition,
};
use super::value_objects::{
    ActivityDefinition, ActivityExecutionClaimStatus, ActivityLifecycleRunState, ActivityRunStatus,
    ActivityTransition, EffectiveSessionContract, ExecutorRunRef, LifecycleExecutionEntry,
    LifecycleRunStatus, ValidationIssue, WorkflowBindingKind, WorkflowContract,
    WorkflowDefinitionSource, normalize_workflow_binding_kinds,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub source: WorkflowDefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub version: i32,
    pub contract: WorkflowContract,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kinds: Vec<WorkflowBindingKind>,
        source: WorkflowDefinitionSource,
        contract: WorkflowContract,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        let binding_kinds = normalize_workflow_binding_kinds(binding_kinds)?;
        validate_workflow_definition(&key, &name, &contract)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            binding_kinds,
            source,
            installed_source: None,
            version: 1,
            contract,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_workflow_definition(&self.key, &self.name, &self.contract) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "workflow_definition_invalid",
                error,
                "contract",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLifecycleDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub source: WorkflowDefinitionSource,
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
        activity_key: impl Into<String>,
        attempt: u32,
        executor_kind: impl Into<String>,
    ) -> Self {
        let activity_key = activity_key.into();
        let now = Utc::now();
        Self {
            run_id,
            activity_key: activity_key.clone(),
            attempt,
            claim_id: Uuid::new_v4(),
            executor_kind: executor_kind.into(),
            status: ActivityExecutionClaimStatus::Claiming,
            idempotency_key: format!("{run_id}:{activity_key}:{attempt}"),
            executor_run_ref: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl ActivityLifecycleDefinition {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kinds: Vec<WorkflowBindingKind>,
        source: WorkflowDefinitionSource,
        entry_activity_key: impl Into<String>,
        activities: Vec<ActivityDefinition>,
        transitions: Vec<ActivityTransition>,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        let entry_activity_key = entry_activity_key.into();
        let binding_kinds = normalize_workflow_binding_kinds(binding_kinds)?;
        validate_activity_lifecycle_definition(
            &key,
            &name,
            &entry_activity_key,
            &activities,
            &transitions,
        )?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            binding_kinds,
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
        match validate_activity_lifecycle_definition(
            &self.key,
            &self.name,
            &self.entry_activity_key,
            &self.activities,
            &self.transitions,
        ) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "activity_lifecycle_definition_invalid",
                error,
                "activities",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    /// Runtime session ID（可选）。
    ///
    /// 仅表示当前 run 关联的 runtime session（用于 event log、debug replay 等）。
    /// 业务归属（如 Story、RoutineExecution）通过 `LifecycleRunLink` 显式关联层表达，
    /// 不再由此字段推断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: LifecycleRunStatus,
    /// 当前所有可执行（Ready/Running）的 node key 集合。
    /// 线性 lifecycle 中此集合只有 0 或 1 个元素。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_node_keys: Vec<String>,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_state: Option<ActivityLifecycleRunState>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    /// 返回「当前活跃」的首个 step key。线性推进时即唯一活跃 step；
    /// DAG 下返回 `active_node_keys.first()`。
    pub fn current_step_key(&self) -> Option<&str> {
        self.active_node_keys.first().map(String::as_str)
    }

    pub fn new_activity(
        project_id: Uuid,
        lifecycle_id: Uuid,
        session_id: Option<String>,
        activity_state: ActivityLifecycleRunState,
    ) -> Result<Self, String> {
        if activity_state.attempts.is_empty() {
            return Err("activity lifecycle run 至少需要一个 attempt".to_string());
        }
        let now = Utc::now();
        let active_node_keys = active_activity_keys(&activity_state);
        let status = lifecycle_status_from_activity_status(activity_state.status);
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            lifecycle_id,
            session_id,
            status,
            active_node_keys,
            execution_log: Vec::new(),
            activity_state: Some(activity_state),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        })
    }

    /// 绑定 runtime session（attempt 开始执行时调用）。
    pub fn bind_runtime_session(&mut self, session_id: String) {
        self.session_id = Some(session_id);
        self.updated_at = Utc::now();
    }

    pub fn replace_activity_state(&mut self, activity_state: ActivityLifecycleRunState) {
        self.status = lifecycle_status_from_activity_status(activity_state.status);
        self.active_node_keys = active_activity_keys(&activity_state);
        self.activity_state = Some(activity_state);
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

fn lifecycle_status_from_activity_status(status: ActivityRunStatus) -> LifecycleRunStatus {
    match status {
        ActivityRunStatus::Ready => LifecycleRunStatus::Ready,
        ActivityRunStatus::Running => LifecycleRunStatus::Running,
        ActivityRunStatus::Blocked => LifecycleRunStatus::Blocked,
        ActivityRunStatus::Completed => LifecycleRunStatus::Completed,
        ActivityRunStatus::Failed => LifecycleRunStatus::Failed,
        ActivityRunStatus::Cancelled => LifecycleRunStatus::Cancelled,
    }
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_step_key: &str,
    primary_workflow: Option<&WorkflowDefinition>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_step_key: Some(active_step_key.to_string()),
            injection: w.contract.injection.clone(),
            hook_rules: w.contract.hook_rules.clone(),
        },
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_step_key: Some(active_step_key.to_string()),
            ..Default::default()
        },
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{WorkflowContextBinding, WorkflowInjectionSpec};

    fn contract() -> WorkflowContract {
        WorkflowContract {
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
            ..WorkflowContract::default()
        }
    }

    #[test]
    fn effective_contract_matches_primary_workflow() {
        let primary = WorkflowDefinition::new(
            Uuid::new_v4(),
            "wf_primary",
            "Primary",
            "desc",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::BuiltinSeed,
            contract(),
        )
        .expect("primary");

        let effective = build_effective_contract("lc", "step", Some(&primary));
        assert_eq!(effective.hook_rules.len(), 0);
    }

    #[test]
    fn activity_execution_claim_uses_attempt_idempotency_key() {
        let run_id = Uuid::new_v4();
        let claim = ActivityExecutionClaim::new(run_id, "plan", 2, "agent");

        assert_eq!(claim.run_id, run_id);
        assert_eq!(claim.activity_key, "plan");
        assert_eq!(claim.attempt, 2);
        assert_eq!(claim.executor_kind, "agent");
        assert_eq!(claim.status, ActivityExecutionClaimStatus::Claiming);
        assert_eq!(claim.idempotency_key, format!("{run_id}:plan:2"));
        assert!(claim.status.is_active());
        assert!(!ActivityExecutionClaimStatus::Succeeded.is_active());
    }
}
