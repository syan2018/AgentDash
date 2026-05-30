use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::common::MountCapability;
use agentdash_domain::workspace::{
    WorkspaceBindingStatus, WorkspaceIdentityKind, WorkspaceResolutionPolicy, WorkspaceStatus,
};

use crate::dto::WorkspaceBindingResponse;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceBindingInput {
    pub id: Option<Uuid>,
    pub backend_id: String,
    pub root_ref: String,
    pub status: Option<WorkspaceBindingStatus>,
    pub detected_facts: Option<Value>,
    pub priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: Option<WorkspaceResolutionPolicy>,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Option<Vec<WorkspaceBindingInput>>,
    pub shortcut_binding: Option<WorkspaceBindingInput>,
    pub mount_capabilities: Option<Vec<MountCapability>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: Option<WorkspaceResolutionPolicy>,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Option<Vec<WorkspaceBindingInput>>,
    pub mount_capabilities: Option<Vec<MountCapability>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceStatusRequest {
    pub status: WorkspaceStatus,
}

#[derive(Debug, Deserialize)]
pub struct DetectWorkspaceRequest {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Serialize)]
pub struct DetectWorkspaceResponse {
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub binding: WorkspaceBindingResponse,
    pub confidence: String,
    pub warnings: Vec<String>,
    pub matched_workspace_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DetectGitRequest {
    pub root_ref: String,
    pub backend_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectGitResponse {
    pub resolved_root_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}
