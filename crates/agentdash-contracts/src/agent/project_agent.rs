use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::agent_run_mailbox::{
    AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunMessageCommandResponse,
    BackendSelectionRequestDto,
};
use crate::workflow::{
    AgentFrameRefDto, AgentRunRefDto, ConversationEffectiveExecutorConfigView, LifecycleRunRefDto,
    SubjectRefDto,
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
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset_name: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateProjectAgentRunRequest {
    /// canonical 用户输入，与 steer / lifecycle message 同形。
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub executor_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject_ref: Option<SubjectRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_selection: Option<BackendSelectionRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAgentRunStartResult {
    pub command_receipt: AgentRunCommandReceipt,
    pub accepted_refs: AgentRunAcceptedRefs,
    pub initial_message: AgentRunMessageCommandResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigView>,
    pub agent: ProjectAgentSummary,
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
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
    pub knowledge_enabled: Option<bool>,
}
