use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    ProjectContextMaintainer,
    StoryLifecycleCompanion,
    TaskExecutionWorker,
    ReviewAgent,
    RecordAgent,
}

/// Workflow Definition 的来源标记。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionSource {
    /// 由内置模板 seed 注册。
    BuiltinSeed,
    /// 用户从编辑器手动创建。
    UserAuthored,
    /// 从已有 definition 克隆。
    Cloned,
}

/// Workflow Definition 的生命周期状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionStatus {
    /// 草稿态，可编辑但不可分配/运行。
    Draft,
    /// 已激活，可分配到项目并启动 run。
    Active,
    /// 已停用，不可新建 assignment/run，已有 run 不受影响。
    Disabled,
}

/// 校验问题的严重级别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

/// 结构化校验问题。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    pub fn error(code: impl Into<String>, message: impl Into<String>, field_path: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), field_path: field_path.into(), severity: ValidationSeverity::Error }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, field_path: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), field_path: field_path.into(), severity: ValidationSeverity::Warning }
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhaseCompletionMode {
    Manual,
    SessionEnded,
    ChecklistPassed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowPhaseDefinition {
    pub key: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub agent_instructions: Vec<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
    #[serde(default)]
    pub requires_session: bool,
    pub completion_mode: WorkflowPhaseCompletionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_artifact_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowRecordPolicy {
    #[serde(default = "bool_true")]
    pub emit_summary: bool,
    #[serde(default = "bool_true")]
    pub emit_journal_update: bool,
    #[serde(default = "bool_true")]
    pub emit_archive_suggestion: bool,
}

impl Default for WorkflowRecordPolicy {
    fn default() -> Self {
        Self {
            emit_summary: true,
            emit_journal_update: true,
            emit_archive_suggestion: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
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
pub enum WorkflowPhaseExecutionStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Phase 完成/推进的来源标记。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowProgressionSource {
    /// Hook runtime 自动推进（唯一的自动 authority）。
    HookRuntime,
    /// 人工通过 API route 手动 override。
    ManualOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowPhaseState {
    pub phase_key: String,
    pub status: WorkflowPhaseExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_binding_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// 记录此 phase 由谁推进完成（缺省视为 unknown/legacy）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_by: Option<WorkflowProgressionSource>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRecordArtifact {
    pub id: Uuid,
    #[serde(default)]
    pub phase_key: String,
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl WorkflowRecordArtifact {
    pub fn new(
        phase_key: impl Into<String>,
        artifact_type: WorkflowRecordArtifactType,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            phase_key: phase_key.into(),
            artifact_type,
            title: title.into(),
            content: content.into(),
            created_at: Utc::now(),
        }
    }
}

pub fn validate_workflow_definition(
    key: &str,
    name: &str,
    phases: &[WorkflowPhaseDefinition],
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("workflow.key 不能为空".to_string());
    }
    if key.chars().any(char::is_whitespace) {
        return Err("workflow.key 不能包含空白字符".to_string());
    }
    if name.trim().is_empty() {
        return Err("workflow.name 不能为空".to_string());
    }
    if phases.is_empty() {
        return Err("workflow.phases 至少需要一个 phase".to_string());
    }

    let mut seen_phase_keys = std::collections::BTreeSet::new();
    for (index, phase) in phases.iter().enumerate() {
        if phase.key.trim().is_empty() {
            return Err(format!("workflow.phases[{index}].key 不能为空"));
        }
        if phase.title.trim().is_empty() {
            return Err(format!("workflow.phases[{index}].title 不能为空"));
        }
        if !seen_phase_keys.insert(phase.key.trim().to_string()) {
            return Err(format!(
                "workflow.phases[{index}].key 重复: {}",
                phase.key.trim()
            ));
        }
        for (binding_index, binding) in phase.context_bindings.iter().enumerate() {
            if binding.locator.trim().is_empty() {
                return Err(format!(
                    "workflow.phases[{index}].context_bindings[{binding_index}].locator 不能为空"
                ));
            }
            if binding.reason.trim().is_empty() {
                return Err(format!(
                    "workflow.phases[{index}].context_bindings[{binding_index}].reason 不能为空"
                ));
            }
        }
    }

    Ok(())
}

fn bool_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase(key: &str) -> WorkflowPhaseDefinition {
        WorkflowPhaseDefinition {
            key: key.to_string(),
            title: key.to_string(),
            description: "desc".to_string(),
            agent_instructions: vec![],
            context_bindings: vec![WorkflowContextBinding {
                kind: WorkflowContextBindingKind::DocumentPath,
                locator: ".trellis/workflow.md".to_string(),
                reason: "workflow".to_string(),
                required: true,
                title: None,
            }],
            requires_session: false,
            completion_mode: WorkflowPhaseCompletionMode::Manual,
            default_artifact_type: None,
            default_artifact_title: None,
        }
    }

    #[test]
    fn validate_workflow_definition_rejects_duplicate_phase_keys() {
        let error = validate_workflow_definition(
            "trellis-dev-workflow",
            "Trellis Dev Workflow",
            &[phase("start"), phase("start")],
        )
        .expect_err("should fail");

        assert!(error.contains("重复"));
    }

    #[test]
    fn validate_workflow_definition_requires_non_empty_name() {
        let error = validate_workflow_definition("trellis", "  ", &[phase("start")])
            .expect_err("should fail");

        assert!(error.contains("workflow.name"));
    }
}
