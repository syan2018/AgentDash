use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendConfig, BackendExecutionLeaseState, BackendExecutionSelectionMode,
    BackendShareScopeKind, BackendVisibility, RuntimeHealthStatus,
};

#[derive(Deserialize)]
pub struct CreateBackendRequest {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub backend_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EnsureLocalRuntimeRequest {
    pub machine_id: String,
    pub machine_label: Option<String>,
    #[serde(default)]
    pub legacy_machine_ids: Vec<String>,
    pub profile_id: String,
    #[serde(default)]
    pub scope: Option<LocalRuntimeScopeRequest>,
    pub capability_slot: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<String>,
    #[serde(default)]
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    #[serde(default)]
    pub device: serde_json::Value,
    #[serde(default)]
    pub rotate_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct LocalRuntimeScopeRequest {
    pub kind: BackendShareScopeKind,
    pub id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnsureLocalRuntimeResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    pub backend_enabled: bool,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub visibility: BackendVisibility,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
}

#[derive(Serialize)]
pub struct BackendWithStatus {
    #[serde(flatten)]
    pub config: BackendConfig,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_health: Option<RuntimeHealthResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_roots: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<agentdash_relay::CapabilitiesPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeHealthResponse {
    pub backend_id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub status: RuntimeHealthStatus,
    pub online: bool,
    pub version: Option<String>,
    pub capabilities: serde_json::Value,
    pub workspace_roots: Vec<String>,
    pub device: serde_json::Value,
    pub connected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnect_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendRuntimeSummaryResponse {
    pub backend_id: String,
    pub name: String,
    pub enabled: bool,
    pub online: bool,
    pub runtime_health: Option<RuntimeHealthResponse>,
    pub executors: Vec<BackendRuntimeExecutorResponse>,
    pub active_session_count: usize,
    pub active_sessions: Vec<BackendActiveSessionResponse>,
    pub allocatable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendRuntimeExecutorResponse {
    pub executor_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
    pub active_session_count: usize,
    pub allocatable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendActiveSessionResponse {
    pub lease_id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub executor_id: String,
    pub workspace_id: Option<Uuid>,
    pub root_ref: Option<String>,
    pub selection_mode: BackendExecutionSelectionMode,
    pub state: BackendExecutionLeaseState,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
    pub activated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct BrowseDirectoryRequest {
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct BrowseDirectoryResponse {
    pub current_path: String,
    pub entries: Vec<BrowseDirectoryEntryResponse>,
}

#[derive(Serialize)]
pub struct BrowseDirectoryEntryResponse {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}
