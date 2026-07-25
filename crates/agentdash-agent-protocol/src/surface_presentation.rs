use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentSurfaceInstructionPresentation {
    SystemGuidelines,
    Identity,
    AssignmentContext,
    Environment,
    MemoryContext,
    UserContext,
    CapabilityManifest { manifest: AgentCapabilityManifest },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityManifest {
    pub tool_capabilities: Vec<String>,
    pub tool_clusters: Vec<String>,
    pub included_tool_paths: Vec<String>,
    pub excluded_tool_paths: Vec<String>,
    pub mcp_servers: Vec<AgentCapabilityMcpServer>,
    pub companion_agents: Vec<AgentCapabilityCompanionAgent>,
    pub channels: Vec<AgentCapabilityChannel>,
    pub vfs: Option<AgentCapabilityVfs>,
    pub skills: Vec<AgentCapabilitySkill>,
    pub skill_diagnostics: Vec<AgentCapabilityDiagnostic>,
    pub memory_sources: Vec<AgentCapabilityMemorySource>,
    pub memory_diagnostics: Vec<AgentCapabilityDiagnostic>,
    pub workspace_module: AgentCapabilityWorkspaceModule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityMcpServer {
    pub name: String,
    pub uses_relay: bool,
    pub status: String,
    pub tool_count: Option<u32>,
    pub reason_code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityCompanionAgent {
    pub agent_key: String,
    pub executor: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityChannel {
    pub channel_ref: String,
    pub aliases: Vec<String>,
    pub operations: Vec<String>,
    pub readiness: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityVfs {
    pub default_mount: Option<String>,
    pub mounts: Vec<AgentCapabilityMount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityMount {
    pub id: String,
    pub display_name: String,
    pub root_ref: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilitySkill {
    pub name: String,
    pub capability_key: String,
    pub provider_key: String,
    pub local_name: String,
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    pub base_dir: Option<String>,
    pub exposure: String,
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityMemorySource {
    pub provider_key: String,
    pub source_key: String,
    pub display_name: String,
    pub source_uri: String,
    pub index_uri: String,
    pub mount_id: String,
    pub scope: String,
    pub index_status: String,
    pub trust_level: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityDiagnostic {
    pub provider_key: String,
    pub code: String,
    pub message: String,
    pub source_key: Option<String>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AgentCapabilityWorkspaceModule {
    pub mode: String,
    pub allowed_module_ids: Vec<String>,
}
