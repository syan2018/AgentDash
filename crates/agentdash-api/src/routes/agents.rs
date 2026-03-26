use std::sync::Arc;

use agentdash_domain::agent::Agent;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub agent_type: String,
    pub base_config: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Agent> for AgentResponse {
    fn from(agent: Agent) -> Self {
        Self {
            id: agent.id.to_string(),
            name: agent.name,
            agent_type: agent.agent_type,
            base_config: agent.base_config,
            created_at: agent.created_at.to_rfc3339(),
            updated_at: agent.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub agent_type: String,
    #[serde(default)]
    pub base_config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub agent_type: Option<String>,
    pub base_config: Option<serde_json::Value>,
}

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<AgentResponse>>, ApiError> {
    let agents = state
        .repos
        .agent_repo
        .list_all()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(agents.into_iter().map(AgentResponse::from).collect()))
}

pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }
    let agent_type = req.agent_type.trim().to_string();
    if agent_type.is_empty() {
        return Err(ApiError::BadRequest("agent_type 不能为空".into()));
    }

    let mut agent = Agent::new(name, agent_type);
    if let Some(config) = req.base_config {
        agent.base_config = config;
    }

    state
        .repos
        .agent_repo
        .create(&agent)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentResponse::from(agent)))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, ApiError> {
    let id = parse_agent_id(&id)?;
    let agent = state
        .repos
        .agent_repo
        .get_by_id(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Agent {id} 不存在")))?;
    Ok(Json(AgentResponse::from(agent)))
}

pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let id = parse_agent_id(&id)?;
    let mut agent = state
        .repos
        .agent_repo
        .get_by_id(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Agent {id} 不存在")))?;

    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("name 不能为空".into()));
        }
        agent.name = trimmed;
    }
    if let Some(agent_type) = req.agent_type {
        agent.agent_type = agent_type.trim().to_string();
    }
    if let Some(config) = req.base_config {
        agent.base_config = config;
    }
    agent.updated_at = chrono::Utc::now();

    state
        .repos
        .agent_repo
        .update(&agent)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentResponse::from(agent)))
}

pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_agent_id(&id)?;
    let links = state
        .repos
        .agent_link_repo
        .list_by_agent(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !links.is_empty() {
        return Err(ApiError::Conflict(format!(
            "Agent 仍关联 {} 个项目，请先解除关联",
            links.len()
        )));
    }
    state
        .repos
        .agent_repo
        .delete(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

fn parse_agent_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("无效的 agent_id: {id}")))
}
