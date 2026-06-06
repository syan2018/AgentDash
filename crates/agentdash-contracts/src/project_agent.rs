use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::workflow::{
    AgentFrameRefDto, LifecycleAgentRefDto, LifecycleRunRefDto, RuntimeSessionRefDto, SubjectRefDto,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
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
pub struct ProjectAgentSummary {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor: ProjectAgentExecutor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset_name: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAgentLaunchResult {
    pub created: bool,
    pub agent: ProjectAgentSummary,
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: LifecycleAgentRefDto,
    pub frame_ref: AgentFrameRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject_ref: Option<SubjectRefDto>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateProjectAgentSessionRequest {
    /// canonical 用户输入，与 steer / lifecycle message 同形。
    pub input: Vec<codex::UserInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub executor_config: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAgentSessionStartResult {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub agent: ProjectAgentSummary,
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: LifecycleAgentRefDto,
    pub frame_ref: AgentFrameRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject_ref: Option<SubjectRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
    pub is_default_for_story: bool,
    #[serde(default)]
    pub is_default_for_task: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
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
    pub is_default_for_story: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub is_default_for_task: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub knowledge_enabled: Option<bool>,
}
