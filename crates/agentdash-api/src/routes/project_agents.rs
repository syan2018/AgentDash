use std::sync::Arc;

use agentdash_contracts::{
    common_response::DeletedFlagResponse,
    project_agent::{
        CreateProjectAgentRequest, ProjectAgent as ProjectAgentResponse, ProjectAgentExecutor,
        ProjectAgentSummary, ThinkingLevel, UpdateProjectAgentRequest,
    },
    workflow::{ConversationEffectiveExecutorConfigView, ConversationModelConfigSource},
};
use agentdash_domain::{
    agent::ProjectAgent, common::AgentPresetConfig, inline_file::InlineFileOwnerKind,
};
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{id}/agents",
            axum::routing::get(list_project_agent_configs).post(create_project_agent),
        )
        .route(
            "/projects/{id}/agents/summary",
            axum::routing::get(list_project_agents),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}",
            axum::routing::put(update_project_agent).delete(delete_project_agent),
        )
}

pub async fn list_project_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentSummary>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let mut response = agents
        .iter()
        .map(build_project_agent_summary)
        .collect::<Result<Vec<_>, _>>()?;
    response.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(Json(response))
}

fn build_project_agent_summary(agent: &ProjectAgent) -> Result<ProjectAgentSummary, ApiError> {
    let preset = agent.preset_config().map_err(ApiError::from)?;
    let executor = preset.to_agent_config(&agent.agent_type);
    let display_name = preset
        .display_name
        .clone()
        .unwrap_or_else(|| agent.name.clone());
    let description = preset.description.clone().unwrap_or_default();
    let thinking_level = executor.thinking_level.map(thinking_level_response);
    Ok(ProjectAgentSummary {
        key: agent.name.clone(),
        display_name,
        description,
        executor: ProjectAgentExecutor {
            executor: executor.executor.clone(),
            provider_id: executor.provider_id.clone(),
            model_id: executor.model_id.clone(),
            agent_id: executor.agent_id.clone(),
            thinking_level,
        },
        effective_executor_config: Some(ConversationEffectiveExecutorConfigView {
            executor: executor.executor,
            provider_id: executor.provider_id,
            model_id: executor.model_id,
            agent_id: executor.agent_id,
            thinking_level: executor
                .thinking_level
                .map(|level| thinking_level_name(level).to_owned()),
            source: ConversationModelConfigSource::ProjectAgentPreset,
        }),
        preset_name: Some(agent.name.clone()),
        source: "project_agent".to_string(),
    })
}

fn thinking_level_response(level: agentdash_platform_spi::ThinkingLevel) -> ThinkingLevel {
    use agentdash_platform_spi::ThinkingLevel as SpiThinkingLevel;

    match level {
        SpiThinkingLevel::Off => ThinkingLevel::Off,
        SpiThinkingLevel::Minimal => ThinkingLevel::Minimal,
        SpiThinkingLevel::Low => ThinkingLevel::Low,
        SpiThinkingLevel::Medium => ThinkingLevel::Medium,
        SpiThinkingLevel::High => ThinkingLevel::High,
        SpiThinkingLevel::Xhigh => ThinkingLevel::Xhigh,
    }
}

fn thinking_level_name(level: agentdash_platform_spi::ThinkingLevel) -> &'static str {
    use agentdash_platform_spi::ThinkingLevel as SpiThinkingLevel;

    match level {
        SpiThinkingLevel::Off => "off",
        SpiThinkingLevel::Minimal => "minimal",
        SpiThinkingLevel::Low => "low",
        SpiThinkingLevel::Medium => "medium",
        SpiThinkingLevel::High => "high",
        SpiThinkingLevel::Xhigh => "xhigh",
    }
}

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
        ProjectPermission::Use,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let response = agents
        .iter()
        .map(build_project_agent_response)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(response))
}

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
        ProjectPermission::Configure,
    )
    .await?;

    let name = required_trimmed(req.name, "name")?;
    let agent_type = required_trimmed(req.agent_type, "agent_type")?;
    ensure_known_execution_profile(state.as_ref(), &agent_type).await?;
    if state
        .repos
        .project_agent_repo
        .get_by_project_and_name(project_id, &name)
        .await
        .map_err(ApiError::from)?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "Project Agent key 已存在: {name}"
        )));
    }

    let lifecycle_key =
        resolve_lifecycle_key_for_project_agent(&state, project_id, req.default_lifecycle_key)
            .await?;
    let mut agent = ProjectAgent::new(project_id, name, agent_type);
    if let Some(config) = req.config {
        agent.config = canonical_project_agent_config(config)?;
    }
    agent.default_lifecycle_key = lifecycle_key;
    state
        .repos
        .project_agent_repo
        .create(&agent)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(build_project_agent_response(&agent)?))
}

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
        ProjectPermission::Configure,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;
    let mut agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent {project_agent_id} 不存在")))?;

    if let Some(name) = req.name {
        agent.name = required_trimmed(name, "name")?;
    }
    if let Some(agent_type) = req.agent_type {
        let agent_type = required_trimmed(agent_type, "agent_type")?;
        ensure_known_execution_profile(state.as_ref(), &agent_type).await?;
        agent.agent_type = agent_type;
    }
    if let Some(config) = req.config {
        agent.config = canonical_project_agent_config(config)?;
    }
    if let Some(default_lifecycle_key) = req.default_lifecycle_key {
        agent.default_lifecycle_key = resolve_lifecycle_key_for_project_agent(
            &state,
            project_id,
            Some(default_lifecycle_key),
        )
        .await?;
    }
    if let Some(knowledge_enabled) = req.knowledge_enabled {
        agent.knowledge_enabled = knowledge_enabled;
    }
    agent.updated_at = chrono::Utc::now();
    state
        .repos
        .project_agent_repo
        .update(&agent)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(build_project_agent_response(&agent)?))
}

pub async fn delete_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
) -> Result<Json<DeletedFlagResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;
    let routines = state
        .repos
        .routine_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    if routines
        .iter()
        .any(|routine| routine.project_agent_id == project_agent_id)
    {
        return Err(ApiError::BadRequest(
            "该 Project Agent 仍被 Routine 使用，需先调整或删除相关 Routine".to_string(),
        ));
    }
    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectAgent, project_agent_id)
        .await
        .map_err(ApiError::from)?;
    state
        .repos
        .project_agent_repo
        .delete(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DeletedFlagResponse { deleted: true }))
}

fn build_project_agent_response(agent: &ProjectAgent) -> Result<ProjectAgentResponse, ApiError> {
    Ok(ProjectAgentResponse {
        id: agent.id.to_string(),
        project_id: agent.project_id.to_string(),
        name: agent.name.clone(),
        agent_type: agent.agent_type.clone(),
        config: canonical_project_agent_config(agent.config.clone())?,
        default_lifecycle_key: agent.default_lifecycle_key.clone(),
        knowledge_enabled: agent.knowledge_enabled,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    })
}

fn canonical_project_agent_config(
    config: serde_json::Value,
) -> Result<serde_json::Value, ApiError> {
    let config = AgentPresetConfig::from_json(&config).map_err(ApiError::from)?;
    serde_json::to_value(config).map_err(|error| ApiError::Internal(error.to_string()))
}

fn required_trimmed(value: String, field: &str) -> Result<String, ApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value)
}

async fn ensure_known_execution_profile(
    state: &AppState,
    agent_type: &str,
) -> Result<(), ApiError> {
    if !crate::routes::execution_profiles::is_known_execution_profile(state, agent_type).await? {
        return Err(ApiError::BadRequest(format!(
            "未知 execution profile: {agent_type}"
        )));
    }
    Ok(())
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}

fn parse_project_agent_id(project_agent_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_agent_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_agent_id: {project_agent_id}")))
}

async fn resolve_lifecycle_key_for_project_agent(
    state: &Arc<AppState>,
    project_id: Uuid,
    lifecycle_key: Option<String>,
) -> Result<Option<String>, ApiError> {
    let Some(lifecycle_key) = lifecycle_key else {
        return Ok(None);
    };
    let lifecycle_key = lifecycle_key.trim().to_string();
    if lifecycle_key.is_empty() {
        return Ok(None);
    }
    state
        .repos
        .workflow_graph_repo
        .get_by_project_and_key(project_id, &lifecycle_key)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Lifecycle `{lifecycle_key}` 不存在")))?;
    Ok(Some(lifecycle_key))
}
