use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{
    WorkflowAgentRole, WorkflowPhaseDefinition, WorkflowPhaseExecutionStatus, WorkflowPhaseState,
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
    pub version: i32,
    pub enabled: bool,
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
            version: 1,
            enabled: true,
            phases,
            record_policy: WorkflowRecordPolicy::default(),
            created_at: now,
            updated_at: now,
        })
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
            })
            .collect::<Vec<_>>();

        Self {
            id: Uuid::new_v4(),
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

    #[test]
    fn workflow_run_ready_with_first_phase_selected() {
        let run = WorkflowRun::new(
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
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        run.complete_phase("start", Some("done".to_string()))
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
            WorkflowTargetKind::Story,
            Uuid::new_v4(),
            &[phase("start"), phase("implement")],
        );

        let binding_id = Uuid::new_v4();
        run.attach_session_binding("start", binding_id)
            .expect("bind session");
        run.append_record_artifact(WorkflowRecordArtifact::new(
            crate::workflow::WorkflowRecordArtifactType::PhaseNote,
            "note",
            "content",
        ));

        assert_eq!(run.phase_states[0].session_binding_id, Some(binding_id));
        assert_eq!(run.record_artifacts.len(), 1);
    }
}
