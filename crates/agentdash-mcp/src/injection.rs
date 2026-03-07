//! MCP 能力注入模块
//!
//! 提供 `McpInjectionConfig`，用于在 Agent 上下文构建时注入 MCP Server 连接信息。
//! Agent 收到包含 MCP 端点信息的上下文后，可自动连接对应的 MCP Server 获取工具。
//!
//! ## 三层注入策略
//!
//! | 调用场景 | 注入的 MCP Server | 工具能力 |
//! |---------|------------------|---------|
//! | Task 执行 | TaskMcpServer | 状态更新、产物上报、上下文查看 |
//! | Story 编排 | StoryMcpServer | 上下文管理、Task 创建、状态推进 |
//! | 全局代理 | RelayMcpServer | 项目管理、Story CRUD |

use agent_client_protocol::{McpServer, McpServerHttp};
use uuid::Uuid;

use crate::scope::ToolScope;

/// MCP 注入配置 — 描述要注入给 Agent 的 MCP 端点
#[derive(Debug, Clone)]
pub struct McpInjectionConfig {
    /// MCP 服务基础 URL（如 "http://localhost:3001"）
    pub base_url: String,
    /// 工具层级
    pub scope: ToolScope,
    /// 关联的 Project ID
    pub project_id: Uuid,
    /// 关联的 Story ID（Story/Task 层必填）
    pub story_id: Option<Uuid>,
    /// 关联的 Task ID（仅 Task 层）
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
        }
    }

    pub fn server_name(&self) -> String {
        match self.scope {
            ToolScope::Relay => "agentdash-relay-tools".to_string(),
            ToolScope::Story => format!(
                "agentdash-story-tools-{}",
                &self.story_id.unwrap().to_string()[..8]
            ),
            ToolScope::Task => format!(
                "agentdash-task-tools-{}",
                &self.task_id.unwrap().to_string()[..8]
            ),
        }
    }

    /// 生成标准格式的上下文片段内容
    pub fn to_context_content(&self) -> String {
        let scope_label = match self.scope {
            ToolScope::Relay => "relay",
            ToolScope::Story => "story",
            ToolScope::Task => "task",
        };
        let tool_desc = match self.scope {
            ToolScope::Relay => "项目管理、Story 创建与状态变更",
            ToolScope::Story => "Story 上下文管理、Task 创建与批量拆解、状态推进",
            ToolScope::Task => "Task 状态更新、执行产物上报、兄弟 Task 查看、Story 上下文读取",
        };

        format!(
            "## MCP: {name}\n- url: {url}\n- scope: {scope}\n\
             可通过此 MCP Server 使用以下能力：{desc}",
            name = self.server_name(),
            url = self.endpoint_url(),
            scope = scope_label,
            desc = tool_desc,
        )
    }

    /// 产出 ACP 协议标准的 `McpServer` — 用于 `session/new` 的 per-session 工具注入
    pub fn to_acp_mcp_server(&self) -> McpServer {
        McpServer::Http(McpServerHttp::new(self.server_name(), self.endpoint_url()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_injection_generates_correct_endpoint() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();

        let config =
            McpInjectionConfig::for_task("http://localhost:3001", project_id, story_id, task_id);

        assert_eq!(
            config.endpoint_url(),
            format!("http://localhost:3001/mcp/task/{task_id}")
        );
        assert!(config.server_name().starts_with("agentdash-task-tools-"));
    }

    #[test]
    fn story_injection_generates_correct_endpoint() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();

        let config = McpInjectionConfig::for_story("http://localhost:3001", project_id, story_id);

        assert_eq!(
            config.endpoint_url(),
            format!("http://localhost:3001/mcp/story/{story_id}")
        );
    }

    #[test]
    fn relay_injection_generates_correct_endpoint() {
        let project_id = Uuid::new_v4();
        let config = McpInjectionConfig::for_relay("http://localhost:3001/", project_id);

        assert_eq!(config.endpoint_url(), "http://localhost:3001/mcp/relay");
    }

    #[test]
    fn context_content_includes_required_fields() {
        let config = McpInjectionConfig::for_task(
            "http://localhost:3001",
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let content = config.to_context_content();

        assert!(content.contains("## MCP: "));
        assert!(content.contains("- url: http://localhost:3001/mcp/task/"));
        assert!(content.contains("- scope: task"));
        assert!(content.contains("Task 状态更新"));
    }

    #[test]
    fn to_acp_mcp_server_produces_http_variant() {
        let task_id = Uuid::new_v4();
        let config = McpInjectionConfig::for_task(
            "http://localhost:3001",
            Uuid::new_v4(),
            Uuid::new_v4(),
            task_id,
        );
        let server = config.to_acp_mcp_server();
        let json = serde_json::to_value(&server).unwrap();
        assert_eq!(json["type"], "http");
        assert!(json["url"].as_str().unwrap().contains("/mcp/task/"));
        assert!(
            json["name"]
                .as_str()
                .unwrap()
                .starts_with("agentdash-task-tools-")
        );
    }
}
