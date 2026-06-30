use serde::{Deserialize, Serialize};

use agentdash_contracts::backend::{
    BackendCapabilitiesResponse, BackendExecutorCapabilityResponse,
    BackendMcpServerCapabilityResponse, BackendResponse, BackendRuntimeHealthResponse,
    BackendWithStatusResponse,
};
use agentdash_domain::backend::{BackendConfig, BackendShareScopeKind, BackendVisibility};

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
    pub profile_id: String,
    #[serde(default)]
    pub scope: Option<LocalRuntimeScopeRequest>,
    pub capability_slot: Option<String>,
    pub name: Option<String>,
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
    // 与 RunnerRegistrationClaimResponse 同构的核心字段，让两条 enrollment 路径
    // 共享 registration source 与 claim 时间语义。
    pub registration_source: String,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
}

pub type BackendWithStatus = BackendWithStatusResponse;
pub type RuntimeHealthResponse = BackendRuntimeHealthResponse;

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

pub fn backend_response(config: BackendConfig) -> BackendResponse {
    BackendResponse::from(config)
}

pub fn backend_capabilities_response(
    value: agentdash_relay::CapabilitiesPayload,
) -> BackendCapabilitiesResponse {
    BackendCapabilitiesResponse {
        executors: value
            .executors
            .into_iter()
            .map(|executor| BackendExecutorCapabilityResponse {
                id: executor.id,
                name: executor.name,
                variants: executor.variants,
                available: executor.available,
            })
            .collect(),
        supports_cancel: value.supports_cancel,
        supports_discover_options: value.supports_discover_options,
        mcp_servers: value
            .mcp_servers
            .into_iter()
            .map(|server| BackendMcpServerCapabilityResponse {
                name: server.name,
                transport: server.transport,
            })
            .collect(),
    }
}
