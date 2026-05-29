use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendWorkspaceInventory, BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus,
    ProjectBackendAccess, ProjectBackendAccessMode, ProjectBackendAccessStatus,
};
use agentdash_domain::workspace::WorkspaceIdentityKind;

#[derive(Debug, Deserialize)]
pub struct CreateProjectBackendAccessRequest {
    pub backend_id: String,
    pub priority: Option<i32>,
    pub root_policy: Option<Value>,
    pub capability_policy: Option<Value>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectBackendAccessRequest {
    pub status: Option<ProjectBackendAccessStatus>,
    pub access_mode: Option<ProjectBackendAccessMode>,
    pub priority: Option<i32>,
    pub root_policy: Option<Value>,
    pub capability_policy: Option<Value>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectBackendAccessResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub backend_id: String,
    pub status: ProjectBackendAccessStatus,
    pub access_mode: ProjectBackendAccessMode,
    pub priority: i32,
    pub root_policy: Value,
    pub capability_policy: Value,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<ProjectBackendAccess> for ProjectBackendAccessResponse {
    fn from(value: ProjectBackendAccess) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            backend_id: value.backend_id,
            status: value.status,
            access_mode: value.access_mode,
            priority: value.priority,
            root_policy: value.root_policy,
            capability_policy: value.capability_policy,
            note: value.note,
            created_by: value.created_by,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BackendWorkspaceInventoryResponse {
    pub id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub detected_facts: Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub source: BackendWorkspaceInventorySource,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<BackendWorkspaceInventory> for BackendWorkspaceInventoryResponse {
    fn from(value: BackendWorkspaceInventory) -> Self {
        Self {
            id: value.id,
            backend_id: value.backend_id,
            root_ref: value.root_ref,
            identity_kind: value.identity_kind,
            identity_payload: value.identity_payload,
            detected_facts: value.detected_facts,
            status: value.status,
            source: value.source,
            last_seen_at: value.last_seen_at,
            last_error: value.last_error,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InventoryRefreshResponse {
    pub access_id: Uuid,
    pub backend_id: String,
    pub refreshed: usize,
    pub failed: usize,
    pub items: Vec<BackendWorkspaceInventoryResponse>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseAccessDirectoryRequest {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterBackendWorkspaceInventoryRequest {
    pub root_ref: String,
}
