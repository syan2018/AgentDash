use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectAgentExecutor {
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectAgentSession {
    pub binding_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub last_activity: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectAgentSummary {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor: ProjectAgentExecutor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset_name: Option<String>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<ProjectAgentSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OpenProjectAgentSessionResult {
    pub created: bool,
    pub session_id: String,
    pub binding_id: String,
    pub agent: ProjectAgentSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectAgent {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub agent_type: String,
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_lifecycle_key: Option<String>,
    pub is_default_for_story: bool,
    pub is_default_for_task: bool,
    pub knowledge_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectAgentRequest {
    pub name: String,
    pub agent_type: String,
    #[serde(default)]
    #[ts(optional)]
    pub config: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    pub is_default_for_story: bool,
    #[serde(default)]
    pub is_default_for_task: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectAgentRequest {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub agent_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub config: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub is_default_for_story: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub is_default_for_task: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub knowledge_enabled: Option<bool>,
}
