use serde::{Deserialize, Serialize};

fn bool_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextContainerCapability {
    Read,
    Write,
    List,
    Search,
    Exec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextContainerFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextContainerProvider {
    InlineFiles {
        files: Vec<ContextContainerFile>,
    },
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextContainerExposure {
    #[serde(default = "bool_true")]
    pub include_in_task_sessions: bool,
    #[serde(default = "bool_true")]
    pub include_in_story_sessions: bool,
    #[serde(default)]
    pub allowed_agent_types: Vec<String>,
}

impl Default for ContextContainerExposure {
    fn default() -> Self {
        Self {
            include_in_task_sessions: true,
            include_in_story_sessions: true,
            allowed_agent_types: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextContainerDefinition {
    pub id: String,
    pub mount_id: String,
    pub display_name: String,
    pub provider: ContextContainerProvider,
    #[serde(default)]
    pub capabilities: Vec<ContextContainerCapability>,
    #[serde(default)]
    pub default_write: bool,
    #[serde(default)]
    pub exposure: ContextContainerExposure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountDerivationPolicy {
    #[serde(default = "bool_true")]
    pub include_local_workspace: bool,
    #[serde(default)]
    pub local_workspace_capabilities: Vec<ContextContainerCapability>,
}

impl Default for MountDerivationPolicy {
    fn default() -> Self {
        Self {
            include_local_workspace: true,
            local_workspace_capabilities: Vec::new(),
        }
    }
}
