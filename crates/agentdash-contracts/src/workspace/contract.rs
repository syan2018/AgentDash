use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::backend::BackendWorkspaceInventoryStatus;
use crate::context::VfsCapabilityDto;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIdentityKind {
    GitRepo,
    P4Workspace,
    LocalDir,
}

impl From<agentdash_domain::workspace::WorkspaceIdentityKind> for WorkspaceIdentityKind {
    fn from(value: agentdash_domain::workspace::WorkspaceIdentityKind) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceIdentityKind::GitRepo => Self::GitRepo,
            agentdash_domain::workspace::WorkspaceIdentityKind::P4Workspace => Self::P4Workspace,
            agentdash_domain::workspace::WorkspaceIdentityKind::LocalDir => Self::LocalDir,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBindingStatus {
    Pending,
    Ready,
    Offline,
    Error,
}

impl From<agentdash_domain::workspace::WorkspaceBindingStatus> for WorkspaceBindingStatus {
    fn from(value: agentdash_domain::workspace::WorkspaceBindingStatus) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceBindingStatus::Pending => Self::Pending,
            agentdash_domain::workspace::WorkspaceBindingStatus::Ready => Self::Ready,
            agentdash_domain::workspace::WorkspaceBindingStatus::Offline => Self::Offline,
            agentdash_domain::workspace::WorkspaceBindingStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceResolutionPolicy {
    PreferDefaultBinding,
    PreferOnline,
}

impl From<agentdash_domain::workspace::WorkspaceResolutionPolicy> for WorkspaceResolutionPolicy {
    fn from(value: agentdash_domain::workspace::WorkspaceResolutionPolicy) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferDefaultBinding => {
                Self::PreferDefaultBinding
            }
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferOnline => {
                Self::PreferOnline
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Pending,
    Ready,
    Active,
    Archived,
    Error,
}

impl From<agentdash_domain::workspace::WorkspaceStatus> for WorkspaceStatus {
    fn from(value: agentdash_domain::workspace::WorkspaceStatus) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceStatus::Pending => Self::Pending,
            agentdash_domain::workspace::WorkspaceStatus::Ready => Self::Ready,
            agentdash_domain::workspace::WorkspaceStatus::Active => Self::Active,
            agentdash_domain::workspace::WorkspaceStatus::Archived => Self::Archived,
            agentdash_domain::workspace::WorkspaceStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceBindingResponse {
    pub id: String,
    pub workspace_id: String,
    pub backend_id: String,
    pub root_ref: String,
    pub status: WorkspaceBindingStatus,
    pub detected_facts: Value,
    pub last_verified_at: Option<String>,
    pub priority: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::workspace::WorkspaceBinding> for WorkspaceBindingResponse {
    fn from(value: agentdash_domain::workspace::WorkspaceBinding) -> Self {
        Self {
            id: value.id.to_string(),
            workspace_id: value.workspace_id.to_string(),
            backend_id: value.backend_id,
            root_ref: value.root_ref,
            status: WorkspaceBindingStatus::from(value.status),
            detected_facts: value.detected_facts,
            last_verified_at: value.last_verified_at.map(|time| time.to_rfc3339()),
            priority: value.priority,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub resolution_policy: WorkspaceResolutionPolicy,
    pub default_binding_id: Option<String>,
    pub status: WorkspaceStatus,
    pub bindings: Vec<WorkspaceBindingResponse>,
    pub mount_capabilities: Vec<VfsCapabilityDto>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::workspace::Workspace> for WorkspaceResponse {
    fn from(value: agentdash_domain::workspace::Workspace) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            name: value.name,
            identity_kind: WorkspaceIdentityKind::from(value.identity_kind),
            identity_payload: value.identity_payload,
            resolution_policy: WorkspaceResolutionPolicy::from(value.resolution_policy),
            default_binding_id: value.default_binding_id.map(|id| id.to_string()),
            status: WorkspaceStatus::from(value.status),
            bindings: value
                .bindings
                .into_iter()
                .map(WorkspaceBindingResponse::from)
                .collect(),
            mount_capabilities: value
                .mount_capabilities
                .into_iter()
                .map(VfsCapabilityDto::from)
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceInventoryCandidate {
    pub backend_id: String,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKind,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub identity_payload: Value,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub detected_facts: Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub matched_workspace_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceBindingSyncResult {
    pub updated_workspace_ids: Vec<String>,
    pub created_bindings: usize,
    pub updated_bindings: usize,
    pub candidates: Vec<WorkspaceInventoryCandidate>,
    pub conflicts: Vec<WorkspaceInventoryCandidate>,
}
