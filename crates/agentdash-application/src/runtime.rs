use std::collections::BTreeMap;

use agentdash_domain::common::MountCapability;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
}

impl ExecutorConfig {
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            variant: None,
            provider_id: None,
            model_id: None,
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
        }
    }
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self::new("CLAUDE_CODE")
    }
}

pub type MountCapabilitySet = MountCapability;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMount {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: Vec<MountCapabilitySet>,
    pub default_write: bool,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl RuntimeMount {
    pub fn supports(&self, capability: MountCapabilitySet) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeAddressSpace {
    #[serde(default)]
    pub mounts: Vec<RuntimeMount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_story_id: Option<String>,
}

impl RuntimeAddressSpace {
    pub fn default_mount(&self) -> Option<&RuntimeMount> {
        let default_id = self.default_mount_id.as_deref()?;
        self.mounts.iter().find(|mount| mount.id == default_id)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeToolScope {
    Relay,
    Story,
    Task,
}

#[derive(Debug, Clone)]
pub struct RuntimeMcpBinding {
    pub base_url: String,
    pub scope: RuntimeToolScope,
    pub project_id: Uuid,
    pub story_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

impl RuntimeMcpBinding {
    pub fn for_task(
        base_url: impl Into<String>,
        project_id: Uuid,
        story_id: Uuid,
        task_id: Uuid,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            scope: RuntimeToolScope::Task,
            project_id,
            story_id: Some(story_id),
            task_id: Some(task_id),
        }
    }

    pub fn for_story(base_url: impl Into<String>, project_id: Uuid, story_id: Uuid) -> Self {
        Self {
            base_url: base_url.into(),
            scope: RuntimeToolScope::Story,
            project_id,
            story_id: Some(story_id),
            task_id: None,
        }
    }

    pub fn for_relay(base_url: impl Into<String>, project_id: Uuid) -> Self {
        Self {
            base_url: base_url.into(),
            scope: RuntimeToolScope::Relay,
            project_id,
            story_id: None,
            task_id: None,
        }
    }

    pub fn endpoint_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.scope {
            RuntimeToolScope::Relay => format!("{base}/mcp/relay"),
            RuntimeToolScope::Story => {
                let story_id = self.story_id.expect("Story 层必须提供 story_id");
                format!("{base}/mcp/story/{story_id}")
            }
            RuntimeToolScope::Task => {
                let task_id = self.task_id.expect("Task 层必须提供 task_id");
                format!("{base}/mcp/task/{task_id}")
            }
        }
    }

    pub fn server_name(&self) -> String {
        match self.scope {
            RuntimeToolScope::Relay => "agentdash-relay-tools".to_string(),
            RuntimeToolScope::Story => {
                let story_id = self.story_id.expect("story id").to_string();
                format!("agentdash-story-tools-{}", &story_id[..8])
            }
            RuntimeToolScope::Task => {
                let task_id = self.task_id.expect("task id").to_string();
                format!("agentdash-task-tools-{}", &task_id[..8])
            }
        }
    }

    pub fn to_runtime_server(&self) -> RuntimeMcpServer {
        RuntimeMcpServer::Http {
            name: self.server_name(),
            url: self.endpoint_url(),
        }
    }

    pub fn to_context_content(&self) -> String {
        let scope_label = match self.scope {
            RuntimeToolScope::Relay => "relay",
            RuntimeToolScope::Story => "story",
            RuntimeToolScope::Task => "task",
        };
        let tool_desc = match self.scope {
            RuntimeToolScope::Relay => "项目管理、Story 创建与状态变更",
            RuntimeToolScope::Story => "Story 上下文管理、Task 创建与批量拆解、状态推进",
            RuntimeToolScope::Task => "Task 状态更新、执行产物上报、兄弟 Task 查看、Story 上下文读取",
        };

        format!(
            "## MCP: {name}\n- url: {url}\n- scope: {scope}\n可通过此 MCP Server 使用以下能力：{desc}",
            name = self.server_name(),
            url = self.endpoint_url(),
            scope = scope_label,
            desc = tool_desc,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFileEntry {
    pub path: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
}
