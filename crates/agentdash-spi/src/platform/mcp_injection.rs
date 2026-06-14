use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{McpTransportConfig, RuntimeMcpServer};

/// MCP 工具层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    Relay,
    Story,
    Task,
    Workflow,
}

/// MCP 注入配置，描述要注入给 Agent 会话的 MCP 端点。
#[derive(Debug, Clone)]
pub struct McpInjectionConfig {
    pub base_url: String,
    pub scope: ToolScope,
    pub project_id: Uuid,
    pub story_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

impl McpInjectionConfig {
    pub fn for_task(
        base_url: impl Into<String>,
        project_id: Uuid,
        story_id: Uuid,
        task_id: Uuid,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            scope: ToolScope::Task,
            project_id,
            story_id: Some(story_id),
            task_id: Some(task_id),
        }
    }

    pub fn for_story(base_url: impl Into<String>, project_id: Uuid, story_id: Uuid) -> Self {
        Self {
            base_url: base_url.into(),
            scope: ToolScope::Story,
            project_id,
            story_id: Some(story_id),
            task_id: None,
        }
    }

    pub fn for_relay(base_url: impl Into<String>, project_id: Uuid) -> Self {
        Self {
            base_url: base_url.into(),
            scope: ToolScope::Relay,
            project_id,
            story_id: None,
            task_id: None,
        }
    }

    pub fn for_workflow(base_url: impl Into<String>, project_id: Uuid) -> Self {
        Self {
            base_url: base_url.into(),
            scope: ToolScope::Workflow,
            project_id,
            story_id: None,
            task_id: None,
        }
    }

    pub fn endpoint_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.scope {
            ToolScope::Relay => format!("{base}/mcp/relay"),
            ToolScope::Story => {
                let story_id = self.story_id.expect("Story 层必须提供 story_id");
                format!("{base}/mcp/story/{story_id}")
            }
            ToolScope::Task => {
                let task_id = self.task_id.expect("Task 层必须提供 task_id");
                format!("{base}/mcp/task/{task_id}")
            }
            ToolScope::Workflow => format!("{base}/mcp/workflow/{}", self.project_id),
        }
    }

    pub fn server_name(&self) -> String {
        match self.scope {
            ToolScope::Relay => "agentdash-relay-tools".to_string(),
            ToolScope::Story => "agentdash-story-tools".to_string(),
            ToolScope::Task => "agentdash-task-tools".to_string(),
            ToolScope::Workflow => "agentdash-workflow-tools".to_string(),
        }
    }

    pub fn to_context_content(&self) -> String {
        let scope_label = match self.scope {
            ToolScope::Relay => "relay",
            ToolScope::Story => "story",
            ToolScope::Task => "task",
            ToolScope::Workflow => "workflow",
        };
        let tool_desc = match self.scope {
            ToolScope::Relay => "项目管理、Story 创建与状态变更",
            ToolScope::Story => "Story 上下文管理、Task 创建与批量拆解、状态推进",
            ToolScope::Task => "Task 状态更新、执行产物上报、兄弟 Task 查看、Story 上下文读取",
            ToolScope::Workflow => "Workflow/Lifecycle 定义的查看、创建与编辑",
        };

        format!(
            "## MCP: {name}\n- scope: {scope}\n\
             可通过此 MCP Server 使用以下能力：{desc}",
            name = self.server_name(),
            scope = scope_label,
            desc = tool_desc,
        )
    }

    pub fn to_runtime_mcp_server(&self) -> RuntimeMcpServer {
        RuntimeMcpServer {
            name: self.server_name(),
            transport: McpTransportConfig::Http {
                url: self.endpoint_url(),
                headers: vec![],
            },
            uses_relay: false,
        }
    }
}
