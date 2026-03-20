use std::sync::Arc;

use agentdash_application::task_execution::TaskExecutionError;
use agentdash_domain::context_container::{ContextContainerDefinition, MountDerivationPolicy};
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::task::{Task, TaskStatus};
use agentdash_mcp::injection::McpInjectionConfig;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    bootstrap::task_execution_gateway::{
        execute_cancel_task, execute_continue_task, execute_get_task_session, execute_start_task,
        resolve_task_agent_config,
    },
    rpc::ApiError,
    session_plan::{
        SessionRuntimePolicySummary, SessionToolVisibilitySummary,
        resolve_effective_session_composition, summarize_runtime_policy,
        summarize_tool_visibility,
    },
};

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
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
    pub executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
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
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<TaskSessionContextSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionContextSnapshot {
    pub executor: TaskSessionExecutorSummary,
    pub project_defaults: SessionProjectDefaults,
    pub story_overrides: SessionStoryOverrides,
    pub effective: SessionEffectiveContext,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionExecutorSummary {
    pub executor: Option<String>,
    pub variant: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub reasoning_id: Option<String>,
    pub permission_policy: Option<String>,
    pub preset_name: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionProjectDefaults {
    pub default_agent_type: Option<String>,
    pub context_containers: Vec<ContextContainerDefinition>,
    pub mount_policy: MountDerivationPolicy,
    pub session_composition: SessionComposition,
}

#[derive(Debug, Serialize)]
pub struct SessionStoryOverrides {
    pub context_containers: Vec<ContextContainerDefinition>,
    pub disabled_container_ids: Vec<String>,
    pub mount_policy_override: Option<MountDerivationPolicy>,
    pub session_composition_override: Option<SessionComposition>,
}

#[derive(Debug, Serialize)]
pub struct SessionEffectiveContext {
    pub mount_policy: MountDerivationPolicy,
    pub session_composition: SessionComposition,
    pub tool_visibility: SessionToolVisibilitySummary,
    pub runtime_policy: SessionRuntimePolicySummary,
}

#[derive(Debug)]
struct BuiltTaskSessionContextResponse {
    address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    context_snapshot: Option<TaskSessionContextSnapshot>,
}

pub async fn start_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
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
    Path(id): Path<String>,
    Json(req): Json<ContinueTaskRequest>,
) -> Result<Json<ContinueTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
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
    Path(id): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let task = execute_cancel_task(state, task_id)
        .await
        .map_err(map_task_execution_error)?;
    Ok(Json(task))
}

pub async fn get_task_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskSessionResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let result = execute_get_task_session(state.clone(), task_id)
        .await
        .map_err(map_task_execution_error)?;

    // 非关键路径：尽量构建结构化上下文快照供前端解释规则，失败时静默降级。
    let built_context = build_task_session_context_response(&state, task_id).await;

    Ok(Json(TaskSessionResponse {
        task_id: result.task_id,
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
/// 这是非关键路径：任何失败都静默降级为 None，不影响主响应。
async fn build_task_session_context_response(
    state: &Arc<AppState>,
    task_id: Uuid,
) -> Option<BuiltTaskSessionContextResponse> {
    let task = state.task_repo.get_by_id(task_id).await.ok()??;
    let story = state.story_repo.get_by_id(task.story_id).await.ok()??;
    let project = state.project_repo.get_by_id(story.project_id).await.ok()??;
    let workspace = if let Some(ws_id) = task.workspace_id {
        state.workspace_repo.get_by_id(ws_id).await.ok()?
    } else {
        None
    };
    let preset_name = normalize_optional_string(task.agent_binding.preset_name.clone());
    let executor_source = resolve_task_executor_source(&task, &project).to_string();
    let (resolved_config, resolution_error) = match resolve_task_agent_config(&task, &project) {
        Ok(config) => (config, None),
        Err(err) => (None, Some(err.to_string())),
    };
    let effective_agent_type = resolved_config.as_ref().map(|config| config.executor.as_str());
    let use_address_space = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_native_agent());
    let address_space = if use_address_space {
        state
            .address_space_service
            .build_task_address_space(&project, &story, workspace.as_ref(), effective_agent_type)
            .ok()
    } else {
        None
    };
    let effective_mount_policy = story
        .context
        .mount_policy_override
        .clone()
        .unwrap_or_else(|| project.config.mount_policy.clone());
    let effective_session_composition = resolve_effective_session_composition(&project, Some(&story));
    let mcp_servers = state
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            vec![
                McpInjectionConfig::for_task(base_url.clone(), story.project_id, task.story_id, task.id)
                    .to_acp_mcp_server(),
            ]
        })
        .unwrap_or_default();
    let tool_visibility = summarize_tool_visibility(address_space.as_ref(), &mcp_servers);
    let runtime_policy = summarize_runtime_policy(
        workspace.is_some(),
        address_space.as_ref(),
        &mcp_servers,
        &tool_visibility.tool_names,
    );

    Some(BuiltTaskSessionContextResponse {
        address_space,
        context_snapshot: Some(TaskSessionContextSnapshot {
            executor: TaskSessionExecutorSummary {
                executor: resolved_config.as_ref().map(|config| config.executor.clone()),
                variant: resolved_config.as_ref().and_then(|config| config.variant.clone()),
                model_id: resolved_config.as_ref().and_then(|config| config.model_id.clone()),
                agent_id: resolved_config.as_ref().and_then(|config| config.agent_id.clone()),
                reasoning_id: resolved_config
                    .as_ref()
                    .and_then(|config| config.reasoning_id.clone()),
                permission_policy: resolved_config
                    .as_ref()
                    .and_then(|config| config.permission_policy.clone()),
                preset_name,
                source: executor_source,
                resolution_error,
            },
            project_defaults: SessionProjectDefaults {
                default_agent_type: normalize_optional_string(project.config.default_agent_type.clone()),
                context_containers: project.config.context_containers.clone(),
                mount_policy: project.config.mount_policy.clone(),
                session_composition: project.config.session_composition.clone(),
            },
            story_overrides: SessionStoryOverrides {
                context_containers: story.context.context_containers.clone(),
                disabled_container_ids: story.context.disabled_container_ids.clone(),
                mount_policy_override: story.context.mount_policy_override.clone(),
                session_composition_override: story.context.session_composition_override.clone(),
            },
            effective: SessionEffectiveContext {
                mount_policy: effective_mount_policy,
                session_composition: effective_session_composition,
                tool_visibility,
                runtime_policy,
            },
        }),
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

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}

fn map_task_execution_error(err: TaskExecutionError) -> ApiError {
    match err {
        TaskExecutionError::BadRequest(message) => ApiError::BadRequest(message),
        TaskExecutionError::NotFound(message) => ApiError::NotFound(message),
        TaskExecutionError::Conflict(message) => ApiError::Conflict(message),
        TaskExecutionError::UnprocessableEntity(message) => ApiError::UnprocessableEntity(message),
        TaskExecutionError::Internal(message) => ApiError::Internal(message),
    }
}
