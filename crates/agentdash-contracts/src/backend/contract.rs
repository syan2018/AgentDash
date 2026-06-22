use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Local,
    Remote,
}

impl From<agentdash_domain::backend::BackendType> for BackendType {
    fn from(value: agentdash_domain::backend::BackendType) -> Self {
        match value {
            agentdash_domain::backend::BackendType::Local => Self::Local,
            agentdash_domain::backend::BackendType::Remote => Self::Remote,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendVisibility {
    Private,
    Shared,
    System,
}

impl From<agentdash_domain::backend::BackendVisibility> for BackendVisibility {
    fn from(value: agentdash_domain::backend::BackendVisibility) -> Self {
        match value {
            agentdash_domain::backend::BackendVisibility::Private => Self::Private,
            agentdash_domain::backend::BackendVisibility::Shared => Self::Shared,
            agentdash_domain::backend::BackendVisibility::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendShareScopeKind {
    User,
    Project,
    System,
}

impl From<agentdash_domain::backend::BackendShareScopeKind> for BackendShareScopeKind {
    fn from(value: agentdash_domain::backend::BackendShareScopeKind) -> Self {
        match value {
            agentdash_domain::backend::BackendShareScopeKind::User => Self::User,
            agentdash_domain::backend::BackendShareScopeKind::Project => Self::Project,
            agentdash_domain::backend::BackendShareScopeKind::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatus {
    Online,
    Offline,
    Starting,
    Degraded,
    Stopping,
    Error,
}

impl From<agentdash_domain::backend::RuntimeHealthStatus> for RuntimeHealthStatus {
    fn from(value: agentdash_domain::backend::RuntimeHealthStatus) -> Self {
        match value {
            agentdash_domain::backend::RuntimeHealthStatus::Online => Self::Online,
            agentdash_domain::backend::RuntimeHealthStatus::Offline => Self::Offline,
            agentdash_domain::backend::RuntimeHealthStatus::Starting => Self::Starting,
            agentdash_domain::backend::RuntimeHealthStatus::Degraded => Self::Degraded,
            agentdash_domain::backend::RuntimeHealthStatus::Stopping => Self::Stopping,
            agentdash_domain::backend::RuntimeHealthStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendRuntimeHealthResponse {
    pub backend_id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub status: RuntimeHealthStatus,
    pub online: bool,
    pub version: Option<String>,
    pub capabilities: Value,
    pub device: Value,
    pub connected_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub disconnect_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendExecutorCapabilityResponse {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendMcpServerCapabilityResponse {
    pub name: String,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendCapabilitiesResponse {
    pub executors: Vec<BackendExecutorCapabilityResponse>,
    pub supports_cancel: bool,
    pub supports_discover_options: bool,
    pub mcp_servers: Vec<BackendMcpServerCapabilityResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendResponse {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub enabled: bool,
    pub backend_type: BackendType,
    pub owner_user_id: Option<String>,
    pub profile_id: Option<String>,
    pub device_id: Option<String>,
    pub machine_id: Option<String>,
    pub machine_label: Option<String>,
    pub visibility: BackendVisibility,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub device: Value,
    pub last_claimed_at: Option<DateTime<Utc>>,
}

impl From<agentdash_domain::backend::BackendConfig> for BackendResponse {
    fn from(value: agentdash_domain::backend::BackendConfig) -> Self {
        Self {
            id: value.id,
            name: value.name,
            endpoint: value.endpoint,
            enabled: value.enabled,
            backend_type: BackendType::from(value.backend_type),
            owner_user_id: value.owner_user_id,
            profile_id: value.profile_id,
            device_id: value.device_id,
            machine_id: value.machine_id,
            machine_label: value.machine_label,
            visibility: BackendVisibility::from(value.visibility),
            share_scope_kind: BackendShareScopeKind::from(value.share_scope_kind),
            share_scope_id: value.share_scope_id,
            capability_slot: value.capability_slot,
            device: value.device,
            last_claimed_at: value.last_claimed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendWithStatusResponse {
    #[serde(flatten)]
    pub backend: BackendResponse,
    pub online: bool,
    pub runtime_health: Option<BackendRuntimeHealthResponse>,
    pub capabilities: Option<BackendCapabilitiesResponse>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBackendAccessStatus {
    Active,
    Paused,
    Revoked,
}

impl From<agentdash_domain::backend::ProjectBackendAccessStatus> for ProjectBackendAccessStatus {
    fn from(value: agentdash_domain::backend::ProjectBackendAccessStatus) -> Self {
        match value {
            agentdash_domain::backend::ProjectBackendAccessStatus::Active => Self::Active,
            agentdash_domain::backend::ProjectBackendAccessStatus::Paused => Self::Paused,
            agentdash_domain::backend::ProjectBackendAccessStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBackendAccessMode {
    ExplicitGrant,
}

impl From<agentdash_domain::backend::ProjectBackendAccessMode> for ProjectBackendAccessMode {
    fn from(value: agentdash_domain::backend::ProjectBackendAccessMode) -> Self {
        match value {
            agentdash_domain::backend::ProjectBackendAccessMode::ExplicitGrant => {
                Self::ExplicitGrant
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateProjectBackendAccessRequest {
    pub backend_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<i32>,
    #[serde(default)]
    #[ts(optional, type = "{ [key in string]?: JsonValue }")]
    pub root_policy: Option<Value>,
    #[serde(default)]
    #[ts(optional, type = "{ [key in string]?: JsonValue }")]
    pub capability_policy: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct UpdateProjectBackendAccessRequest {
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<ProjectBackendAccessStatus>,
    #[serde(default)]
    #[ts(optional)]
    pub access_mode: Option<ProjectBackendAccessMode>,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<i32>,
    #[serde(default)]
    #[ts(optional, type = "{ [key in string]?: JsonValue }")]
    pub root_policy: Option<Value>,
    #[serde(default)]
    #[ts(optional, type = "{ [key in string]?: JsonValue }")]
    pub capability_policy: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectBackendAccessResponse {
    pub id: String,
    pub project_id: String,
    pub backend_id: String,
    pub status: ProjectBackendAccessStatus,
    pub access_mode: ProjectBackendAccessMode,
    pub priority: i32,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub root_policy: Value,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub capability_policy: Value,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<agentdash_domain::backend::ProjectBackendAccess> for ProjectBackendAccessResponse {
    fn from(value: agentdash_domain::backend::ProjectBackendAccess) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            backend_id: value.backend_id,
            status: ProjectBackendAccessStatus::from(value.status),
            access_mode: ProjectBackendAccessMode::from(value.access_mode),
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendWorkspaceInventoryStatus {
    Available,
    Stale,
    Offline,
    Error,
}

impl From<agentdash_domain::backend::BackendWorkspaceInventoryStatus>
    for BackendWorkspaceInventoryStatus
{
    fn from(value: agentdash_domain::backend::BackendWorkspaceInventoryStatus) -> Self {
        match value {
            agentdash_domain::backend::BackendWorkspaceInventoryStatus::Available => {
                Self::Available
            }
            agentdash_domain::backend::BackendWorkspaceInventoryStatus::Stale => Self::Stale,
            agentdash_domain::backend::BackendWorkspaceInventoryStatus::Offline => Self::Offline,
            agentdash_domain::backend::BackendWorkspaceInventoryStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendWorkspaceInventorySource {
    ManualRegister,
}

impl From<agentdash_domain::backend::BackendWorkspaceInventorySource>
    for BackendWorkspaceInventorySource
{
    fn from(value: agentdash_domain::backend::BackendWorkspaceInventorySource) -> Self {
        match value {
            agentdash_domain::backend::BackendWorkspaceInventorySource::ManualRegister => {
                Self::ManualRegister
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendWorkspaceInventoryResponse {
    pub id: String,
    pub backend_id: String,
    pub root_ref: String,
    #[ts(type = "\"git_repo\" | \"p4_workspace\" | \"local_dir\"")]
    pub identity_kind: crate::workspace::WorkspaceIdentityKind,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub identity_payload: Value,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub detected_facts: Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub source: BackendWorkspaceInventorySource,
    pub last_seen_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<agentdash_domain::backend::BackendWorkspaceInventory>
    for BackendWorkspaceInventoryResponse
{
    fn from(value: agentdash_domain::backend::BackendWorkspaceInventory) -> Self {
        Self {
            id: value.id.to_string(),
            backend_id: value.backend_id,
            root_ref: value.root_ref,
            identity_kind: crate::workspace::WorkspaceIdentityKind::from(value.identity_kind),
            identity_payload: value.identity_payload,
            detected_facts: value.detected_facts,
            status: BackendWorkspaceInventoryStatus::from(value.status),
            source: BackendWorkspaceInventorySource::from(value.source),
            last_seen_at: value.last_seen_at,
            last_error: value.last_error,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct RegisterBackendWorkspaceInventoryRequest {
    pub root_ref: String,
}
