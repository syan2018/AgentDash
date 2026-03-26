use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTargetKind {
    Project,
    Story,
    Task,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentRole {
    Project,
    Story,
    Task,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionSource {
    BuiltinSeed,
    UserAuthored,
    Cloned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionStatus {
    Draft,
    Active,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Error,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowContextBindingKind {
    DocumentPath,
    RuntimeContext,
    Checklist,
    JournalTarget,
    ActionRef,
    ArtifactRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowContextBinding {
    pub kind: WorkflowContextBindingKind,
    pub locator: String,
    pub reason: String,
    #[serde(default = "bool_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowInjectionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default)]
    pub instructions: Vec<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowConstraintKind {
    DenyTaskStatusTransition,
    BlockStopUntilChecksPass,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowConstraintSpec {
    pub key: String,
    pub kind: WorkflowConstraintKind,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCheckKind {
    TaskStatusIn,
    ArtifactExists,
    ArtifactCountGte,
    SessionTerminalIn,
    ChecklistEvidencePresent,
    ExplicitActionReceived,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowCheckSpec {
    pub key: String,
    pub kind: WorkflowCheckKind,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRecordArtifactType {
    SessionSummary,
    JournalUpdate,
    ArchiveSuggestion,
    PhaseNote,
    ChecklistEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowCompletionSpec {
    #[serde(default)]
    pub checks: Vec<WorkflowCheckSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowContract {
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub constraints: Vec<WorkflowConstraintSpec>,
    #[serde(default)]
    pub completion: WorkflowCompletionSpec,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct LifecycleStepDefinition {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunStatus {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStepExecutionStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleStepState {
    pub step_key: String,
    pub status: LifecycleStepExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRecordArtifact {
    pub id: Uuid,
    #[serde(default)]
    pub step_key: String,
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl WorkflowRecordArtifact {
    pub fn new(
        step_key: impl Into<String>,
        artifact_type: WorkflowRecordArtifactType,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            step_key: step_key.into(),
            artifact_type,
            title: title.into(),
            content: content.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EffectiveSessionContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_step_key: Option<String>,
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub constraints: Vec<WorkflowConstraintSpec>,
    #[serde(default)]
    pub completion: WorkflowCompletionSpec,
}

pub fn validate_workflow_definition(
    key: &str,
    name: &str,
    contract: &WorkflowContract,
) -> Result<(), String> {
    validate_identity("workflow.key", key)?;
    validate_non_empty("workflow.name", name)?;
    validate_contract(contract, "workflow.contract")
}

pub fn validate_lifecycle_definition(
    key: &str,
    name: &str,
    entry_step_key: &str,
    steps: &[LifecycleStepDefinition],
) -> Result<(), String> {
    validate_identity("lifecycle.key", key)?;
    validate_non_empty("lifecycle.name", name)?;
    validate_identity("lifecycle.entry_step_key", entry_step_key)?;
    if steps.is_empty() {
        return Err("lifecycle.steps 至少需要一个 step".to_string());
    }

    let mut seen_step_keys = std::collections::BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        validate_identity(&format!("lifecycle.steps[{index}].key"), &step.key)?;
        if let Some(workflow_key) = step
            .workflow_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            validate_identity(
                &format!("lifecycle.steps[{index}].workflow_key"),
                workflow_key,
            )?;
        }
        if !seen_step_keys.insert(step.key.clone()) {
            return Err(format!("lifecycle.steps[{index}].key 重复: {}", step.key));
        }
    }

    if !steps.iter().any(|step| step.key == entry_step_key) {
        return Err(format!(
            "lifecycle.entry_step_key `{entry_step_key}` 未出现在 lifecycle.steps 中"
        ));
    }

    Ok(())
}

fn validate_contract(contract: &WorkflowContract, field_path: &str) -> Result<(), String> {
    let mut seen_constraint_keys = std::collections::BTreeSet::new();
    for (index, binding) in contract.injection.context_bindings.iter().enumerate() {
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].locator"),
            &binding.locator,
        )?;
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].reason"),
            &binding.reason,
        )?;
    }
    for (index, constraint) in contract.constraints.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.constraints[{index}].key"),
            &constraint.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.constraints[{index}].description"),
            &constraint.description,
        )?;
        if !seen_constraint_keys.insert(constraint.key.clone()) {
            return Err(format!(
                "{field_path}.constraints[{index}].key 重复: {}",
                constraint.key
            ));
        }
    }

    let mut seen_check_keys = std::collections::BTreeSet::new();
    for (index, check) in contract.completion.checks.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.completion.checks[{index}].key"),
            &check.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.completion.checks[{index}].description"),
            &check.description,
        )?;
        if !seen_check_keys.insert(check.key.clone()) {
            return Err(format!(
                "{field_path}.completion.checks[{index}].key 重复: {}",
                check.key
            ));
        }
    }

    Ok(())
}

fn validate_identity(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    if value.chars().any(char::is_whitespace) {
        return Err(format!("{field} 不能包含空白字符"));
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} 不能为空"));
    }
    Ok(())
}

fn bool_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contract() -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions: vec!["read spec first".to_string()],
                context_bindings: vec![WorkflowContextBinding {
                    kind: WorkflowContextBindingKind::DocumentPath,
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
    fn validate_workflow_definition_rejects_duplicate_constraint_keys() {
        let mut contract = sample_contract();
        contract.constraints = vec![
            WorkflowConstraintSpec {
                key: "a".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "x".to_string(),
                payload: None,
            },
            WorkflowConstraintSpec {
                key: "a".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "y".to_string(),
                payload: None,
            },
        ];

        let error = validate_workflow_definition("wf", "Workflow", &contract).expect_err("fail");
        assert!(error.contains("重复"));
    }

    #[test]
    fn validate_lifecycle_definition_requires_entry_step() {
        let steps = vec![LifecycleStepDefinition {
            key: "start".to_string(),
            description: String::new(),
            workflow_key: Some("wf_start".to_string()),
        }];

        let error =
            validate_lifecycle_definition("lc", "Lifecycle", "missing", &steps).expect_err("fail");
        assert!(error.contains("entry_step_key"));
    }
}
