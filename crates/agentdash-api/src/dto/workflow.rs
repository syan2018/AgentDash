use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowAssignment, WorkflowContextBinding, WorkflowDefinition,
    WorkflowPhaseCompletionMode, WorkflowPhaseDefinition, WorkflowPhaseExecutionStatus,
    WorkflowPhaseState, WorkflowRecordArtifact, WorkflowRecordArtifactType, WorkflowRecordPolicy,
    WorkflowRun, WorkflowRunStatus, WorkflowTargetKind,
};

#[derive(Debug, Serialize)]
pub struct WorkflowDefinitionResponse {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub version: i32,
    pub enabled: bool,
    pub phases: Vec<WorkflowPhaseDefinitionResponse>,
    pub record_policy: WorkflowRecordPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowPhaseDefinitionResponse {
    pub key: String,
    pub title: String,
    pub description: String,
    pub context_bindings: Vec<WorkflowContextBinding>,
    pub requires_session: bool,
    pub completion_mode: WorkflowPhaseCompletionMode,
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
            version: value.version,
            enabled: value.enabled,
            phases: value.phases.into_iter().map(Into::into).collect(),
            record_policy: value.record_policy,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<WorkflowPhaseDefinition> for WorkflowPhaseDefinitionResponse {
    fn from(value: WorkflowPhaseDefinition) -> Self {
        Self {
            key: value.key,
            title: value.title,
            description: value.description,
            context_bindings: value.context_bindings,
            requires_session: value.requires_session,
            completion_mode: value.completion_mode,
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
            artifact_type: value.artifact_type,
            title: value.title,
            content: value.content,
            created_at: value.created_at,
        }
    }
}
