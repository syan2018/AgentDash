use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
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

// ---------------------------------------------------------------------------
// Request / Response 类型
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListSettingsQuery {
    pub category: Option<String>,
}

#[derive(Serialize)]
pub struct SettingResponse {
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
    pub updated: Vec<String>,
}

// ---------------------------------------------------------------------------
// GET /api/settings
// ---------------------------------------------------------------------------

pub async fn list_settings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSettingsQuery>,
) -> Result<Json<Vec<SettingResponse>>, ApiError> {
    let settings = state
        .repos
        .settings_repo
        .list(query.category.as_deref())
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
                key: s.key,
                value,
                updated_at: s.updated_at.to_rfc3339(),
                masked,
            }
        })
        .collect();

    Ok(Json(responses))
}

// ---------------------------------------------------------------------------
// PUT /api/settings
// ---------------------------------------------------------------------------

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<UpdateSettingsResponse>, ApiError> {
    if req.settings.is_empty() {
        return Err(ApiError::BadRequest("settings 不能为空".to_string()));
    }

    // 跳过客户端回传的脱敏占位值（包含 "..." 或 "****"）
    let entries: Vec<(String, serde_json::Value)> = req
        .settings
        .into_iter()
        .filter(|s| {
            if !is_sensitive_key(&s.key) {
                return true;
            }
            match s.value.as_str() {
                Some(v) if v.contains("...") || v == "****" => false,
                _ => true,
            }
        })
        .map(|s| (s.key, s.value))
        .collect();

    let updated_keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();

    state.repos.settings_repo.set_batch(&entries).await?;

    Ok(Json(UpdateSettingsResponse {
        updated: updated_keys,
    }))
}

// ---------------------------------------------------------------------------
// DELETE /api/settings/{key}
// ---------------------------------------------------------------------------

pub async fn delete_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.repos.settings_repo.delete(&key).await?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("设置项 '{key}' 不存在")))
    }
}
