use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{
    EffectiveSessionContract, LifecycleExecutionEntry, LifecycleRunStatus, LifecycleStepDefinition,
    LifecycleStepExecutionStatus, LifecycleStepState, ValidationIssue, WorkflowBindingKind,
    WorkflowBindingRole, WorkflowCheckKind, WorkflowConstraintKind, WorkflowContract,
    WorkflowDefinitionSource, WorkflowDefinitionStatus, WorkflowHookRuleSpec, WorkflowHookTrigger,
    WorkflowRecordArtifact, validate_lifecycle_definition, validate_workflow_definition,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub contract: WorkflowContract,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kind: WorkflowBindingKind,
        source: WorkflowDefinitionSource,
        contract: WorkflowContract,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        validate_workflow_definition(&key, &name, &contract)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            key,
            name,
            description: description.into(),
            binding_kind,
            recommended_binding_roles: Vec::new(),
            source,
            status: match source {
                WorkflowDefinitionSource::BuiltinSeed => WorkflowDefinitionStatus::Active,
                _ => WorkflowDefinitionStatus::Draft,
            },
            version: 1,
            contract,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == WorkflowDefinitionStatus::Active
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
pub struct LifecycleDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LifecycleDefinition {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kind: WorkflowBindingKind,
        source: WorkflowDefinitionSource,
        entry_step_key: impl Into<String>,
        steps: Vec<LifecycleStepDefinition>,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        let entry_step_key = entry_step_key.into();
        validate_lifecycle_definition(&key, &name, &entry_step_key, &steps)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            key,
            name,
            description: description.into(),
            binding_kind,
            recommended_binding_roles: Vec::new(),
            source,
            status: match source {
                WorkflowDefinitionSource::BuiltinSeed => WorkflowDefinitionStatus::Active,
                _ => WorkflowDefinitionStatus::Draft,
            },
            version: 1,
            entry_step_key,
            steps,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == WorkflowDefinitionStatus::Active
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_lifecycle_definition(
            &self.key,
            &self.name,
            &self.entry_step_key,
            &self.steps,
        ) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "lifecycle_definition_invalid",
                error,
                "steps",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowAssignment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub role: WorkflowBindingRole,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowAssignment {
    pub fn new(project_id: Uuid, lifecycle_id: Uuid, role: WorkflowBindingRole) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            lifecycle_id,
            role,
            enabled: true,
            is_default: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub binding_kind: WorkflowBindingKind,
    pub binding_id: Uuid,
    pub status: LifecycleRunStatus,
    pub current_step_key: Option<String>,
    #[serde(default)]
    pub step_states: Vec<LifecycleStepState>,
    #[serde(default)]
    pub record_artifacts: Vec<WorkflowRecordArtifact>,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    pub fn new(
        project_id: Uuid,
        lifecycle_id: Uuid,
        binding_kind: WorkflowBindingKind,
        binding_id: Uuid,
        steps: &[LifecycleStepDefinition],
        entry_step_key: &str,
    ) -> Result<Self, String> {
        if steps.is_empty() {
            return Err("lifecycle run 至少需要一个 step".to_string());
        }
        if !steps.iter().any(|step| step.key == entry_step_key) {
            return Err(format!("entry_step_key `{entry_step_key}` 不存在"));
        }

        let now = Utc::now();
        let step_states = steps
            .iter()
            .map(|step| LifecycleStepState {
                step_key: step.key.clone(),
                status: if step.key == entry_step_key {
                    LifecycleStepExecutionStatus::Ready
                } else {
                    LifecycleStepExecutionStatus::Pending
                },
                started_at: None,
                completed_at: None,
                summary: None,
                context_snapshot: None,
            })
            .collect::<Vec<_>>();

        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            lifecycle_id,
            binding_kind,
            binding_id,
            status: LifecycleRunStatus::Ready,
            current_step_key: Some(entry_step_key.to_string()),
            step_states,
            record_artifacts: Vec::new(),
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        })
    }

    pub fn activate_step(&mut self, step_key: &str) -> Result<(), String> {
        let Some(index) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };
        if self.current_step_key.as_deref() != Some(step_key) {
            return Err(format!("当前可激活的 step 不是 {step_key}"));
        }

        match self.step_states[index].status {
            LifecycleStepExecutionStatus::Ready => {}
            LifecycleStepExecutionStatus::Pending => {
                return Err(format!("step 尚未 ready: {step_key}"));
            }
            LifecycleStepExecutionStatus::Running => {
                return Err(format!("step 已在运行中: {step_key}"));
            }
            LifecycleStepExecutionStatus::Completed => {
                return Err(format!("step 已完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Failed => {
                return Err(format!("step 已失败，无法重新激活: {step_key}"));
            }
            LifecycleStepExecutionStatus::Skipped => {
                return Err(format!("step 已跳过，无法激活: {step_key}"));
            }
        }

        let now = Utc::now();
        self.status = LifecycleRunStatus::Running;
        self.step_states[index].status = LifecycleStepExecutionStatus::Running;
        self.step_states[index].started_at.get_or_insert(now);
        self.updated_at = now;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn complete_step(&mut self, step_key: &str, summary: Option<String>) -> Result<(), String> {
        let Some(current_idx) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };
        if self.current_step_key.as_deref() != Some(step_key) {
            return Err(format!("当前可完成的 step 不是 {step_key}"));
        }

        match self.step_states[current_idx].status {
            LifecycleStepExecutionStatus::Ready | LifecycleStepExecutionStatus::Running => {}
            LifecycleStepExecutionStatus::Pending => {
                return Err(format!("step 尚未 ready: {step_key}"));
            }
            LifecycleStepExecutionStatus::Completed => {
                return Err(format!("step 已完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Failed => {
                return Err(format!("step 已失败，无法直接完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Skipped => {
                return Err(format!("step 已跳过，无法完成: {step_key}"));
            }
        }

        let now = Utc::now();
        self.step_states[current_idx].started_at.get_or_insert(now);
        self.step_states[current_idx].status = LifecycleStepExecutionStatus::Completed;
        self.step_states[current_idx].completed_at = Some(now);
        self.step_states[current_idx].summary = summary;

        if current_idx + 1 < self.step_states.len() {
            let next_idx = current_idx + 1;
            let next_key = self.step_states[next_idx].step_key.clone();
            self.step_states[next_idx].status = LifecycleStepExecutionStatus::Ready;
            self.current_step_key = Some(next_key);
            self.status = LifecycleRunStatus::Ready;
        } else {
            self.current_step_key = None;
            self.status = LifecycleRunStatus::Completed;
        }

        self.updated_at = now;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn append_record_artifact(&mut self, artifact: WorkflowRecordArtifact) {
        self.record_artifacts.push(artifact);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
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

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_step_key: &str,
    primary_workflow: Option<&WorkflowDefinition>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => {
            let hook_rules = if w.contract.hook_rules.is_empty() {
                migrate_legacy_to_hook_rules(&w.contract)
            } else {
                w.contract.hook_rules.clone()
            };
            EffectiveSessionContract {
                lifecycle_key: Some(lifecycle_key.to_string()),
                active_step_key: Some(active_step_key.to_string()),
                injection: w.contract.injection.clone(),
                hook_rules,
                constraints: w.contract.constraints.clone(),
                completion: w.contract.completion.clone(),
            }
        }
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_step_key: Some(active_step_key.to_string()),
            ..Default::default()
        },
    }
}

/// When a WorkflowContract has no `hook_rules` but uses legacy `constraints`/`checks`,
/// synthesize equivalent hook_rules so the new evaluation path can handle them.
///
/// NOTE: `stop_gate_checks_pending` 不再自动迁移。该 hook 必须由 workflow
/// 定义方在 `hook_rules` 中显式声明，而不是从 constraint/check 隐式派生。
fn migrate_legacy_to_hook_rules(contract: &WorkflowContract) -> Vec<WorkflowHookRuleSpec> {
    let mut rules = Vec::new();

    for constraint in &contract.constraints {
        if let WorkflowConstraintKind::Custom = constraint.kind {
            let is_deny_artifact = constraint
                .payload
                .as_ref()
                .and_then(|p| p.get("policy"))
                .and_then(serde_json::Value::as_str)
                == Some("deny_record_artifact_types");
            if is_deny_artifact {
                let artifact_types = constraint
                    .payload
                    .as_ref()
                    .and_then(|p| p.get("artifact_types"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Array(vec![]));
                rules.push(WorkflowHookRuleSpec {
                    key: format!("migrated:{}", constraint.key),
                    trigger: WorkflowHookTrigger::BeforeTool,
                    description: constraint.description.clone(),
                    preset: Some("block_record_artifact".to_string()),
                    params: Some(serde_json::json!({ "artifact_types": artifact_types })),
                    script: None,
                    enabled: true,
                });
            }
        }
    }

    for check in &contract.completion.checks {
        let (preset_key, trigger) = match check.kind {
            WorkflowCheckKind::SessionTerminalIn => {
                ("session_terminal_advance", WorkflowHookTrigger::BeforeStop)
            }
            _ => continue,
        };
        let key = format!("migrated:{}", check.key);
        if rules.iter().any(|r| r.key == key) {
            continue;
        }
        rules.push(WorkflowHookRuleSpec {
            key,
            trigger,
            description: check.description.clone(),
            preset: Some(preset_key.to_string()),
            params: None,
            script: None,
            enabled: true,
        });
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        WorkflowCompletionSpec, WorkflowConstraintKind, WorkflowConstraintSpec,
        WorkflowContextBinding, WorkflowInjectionSpec,
    };

    fn contract() -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions: vec!["follow the workflow".to_string()],
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
                ..WorkflowInjectionSpec::default()
            },
            constraints: vec![WorkflowConstraintSpec {
                key: "c1".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "constraint".to_string(),
                payload: None,
            }],
            completion: WorkflowCompletionSpec {
                default_artifact_type: Some(crate::workflow::WorkflowRecordArtifactType::PhaseNote),
                default_artifact_title: Some("artifact".to_string()),
                ..WorkflowCompletionSpec::default()
            },
            ..WorkflowContract::default()
        }
    }

    fn step(key: &str, workflow_key: &str) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: Some(workflow_key.to_string()),
        }
    }

    #[test]
    fn lifecycle_run_completes_and_advances() {
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowBindingKind::Task,
            Uuid::new_v4(),
            &[step("start", "wf_start"), step("check", "wf_check")],
            "start",
        )
        .expect("run");

        run.complete_step("start", Some("done".to_string()))
            .expect("complete");

        assert_eq!(run.current_step_key.as_deref(), Some("check"));
        assert_eq!(run.status, LifecycleRunStatus::Ready);
    }

    #[test]
    fn effective_contract_matches_primary_workflow() {
        let primary = WorkflowDefinition::new(
            "wf_primary",
            "Primary",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract(),
        )
        .expect("primary");

        let effective = build_effective_contract("lc", "step", Some(&primary));
        assert_eq!(effective.constraints.len(), 1);
    }

    #[test]
    fn lifecycle_definition_validates_step_graph() {
        let lifecycle = LifecycleDefinition::new(
            "lc",
            "Lifecycle",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            "start",
            vec![step("start", "wf_start")],
        )
        .expect("lifecycle");

        assert!(lifecycle.is_active());
    }

    #[test]
    fn workflow_assignment_uses_lifecycle_id() {
        let lifecycle_id = Uuid::new_v4();
        let assignment =
            WorkflowAssignment::new(Uuid::new_v4(), lifecycle_id, WorkflowBindingRole::Task);

        assert_eq!(assignment.lifecycle_id, lifecycle_id);
    }
}
