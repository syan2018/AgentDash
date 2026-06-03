use serde::{Deserialize, Serialize};

use agentdash_domain::routine::{Routine, RoutineExecution};
use agentdash_domain::workflow::AgentRuntimeRefs;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RoutineCreationResponse {
    #[serde(flatten)]
    pub routine: RoutineResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RegenerateTokenResponse {
    pub endpoint_id: String,
    pub webhook_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RoutineResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub prompt_template: String,
    pub project_agent_id: String,
    pub trigger_config: serde_json::Value,
    pub dispatch_strategy: serde_json::Value,
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
            project_agent_id: r.project_agent_id.to_string(),
            trigger_config: serde_json::to_value(&r.trigger_config).unwrap_or_default(),
            dispatch_strategy: serde_json::to_value(&r.dispatch_strategy).unwrap_or_default(),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_refs: Option<AgentRuntimeRefs>,
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
            runtime_refs: e.dispatch_refs.map(|r| r.runtime_refs),
            status: format!("{:?}", e.status).to_lowercase(),
            started_at: e.started_at.to_rfc3339(),
            completed_at: e.completed_at.map(|t| t.to_rfc3339()),
            error: e.error,
            entity_key: e.entity_key,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRoutineRequest {
    pub name: String,
    pub prompt_template: String,
    pub project_agent_id: String,
    pub trigger_config: serde_json::Value,
    #[serde(default)]
    pub dispatch_strategy: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoutineRequest {
    pub name: Option<String>,
    pub prompt_template: Option<String>,
    pub project_agent_id: Option<String>,
    pub trigger_config: Option<serde_json::Value>,
    pub dispatch_strategy: Option<serde_json::Value>,
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
