use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use agentdash_domain::settings::{SettingScope, SettingScopeKind};
use agentdash_plugin_api::{AuthIdentity, AuthMode};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

const SENSITIVE_PATTERNS: &[&str] = &["api_key", "secret", "token", "password"];

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_PATTERNS.iter().any(|p| lower.contains(p))
}

/// 对敏感值做脱敏处理：保留首尾各 4 字符，中间以 `...` 替代
fn mask_value(value: &serde_json::Value) -> serde_json::Value {
    if let Some(s) = value.as_str() {
        if s.len() > 8 {
            serde_json::Value::String(format!("{}...{}", &s[..4], &s[s.len() - 4..]))
        } else {
            serde_json::Value::String("****".to_string())
        }
    } else {
        serde_json::Value::String("****".to_string())
    }
}

#[derive(Deserialize)]
pub struct SettingsScopeQuery {
    pub category: Option<String>,
    pub scope: Option<SettingScopeKind>,
    pub project_id: Option<String>,
}

#[derive(Serialize)]
pub struct SettingResponse {
    pub scope_kind: SettingScopeKind,
    pub scope_id: Option<String>,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: String,
    pub masked: bool,
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub settings: Vec<SettingUpdate>,
}

#[derive(Deserialize)]
pub struct SettingUpdate {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Serialize)]
pub struct UpdateSettingsResponse {
    pub scope_kind: SettingScopeKind,
    pub scope_id: Option<String>,
    pub updated: Vec<String>,
}

pub async fn list_settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<SettingsScopeQuery>,
) -> Result<Json<Vec<SettingResponse>>, ApiError> {
    let scope = resolve_scope_for_read(state.as_ref(), &current_user, &query).await?;
    let settings = state
        .repos
        .settings_repo
        .list(&scope, query.category.as_deref())
        .await?;

    let responses: Vec<SettingResponse> = settings
        .into_iter()
        .map(|s| {
            let masked = is_sensitive_key(&s.key);
            let value = if masked {
                mask_value(&s.value)
            } else {
                s.value
            };
            SettingResponse {
                scope_kind: s.scope_kind,
                scope_id: s.scope_id,
                key: s.key,
                value,
                updated_at: s.updated_at.to_rfc3339(),
                masked,
            }
        })
        .collect();

    Ok(Json(responses))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<SettingsScopeQuery>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<UpdateSettingsResponse>, ApiError> {
    if req.settings.is_empty() {
        return Err(ApiError::BadRequest("settings 不能为空".to_string()));
    }

    let scope = resolve_scope_for_write(state.as_ref(), &current_user, &query).await?;

    let entries: Vec<(String, serde_json::Value)> = req
        .settings
        .into_iter()
        .filter(|s| {
            if !is_sensitive_key(&s.key) {
                return true;
            }
            !matches!(s.value.as_str(), Some(v) if v.contains("...") || v == "****")
        })
        .map(|s| (s.key, s.value))
        .collect();

    let updated_keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();

    state
        .repos
        .settings_repo
        .set_batch(&scope, &entries)
        .await?;

    Ok(Json(UpdateSettingsResponse {
        scope_kind: scope.kind,
        scope_id: scope.scope_id,
        updated: updated_keys,
    }))
}

pub async fn delete_setting(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(key): Path<String>,
    Query(query): Query<SettingsScopeQuery>,
) -> Result<StatusCode, ApiError> {
    let scope = resolve_scope_for_write(state.as_ref(), &current_user, &query).await?;
    let deleted = state.repos.settings_repo.delete(&scope, &key).await?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!(
            "设置项 '{}' 在 {} scope 中不存在",
            key,
            scope.kind.as_str()
        )))
    }
}

async fn resolve_scope_for_read(
    state: &AppState,
    current_user: &AuthIdentity,
    query: &SettingsScopeQuery,
) -> Result<SettingScope, ApiError> {
    resolve_scope(state, current_user, query, false).await
}

async fn resolve_scope_for_write(
    state: &AppState,
    current_user: &AuthIdentity,
    query: &SettingsScopeQuery,
) -> Result<SettingScope, ApiError> {
    resolve_scope(state, current_user, query, true).await
}

async fn resolve_scope(
    state: &AppState,
    current_user: &AuthIdentity,
    query: &SettingsScopeQuery,
    require_write: bool,
) -> Result<SettingScope, ApiError> {
    match query.scope.unwrap_or(SettingScopeKind::System) {
        SettingScopeKind::System => {
            require_system_settings_access(current_user)?;
            Ok(SettingScope::system())
        }
        SettingScopeKind::User => Ok(SettingScope::user(current_user.user_id.clone())),
        SettingScopeKind::Project => {
            let raw_project_id = query
                .project_id
                .as_deref()
                .ok_or_else(|| ApiError::BadRequest("project scope 需要提供 project_id".into()))?;
            let project_id = uuid::Uuid::parse_str(raw_project_id)
                .map_err(|_| ApiError::BadRequest("无效的 project_id".into()))?;
            load_project_with_permission(
                state,
                current_user,
                project_id,
                if require_write {
                    ProjectPermission::Edit
                } else {
                    ProjectPermission::View
                },
            )
            .await?;
            Ok(SettingScope::project(raw_project_id.to_string()))
        }
    }
}

fn require_system_settings_access(current_user: &AuthIdentity) -> Result<(), ApiError> {
    if current_user.is_admin || current_user.auth_mode == AuthMode::Personal {
        return Ok(());
    }

    Err(ApiError::Forbidden(
        "企业模式下仅管理员可以访问 system scope 设置".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_plugin_api::{AuthGroup, AuthMode};

    fn user(auth_mode: AuthMode, is_admin: bool) -> AuthIdentity {
        AuthIdentity {
            auth_mode,
            user_id: "alice".to_string(),
            subject: "alice".to_string(),
            display_name: Some("Alice".to_string()),
            email: Some("alice@example.com".to_string()),
            groups: vec![AuthGroup {
                group_id: "eng".to_string(),
                display_name: Some("Engineering".to_string()),
            }],
            is_admin,
            provider: Some("test".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    #[test]
    fn personal_mode_can_access_system_scope() {
        let result = require_system_settings_access(&user(AuthMode::Personal, false));
        assert!(result.is_ok());
    }

    #[test]
    fn enterprise_non_admin_cannot_access_system_scope() {
        let result = require_system_settings_access(&user(AuthMode::Enterprise, false));
        assert!(matches!(result, Err(ApiError::Forbidden(_))));
    }

    #[test]
    fn enterprise_admin_can_access_system_scope() {
        let result = require_system_settings_access(&user(AuthMode::Enterprise, true));
        assert!(result.is_ok());
    }
}
