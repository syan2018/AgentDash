use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use agentdash_domain::common::{
    Vfs, AgentConfig, Mount, MountCapability, SystemPromptMode, ThinkingLevel,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMcpServer {
    Http {
        name: String,
        url: String,
    },
    Sse {
        name: String,
        url: String,
    },
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Unsupported {
        name: String,
        transport: String,
        target: String,
    },
}

impl RuntimeMcpServer {
    pub fn name(&self) -> &str {
        match self {
            RuntimeMcpServer::Http { name, .. }
            | RuntimeMcpServer::Sse { name, .. }
            | RuntimeMcpServer::Stdio { name, .. }
            | RuntimeMcpServer::Unsupported { name, .. } => name,
        }
    }

    pub fn transport_label(&self) -> &'static str {
        match self {
            RuntimeMcpServer::Http { .. } => "http",
            RuntimeMcpServer::Sse { .. } => "sse",
            RuntimeMcpServer::Stdio { .. } => "stdio",
            RuntimeMcpServer::Unsupported { .. } => "unsupported",
        }
    }

    pub fn target(&self) -> String {
        match self {
            RuntimeMcpServer::Http { url, .. } | RuntimeMcpServer::Sse { url, .. } => url.clone(),
            RuntimeMcpServer::Stdio { command, .. } => command.clone(),
            RuntimeMcpServer::Unsupported { target, .. } => target.clone(),
        }
    }
}

pub use agentdash_spi::mount::RuntimeFileEntry;
