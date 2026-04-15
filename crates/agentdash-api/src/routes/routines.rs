use std::sync::Arc;

use agentdash_domain::routine::{Routine, RoutineExecution, RoutineTriggerConfig, SessionStrategy};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

// ────────────────────────── Response DTOs ──────────────────────────

/// 创建 Routine 的响应 — 包含一次性可见的 webhook_token
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RoutineCreationResponse {
    #[serde(flatten)]
    pub routine: RoutineResponse,
    /// 仅在创建 Webhook 类型时返回，且只此一次可见
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_token: Option<String>,
}

/// Webhook token 重新生成响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RegenerateTokenResponse {
    pub endpoint_id: String,
    /// 新生成的明文 token（仅此一次可见）
    pub webhook_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RoutineResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub prompt_template: String,
    pub agent_id: String,
    pub trigger_config: serde_json::Value,
    pub session_strategy: serde_json::Value,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_fired_at: Option<String>,
}

impl From<Routine> for RoutineResponse {
    fn from(r: Routine) -> Self {
        Self {
            id: r.id.to_string(),
            project_id: r.project_id.to_string(),
            name: r.name,
            prompt_template: r.prompt_template,
            agent_id: r.agent_id.to_string(),
            trigger_config: serde_json::to_value(&r.trigger_config).unwrap_or_default(),
            session_strategy: serde_json::to_value(&r.session_strategy).unwrap_or_default(),
            enabled: r.enabled,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            last_fired_at: r.last_fired_at.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RoutineExecutionResponse {
    pub id: String,
    pub routine_id: String,
    pub trigger_source: String,
    pub trigger_payload: Option<serde_json::Value>,
    pub resolved_prompt: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub entity_key: Option<String>,
}

impl From<RoutineExecution> for RoutineExecutionResponse {
    fn from(e: RoutineExecution) -> Self {
        Self {
            id: e.id.to_string(),
            routine_id: e.routine_id.to_string(),
            trigger_source: e.trigger_source,
            trigger_payload: e.trigger_payload,
            resolved_prompt: e.resolved_prompt,
            session_id: e.session_id,
            status: format!("{:?}", e.status).to_lowercase(),
            started_at: e.started_at.to_rfc3339(),
            completed_at: e.completed_at.map(|t| t.to_rfc3339()),
            error: e.error,
            entity_key: e.entity_key,
        }
    }
}

// ────────────────────────── Request DTOs ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRoutineRequest {
    pub name: String,
    pub prompt_template: String,
    pub agent_id: String,
    pub trigger_config: serde_json::Value,
    #[serde(default)]
    pub session_strategy: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoutineRequest {
    pub name: Option<String>,
    pub prompt_template: Option<String>,
    pub agent_id: Option<String>,
    pub trigger_config: Option<serde_json::Value>,
    pub session_strategy: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct EnableRoutineRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct FireWebhookRequest {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ListExecutionsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    50
}

// ────────────────────────── Handlers ──────────────────────────

pub async fn list_routines(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<RoutineResponse>>, ApiError> {
    let project_id = parse_uuid(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let routines = state
        .repos
        .routine_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        routines.into_iter().map(RoutineResponse::from).collect(),
    ))
}

pub async fn create_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateRoutineRequest>,
) -> Result<Json<RoutineCreationResponse>, ApiError> {
    let project_id = parse_uuid(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let agent_id = parse_uuid(&req.agent_id)?;
    ensure_project_agent_link_exists(state.as_ref(), project_id, agent_id).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }

    let trigger_config: RoutineTriggerConfig = serde_json::from_value(req.trigger_config)
        .map_err(|e| ApiError::BadRequest(format!("trigger_config 格式错误: {e}")))?;

    // Webhook 类型：服务端自动生成 endpoint_id 和 auth_token_hash
    let mut webhook_plaintext_token: Option<String> = None;
    let trigger_config = match trigger_config {
        RoutineTriggerConfig::Webhook { .. } => {
            let endpoint_id = format!("trig_{}", Uuid::new_v4().simple());
            let token = Uuid::new_v4().to_string();
            let hash = bcrypt::hash(&token, bcrypt::DEFAULT_COST)
                .map_err(|e| ApiError::Internal(format!("bcrypt hash 失败: {e}")))?;
            webhook_plaintext_token = Some(token);
            RoutineTriggerConfig::Webhook {
                endpoint_id,
                auth_token_hash: hash,
            }
        }
        other => other,
    };

    let session_strategy: SessionStrategy = match req.session_strategy {
        Some(v) => serde_json::from_value(v)
            .map_err(|e| ApiError::BadRequest(format!("session_strategy 格式错误: {e}")))?,
        None => SessionStrategy::default(),
    };

    let routine = Routine::new(
        project_id,
        name,
        req.prompt_template,
        agent_id,
        trigger_config,
        session_strategy,
    );

    state
        .repos
        .routine_repo
        .create(&routine)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 通知 cron 调度器配置变更
    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(RoutineCreationResponse {
        routine: RoutineResponse::from(routine),
        webhook_token: webhook_plaintext_token,
    }))
}

pub async fn get_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<RoutineResponse>, ApiError> {
    let routine =
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::View)
            .await?;
    Ok(Json(RoutineResponse::from(routine)))
}

pub async fn update_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoutineRequest>,
) -> Result<Json<RoutineResponse>, ApiError> {
    let mut routine =
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;

    if let Some(name) = req.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(ApiError::BadRequest("name 不能为空".into()));
        }
        routine.name = name;
    }
    if let Some(template) = req.prompt_template {
        routine.prompt_template = template;
    }
    if let Some(agent_id) = req.agent_id {
        let agent_id = parse_uuid(&agent_id)?;
        ensure_project_agent_link_exists(state.as_ref(), routine.project_id, agent_id).await?;
        routine.agent_id = agent_id;
    }
    if let Some(tc) = req.trigger_config {
        routine.trigger_config = serde_json::from_value(tc)
            .map_err(|e| ApiError::BadRequest(format!("trigger_config 格式错误: {e}")))?;
    }
    if let Some(ss) = req.session_strategy {
        routine.session_strategy = serde_json::from_value(ss)
            .map_err(|e| ApiError::BadRequest(format!("session_strategy 格式错误: {e}")))?;
    }
    if let Some(enabled) = req.enabled {
        routine.enabled = enabled;
    }

    routine.updated_at = chrono::Utc::now();

    state
        .repos
        .routine_repo
        .update(&routine)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(RoutineResponse::from(routine)))
}

pub async fn delete_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let routine =
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;
    state
        .repos
        .routine_repo
        .delete(routine.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(serde_json::json!({"deleted": true})))
}

pub async fn enable_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<EnableRoutineRequest>,
) -> Result<Json<RoutineResponse>, ApiError> {
    let mut routine =
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;

    routine.enabled = req.enabled;
    routine.updated_at = chrono::Utc::now();

    state
        .repos
        .routine_repo
        .update(&routine)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(RoutineResponse::from(routine)))
}

pub async fn list_executions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(routine_id): Path<String>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<Vec<RoutineExecutionResponse>>, ApiError> {
    let routine = load_routine_with_permission(
        state.as_ref(),
        &current_user,
        &routine_id,
        ProjectPermission::View,
    )
    .await?;
    let executions = state
        .repos
        .routine_execution_repo
        .list_by_routine(routine.id, query.limit, query.offset)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        executions
            .into_iter()
            .map(RoutineExecutionResponse::from)
            .collect(),
    ))
}

pub async fn regenerate_webhook_token(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<RegenerateTokenResponse>, ApiError> {
    let mut routine =
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;

    let RoutineTriggerConfig::Webhook { endpoint_id, .. } = &routine.trigger_config else {
        return Err(ApiError::BadRequest(
            "只有 Webhook 类型的 Routine 才能重新生成 token".into(),
        ));
    };
    let endpoint_id = endpoint_id.clone();

    let token = Uuid::new_v4().to_string();
    let hash = bcrypt::hash(&token, bcrypt::DEFAULT_COST)
        .map_err(|e| ApiError::Internal(format!("bcrypt hash 失败: {e}")))?;

    routine.trigger_config = RoutineTriggerConfig::Webhook {
        endpoint_id: endpoint_id.clone(),
        auth_token_hash: hash,
    };
    routine.updated_at = chrono::Utc::now();

    state
        .repos
        .routine_repo
        .update(&routine)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RegenerateTokenResponse {
        endpoint_id,
        webhook_token: token,
    }))
}

pub async fn fire_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(endpoint_id): Path<String>,
    Json(req): Json<FireWebhookRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 通过 endpoint_id 查找 Routine
    let routine = state
        .repos
        .routine_repo
        .find_by_endpoint_id(&endpoint_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Endpoint {endpoint_id} 不存在")))?;

    if !routine.enabled {
        return Err(ApiError::BadRequest("Routine 已禁用".into()));
    }

    verify_webhook_token(&routine, &headers)?;

    let executor = state
        .services
        .routine_executor
        .as_ref()
        .ok_or_else(|| ApiError::Internal("RoutineExecutor 未初始化".into()))?;

    let exec_id = executor
        .fire_webhook(routine.id, req.text.as_deref(), req.payload)
        .await
        .map_err(|e| ApiError::Internal(e))?;

    Ok(Json(serde_json::json!({
        "execution_id": exec_id.to_string(),
        "status": "pending"
    })))
}

// ────────────────────────── Helpers ──────────────────────────

fn parse_uuid(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("Invalid UUID: {id}")))
}

async fn load_routine_with_permission(
    state: &AppState,
    current_user: &agentdash_spi::auth::AuthIdentity,
    routine_id: &str,
    permission: ProjectPermission,
) -> Result<Routine, ApiError> {
    let routine_id = parse_uuid(routine_id)?;
    let routine = state
        .repos
        .routine_repo
        .get_by_id(routine_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Routine {routine_id} 不存在")))?;
    load_project_with_permission(state, current_user, routine.project_id, permission).await?;
    Ok(routine)
}

async fn ensure_project_agent_link_exists(
    state: &AppState,
    project_id: Uuid,
    agent_id: Uuid,
) -> Result<(), ApiError> {
    let link = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if link.is_none() {
        return Err(ApiError::BadRequest(format!(
            "Project {project_id} 未关联 Agent {agent_id}"
        )));
    }
    Ok(())
}

fn verify_webhook_token(routine: &Routine, headers: &HeaderMap) -> Result<(), ApiError> {
    let RoutineTriggerConfig::Webhook {
        auth_token_hash, ..
    } = &routine.trigger_config
    else {
        return Err(ApiError::BadRequest("Routine 不是 webhook trigger".into()));
    };

    let token = extract_bearer_token(headers)
        .ok_or_else(|| ApiError::Unauthorized("缺少 Bearer token".into()))?;

    let token_valid = if auth_token_hash.starts_with("$2") {
        bcrypt::verify(token, auth_token_hash)
            .map_err(|e| ApiError::Internal(format!("Webhook token hash 校验失败: {e}")))?
    } else {
        token == auth_token_hash
    };

    if !token_valid {
        return Err(ApiError::Unauthorized("Webhook token 无效".into()));
    }

    Ok(())
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}
