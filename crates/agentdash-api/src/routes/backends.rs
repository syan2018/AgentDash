use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use agentdash_domain::backend::{BackendConfig, BackendType};

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateBackendRequest {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub backend_type: Option<String>,
}

pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BackendConfig>>, ApiError> {
    let backends = state.backend_repo.list_backends().await?;
    Ok(Json(backends))
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
