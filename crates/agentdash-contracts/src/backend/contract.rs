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
    IdentityDiscovery,
}

impl From<agentdash_domain::backend::BackendWorkspaceInventorySource>
    for BackendWorkspaceInventorySource
{
    fn from(value: agentdash_domain::backend::BackendWorkspaceInventorySource) -> Self {
        match value {
            agentdash_domain::backend::BackendWorkspaceInventorySource::ManualRegister => {
                Self::ManualRegister
            }
            agentdash_domain::backend::BackendWorkspaceInventorySource::IdentityDiscovery => {
                Self::IdentityDiscovery
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerRegistrationTokenStatus {
    Active,
    Expired,
    Revoked,
}

impl From<agentdash_domain::backend::RunnerRegistrationTokenStatus>
    for RunnerRegistrationTokenStatus
{
    fn from(value: agentdash_domain::backend::RunnerRegistrationTokenStatus) -> Self {
        match value {
            agentdash_domain::backend::RunnerRegistrationTokenStatus::Active => Self::Active,
            agentdash_domain::backend::RunnerRegistrationTokenStatus::Expired => Self::Expired,
            agentdash_domain::backend::RunnerRegistrationTokenStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct RunnerRegistrationTokenCreateRequest {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[ts(optional)]
    pub default_capability_slot: Option<String>,
    #[serde(default)]
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub machine_policy: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RunnerRegistrationTokenMetadataResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub token_prefix: String,
    pub status: RunnerRegistrationTokenStatus,
    pub created_by_user_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_claimed_backend_id: Option<String>,
    pub default_capability_slot: String,
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub machine_policy: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<agentdash_domain::backend::RunnerRegistrationToken>
    for RunnerRegistrationTokenMetadataResponse
{
    fn from(value: agentdash_domain::backend::RunnerRegistrationToken) -> Self {
        let status = RunnerRegistrationTokenStatus::from(value.status_at(Utc::now()));
        Self {
            id: value.id,
            project_id: value.project_id.to_string(),
            name: value.name,
            token_prefix: value.token_prefix,
            status,
            created_by_user_id: value.created_by_user_id,
            expires_at: value.expires_at,
            revoked_at: value.revoked_at,
            last_used_at: value.last_used_at,
            last_claimed_backend_id: value.last_claimed_backend_id,
            default_capability_slot: value.default_capability_slot,
            machine_policy: value.machine_policy,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RunnerRegistrationTokenCreateResponse {
    pub token: RunnerRegistrationTokenMetadataResponse,
    pub registration_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RunnerRegistrationTokenRotateResponse {
    pub token: RunnerRegistrationTokenMetadataResponse,
    pub registration_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RunnerRegistrationTokenRevokeResponse {
    pub token: RunnerRegistrationTokenMetadataResponse,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct RunnerRegistrationClaimRequest {
    #[serde(default)]
    #[ts(optional)]
    pub registration_token: Option<String>,
    pub machine_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub machine_label: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub runner_name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub client_version: Option<String>,
    #[serde(default)]
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub device: Value,
    #[serde(default)]
    pub executor_enabled: bool,
    #[serde(default)]
    #[ts(optional)]
    pub capability_slot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RunnerRegistrationClaimResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    pub machine_id: String,
    pub machine_label: String,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub registration_source: String,
    pub claimed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::RunnerRegistrationToken;
    use agentdash_domain::project::Project;

    #[test]
    fn runner_registration_metadata_response_does_not_expose_secrets() {
        let issued = RunnerRegistrationToken::new_project_scoped(
            Project::new("Runner Project".to_string(), String::new()).id,
            "CI runner".to_string(),
            "user-owner".to_string(),
            Utc::now() + chrono::Duration::hours(1),
            "default".to_string(),
            serde_json::json!({}),
        );

        let value =
            serde_json::to_value(RunnerRegistrationTokenMetadataResponse::from(issued.token))
                .expect("metadata response should serialize");

        let object = value.as_object().expect("metadata response object");
        assert!(object.contains_key("token_prefix"));
        assert!(!object.contains_key("registration_token"));
        assert!(!object.contains_key("token_secret_hash"));
        assert!(!object.contains_key("secret"));
        assert!(!object.contains_key("auth_token"));
        assert!(
            !value
                .to_string()
                .contains(issued.registration_token.as_str())
        );
    }

    #[test]
    fn runner_registration_management_responses_only_return_plaintext_on_create_or_rotate() {
        let issued = RunnerRegistrationToken::new_project_scoped(
            Project::new("Runner Project".to_string(), String::new()).id,
            "CI runner".to_string(),
            "user-owner".to_string(),
            Utc::now() + chrono::Duration::hours(1),
            "default".to_string(),
            serde_json::json!({}),
        );
        let metadata = RunnerRegistrationTokenMetadataResponse::from(issued.token.clone());

        let create = serde_json::to_value(RunnerRegistrationTokenCreateResponse {
            token: metadata.clone(),
            registration_token: issued.registration_token.clone(),
        })
        .expect("create response should serialize");
        assert_eq!(create["registration_token"], issued.registration_token);
        assert!(create["token"].get("token_secret_hash").is_none());
        assert!(create["token"].get("auth_token").is_none());

        let rotate = serde_json::to_value(RunnerRegistrationTokenRotateResponse {
            token: metadata.clone(),
            registration_token: issued.registration_token.clone(),
        })
        .expect("rotate response should serialize");
        assert_eq!(rotate["registration_token"], issued.registration_token);

        let revoke =
            serde_json::to_value(RunnerRegistrationTokenRevokeResponse { token: metadata })
                .expect("revoke response should serialize");
        assert!(revoke.get("registration_token").is_none());
        assert!(revoke["token"].get("token_secret_hash").is_none());
        assert!(revoke["token"].get("auth_token").is_none());
    }
}
