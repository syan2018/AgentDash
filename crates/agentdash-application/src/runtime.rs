use serde::{Deserialize, Serialize};

pub use agentdash_domain::common::{
    AgentConfig, AgentPresetConfig, Mount, MountCapability, SystemPromptMode, ThinkingLevel, Vfs,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerSummary {
    pub name: String,
    pub transport: String,
    pub target: String,
}

impl McpServerSummary {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn transport_label(&self) -> &str {
        self.transport.as_str()
    }

    pub fn target(&self) -> String {
        self.target.clone()
    }
}

pub use agentdash_spi::platform::mount::RuntimeFileEntry;
