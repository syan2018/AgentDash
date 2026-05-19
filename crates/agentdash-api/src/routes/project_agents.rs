use std::sync::Arc;

use agentdash_domain::{
    agent::ProjectAgent,
    common::AgentPresetConfig,
    inline_file::InlineFileOwnerKind,
    project::Project,
    session_binding::{SessionBinding, SessionOwnerType},
    workspace::Workspace,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::SessionMcpServer;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

#[derive(Debug, Clone)]
pub(crate) struct ProjectAgentBridge {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor_config: AgentConfig,
    pub preset_config: AgentPresetConfig,
    pub preset_name: Option<String>,
    pub source: String,
    /// MCP servers parsed from preset config — injected into ExecutionContext for project-agent sessions
    pub preset_mcp_servers: Vec<SessionMcpServer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentExecutorResponse {
    pub executor: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<agentdash_spi::ThinkingLevel>,
    pub permission_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSessionResponse {
    pub binding_id: String,
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSummaryResponse {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor: ProjectAgentExecutorResponse,
    pub preset_name: Option<String>,
    pub source: String,
    pub session: Option<ProjectAgentSessionResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OpenProjectAgentSessionResponse {
    pub created: bool,
    pub session_id: String,
    pub binding_id: String,
    pub agent: ProjectAgentSummaryResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_agent_summary_response_serializes_as_snake_case() {
        let value = serde_json::to_value(ProjectAgentSummaryResponse {
            key: "default".to_string(),
            display_name: "项目默认 Agent".to_string(),
            description: "desc".to_string(),
            executor: ProjectAgentExecutorResponse {
                executor: "PI_AGENT".to_string(),
                provider_id: Some("openai".to_string()),
                model_id: Some("test-model".to_string()),
                agent_id: None,
                thinking_level: None,
                permission_policy: Some("AUTO".to_string()),
            },
            preset_name: None,
            source: "project.config.default_agent_type".to_string(),
            session: Some(ProjectAgentSessionResponse {
                binding_id: "binding-1".to_string(),
                session_id: "sess-1".to_string(),
                session_title: Some("title".to_string()),
                last_activity: Some(1),
            }),
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("presetName").is_none());
    }

    #[test]
    fn parse_project_agent_session_label_requires_expected_prefix() {
        assert_eq!(
            parse_project_agent_session_label("project_agent:agent-1"),
            Some("agent-1")
        );
        assert_eq!(parse_project_agent_session_label("agent-1"), None);
        assert_eq!(parse_project_agent_session_label("project_agent:   "), None);
    }
}

pub async fn list_project_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentSummaryResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut response = Vec::with_capacity(agents.len());
    for agent in &agents {
        let bridge = build_agent_bridge(state.as_ref(), agent).await?;
        let session = find_project_agent_session(&state, project.id, &bridge.key).await?;
        response.push(build_project_agent_summary(&project, &bridge, session));
    }

    response.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct OpenSessionQuery {
    #[serde(default)]
    pub force_new: bool,
}

pub async fn open_project_agent_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
    Query(query): Query<OpenSessionQuery>,
) -> Result<Json<OpenProjectAgentSessionResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let project_agent_id = parse_project_agent_id(&agent_key)?;
    let project_agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let agent = build_agent_bridge(state.as_ref(), &project_agent).await?;

    let label = project_agent_session_label(&agent.key);

    if !query.force_new {
        let existing_binding = state
            .repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Project, project.id, &label)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        if let Some(binding) = existing_binding
            && let Some(meta) = state
                .services
                .session_core
                .get_session_meta(&binding.session_id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?
        {
            let session_id = binding.session_id.clone();
            let binding_id = binding.id.to_string();
            let session = Some(ProjectAgentSessionResponse {
                binding_id: binding_id.clone(),
                session_id: session_id.clone(),
                session_title: Some(meta.title),
                last_activity: Some(meta.updated_at),
            });
            let summary = build_project_agent_summary(&project, &agent, session);
            return Ok(Json(OpenProjectAgentSessionResponse {
                created: false,
                session_id,
                binding_id,
                agent: summary,
            }));
        }
    }

    // Clean up stale binding (session gone from executor hub) -- but only the latest one
    if let Some(binding) = state
        .repos
        .session_binding_repo
        .find_by_owner_and_label(SessionOwnerType::Project, project.id, &label)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        let session_alive = state
            .services
            .session_core
            .get_session_meta(&binding.session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .is_some();

        if !session_alive {
            state
                .repos
                .session_binding_repo
                .delete(binding.id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
    }

    let meta = state
        .services
        .session_core
        .create_session("")
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let binding = SessionBinding::new(
        project.id,
        meta.id.clone(),
        SessionOwnerType::Project,
        project.id,
        label,
    );
    state
        .repos
        .session_binding_repo
        .create(&binding)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    state
        .services
        .session_core
        .mark_owner_bootstrap_pending(&meta.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    // 自动启动 Lifecycle Run（如果 ProjectAgent 配置了 default_lifecycle_key）
    if let Some(lifecycle_key) = project_agent.default_lifecycle_key.as_deref()
        && let Err(err) =
            auto_start_lifecycle_run(&state, project.id, &meta.id, lifecycle_key).await
    {
        tracing::warn!(
            project_id = %project.id,
            agent_key = %agent_key,
            lifecycle_key = %lifecycle_key,
            error = %err,
            "自动启动 Lifecycle Run 失败（不阻塞 session 创建）"
        );
    }

    let session = Some(ProjectAgentSessionResponse {
        binding_id: binding.id.to_string(),
        session_id: meta.id.clone(),
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
    });
    let summary = build_project_agent_summary(&project, &agent, session);

    Ok(Json(OpenProjectAgentSessionResponse {
        created: true,
        session_id: meta.id,
        binding_id: binding.id.to_string(),
        agent: summary,
    }))
}

/// 从 agent_key（UUID 字符串）异步解析 ProjectAgentBridge
pub(crate) async fn resolve_project_agent_bridge_async(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_key: &str,
) -> Result<Option<ProjectAgentBridge>, ApiError> {
    let project_agent_id = match Uuid::parse_str(agent_key) {
        Ok(project_agent_id) => project_agent_id,
        Err(_) => return Ok(None),
    };
    let agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let Some(agent) = agent else {
        return Ok(None);
    };
    Ok(Some(build_agent_bridge(state.as_ref(), &agent).await?))
}

pub(crate) async fn resolve_project_workspace(
    state: &Arc<AppState>,
    project: &Project,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return state
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()));
    }
    Ok(None)
}

pub(crate) const PROJECT_AGENT_SESSION_LABEL_PREFIX: &str = "project_agent:";

pub(crate) fn project_agent_session_label(agent_key: &str) -> String {
    format!("{PROJECT_AGENT_SESSION_LABEL_PREFIX}{}", agent_key.trim())
}

pub(crate) fn parse_project_agent_session_label(label: &str) -> Option<&str> {
    let agent_key = label
        .trim()
        .strip_prefix(PROJECT_AGENT_SESSION_LABEL_PREFIX)?;
    if agent_key.trim().is_empty() {
        return None;
    }
    Some(agent_key)
}

fn build_project_agent_summary(
    _project: &Project,
    agent: &ProjectAgentBridge,
    session: Option<ProjectAgentSessionResponse>,
) -> ProjectAgentSummaryResponse {
    ProjectAgentSummaryResponse {
        key: agent.key.clone(),
        display_name: agent.display_name.clone(),
        description: agent.description.clone(),
        executor: ProjectAgentExecutorResponse {
            executor: agent.executor_config.executor.clone(),
            provider_id: agent.executor_config.provider_id.clone(),
            model_id: agent.executor_config.model_id.clone(),
            agent_id: agent.executor_config.agent_id.clone(),
            thinking_level: agent.executor_config.thinking_level,
            permission_policy: agent.executor_config.permission_policy.clone(),
        },
        preset_name: agent.preset_name.clone(),
        source: agent.source.clone(),
        session,
    }
}

async fn find_project_agent_session(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_key: &str,
) -> Result<Option<ProjectAgentSessionResponse>, ApiError> {
    let binding = state
        .repos
        .session_binding_repo
        .find_by_owner_and_label(
            SessionOwnerType::Project,
            project_id,
            &project_agent_session_label(agent_key),
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let Some(binding) = binding else {
        return Ok(None);
    };

    let meta = state
        .services
        .session_core
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Some(ProjectAgentSessionResponse {
        binding_id: binding.id.to_string(),
        session_id: binding.session_id,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
    }))
}

pub async fn list_project_agent_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
) -> Result<Json<Vec<ProjectAgentSessionResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let label = project_agent_session_label(&agent_key);
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Project, project.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let matching: Vec<_> = bindings.into_iter().filter(|b| b.label == label).collect();

    let mut sessions = Vec::with_capacity(matching.len());
    for binding in matching {
        let meta = state
            .services
            .session_core
            .get_session_meta(&binding.session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        sessions.push(ProjectAgentSessionResponse {
            binding_id: binding.id.to_string(),
            session_id: binding.session_id,
            session_title: meta.as_ref().map(|m| m.title.clone()),
            last_activity: meta.as_ref().map(|m| m.updated_at),
        });
    }

    sessions.sort_by(|a, b| {
        let at = b.last_activity.unwrap_or(0);
        let bt = a.last_activity.unwrap_or(0);
        at.cmp(&bt)
    });

    Ok(Json(sessions))
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}

fn parse_project_agent_id(project_agent_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_agent_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_agent_id: {project_agent_id}")))
}

// ─── Project Agent API ───

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub agent_type: String,
    pub config: agentdash_domain::common::AgentPresetConfig,
    pub default_lifecycle_key: Option<String>,
    pub is_default_for_story: bool,
    pub is_default_for_task: bool,
    pub knowledge_enabled: bool,
    pub project_container_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn build_project_agent_response(agent: &ProjectAgent) -> Result<ProjectAgentResponse, ApiError> {
    Ok(ProjectAgentResponse {
        id: agent.id.to_string(),
        project_id: agent.project_id.to_string(),
        name: agent.name.clone(),
        agent_type: agent.agent_type.clone(),
        config: agent
            .preset_config()
            .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        default_lifecycle_key: agent.default_lifecycle_key.clone(),
        is_default_for_story: agent.is_default_for_story,
        is_default_for_task: agent.is_default_for_task,
        knowledge_enabled: agent.knowledge_enabled,
        project_container_ids: agent.project_container_ids.clone(),
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    })
}

/// GET /projects/{id}/agents — 列出项目内所有 Project Agent
pub async fn list_project_agent_configs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let response = agents
        .iter()
        .map(build_project_agent_response)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectAgentRequest {
    pub name: String,
    pub agent_type: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    pub is_default_for_story: bool,
    #[serde(default)]
    pub is_default_for_task: bool,
}

/// POST /projects/{id}/agents — 创建项目私有 Agent
pub async fn create_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateProjectAgentRequest>,
) -> Result<Json<ProjectAgentResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }
    let agent_type = req.agent_type.trim().to_string();
    if agent_type.is_empty() {
        return Err(ApiError::BadRequest("agent_type 不能为空".into()));
    }
    if state
        .repos
        .project_agent_repo
        .get_by_project_and_name(project_id, &name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "Project Agent key 已存在: {name}"
        )));
    }

    let lifecycle_key = resolve_lifecycle_key_for_project_agent(
        &state,
        project_id,
        req.default_lifecycle_key,
        req.default_workflow_key,
    )
    .await?;

    let mut agent = ProjectAgent::new(project_id, name, agent_type);
    if let Some(config) = req.config {
        agent.config = config;
    }
    agent.default_lifecycle_key = lifecycle_key;
    agent.is_default_for_story = req.is_default_for_story;
    agent.is_default_for_task = req.is_default_for_task;

    state
        .repos
        .project_agent_repo
        .create(&agent)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(build_project_agent_response(&agent)?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectAgentRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    pub is_default_for_story: Option<bool>,
    #[serde(default)]
    pub is_default_for_task: Option<bool>,
    #[serde(default)]
    pub knowledge_enabled: Option<bool>,
    #[serde(default)]
    pub project_container_ids: Option<Vec<String>>,
}

/// PUT /projects/{id}/agents/{project_agent_id} — 更新 Project Agent
pub async fn update_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
    Json(req): Json<UpdateProjectAgentRequest>,
) -> Result<Json<ProjectAgentResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;

    let mut agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent {project_agent_id} 不存在")))?;

    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("name 不能为空".into()));
        }
        agent.name = trimmed;
    }
    if let Some(agent_type) = req.agent_type {
        let trimmed = agent_type.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("agent_type 不能为空".into()));
        }
        agent.agent_type = trimmed;
    }
    if let Some(config) = req.config {
        agent.config = config;
    }
    if req.default_lifecycle_key.is_some() || req.default_workflow_key.is_some() {
        agent.default_lifecycle_key = resolve_lifecycle_key_for_project_agent(
            &state,
            project_id,
            req.default_lifecycle_key,
            req.default_workflow_key,
        )
        .await?;
    }
    if let Some(v) = req.is_default_for_story {
        agent.is_default_for_story = v;
    }
    if let Some(v) = req.is_default_for_task {
        agent.is_default_for_task = v;
    }
    if let Some(v) = req.knowledge_enabled {
        agent.knowledge_enabled = v;
    }
    if let Some(ids) = req.project_container_ids {
        agent.project_container_ids = ids;
    }
    agent.updated_at = chrono::Utc::now();

    state
        .repos
        .project_agent_repo
        .update(&agent)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(build_project_agent_response(&agent)?))
}

/// DELETE /projects/{id}/agents/{project_agent_id} — 删除 Project Agent
pub async fn delete_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;

    let routines = state
        .repos
        .routine_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if routines
        .iter()
        .any(|routine| routine.project_agent_id == project_agent_id)
    {
        return Err(ApiError::BadRequest(
            "该 Project Agent 仍被 Routine 使用，需先调整或删除相关 Routine".into(),
        ));
    }

    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectAgent, project_agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state
        .repos
        .project_agent_repo
        .delete(project_id, project_agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// 统一处理 lifecycle_key / workflow_key 的解析
///
/// 如果用户指定了 `default_workflow_key`（单个 workflow），
/// 自动创建一个单步 lifecycle 包装它。
async fn resolve_lifecycle_key_for_project_agent(
    state: &Arc<AppState>,
    project_id: Uuid,
    lifecycle_key: Option<String>,
    workflow_key: Option<String>,
) -> Result<Option<String>, ApiError> {
    if let Some(lk) = lifecycle_key {
        let trimmed = lk.trim().to_string();
        return Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        });
    }

    if let Some(wk) = workflow_key {
        let wk = wk.trim().to_string();
        if wk.is_empty() {
            return Ok(None);
        }

        let workflow = state
            .repos
            .workflow_definition_repo
            .get_by_key(&wk)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Workflow `{wk}` 不存在")))?;

        let auto_key = format!("auto:{wk}");

        let existing = state
            .repos
            .lifecycle_definition_repo
            .get_by_key(&auto_key)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        if existing.is_none() {
            use agentdash_domain::workflow::{
                LifecycleDefinition, LifecycleStepDefinition, WorkflowDefinitionSource,
            };
            let lifecycle = LifecycleDefinition {
                id: Uuid::new_v4(),
                project_id,
                key: auto_key.clone(),
                name: format!("Auto: {wk}"),
                description: format!("自动创建：包装单个 workflow `{wk}`"),
                binding_kinds: workflow.binding_kinds.clone(),
                source: WorkflowDefinitionSource::UserAuthored,
                installed_source: None,
                version: 1,
                steps: vec![LifecycleStepDefinition {
                    key: "main".to_string(),
                    description: String::new(),
                    workflow_key: Some(wk),
                    node_type: Default::default(),
                    output_ports: vec![],
                    input_ports: vec![],
                    capability_config: Default::default(),
                }],
                edges: vec![],
                entry_step_key: "main".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            state
                .repos
                .lifecycle_definition_repo
                .create(&lifecycle)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }

        return Ok(Some(auto_key));
    }

    Ok(None)
}

/// 从 ProjectAgent 构建 ProjectAgentBridge。
pub(crate) async fn build_agent_bridge(
    state: &AppState,
    agent: &ProjectAgent,
) -> Result<ProjectAgentBridge, ApiError> {
    let preset = agent
        .preset_config()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let executor_config = preset.to_agent_config(&agent.agent_type);

    let display_name = preset
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&agent.name)
        .to_string();

    let description = preset
        .description
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Agent `{}`，执行器 {}。", agent.name, agent.agent_type));

    let preset_mcp_servers = agentdash_application::mcp_preset::resolve_preset_mcp_refs(
        state.repos.mcp_preset_repo.as_ref(),
        agent.project_id,
        preset.mcp_preset_keys.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(|error| {
        ApiError::Internal(format!(
            "Project Agent `{}` 的 mcp_preset_keys 配置非法: {error}",
            agent.id
        ))
    })?;

    Ok(ProjectAgentBridge {
        key: agent.id.to_string(),
        display_name,
        description,
        executor_config,
        preset_config: preset,
        preset_name: Some(agent.name.clone()),
        source: format!("project_agents[{}]", agent.id),
        preset_mcp_servers,
    })
}

/// 自动启动 lifecycle run（首步含 workflow_key 时同时激活首步）
async fn auto_start_lifecycle_run(
    state: &Arc<AppState>,
    project_id: Uuid,
    session_id: &str,
    lifecycle_key: &str,
) -> Result<(), String> {
    use agentdash_application::workflow::{
        LifecycleRunService, StartLifecycleRunCommand, build_step_projector_from_repos,
    };

    let service = LifecycleRunService::new(
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .with_projector(build_step_projector_from_repos(&state.repos));

    let cmd = StartLifecycleRunCommand {
        project_id,
        lifecycle_id: None,
        lifecycle_key: Some(lifecycle_key.to_string()),
        session_id: session_id.to_string(),
    };

    let run = service
        .start_run(cmd)
        .await
        .map_err(|e| format!("start_run 失败: {e}"))?;

    // 自动激活首步
    if let Some(step_key) = run.current_step_key() {
        use agentdash_application::workflow::BindAndActivateLifecycleStepCommand;
        let activate_cmd = BindAndActivateLifecycleStepCommand {
            run_id: run.id,
            step_key: step_key.to_string(),
            session_id: session_id.to_string(),
        };
        if let Err(e) = service.bind_session_and_activate_step(activate_cmd).await {
            tracing::warn!(run_id = %run.id, step_key = %step_key, error = %e, "自动激活首步失败");
        }
    }

    Ok(())
}
