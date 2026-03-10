use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use agentdash_domain::backend::{BackendConfig, BackendType};

use crate::app_state::AppState;
use crate::relay::registry::OnlineBackendInfo;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateBackendRequest {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub backend_type: Option<String>,
}

#[derive(Serialize)]
pub struct BackendWithStatus {
    #[serde(flatten)]
    pub config: BackendConfig,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessible_roots: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<agentdash_relay::CapabilitiesPayload>,
}

pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BackendWithStatus>>, ApiError> {
    let backends = state.backend_repo.list_backends().await?;
    let online_list = state.backend_registry.list_online().await;
    let mut result = Vec::with_capacity(backends.len() + online_list.len());

    let mut seen_ids = std::collections::HashSet::new();

    for b in backends {
        seen_ids.insert(b.id.clone());
        let online_info = online_list.iter().find(|o| o.backend_id == b.id);
        result.push(BackendWithStatus {
            online: online_info.is_some(),
            accessible_roots: online_info.map(|o| o.accessible_roots.clone()),
            capabilities: online_info.map(|o| o.capabilities.clone()),
            config: b,
        });
    }

    for o in &online_list {
        if seen_ids.contains(&o.backend_id) {
            continue;
        }
        result.push(BackendWithStatus {
            online: true,
            accessible_roots: Some(o.accessible_roots.clone()),
            capabilities: Some(o.capabilities.clone()),
            config: BackendConfig {
                id: o.backend_id.clone(),
                name: o.name.clone(),
                endpoint: String::new(),
                auth_token: None,
                enabled: true,
                backend_type: BackendType::Remote,
            },
        });
    }

    Ok(Json(result))
}

/// 列出通过 WebSocket 连接的在线后端
pub async fn list_online_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<OnlineBackendInfo>>, ApiError> {
    let online = state.backend_registry.list_online().await;
    Ok(Json(online))
}

pub async fn get_backend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BackendConfig>, ApiError> {
    let backend = state.backend_repo.get_backend(&id).await?;
    Ok(Json(backend))
}

pub async fn add_backend(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBackendRequest>,
) -> Result<Json<BackendConfig>, ApiError> {
    let config = BackendConfig {
        id: req.id,
        name: req.name,
        endpoint: req.endpoint,
        auth_token: req.auth_token,
        enabled: true,
        backend_type: match req.backend_type.as_deref() {
            Some("remote") => BackendType::Remote,
            _ => BackendType::Local,
        },
    };
    state.backend_repo.add_backend(&config).await?;
    Ok(Json(config))
}

pub async fn remove_backend(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.backend_repo.remove_backend(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
