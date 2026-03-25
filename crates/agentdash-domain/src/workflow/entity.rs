use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{
    EffectiveSessionAttachment, EffectiveSessionContract, LifecycleProgressionSource,
    LifecycleRunStatus, LifecycleStepDefinition, LifecycleStepExecutionStatus, LifecycleStepState,
    ValidationIssue, WorkflowAgentRole, WorkflowAttachmentMode, WorkflowContract,
    WorkflowDefinitionSource, WorkflowDefinitionStatus, WorkflowRecordArtifact,
    WorkflowRecordPolicy, WorkflowRuntimeAttachment, WorkflowSessionBinding, WorkflowTargetKind,
    validate_lifecycle_definition, validate_workflow_definition,
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
    pub contract: WorkflowContract,
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
            target_kind,
            recommended_role: None,
            source,
            status: match source {
                WorkflowDefinitionSource::BuiltinSeed => WorkflowDefinitionStatus::Active,
                _ => WorkflowDefinitionStatus::Draft,
            },
            version: 1,
            contract,
            record_policy: WorkflowRecordPolicy::default(),
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
    pub target_kind: WorkflowTargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_role: Option<WorkflowAgentRole>,
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
        target_kind: WorkflowTargetKind,
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
            target_kind,
            recommended_role: None,
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
    pub role: WorkflowAgentRole,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowAssignment {
    pub fn new(project_id: Uuid, lifecycle_id: Uuid, role: WorkflowAgentRole) -> Self {
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
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub status: LifecycleRunStatus,
    pub current_step_key: Option<String>,
    #[serde(default)]
    pub step_states: Vec<LifecycleStepState>,
    #[serde(default)]
    pub runtime_attachments: Vec<WorkflowRuntimeAttachment>,
    #[serde(default)]
    pub record_artifacts: Vec<WorkflowRecordArtifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    pub fn new(
        project_id: Uuid,
        lifecycle_id: Uuid,
        target_kind: WorkflowTargetKind,
        target_id: Uuid,
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
                session_binding_id: None,
                started_at: None,
                completed_at: None,
                summary: None,
                completed_by: None,
            })
            .collect::<Vec<_>>();

        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            lifecycle_id,
            target_kind,
            target_id,
            status: LifecycleRunStatus::Ready,
            current_step_key: Some(entry_step_key.to_string()),
            step_states,
            runtime_attachments: Vec::new(),
            record_artifacts: Vec::new(),
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

    pub fn attach_session_binding(
        &mut self,
        step_key: &str,
        session_binding_id: Uuid,
    ) -> Result<(), String> {
        let Some(index) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };

        self.step_states[index].session_binding_id = Some(session_binding_id);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
        Ok(())
    }

    pub fn complete_step(
        &mut self,
        step_key: &str,
        summary: Option<String>,
        completed_by: Option<LifecycleProgressionSource>,
        next_step_key: Option<&str>,
    ) -> Result<(), String> {
        let Some(index) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };
        if self.current_step_key.as_deref() != Some(step_key) {
            return Err(format!("当前可完成的 step 不是 {step_key}"));
        }

        match self.step_states[index].status {
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
        self.step_states[index].started_at.get_or_insert(now);
        self.step_states[index].status = LifecycleStepExecutionStatus::Completed;
        self.step_states[index].completed_at = Some(now);
        self.step_states[index].summary = summary;
        self.step_states[index].completed_by = completed_by;

        if let Some(next_step_key) = next_step_key {
            let Some(next_index) = self
                .step_states
                .iter()
                .position(|step| step.step_key == next_step_key)
            else {
                return Err(format!("下一个 step 不存在: {next_step_key}"));
            };
            self.step_states[next_index].status = LifecycleStepExecutionStatus::Ready;
            self.current_step_key = Some(next_step_key.to_string());
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

    pub fn attach_runtime_workflow(&mut self, attachment: WorkflowRuntimeAttachment) {
        self.runtime_attachments.push(attachment);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
    }
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_step_key: &str,
    primary_workflow: &WorkflowDefinition,
    overlays: &[WorkflowDefinition],
    runtime_attachments: &[WorkflowDefinition],
) -> EffectiveSessionContract {
    let mut contract = EffectiveSessionContract {
        lifecycle_key: Some(lifecycle_key.to_string()),
        active_step_key: Some(active_step_key.to_string()),
        injection: primary_workflow.contract.injection.clone(),
        hook_policy: primary_workflow.contract.hook_policy.clone(),
        completion: primary_workflow.contract.completion.clone(),
        attachments: vec![EffectiveSessionAttachment {
            workflow_id: primary_workflow.id,
            workflow_key: primary_workflow.key.clone(),
            workflow_name: primary_workflow.name.clone(),
            mode: WorkflowAttachmentMode::Primary,
            lifetime: super::value_objects::WorkflowAttachmentLifetime::UntilStepExit,
            priority: 0,
            reason: Some("lifecycle_step_primary_workflow".to_string()),
        }],
    };

    for overlay in overlays.iter().chain(runtime_attachments.iter()) {
        contract
            .injection
            .instructions
            .extend(overlay.contract.injection.instructions.clone());
        contract
            .injection
            .context_bindings
            .extend(overlay.contract.injection.context_bindings.clone());
        contract
            .hook_policy
            .constraints
            .extend(overlay.contract.hook_policy.constraints.clone());
        contract
            .completion
            .checks
            .extend(overlay.contract.completion.checks.clone());
        if contract.injection.goal.is_none() {
            contract.injection.goal = overlay.contract.injection.goal.clone();
        }
        if contract.completion.default_artifact_type.is_none() {
            contract.completion.default_artifact_type =
                overlay.contract.completion.default_artifact_type;
        }
        if contract.completion.default_artifact_title.is_none() {
            contract.completion.default_artifact_title =
                overlay.contract.completion.default_artifact_title.clone();
        }
        if contract.injection.session_binding == WorkflowSessionBinding::NotRequired
            && overlay.contract.injection.session_binding != WorkflowSessionBinding::NotRequired
        {
            contract.injection.session_binding = overlay.contract.injection.session_binding;
        }
        contract.attachments.push(EffectiveSessionAttachment {
            workflow_id: overlay.id,
            workflow_key: overlay.key.clone(),
            workflow_name: overlay.name.clone(),
            mode: WorkflowAttachmentMode::Overlay,
            lifetime: super::value_objects::WorkflowAttachmentLifetime::UntilStepExit,
            priority: 0,
            reason: Some("workflow_overlay".to_string()),
        });
    }

    contract
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        LifecycleFailureAction, LifecycleTransitionPolicy, LifecycleTransitionPolicyKind,
        LifecycleTransitionSpec, WorkflowAttachmentLifetime, WorkflowCompletionSpec,
        WorkflowConstraintKind, WorkflowConstraintSpec, WorkflowContextBinding,
        WorkflowContextBindingKind, WorkflowHookPolicySpec, WorkflowInjectionSpec,
    };

    fn contract(session_binding: WorkflowSessionBinding) -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions: vec!["follow the workflow".to_string()],
                context_bindings: vec![WorkflowContextBinding {
                    kind: WorkflowContextBindingKind::DocumentPath,
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
                session_binding,
                ..WorkflowInjectionSpec::default()
            },
            hook_policy: WorkflowHookPolicySpec {
                constraints: vec![WorkflowConstraintSpec {
                    key: "c1".to_string(),
                    kind: WorkflowConstraintKind::Custom,
                    description: "constraint".to_string(),
                    payload: None,
                }],
            },
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
            title: key.to_string(),
            description: String::new(),
            primary_workflow_key: workflow_key.to_string(),
            session_binding: WorkflowSessionBinding::NotRequired,
            attached_workflows: vec![],
            transition: LifecycleTransitionSpec {
                policy: LifecycleTransitionPolicy {
                    kind: LifecycleTransitionPolicyKind::Manual,
                    next_step_key: None,
                    session_terminal_states: vec![],
                    action_key: None,
                },
                on_failure: Some(LifecycleFailureAction::Stay),
            },
        }
    }

    #[test]
    fn lifecycle_run_completes_and_advances() {
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            WorkflowTargetKind::Task,
            Uuid::new_v4(),
            &[step("start", "wf_start"), step("check", "wf_check")],
            "start",
        )
        .expect("run");

        run.complete_step("start", Some("done".to_string()), None, Some("check"))
            .expect("complete");

        assert_eq!(run.current_step_key.as_deref(), Some("check"));
        assert_eq!(run.status, LifecycleRunStatus::Ready);
    }

    #[test]
    fn effective_contract_merges_overlay_constraints() {
        let primary = WorkflowDefinition::new(
            "wf_primary",
            "Primary",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract(WorkflowSessionBinding::NotRequired),
        )
        .expect("primary");
        let overlay = WorkflowDefinition::new(
            "wf_overlay",
            "Overlay",
            "desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract(WorkflowSessionBinding::Required),
        )
        .expect("overlay");

        let merged =
            build_effective_contract("lc", "step", &primary, std::slice::from_ref(&overlay), &[]);
        assert_eq!(merged.attachments.len(), 2);
        assert_eq!(
            merged.injection.session_binding,
            WorkflowSessionBinding::Required
        );
        assert_eq!(merged.hook_policy.constraints.len(), 2);
    }

    #[test]
    fn lifecycle_definition_validates_step_graph() {
        let lifecycle = LifecycleDefinition::new(
            "lc",
            "Lifecycle",
            "desc",
            WorkflowTargetKind::Task,
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
        let assignment = WorkflowAssignment::new(
            Uuid::new_v4(),
            lifecycle_id,
            WorkflowAgentRole::TaskExecutionWorker,
        );

        assert_eq!(assignment.lifecycle_id, lifecycle_id);
    }
}
