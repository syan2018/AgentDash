use std::sync::Arc;

use agentdash_application::task_execution::TaskExecutionError;
use agentdash_domain::task::{Task, TaskStatus};
use agentdash_mcp::injection::McpInjectionConfig;

use crate::address_space_access::SessionMountTarget;
use crate::dto::TaskResponse;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::bootstrap_plan::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use agentdash_application::session_context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_task_story_project_with_permission},
    bootstrap::task_execution_gateway::{
        execute_cancel_task, execute_continue_task, execute_get_task_session, execute_start_task,
        resolve_task_agent_config,
    },
    rpc::ApiError,
    runtime_bridge::{
        acp_mcp_servers_to_runtime, mcp_injection_config_to_runtime_binding,
        runtime_mcp_servers_to_acp,
    },
};
use agentdash_executor::is_native_agent;

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_connector_contract::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_connector_contract::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionResponse {
    pub task_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_connector_contract::ExecutionAddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug)]
pub(crate) struct BuiltTaskSessionContextResponse {
    pub(crate) address_space: Option<agentdash_connector_contract::ExecutionAddressSpace>,
    pub(crate) context_snapshot: Option<SessionContextSnapshot>,
}

pub async fn start_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let result = execute_start_task(state, task_id, req.override_prompt, req.executor_config)
        .await
        .map_err(map_task_execution_error)?;

    Ok(Json(StartTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn continue_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ContinueTaskRequest>,
) -> Result<Json<ContinueTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let result = execute_continue_task(state, task_id, req.additional_prompt, req.executor_config)
        .await
        .map_err(map_task_execution_error)?;

    Ok(Json(ContinueTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let task = execute_cancel_task(state, task_id)
        .await
        .map_err(map_task_execution_error)?;
    Ok(Json(TaskResponse::from(task)))
}

pub async fn get_task_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskSessionResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let (task, _, _) = load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::View,
    )
    .await?;
    let result = execute_get_task_session(state.clone(), task_id)
        .await
        .map_err(map_task_execution_error)?;

    // 非关键路径：尽量构建结构化上下文快照供前端解释规则，失败时静默降级。
    let built_context = build_task_session_context_response(&state, task_id).await;

    Ok(Json(TaskSessionResponse {
        task_id: result.task_id,
        workspace_id: task.workspace_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        task_status: result.task_status,
        agent_binding: result.agent_binding,
        session_title: result.session_title,
        last_activity: result.last_activity,
        address_space: built_context
            .as_ref()
            .and_then(|context| context.address_space.clone()),
        context_snapshot: built_context.and_then(|context| context.context_snapshot),
    }))
}

/// 为 task session 响应按需构建结构化上下文快照。
/// 非关键路径：任何失败都静默降级为 None。
pub(crate) async fn build_task_session_context_response(
    state: &Arc<AppState>,
    task_id: Uuid,
) -> Option<BuiltTaskSessionContextResponse> {
    let task = state.repos.task_repo.get_by_id(task_id).await.ok()??;
    let story = state
        .repos
        .story_repo
        .get_by_id(task.story_id)
        .await
        .ok()??;
    let project = state
        .repos
        .project_repo
        .get_by_id(task.project_id)
        .await
        .ok()??;
    let workspace = if let Some(ws_id) = task.workspace_id {
        state.repos.workspace_repo.get_by_id(ws_id).await.ok()?
    } else {
        None
    };
    let preset_name = normalize_optional_string(task.agent_binding.preset_name.clone());
    let executor_source = resolve_task_executor_source(&task, &project).to_string();
    let (resolved_config, resolution_error) = match resolve_task_agent_config(&task, &project) {
        Ok(config) => (config, None),
        Err(err) => (None, Some(err.to_string())),
    };
    let effective_agent_type = resolved_config.as_ref().map(|c| c.executor.as_str());
    let use_address_space = resolved_config
        .as_ref()
        .is_some_and(|c| is_native_agent(c));
    let address_space = if use_address_space {
        state
            .services
            .address_space_service
            .build_address_space(&project, Some(&story), workspace.as_ref(), SessionMountTarget::Task, effective_agent_type)
            .ok()
    } else {
        None
    };
    let mcp_servers = state
        .config
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            runtime_mcp_servers_to_acp(&[
                mcp_injection_config_to_runtime_binding(&McpInjectionConfig::for_task(
                    base_url.clone(),
                    task.project_id,
                    task.story_id,
                    task.id,
                ))
                .to_runtime_server(),
            ])
        })
        .unwrap_or_default();

    let story_overrides = extract_story_overrides(&story);
    let runtime_address_space = address_space.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story),
        workspace,
        resolved_config,
        address_space: runtime_address_space,
        mcp_servers: acp_mcp_servers_to_runtime(&mcp_servers),
        working_dir: None,
        workspace_root: None,
        executor_preset_name: preset_name,
        executor_source,
        executor_resolution_error: resolution_error,
        owner_variant: BootstrapOwnerVariant::Task { story_overrides },
        workflow: None,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContextResponse {
        address_space: plan.address_space.clone(),
        context_snapshot: Some(snapshot),
    })
}

fn resolve_task_executor_source(
    task: &Task,
    project: &agentdash_domain::project::Project,
) -> &'static str {
    if task
        .agent_binding
        .agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.agent_type";
    }

    if task
        .agent_binding
        .preset_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.preset_name";
    }

    if project
        .config
        .default_agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "project.config.default_agent_type";
    }

    "unresolved"
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}

pub(crate) fn map_task_execution_error(err: TaskExecutionError) -> ApiError {
    match err {
        TaskExecutionError::BadRequest(message) => ApiError::BadRequest(message),
        TaskExecutionError::NotFound(message) => ApiError::NotFound(message),
        TaskExecutionError::Conflict(message) => ApiError::Conflict(message),
        TaskExecutionError::UnprocessableEntity(message) => ApiError::UnprocessableEntity(message),
        TaskExecutionError::Internal(message) => ApiError::Internal(message),
    }
}
