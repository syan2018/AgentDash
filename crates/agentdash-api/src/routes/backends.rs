use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{BackendConfig, BackendRepository, BackendType};

use crate::app_state::AppState;
use crate::relay::registry::OnlineBackendInfo;
use crate::rpc::ApiError;
use agentdash_application::session_context::normalize_optional_string;

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
    let backends = state.repos.backend_repo.list_backends().await?;
    let online_list = state.services.backend_registry.list_online().await;
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
    let online = state.services.backend_registry.list_online().await;
    Ok(Json(online))
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
    };
    state.repos.backend_repo.add_backend(&config).await?;
    Ok(Json(config))
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
        && let Some(token) = normalize_optional_string(config.auth_token.clone()) {
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
    if !state.services.backend_registry.is_online(backend_id).await {
        return Err(ApiError::Conflict(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }

    let cmd = agentdash_relay::RelayMessage::CommandBrowseDirectory {
        id: agentdash_relay::RelayMessage::new_id("browse-dir"),
        payload: agentdash_relay::CommandBrowseDirectoryPayload {
            path: req.path,
        },
    };
    let resp = state
        .services
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| ApiError::Internal(format!("relay browse_directory 失败: {e}")))?;

    match resp {
        agentdash_relay::RelayMessage::ResponseBrowseDirectory {
            payload: Some(payload),
            error: None,
            ..
        } => Ok(Json(BrowseDirectoryResponse {
            current_path: payload.current_path,
            entries: payload
                .entries
                .into_iter()
                .map(|e| BrowseDirectoryEntryResponse {
                    name: e.name,
                    path: e.path,
                    is_dir: e.is_dir,
                })
                .collect(),
        })),
        agentdash_relay::RelayMessage::ResponseBrowseDirectory {
            error: Some(err), ..
        } => Err(ApiError::Internal(format!(
            "远程 browse_directory 错误: {}",
            err.message
        ))),
        _ => Err(ApiError::Internal(
            "远程 browse_directory 返回了意外响应".into(),
        )),
    }
}
