use std::sync::Arc;

use agentdash_contracts::common_response::{DeletedFlagResponse, PendingExecutionResponse};
use agentdash_contracts::routine::{
    CreateRoutineRequest, EnableRoutineRequest, FireWebhookRequest, ListExecutionsQuery,
    RegenerateTokenResponse, RoutineCreationResponse, RoutineDispatchStrategyDto,
    RoutineExecutionResponse, RoutineResponse, RoutineTriggerConfigRequest, UpdateRoutineRequest,
};
use agentdash_domain::routine::{DispatchStrategy, Routine, RoutineTriggerConfig};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

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
        ProjectPermission::Use,
    )
    .await?;
    let routines = state.repos.routine_repo.list_by_project(project_id).await?;
    Ok(Json(
        routines.into_iter().map(RoutineResponse::from).collect(),
    ))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{id}/routines",
            axum::routing::get(list_routines).post(create_routine),
        )
        .route(
            "/routines/{id}",
            axum::routing::get(get_routine)
                .put(update_routine)
                .delete(delete_routine),
        )
        .route(
            "/routines/{id}/enable",
            axum::routing::patch(enable_routine),
        )
        .route(
            "/routines/{id}/regenerate-token",
            axum::routing::post(regenerate_webhook_token),
        )
        .route(
            "/routines/{id}/executions",
            axum::routing::get(list_executions),
        )
}

pub fn public_router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new().route(
        "/routine-triggers/{endpoint_id}/fire",
        axum::routing::post(fire_webhook),
    )
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
        ProjectPermission::Configure,
    )
    .await?;
    let project_agent_id = parse_uuid(&req.project_agent_id)?;
    ensure_project_agent_exists(state.as_ref(), project_id, project_agent_id).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }

    let mut webhook_plaintext_token: Option<String> = None;
    let trigger_config = match req.trigger_config {
        RoutineTriggerConfigRequest::Webhook {} => {
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
        other => routine_trigger_request_into_domain(other)?,
    };

    let dispatch_strategy: DispatchStrategy = match req.dispatch_strategy {
        Some(strategy) => routine_dispatch_strategy_into_domain(strategy),
        None => DispatchStrategy::default(),
    };

    let routine = Routine::new(
        project_id,
        name,
        req.prompt_template,
        project_agent_id,
        trigger_config,
        dispatch_strategy,
    );

    state.repos.routine_repo.create(&routine).await?;

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
        load_routine_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Use)
            .await?;
    Ok(Json(RoutineResponse::from(routine)))
}

pub async fn update_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoutineRequest>,
) -> Result<Json<RoutineResponse>, ApiError> {
    let mut routine = load_routine_with_permission(
        state.as_ref(),
        &current_user,
        &id,
        ProjectPermission::Configure,
    )
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
    if let Some(project_agent_id) = req.project_agent_id {
        let project_agent_id = parse_uuid(&project_agent_id)?;
        ensure_project_agent_exists(state.as_ref(), routine.project_id, project_agent_id).await?;
        routine.project_agent_id = project_agent_id;
    }
    if let Some(trigger_config) = req.trigger_config {
        routine.trigger_config =
            routine_trigger_update_into_domain(trigger_config, &routine.trigger_config)?;
    }
    if let Some(dispatch_strategy) = req.dispatch_strategy {
        routine.dispatch_strategy = routine_dispatch_strategy_into_domain(dispatch_strategy);
    }
    if let Some(enabled) = req.enabled {
        routine.enabled = enabled;
    }

    routine.updated_at = chrono::Utc::now();

    state.repos.routine_repo.update(&routine).await?;

    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(RoutineResponse::from(routine)))
}

pub async fn delete_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedFlagResponse>, ApiError> {
    let routine = load_routine_with_permission(
        state.as_ref(),
        &current_user,
        &id,
        ProjectPermission::Configure,
    )
    .await?;
    state.repos.routine_repo.delete(routine.id).await?;

    state.services.cron_scheduler.notify_config_changed();

    Ok(Json(DeletedFlagResponse { deleted: true }))
}

pub async fn enable_routine(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<EnableRoutineRequest>,
) -> Result<Json<RoutineResponse>, ApiError> {
    let mut routine = load_routine_with_permission(
        state.as_ref(),
        &current_user,
        &id,
        ProjectPermission::Configure,
    )
    .await?;

    routine.enabled = req.enabled;
    routine.updated_at = chrono::Utc::now();

    state.repos.routine_repo.update(&routine).await?;

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
        ProjectPermission::Use,
    )
    .await?;
    let executions = state
        .repos
        .routine_execution_repo
        .list_by_routine(routine.id, query.limit, query.offset)
        .await?;
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
    let mut routine = load_routine_with_permission(
        state.as_ref(),
        &current_user,
        &id,
        ProjectPermission::Configure,
    )
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

    state.repos.routine_repo.update(&routine).await?;

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
) -> Result<Json<PendingExecutionResponse>, ApiError> {
    // 通过 endpoint_id 查找 Routine
    let routine = state
        .repos
        .routine_repo
        .find_by_endpoint_id(&endpoint_id)
        .await?
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
        .map_err(ApiError::from)?;

    Ok(Json(PendingExecutionResponse {
        execution_id: exec_id.to_string(),
        status: "pending".to_string(),
    }))
}

// ────────────────────────── Helpers ──────────────────────────

fn parse_uuid(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("Invalid UUID: {id}")))
}

fn routine_trigger_request_into_domain(
    trigger_config: RoutineTriggerConfigRequest,
) -> Result<RoutineTriggerConfig, ApiError> {
    match trigger_config {
        RoutineTriggerConfigRequest::Scheduled {
            cron_expression,
            timezone,
        } => Ok(RoutineTriggerConfig::Scheduled {
            cron_expression,
            timezone,
        }),
        RoutineTriggerConfigRequest::Webhook {} => Err(ApiError::BadRequest(
            "Webhook trigger_config 只能在创建时由服务端生成".into(),
        )),
        RoutineTriggerConfigRequest::Plugin {
            provider_key,
            provider_config,
        } => Ok(RoutineTriggerConfig::Plugin {
            provider_key,
            provider_config,
        }),
    }
}

fn routine_trigger_update_into_domain(
    trigger_config: RoutineTriggerConfigRequest,
    current: &RoutineTriggerConfig,
) -> Result<RoutineTriggerConfig, ApiError> {
    match trigger_config {
        RoutineTriggerConfigRequest::Webhook {} => {
            let RoutineTriggerConfig::Webhook {
                endpoint_id,
                auth_token_hash,
            } = current
            else {
                return Err(ApiError::BadRequest(
                    "Webhook trigger_config 只能保留现有 Webhook Routine".into(),
                ));
            };
            Ok(RoutineTriggerConfig::Webhook {
                endpoint_id: endpoint_id.clone(),
                auth_token_hash: auth_token_hash.clone(),
            })
        }
        other => routine_trigger_request_into_domain(other),
    }
}

fn routine_dispatch_strategy_into_domain(strategy: RoutineDispatchStrategyDto) -> DispatchStrategy {
    match strategy {
        RoutineDispatchStrategyDto::Fresh => DispatchStrategy::Fresh,
        RoutineDispatchStrategyDto::Reuse => DispatchStrategy::Reuse,
        RoutineDispatchStrategyDto::PerEntity { entity_key_path } => {
            DispatchStrategy::PerEntity { entity_key_path }
        }
    }
}

async fn load_routine_with_permission(
    state: &AppState,
    current_user: &agentdash_platform_spi::platform::auth::AuthIdentity,
    routine_id: &str,
    permission: ProjectPermission,
) -> Result<Routine, ApiError> {
    let routine_id = parse_uuid(routine_id)?;
    let routine = state
        .repos
        .routine_repo
        .get_by_id(routine_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Routine {routine_id} 不存在")))?;
    load_project_with_permission(state, current_user, routine.project_id, permission).await?;
    Ok(routine)
}

async fn ensure_project_agent_exists(
    state: &AppState,
    project_id: Uuid,
    project_agent_id: Uuid,
) -> Result<(), ApiError> {
    let agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await?;
    if agent.is_none() {
        return Err(ApiError::BadRequest(format!(
            "Project {project_id} 不存在 Project Agent {project_agent_id}"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_routine_dispatch_strategy_request_to_domain() {
        assert!(matches!(
            routine_dispatch_strategy_into_domain(RoutineDispatchStrategyDto::Fresh),
            DispatchStrategy::Fresh
        ));
        assert!(matches!(
            routine_dispatch_strategy_into_domain(RoutineDispatchStrategyDto::Reuse),
            DispatchStrategy::Reuse
        ));
        assert!(matches!(
            routine_dispatch_strategy_into_domain(RoutineDispatchStrategyDto::PerEntity {
                entity_key_path: "payload.issue_id".to_string(),
            }),
            DispatchStrategy::PerEntity { entity_key_path } if entity_key_path == "payload.issue_id"
        ));
    }
}
