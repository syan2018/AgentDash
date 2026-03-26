use agentdash_application::workflow::{
    BuiltinLifecycleTemplate, BuiltinWorkflowTemplate, BuiltinWorkflowTemplateBundle,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleRun, LifecycleRunStatus, LifecycleStepDefinition,
    LifecycleStepExecutionStatus, LifecycleStepState, ValidationIssue, WorkflowAgentRole,
    WorkflowAssignment, WorkflowCheckKind, WorkflowCheckSpec, WorkflowCompletionSpec,
    WorkflowConstraintKind, WorkflowConstraintSpec, WorkflowContextBinding, WorkflowContextBindingKind,
    WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource, WorkflowDefinitionStatus,
    WorkflowInjectionSpec, WorkflowRecordArtifact, WorkflowRecordArtifactType, WorkflowSessionBinding,
    WorkflowSessionTerminalState, WorkflowTargetKind,
};

#[derive(Debug, Serialize)]
pub struct WorkflowDefinitionResponse {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    #[serde(default)]
    pub recommended_roles: Vec<WorkflowAgentRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub contract: WorkflowContractResponse,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct LifecycleDefinitionResponse {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    #[serde(default)]
    pub recommended_roles: Vec<WorkflowAgentRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinitionResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowValidationResponse {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowTemplateResponse {
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    #[serde(default)]
    pub recommended_roles: Vec<WorkflowAgentRole>,
    pub workflows: Vec<BuiltinWorkflowTemplateResponse>,
    pub lifecycle: BuiltinLifecycleTemplateResponse,
}

#[derive(Debug, Serialize)]
pub struct BuiltinWorkflowTemplateResponse {
    pub key: String,
    pub name: String,
    pub description: String,
    pub contract: WorkflowContractResponse,
}

#[derive(Debug, Serialize)]
pub struct BuiltinLifecycleTemplateResponse {
    pub key: String,
    pub name: String,
    pub description: String,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinitionResponse>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowContractResponse {
    pub injection: WorkflowInjectionResponse,
    pub constraints: Vec<WorkflowConstraintResponse>,
    pub completion: WorkflowCompletionResponse,
}

#[derive(Debug, Serialize)]
pub struct WorkflowInjectionResponse {
    pub goal: Option<String>,
    pub instructions: Vec<String>,
    pub context_bindings: Vec<WorkflowContextBindingResponse>,
    pub session_binding: WorkflowSessionBinding,
}

#[derive(Debug, Serialize)]
pub struct WorkflowContextBindingResponse {
    pub kind: WorkflowContextBindingKind,
    pub locator: String,
    pub reason: String,
    pub required: bool,
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowConstraintResponse {
    pub key: String,
    pub kind: WorkflowConstraintKind,
    pub description: String,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowCheckResponse {
    pub key: String,
    pub kind: WorkflowCheckKind,
    pub description: String,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowCompletionResponse {
    pub checks: Vec<WorkflowCheckResponse>,
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    pub default_artifact_title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LifecycleStepDefinitionResponse {
    pub key: String,
    pub title: String,
    pub description: String,
    pub primary_workflow_key: String,
    pub session_binding: WorkflowSessionBinding,
    pub transition_policy: String,
    pub next_step_key: Option<String>,
    pub session_terminal_states: Vec<WorkflowSessionTerminalState>,
    pub action_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowAssignmentResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub role: WorkflowAgentRole,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowRunResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub status: LifecycleRunStatus,
    pub current_step_key: Option<String>,
    pub step_states: Vec<LifecycleStepStateResponse>,
    pub record_artifacts: Vec<WorkflowRecordArtifactResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct LifecycleStepStateResponse {
    pub step_key: String,
    pub status: LifecycleStepExecutionStatus,
    pub session_binding_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowRecordArtifactResponse {
    pub id: Uuid,
    pub step_key: String,
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl From<WorkflowDefinition> for WorkflowDefinitionResponse {
    fn from(value: WorkflowDefinition) -> Self {
        Self {
            id: value.id,
            key: value.key,
            name: value.name,
            description: value.description,
            target_kind: value.target_kind,
            recommended_roles: value.recommended_roles,
            source: value.source,
            status: value.status,
            version: value.version,
            contract: value.contract.into(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<LifecycleDefinition> for LifecycleDefinitionResponse {
    fn from(value: LifecycleDefinition) -> Self {
        Self {
            id: value.id,
            key: value.key,
            name: value.name,
            description: value.description,
            target_kind: value.target_kind,
            recommended_roles: value.recommended_roles,
            source: value.source,
            status: value.status,
            version: value.version,
            entry_step_key: value.entry_step_key,
            steps: value.steps.into_iter().map(Into::into).collect(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<BuiltinWorkflowTemplateBundle> for WorkflowTemplateResponse {
    fn from(value: BuiltinWorkflowTemplateBundle) -> Self {
        Self {
            key: value.key,
            name: value.name,
            description: value.description,
            target_kind: value.target_kind,
            recommended_roles: value.recommended_roles,
            workflows: value.workflows.into_iter().map(Into::into).collect(),
            lifecycle: value.lifecycle.into(),
        }
    }
}

impl From<BuiltinWorkflowTemplate> for BuiltinWorkflowTemplateResponse {
    fn from(value: BuiltinWorkflowTemplate) -> Self {
        Self {
            key: value.key,
            name: value.name,
            description: value.description,
            contract: value.contract.into(),
        }
    }
}

impl From<BuiltinLifecycleTemplate> for BuiltinLifecycleTemplateResponse {
    fn from(value: BuiltinLifecycleTemplate) -> Self {
        Self {
            key: value.key,
            name: value.name,
            description: value.description,
            entry_step_key: value.entry_step_key,
            steps: value.steps.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WorkflowContract> for WorkflowContractResponse {
    fn from(value: WorkflowContract) -> Self {
        Self {
            injection: value.injection.into(),
            constraints: value.constraints.into_iter().map(Into::into).collect(),
            completion: value.completion.into(),
        }
    }
}

impl From<WorkflowInjectionSpec> for WorkflowInjectionResponse {
    fn from(value: WorkflowInjectionSpec) -> Self {
        Self {
            goal: value.goal,
            instructions: value.instructions,
            context_bindings: value.context_bindings.into_iter().map(Into::into).collect(),
            session_binding: value.session_binding,
        }
    }
}

impl From<WorkflowContextBinding> for WorkflowContextBindingResponse {
    fn from(value: WorkflowContextBinding) -> Self {
        Self {
            kind: value.kind,
            locator: value.locator,
            reason: value.reason,
            required: value.required,
            title: value.title,
        }
    }
}

impl From<WorkflowConstraintSpec> for WorkflowConstraintResponse {
    fn from(value: WorkflowConstraintSpec) -> Self {
        Self {
            key: value.key,
            kind: value.kind,
            description: value.description,
            payload: value.payload,
        }
    }
}

impl From<WorkflowCheckSpec> for WorkflowCheckResponse {
    fn from(value: WorkflowCheckSpec) -> Self {
        Self {
            key: value.key,
            kind: value.kind,
            description: value.description,
            payload: value.payload,
        }
    }
}

impl From<WorkflowCompletionSpec> for WorkflowCompletionResponse {
    fn from(value: WorkflowCompletionSpec) -> Self {
        Self {
            checks: value.checks.into_iter().map(Into::into).collect(),
            default_artifact_type: value.default_artifact_type,
            default_artifact_title: value.default_artifact_title,
        }
    }
}

impl From<LifecycleStepDefinition> for LifecycleStepDefinitionResponse {
    fn from(value: LifecycleStepDefinition) -> Self {
        Self {
            key: value.key,
            title: value.title,
            description: value.description,
            primary_workflow_key: value.primary_workflow_key,
            session_binding: value.session_binding,
            transition_policy: lifecycle_transition_policy_tag(&value.transition.policy.kind)
                .to_string(),
            next_step_key: value.transition.policy.next_step_key,
            session_terminal_states: value.transition.policy.session_terminal_states,
            action_key: value.transition.policy.action_key,
        }
    }
}

fn lifecycle_transition_policy_tag(
    kind: &agentdash_domain::workflow::LifecycleTransitionPolicyKind,
) -> &'static str {
    use agentdash_domain::workflow::LifecycleTransitionPolicyKind;

    match kind {
        LifecycleTransitionPolicyKind::Manual => "manual",
        LifecycleTransitionPolicyKind::AllChecksPass => "all_checks_pass",
        LifecycleTransitionPolicyKind::AnyChecksPass => "any_checks_pass",
        LifecycleTransitionPolicyKind::SessionTerminalMatches => "session_terminal_matches",
        LifecycleTransitionPolicyKind::ExplicitAction => "explicit_action",
    }
}

impl From<WorkflowAssignment> for WorkflowAssignmentResponse {
    fn from(value: WorkflowAssignment) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            lifecycle_id: value.lifecycle_id,
            role: value.role,
            enabled: value.enabled,
            is_default: value.is_default,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<LifecycleRun> for WorkflowRunResponse {
    fn from(value: LifecycleRun) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            lifecycle_id: value.lifecycle_id,
            target_kind: value.target_kind,
            target_id: value.target_id,
            status: value.status,
            current_step_key: value.current_step_key,
            step_states: value.step_states.into_iter().map(Into::into).collect(),
            record_artifacts: value.record_artifacts.into_iter().map(Into::into).collect(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_activity_at: value.last_activity_at,
        }
    }
}

impl From<LifecycleStepState> for LifecycleStepStateResponse {
    fn from(value: LifecycleStepState) -> Self {
        Self {
            step_key: value.step_key,
            status: value.status,
            session_binding_id: value.session_binding_id,
            started_at: value.started_at,
            completed_at: value.completed_at,
            summary: value.summary,
        }
    }
}

impl From<WorkflowRecordArtifact> for WorkflowRecordArtifactResponse {
    fn from(value: WorkflowRecordArtifact) -> Self {
        Self {
            id: value.id,
            step_key: value.step_key,
            artifact_type: value.artifact_type,
            title: value.title,
            content: value.content,
            created_at: value.created_at,
        }
    }
}
