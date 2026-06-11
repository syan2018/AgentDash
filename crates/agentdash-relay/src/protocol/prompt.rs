use std::collections::HashMap;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIdentityKindRelay {
    GitRepo,
    P4Workspace,
    LocalDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPromptPayload {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_blocks: Option<serde_json::Value>,
    pub mount_root_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_identity_kind: Option<WorkspaceIdentityKindRelay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_identity_payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<AgentConfigRelay>,
    #[serde(default)]
    pub mcp_servers: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigRelay {
    pub executor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
}

// ── command.cancel ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCancelPayload {
    pub session_id: String,
}

// ── command.steer ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSteerPayload {
    pub session_id: String,
    pub input: Vec<codex::UserInput>,
    pub expected_turn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePromptPayload {
    pub turn_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCancelPayload {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSteerPayload {
    pub status: String,
}
