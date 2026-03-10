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
}

pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BackendWithStatus>>, ApiError> {
    let backends = state.backend_repo.list_backends().await?;
    let mut result = Vec::with_capacity(backends.len());
    for b in backends {
        let online = state.backend_registry.is_online(&b.id).await;
        result.push(BackendWithStatus { config: b, online });
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
