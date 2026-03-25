use agentdash_application::workflow::BuiltinWorkflowTemplate;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::workflow::{
    ValidationIssue, WorkflowAgentRole, WorkflowAssignment, WorkflowContextBinding,
    WorkflowContextBindingKind, WorkflowDefinition, WorkflowDefinitionSource,
    WorkflowDefinitionStatus, WorkflowPhaseCompletionMode, WorkflowPhaseDefinition,
    WorkflowPhaseExecutionStatus, WorkflowPhaseState, WorkflowRecordArtifact,
    WorkflowRecordArtifactType, WorkflowRecordPolicy, WorkflowRun, WorkflowRunStatus,
    WorkflowTargetKind,
};

#[derive(Debug, Serialize)]
pub struct WorkflowDefinitionResponse {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub recommended_role: Option<WorkflowAgentRole>,
    pub source: WorkflowDefinitionSource,
    pub status: WorkflowDefinitionStatus,
    pub version: i32,
    pub phases: Vec<WorkflowPhaseDefinitionResponse>,
    pub record_policy: WorkflowRecordPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 校验结果 DTO。
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
    pub recommended_role: WorkflowAgentRole,
    pub phases: Vec<WorkflowPhaseDefinitionResponse>,
    pub record_policy: WorkflowRecordPolicy,
}

#[derive(Debug, Serialize)]
pub struct WorkflowPhaseDefinitionResponse {
    pub key: String,
    pub title: String,
    pub description: String,
    pub agent_instructions: Vec<String>,
    pub context_bindings: Vec<WorkflowContextBindingResponse>,
    pub requires_session: bool,
    pub completion_mode: WorkflowPhaseCompletionMode,
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    pub default_artifact_title: Option<String>,
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
pub struct WorkflowAssignmentResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workflow_id: Uuid,
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
    pub workflow_id: Uuid,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub status: WorkflowRunStatus,
    pub current_phase_key: Option<String>,
    pub phase_states: Vec<WorkflowPhaseStateResponse>,
    pub record_artifacts: Vec<WorkflowRecordArtifactResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowPhaseStateResponse {
    pub phase_key: String,
    pub status: WorkflowPhaseExecutionStatus,
    pub session_binding_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowRecordArtifactResponse {
    pub id: Uuid,
    pub phase_key: String,
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
            recommended_role: value.recommended_role,
            source: value.source,
            status: value.status,
            version: value.version,
            phases: value.phases.into_iter().map(Into::into).collect(),
            record_policy: value.record_policy,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<BuiltinWorkflowTemplate> for WorkflowTemplateResponse {
    fn from(value: BuiltinWorkflowTemplate) -> Self {
        Self {
            key: value.key,
            name: value.name,
            description: value.description,
            target_kind: value.target_kind,
            recommended_role: value.recommended_role,
            phases: value.phases.into_iter().map(Into::into).collect(),
            record_policy: value.record_policy,
        }
    }
}

impl From<WorkflowPhaseDefinition> for WorkflowPhaseDefinitionResponse {
    fn from(value: WorkflowPhaseDefinition) -> Self {
        Self {
            key: value.key,
            title: value.title,
            description: value.description,
            agent_instructions: value.agent_instructions,
            context_bindings: value.context_bindings.into_iter().map(Into::into).collect(),
            requires_session: value.requires_session,
            completion_mode: value.completion_mode,
            default_artifact_type: value.default_artifact_type,
            default_artifact_title: value.default_artifact_title,
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

impl From<WorkflowAssignment> for WorkflowAssignmentResponse {
    fn from(value: WorkflowAssignment) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            workflow_id: value.workflow_id,
            role: value.role,
            enabled: value.enabled,
            is_default: value.is_default,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<WorkflowRun> for WorkflowRunResponse {
    fn from(value: WorkflowRun) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            workflow_id: value.workflow_id,
            target_kind: value.target_kind,
            target_id: value.target_id,
            status: value.status,
            current_phase_key: value.current_phase_key,
            phase_states: value.phase_states.into_iter().map(Into::into).collect(),
            record_artifacts: value.record_artifacts.into_iter().map(Into::into).collect(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_activity_at: value.last_activity_at,
        }
    }
}

impl From<WorkflowPhaseState> for WorkflowPhaseStateResponse {
    fn from(value: WorkflowPhaseState) -> Self {
        Self {
            phase_key: value.phase_key,
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
            phase_key: value.phase_key,
            artifact_type: value.artifact_type,
            title: value.title,
            content: value.content,
            created_at: value.created_at,
        }
    }
}
