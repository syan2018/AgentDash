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
pub struct ExecutionProfileDto {
    pub id: String,
    pub name: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileDiscoveryResponse {
    pub executors: Vec<ExecutionProfileDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileProviderDto {
    pub id: String,
    pub name: String,
    pub executable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileModelDto {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub reasoning: bool,
    pub supports_image: bool,
    pub context_window: u32,
    pub blocked: bool,
    pub discovered: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileModelSelectorDto {
    pub providers: Vec<ExecutionProfileProviderDto>,
    pub models: Vec<ExecutionProfileModelDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_model: Option<String>,
    pub agents: Vec<ExecutionProfileAgentDto>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileAgentDto {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileOptionsDto {
    pub model_selector: ExecutionProfileModelSelectorDto,
    pub slash_commands: Vec<ExecutionProfileSlashCommandDto>,
    pub loading_models: bool,
    pub loading_agents: bool,
    pub loading_slash_commands: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProfileSlashCommandDto {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
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
#[serde(deny_unknown_fields)]
pub struct AgentRunModelSelectionRequest {
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
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRuntimeOptionsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_policy: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectAgentRunRequest {
    /// canonical 用户输入，与 steer / lifecycle message 同形。
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_selection: Option<AgentRunModelSelectionRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_options: Option<AgentRunRuntimeOptionsRequest>,
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
