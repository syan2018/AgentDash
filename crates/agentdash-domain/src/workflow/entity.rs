use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{
    ValidationIssue, WorkflowAgentRole, WorkflowContextBindingKind, WorkflowDefinitionSource,
    WorkflowDefinitionStatus, WorkflowPhaseCompletionMode, WorkflowPhaseDefinition,
    WorkflowPhaseExecutionStatus, WorkflowPhaseState, WorkflowProgressionSource,
    WorkflowRecordArtifact, WorkflowRecordPolicy, WorkflowRunStatus, WorkflowTargetKind,
    validate_workflow_definition,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_role: Option<WorkflowAgentRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub phases: Vec<WorkflowPhaseDefinition>,
    pub record_policy: WorkflowRecordPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        target_kind: WorkflowTargetKind,
        source: WorkflowDefinitionSource,
        phases: Vec<WorkflowPhaseDefinition>,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        validate_workflow_definition(&key, &name, &phases)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            key,
            name,
            description: description.into(),
            target_kind,
            recommended_role: None,
            source,
            status: match source {
                WorkflowDefinitionSource::BuiltinSeed => WorkflowDefinitionStatus::Active,
                _ => WorkflowDefinitionStatus::Draft,
            },
            version: 1,
            phases,
            record_policy: WorkflowRecordPolicy::default(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == WorkflowDefinitionStatus::Active
    }

    /// 结构化全量校验，覆盖基础字段 + 语义一致性检查。
    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        if self.key.trim().is_empty() {
            issues.push(ValidationIssue::error("empty_key", "workflow.key 不能为空", "key"));
        } else if self.key.chars().any(char::is_whitespace) {
            issues.push(ValidationIssue::error("invalid_key", "workflow.key 不能包含空白字符", "key"));
        }
        if self.name.trim().is_empty() {
            issues.push(ValidationIssue::error("empty_name", "workflow.name 不能为空", "name"));
        }
        if self.phases.is_empty() {
            issues.push(ValidationIssue::error("no_phases", "workflow.phases 至少需要一个 phase", "phases"));
            return issues;
        }

        let mut seen_keys = std::collections::BTreeSet::new();
        for (i, phase) in self.phases.iter().enumerate() {
            let prefix = format!("phases[{i}]");
            if phase.key.trim().is_empty() {
                issues.push(ValidationIssue::error("empty_phase_key", format!("{prefix}.key 不能为空"), format!("{prefix}.key")));
            }
            if phase.title.trim().is_empty() {
                issues.push(ValidationIssue::error("empty_phase_title", format!("{prefix}.title 不能为空"), format!("{prefix}.title")));
            }
            if !seen_keys.insert(phase.key.trim().to_string()) {
                issues.push(ValidationIssue::error("duplicate_phase_key", format!("{prefix}.key 重复: {}", phase.key.trim()), format!("{prefix}.key")));
            }

            for (j, binding) in phase.context_bindings.iter().enumerate() {
                let bp = format!("{prefix}.context_bindings[{j}]");
                if binding.locator.trim().is_empty() {
                    issues.push(ValidationIssue::error("empty_locator", format!("{bp}.locator 不能为空"), format!("{bp}.locator")));
                }
                if binding.reason.trim().is_empty() {
                    issues.push(ValidationIssue::error("empty_reason", format!("{bp}.reason 不能为空"), format!("{bp}.reason")));
                }
            }

            // checklist_passed 必须有 Checklist binding
            if phase.completion_mode == WorkflowPhaseCompletionMode::ChecklistPassed {
                let has_checklist = phase.context_bindings.iter().any(|b| b.kind == WorkflowContextBindingKind::Checklist);
                if !has_checklist {
                    issues.push(ValidationIssue::error(
                        "checklist_missing",
                        format!("{prefix} 使用 checklist_passed 完成模式，但未配置 Checklist 类型 binding"),
                        format!("{prefix}.completion_mode"),
                    ));
                }
            }

            // requires_session 的合理性提示
            if phase.requires_session && self.target_kind == WorkflowTargetKind::Project {
                issues.push(ValidationIssue::warning(
                    "session_on_project",
                    format!("{prefix} 要求 session，但 target_kind 为 project，project 级 session 可能无法满足"),
                    format!("{prefix}.requires_session"),
                ));
            }
        }

        issues
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowAssignment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workflow_id: Uuid,
    pub role: WorkflowAgentRole,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowAssignment {
    pub fn new(project_id: Uuid, workflow_id: Uuid, role: WorkflowAgentRole) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            workflow_id,
            role,
            enabled: true,
            is_default: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workflow_id: Uuid,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub status: WorkflowRunStatus,
    pub current_phase_key: Option<String>,
    #[serde(default)]
    pub phase_states: Vec<WorkflowPhaseState>,
    #[serde(default)]
    pub record_artifacts: Vec<WorkflowRecordArtifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl WorkflowRun {
    pub fn new(
        project_id: Uuid,
        workflow_id: Uuid,
        target_kind: WorkflowTargetKind,
        target_id: Uuid,
        phases: &[WorkflowPhaseDefinition],
    ) -> Self {
        let now = Utc::now();
        let phase_states = phases
            .iter()
            .enumerate()
            .map(|(index, phase)| WorkflowPhaseState {
                phase_key: phase.key.clone(),
                status: if index == 0 {
                    WorkflowPhaseExecutionStatus::Ready
                } else {
                    WorkflowPhaseExecutionStatus::Pending
                },
                session_binding_id: None,
                started_at: None,
                completed_at: None,
                summary: None,
                completed_by: None,
            })
            .collect::<Vec<_>>();

        Self {
            id: Uuid::new_v4(),
            project_id,
            workflow_id,
            target_kind,
            target_id,
            status: WorkflowRunStatus::Ready,
            current_phase_key: phase_states.first().map(|phase| phase.phase_key.clone()),
            phase_states,
            record_artifacts: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    pub fn activate_phase(&mut self, phase_key: &str) -> Result<(), String> {
        let Some(index) = self
            .phase_states
            .iter()
            .position(|phase| phase.phase_key == phase_key)
        else {
            return Err(format!("workflow run 不存在 phase: {phase_key}"));
        };

        if self.current_phase_key.as_deref() != Some(phase_key) {
            return Err(format!("当前可激活的 phase 不是 {phase_key}"));
        }

        match self.phase_states[index].status {
            WorkflowPhaseExecutionStatus::Ready => {}
            WorkflowPhaseExecutionStatus::Pending => {
                return Err(format!("phase 尚未 ready: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Running => {
                return Err(format!("phase 已在运行中: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Completed => {
                return Err(format!("phase 已完成: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Failed => {
                return Err(format!("phase 已失败，无法重新激活: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Skipped => {
                return Err(format!("phase 已跳过，无法激活: {phase_key}"));
            }
        }

        let now = Utc::now();
        self.status = WorkflowRunStatus::Running;
        self.current_phase_key = Some(phase_key.to_string());
        self.phase_states[index].status = WorkflowPhaseExecutionStatus::Running;
        self.phase_states[index].started_at.get_or_insert(now);
        self.updated_at = now;
        self.last_activity_at = now;
        Ok(())
    }

    pub fn attach_session_binding(
        &mut self,
        phase_key: &str,
        session_binding_id: Uuid,
    ) -> Result<(), String> {
        let Some(index) = self
            .phase_states
            .iter()
            .position(|phase| phase.phase_key == phase_key)
        else {
            return Err(format!("workflow run 不存在 phase: {phase_key}"));
        };

        self.phase_states[index].session_binding_id = Some(session_binding_id);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
        Ok(())
    }

    pub fn complete_phase(
        &mut self,
        phase_key: &str,
        summary: Option<String>,
        completed_by: Option<WorkflowProgressionSource>,
    ) -> Result<(), String> {
        let Some(index) = self
            .phase_states
            .iter()
            .position(|phase| phase.phase_key == phase_key)
        else {
            return Err(format!("workflow run 不存在 phase: {phase_key}"));
        };

        if self.current_phase_key.as_deref() != Some(phase_key) {
            return Err(format!("当前可完成的 phase 不是 {phase_key}"));
        }

        let now = Utc::now();
        match self.phase_states[index].status {
            WorkflowPhaseExecutionStatus::Ready | WorkflowPhaseExecutionStatus::Running => {}
            WorkflowPhaseExecutionStatus::Pending => {
                return Err(format!("phase 尚未 ready: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Completed => {
                return Err(format!("phase 已完成: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Failed => {
                return Err(format!("phase 已失败，无法直接完成: {phase_key}"));
            }
            WorkflowPhaseExecutionStatus::Skipped => {
                return Err(format!("phase 已跳过，无法完成: {phase_key}"));
            }
        }

        self.phase_states[index].started_at.get_or_insert(now);
        self.phase_states[index].status = WorkflowPhaseExecutionStatus::Completed;
        self.phase_states[index].completed_at = Some(now);
        self.phase_states[index].summary = summary;
        self.phase_states[index].completed_by = completed_by;

        let next_index = self
            .phase_states
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, phase)| phase.status == WorkflowPhaseExecutionStatus::Pending)
            .map(|(next_index, _)| next_index);

        if let Some(next_index) = next_index {
            self.phase_states[next_index].status = WorkflowPhaseExecutionStatus::Ready;
            self.current_phase_key = Some(self.phase_states[next_index].phase_key.clone());
            self.status = WorkflowRunStatus::Ready;
        } else {
            self.current_phase_key = None;
            self.status = WorkflowRunStatus::Completed;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        WorkflowContextBinding, WorkflowContextBindingKind, WorkflowPhaseCompletionMode,
        ValidationSeverity,
    };

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
            requires_session: true,
            completion_mode: WorkflowPhaseCompletionMode::Manual,
            default_artifact_type: None,
            default_artifact_title: None,
        }
    }

    fn checklist_phase(key: &str) -> WorkflowPhaseDefinition {
        WorkflowPhaseDefinition {
            completion_mode: WorkflowPhaseCompletionMode::ChecklistPassed,
            context_bindings: vec![],
            ..phase(key)
        }
    }

    #[test]
    fn workflow_run_ready_with_first_phase_selected() {
        let run = WorkflowRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        assert_eq!(run.status, WorkflowRunStatus::Ready);
        assert_eq!(run.current_phase_key.as_deref(), Some("start"));
        assert_eq!(
            run.phase_states[0].status,
            WorkflowPhaseExecutionStatus::Ready
        );
    }

    #[test]
    fn workflow_run_complete_phase_advances_to_next() {
        let mut run = WorkflowRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        run.complete_phase("start", Some("done".to_string()), None)
            .expect("complete");

        assert_eq!(run.status, WorkflowRunStatus::Ready);
        assert_eq!(run.current_phase_key.as_deref(), Some("implement"));
        assert_eq!(
            run.phase_states[1].status,
            WorkflowPhaseExecutionStatus::Ready
        );
    }

    #[test]
    fn workflow_run_rejects_activation_for_non_current_phase() {
        let mut run = WorkflowRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        let error = run.activate_phase("implement").expect_err("should fail");

        assert!(error.contains("当前可激活"));
    }

    #[test]
    fn workflow_run_can_attach_session_binding_and_record_artifact() {
        let mut run = WorkflowRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        let binding_id = Uuid::new_v4();
        run.attach_session_binding("start", binding_id)
            .expect("bind session");
        run.append_record_artifact(WorkflowRecordArtifact::new(
            "start",
            crate::workflow::WorkflowRecordArtifactType::PhaseNote,
            "note",
            "content",
        ));

        assert_eq!(run.phase_states[0].session_binding_id, Some(binding_id));
        assert_eq!(run.record_artifacts.len(), 1);
    }

    #[test]
    fn validate_full_detects_checklist_without_binding() {
        let def = WorkflowDefinition::new(
            "test-wf",
            "Test",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![checklist_phase("check")],
        )
        .expect("create");

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "checklist_missing" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn validate_full_warns_session_on_project() {
        let mut def = WorkflowDefinition::new(
            "proj-wf",
            "Proj",
            "desc",
            WorkflowTargetKind::Project,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");
        def.phases[0].requires_session = true;

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "session_on_project" && i.severity == ValidationSeverity::Warning));
    }

    #[test]
    fn new_definition_builtin_seed_is_active() {
        let def = WorkflowDefinition::new(
            "builtin-wf",
            "Builtin",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            vec![phase("start")],
        )
        .expect("create");

        assert!(def.is_active());
        assert_eq!(def.source, WorkflowDefinitionSource::BuiltinSeed);
    }

    #[test]
    fn new_definition_user_authored_is_draft() {
        let def = WorkflowDefinition::new(
            "user-wf",
            "User",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");

        assert!(!def.is_active());
        assert_eq!(def.status, WorkflowDefinitionStatus::Draft);
    }

    #[test]
    fn validate_full_detects_empty_key() {
        let mut def = WorkflowDefinition::new(
            "temp",
            "Valid Name",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");
        def.key = "".to_string();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "empty_key"));
    }

    #[test]
    fn validate_full_detects_key_with_whitespace() {
        let mut def = WorkflowDefinition::new(
            "temp",
            "Name",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");
        def.key = "has space".to_string();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "invalid_key"));
    }

    #[test]
    fn validate_full_detects_duplicate_phase_keys() {
        let mut def = WorkflowDefinition::new(
            "dup-keys",
            "Dup",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start"), phase("implement")],
        )
        .expect("create");
        def.phases[1].key = "start".to_string();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "duplicate_phase_key"));
    }

    #[test]
    fn validate_full_detects_empty_phase_title() {
        let mut def = WorkflowDefinition::new(
            "empty-title",
            "Name",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");
        def.phases[0].title = "".to_string();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "empty_phase_title"));
    }

    #[test]
    fn validate_full_detects_empty_binding_locator() {
        let mut def = WorkflowDefinition::new(
            "empty-loc",
            "Name",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("check")],
        )
        .expect("create");
        def.phases[0].context_bindings[0].locator = "".to_string();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "empty_locator"));
    }

    #[test]
    fn validate_full_no_issues_for_valid_definition() {
        let def = WorkflowDefinition::new(
            "valid-wf",
            "Valid Workflow",
            "A well-formed definition",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start"), phase("implement"), phase("check")],
        )
        .expect("create");

        let issues = def.validate_full();
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn validate_full_detects_no_phases() {
        let mut def = WorkflowDefinition::new(
            "temp",
            "Name",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            vec![phase("start")],
        )
        .expect("create");
        def.phases.clear();

        let issues = def.validate_full();
        assert!(issues.iter().any(|i| i.code == "no_phases"));
    }

    #[test]
    fn cloned_definition_starts_as_draft() {
        let def = WorkflowDefinition::new(
            "cloned-wf",
            "Cloned",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::Cloned,
            vec![phase("start")],
        )
        .expect("create");

        assert_eq!(def.status, WorkflowDefinitionStatus::Draft);
        assert_eq!(def.source, WorkflowDefinitionSource::Cloned);
    }
}
