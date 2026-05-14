use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendType, LocalBackendClaim, RuntimeHealth,
    RuntimeHealthStatus,
};

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::relay::registry::OnlineBackendInfo;
use crate::rpc::ApiError;
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput,
};
use agentdash_application::session::context::normalize_optional_string;

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
    pub device_id: String,
    pub profile_id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub accessible_roots: Vec<String>,
    #[serde(default)]
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    #[serde(default)]
    pub device: serde_json::Value,
    #[serde(default)]
    pub rotate_token: bool,
}

#[derive(Debug, Serialize)]
pub struct EnsureLocalRuntimeResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    pub backend_enabled: bool,
    pub profile_id: String,
    pub device_id: String,
}

#[derive(Serialize)]
pub struct BackendWithStatus {
    #[serde(flatten)]
    pub config: BackendConfig,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_health: Option<RuntimeHealthResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessible_roots: Option<Vec<String>>,
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
    pub accessible_roots: Vec<String>,
    pub device: serde_json::Value,
    pub connected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnect_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BackendWithStatus>>, ApiError> {
    let backends = state.repos.backend_repo.list_backends().await?;
    let online_list = state.services.backend_registry.list_online().await;
    let runtime_health = state
        .repos
        .runtime_health_repo
        .list_runtime_health()
        .await?;
    let runtime_health_by_backend = runtime_health
        .into_iter()
        .map(|health| (health.backend_id.clone(), health))
        .collect::<std::collections::HashMap<_, _>>();
    let mut result = Vec::with_capacity(backends.len() + online_list.len());

    let mut seen_ids = std::collections::HashSet::new();

    for b in backends {
        seen_ids.insert(b.id.clone());
        let online_info = online_list.iter().find(|o| o.backend_id == b.id);
        let runtime_health = runtime_health_by_backend
            .get(&b.id)
            .cloned()
            .map(|health| runtime_health_response(health, online_info.is_some()));
        result.push(BackendWithStatus {
            online: online_info.is_some(),
            runtime_health,
            accessible_roots: online_info.map(|o| o.accessible_roots.clone()),
            capabilities: online_info.map(|o| o.capabilities.clone()),
            config: b,
        });
    }

    for o in &online_list {
        if seen_ids.contains(&o.backend_id) {
            continue;
        }
        let runtime_health = runtime_health_by_backend
            .get(&o.backend_id)
            .cloned()
            .map(|health| runtime_health_response(health, true));
        result.push(BackendWithStatus {
            online: true,
            runtime_health,
            accessible_roots: Some(o.accessible_roots.clone()),
            capabilities: Some(o.capabilities.clone()),
            config: BackendConfig {
                id: o.backend_id.clone(),
                name: o.name.clone(),
                endpoint: String::new(),
                auth_token: None,
                enabled: true,
                backend_type: BackendType::Remote,
                owner_user_id: None,
                profile_id: None,
                device_id: None,
                device: serde_json::json!({}),
                last_claimed_at: None,
            },
        });
    }

    Ok(Json(result))
}

pub async fn list_runtime_health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<RuntimeHealthResponse>>, ApiError> {
    let online_ids = state
        .services
        .backend_registry
        .list_online_ids()
        .await
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let items = state
        .repos
        .runtime_health_repo
        .list_runtime_health()
        .await?
        .into_iter()
        .map(|health| {
            let online = online_ids.contains(&health.backend_id);
            runtime_health_response(health, online)
        })
        .collect();
    Ok(Json(items))
}

pub async fn get_runtime_health(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RuntimeHealthResponse>, ApiError> {
    let health = state
        .repos
        .runtime_health_repo
        .get_runtime_health(&id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "runtime_health",
            id: id.clone(),
        })?;
    let online = state.services.backend_registry.is_online(&id).await;
    Ok(Json(runtime_health_response(health, online)))
}

/// 列出通过 WebSocket 连接的在线后端
pub async fn list_online_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<OnlineBackendInfo>>, ApiError> {
    let online = state.services.backend_registry.list_online().await;
    Ok(Json(online))
}

fn runtime_health_response(health: RuntimeHealth, online: bool) -> RuntimeHealthResponse {
    RuntimeHealthResponse {
        backend_id: health.backend_id,
        profile_id: health.profile_id,
        name: health.name,
        status: health.status,
        online,
        version: health.version,
        capabilities: health.capabilities,
        accessible_roots: health.accessible_roots,
        device: health.device,
        connected_at: health.connected_at,
        last_seen_at: health.last_seen_at,
        disconnected_at: health.disconnected_at,
        disconnect_reason: health.disconnect_reason,
        created_at: health.created_at,
        updated_at: health.updated_at,
    }
}

pub async fn get_backend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BackendConfig>, ApiError> {
    let backend = state.repos.backend_repo.get_backend(&id).await?;
    Ok(Json(backend))
}

pub async fn add_backend(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBackendRequest>,
) -> Result<Json<BackendConfig>, ApiError> {
    let id = req.id.trim();
    if id.is_empty() {
        return Err(ApiError::BadRequest("backend id 不能为空".into()));
    }

    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("backend name 不能为空".into()));
    }

    let endpoint = req.endpoint.trim().to_string();
    let requested_token = normalize_optional_string(req.auth_token);
    let existing = match state.repos.backend_repo.get_backend(id).await {
        Ok(config) => Some(config),
        Err(DomainError::NotFound { .. }) => None,
        Err(err) => {
            return Err(ApiError::Internal(format!("读取 Backend 配置失败: {err}")));
        }
    };
    let auth_token = resolve_backend_auth_token(
        state.repos.backend_repo.as_ref(),
        id,
        requested_token,
        existing.as_ref(),
    )
    .await?;

    let config = BackendConfig {
        id: id.to_string(),
        name: name.to_string(),
        endpoint,
        auth_token: Some(auth_token),
        enabled: existing.as_ref().map(|item| item.enabled).unwrap_or(true),
        backend_type: match req.backend_type.as_deref() {
            Some("remote") => BackendType::Remote,
            _ => BackendType::Local,
        },
        owner_user_id: None, // TODO: 从 CurrentUser 提取
        profile_id: existing.as_ref().and_then(|item| item.profile_id.clone()),
        device_id: existing.as_ref().and_then(|item| item.device_id.clone()),
        device: existing
            .as_ref()
            .map(|item| item.device.clone())
            .unwrap_or_else(|| serde_json::json!({})),
        last_claimed_at: existing.as_ref().and_then(|item| item.last_claimed_at),
    };
    state.repos.backend_repo.add_backend(&config).await?;
    Ok(Json(config))
}

pub async fn ensure_local_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<EnsureLocalRuntimeRequest>,
) -> Result<Json<EnsureLocalRuntimeResponse>, ApiError> {
    let profile_id = normalize_required("profile_id", &req.profile_id)?;
    let device_id = normalize_required("device_id", &req.device_id)?;
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_local_runtime_name(&device_id));

    let relay_ws_url = relay_ws_url_from_headers(&headers);
    let backend_id = stable_local_backend_id(&current_user.user_id, &profile_id, &device_id);
    let mut device = normalize_device_payload(req.device)?;
    if let Some(client_version) = normalize_optional_string(req.client_version) {
        device["client_version"] = serde_json::Value::String(client_version);
    }
    device["executor_enabled"] = serde_json::Value::Bool(req.executor_enabled);
    device["accessible_root_count"] =
        serde_json::Value::Number(serde_json::Number::from(req.accessible_roots.len() as u64));

    let claim = LocalBackendClaim {
        owner_user_id: current_user.user_id.clone(),
        profile_id: profile_id.clone(),
        device_id: device_id.clone(),
        backend_id,
        name,
        endpoint: relay_ws_url.clone(),
        auth_token: generate_backend_auth_token(),
        device,
        rotate_token: req.rotate_token,
    };

    let backend = state
        .repos
        .backend_repo
        .ensure_local_backend(&claim)
        .await?;
    let auth_token = normalize_optional_string(backend.auth_token.clone()).ok_or_else(|| {
        ApiError::Internal(format!(
            "本机 backend `{}` 缺少 server 颁发的 relay token",
            backend.id
        ))
    })?;

    Ok(Json(EnsureLocalRuntimeResponse {
        backend_id: backend.id,
        name: backend.name,
        relay_ws_url: backend.endpoint,
        auth_token,
        backend_enabled: backend.enabled,
        profile_id,
        device_id,
    }))
}

async fn resolve_backend_auth_token(
    backend_repo: &dyn BackendRepository,
    backend_id: &str,
    requested_token: Option<String>,
    existing: Option<&BackendConfig>,
) -> Result<String, ApiError> {
    if let Some(token) = requested_token {
        return Ok(token);
    }

    if let Some(config) = existing
        && let Some(token) = normalize_optional_string(config.auth_token.clone())
    {
        return Ok(token);
    }

    match backend_repo.get_backend(backend_id).await {
        Ok(config) => Ok(normalize_optional_string(config.auth_token)
            .unwrap_or_else(generate_backend_auth_token)),
        Err(DomainError::NotFound { .. }) => Ok(generate_backend_auth_token()),
        Err(err) => Err(ApiError::Internal(format!(
            "读取 Backend token 失败: {err}"
        ))),
    }
}

fn generate_backend_auth_token() -> String {
    Uuid::new_v4().to_string()
}

fn normalize_required(field: &str, raw: &str) -> Result<String, ApiError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

fn normalize_device_payload(value: serde_json::Value) -> Result<serde_json::Value, ApiError> {
    match value {
        serde_json::Value::Null => Ok(serde_json::json!({})),
        serde_json::Value::Object(_) => Ok(value),
        _ => Err(ApiError::BadRequest(
            "device 必须是 JSON object 或 null".to_string(),
        )),
    }
}

fn default_local_runtime_name(device_id: &str) -> String {
    let suffix = device_id
        .rsplit([':', '/', '\\'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("desktop");
    format!("AgentDash Desktop ({suffix})")
}

fn stable_local_backend_id(owner_user_id: &str, profile_id: &str, device_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(owner_user_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(profile_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(device_id.as_bytes());
    let digest = hasher.finalize();
    format!("local_{}", hex_prefix(&digest, 24))
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte >> 4) as usize] as char);
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn relay_ws_url_from_headers(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:3001");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim())
        .unwrap_or("http");
    let ws_scheme = if proto.eq_ignore_ascii_case("https") {
        "wss"
    } else {
        "ws"
    };
    format!("{ws_scheme}://{host}/ws/backend")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::{UserPreferences, ViewConfig};

    struct MockBackendRepository {
        existing: Option<BackendConfig>,
    }

    #[async_trait::async_trait]
    impl BackendRepository for MockBackendRepository {
        async fn add_backend(&self, _config: &BackendConfig) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
            self.existing
                .clone()
                .filter(|item| item.id == id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "backend",
                    id: id.to_string(),
                })
        }

        async fn get_backend_by_auth_token(
            &self,
            _token: &str,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
        }

        async fn ensure_local_backend(
            &self,
            _claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
        }

        async fn remove_backend(&self, _id: &str) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_preferences(&self, _prefs: &UserPreferences) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }
    }

    fn backend(id: &str, token: Option<&str>) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: "backend".to_string(),
            endpoint: String::new(),
            auth_token: token.map(str::to_string),
            enabled: true,
            backend_type: BackendType::Local,
            owner_user_id: None,
            profile_id: None,
            device_id: None,
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }

    #[tokio::test]
    async fn resolve_backend_auth_token_prefers_requested_token() {
        let repo = MockBackendRepository {
            existing: Some(backend("local-a", Some("persisted-token"))),
        };

        let token = resolve_backend_auth_token(
            &repo,
            "local-a",
            Some("manual-token".to_string()),
            repo.existing.as_ref(),
        )
        .await
        .expect("应能返回手动 token");

        assert_eq!(token, "manual-token");
    }

    #[tokio::test]
    async fn resolve_backend_auth_token_reuses_existing_token() {
        let repo = MockBackendRepository {
            existing: Some(backend("local-a", Some("persisted-token"))),
        };

        let token = resolve_backend_auth_token(&repo, "local-a", None, repo.existing.as_ref())
            .await
            .expect("应能复用已存在 token");

        assert_eq!(token, "persisted-token");
    }

    #[tokio::test]
    async fn resolve_backend_auth_token_generates_when_missing() {
        let repo = MockBackendRepository {
            existing: Some(backend("local-a", None)),
        };

        let token = resolve_backend_auth_token(&repo, "local-a", None, repo.existing.as_ref())
            .await
            .expect("应能生成 token");

        assert!(!token.trim().is_empty());
        assert_ne!(token, "persisted-token");
    }

    #[test]
    fn stable_local_backend_id_is_deterministic_and_scoped() {
        let first = stable_local_backend_id("user-a", "profile-a", "device-a");
        let again = stable_local_backend_id("user-a", "profile-a", "device-a");
        let other_user = stable_local_backend_id("user-b", "profile-a", "device-a");

        assert_eq!(first, again);
        assert_ne!(first, other_user);
        assert!(first.starts_with("local_"));
    }

    #[test]
    fn relay_ws_url_prefers_forwarded_https() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-host", "dash.example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        assert_eq!(
            relay_ws_url_from_headers(&headers),
            "wss://dash.example.com/ws/backend"
        );
    }

    #[test]
    fn normalize_device_payload_rejects_non_object() {
        assert!(normalize_device_payload(serde_json::json!("windows")).is_err());
        assert!(normalize_device_payload(serde_json::json!({ "os": "windows" })).is_ok());
    }
}

pub async fn remove_backend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.repos.backend_repo.remove_backend(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

// ─── 目录浏览 ─────────────────────────────────────────────

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

pub async fn browse_directory(
    State(state): State<Arc<AppState>>,
    Path(backend_id): Path<String>,
    Json(req): Json<BrowseDirectoryRequest>,
) -> Result<Json<BrowseDirectoryResponse>, ApiError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("backend_id 不能为空".into()));
    }

    let input = serde_json::to_value(WorkspaceBrowseDirectoryInput {
        backend_id: backend_id.to_string(),
        path: req.path,
    })
    .map_err(|error| {
        ApiError::BadRequest(format!("workspace.browse_directory 输入非法: {error}"))
    })?;
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION).map_err(|error| {
            ApiError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
        })?,
        RuntimeActor::PlatformUser { user_id: None },
        RuntimeContext::Setup {
            project_id: None,
            workspace_id: None,
            backend_id: Some(backend_id.to_string()),
            root_ref: None,
        },
        input,
    );
    let invocation = state.services.runtime_gateway.invoke(request).await?;
    let output = serde_json::from_value::<WorkspaceBrowseDirectoryOutput>(invocation.output.output)
        .map_err(|error| {
            ApiError::Internal(format!(
                "workspace.browse_directory 返回值解析失败: {error}"
            ))
        })?;

    Ok(Json(BrowseDirectoryResponse {
        current_path: output.current_path,
        entries: output
            .entries
            .into_iter()
            .map(|entry| BrowseDirectoryEntryResponse {
                name: entry.name,
                path: entry.path,
                is_dir: entry.is_dir,
            })
            .collect(),
    }))
}
