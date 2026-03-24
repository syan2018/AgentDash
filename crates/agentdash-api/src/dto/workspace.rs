use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
    WorkspaceResolutionPolicy, WorkspaceStatus,
};

#[derive(Debug, Serialize)]
pub struct WorkspaceBindingResponse {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub status: WorkspaceBindingStatus,
    pub detected_facts: Value,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WorkspaceBinding> for WorkspaceBindingResponse {
    fn from(binding: WorkspaceBinding) -> Self {
        Self {
            id: binding.id,
            workspace_id: binding.workspace_id,
            backend_id: binding.backend_id,
            root_ref: binding.root_ref,
            status: binding.status,
            detected_facts: binding.detected_facts,
            last_verified_at: binding.last_verified_at,
            priority: binding.priority,
            created_at: binding.created_at,
            updated_at: binding.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub resolution_policy: WorkspaceResolutionPolicy,
    pub default_binding_id: Option<Uuid>,
    pub status: WorkspaceStatus,
    pub bindings: Vec<WorkspaceBindingResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Workspace> for WorkspaceResponse {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: workspace.id,
            project_id: workspace.project_id,
            name: workspace.name,
            identity_kind: workspace.identity_kind,
            identity_payload: workspace.identity_payload,
            resolution_policy: workspace.resolution_policy,
            default_binding_id: workspace.default_binding_id,
            status: workspace.status,
            bindings: workspace
                .bindings
                .into_iter()
                .map(WorkspaceBindingResponse::from)
                .collect(),
            created_at: workspace.created_at,
            updated_at: workspace.updated_at,
        }
    }
}
