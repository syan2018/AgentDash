//! Story 层 MCP Server — Story 上下文编排工具
//!
//! 面向编排 Agent（如 PlanAgent），在 Story 粒度上提供上下文管理能力。
//! 典型场景：PlanAgent 探索项目代码后，更新 Story 上下文、拆解 Task。
//!
//! 每个 StoryMcpServer 实例绑定到一个具体的 Story，工具操作范围受限于该 Story。

use std::sync::Arc;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::McpError;
use crate::services::McpServices;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateStoryContextParams {
    #[schemars(description = "PRD 文档内容（覆盖更新）")]
    pub prd_doc: Option<String>,
    #[schemars(description = "追加的规范引用列表")]
    pub add_spec_refs: Option<Vec<String>>,
    #[schemars(description = "追加的资源清单项 [{name, uri, resource_type}]")]
    pub add_resources: Option<Vec<ResourceInput>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResourceInput {
    pub name: String,
    pub uri: String,
    pub resource_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskParams {
    #[schemars(description = "Task 标题")]
    pub title: String,
    #[schemars(description = "Task 描述（包含执行指令和上下文）")]
    pub description: String,
    #[schemars(description = "关联的 Workspace UUID（可选）")]
    pub workspace_id: Option<String>,
    #[schemars(description = "Agent 类型提示（如 claude-code / codex）")]
    pub agent_type: Option<String>,
    #[schemars(description = "初始上下文（拼接在提示词前的额外信息）")]
    pub initial_context: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BatchCreateTasksParams {
    #[schemars(description = "批量创建的 Task 列表")]
    pub tasks: Vec<TaskInput>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskInput {
    pub title: String,
    pub description: String,
    pub workspace_id: Option<String>,
    pub agent_type: Option<String>,
    pub initial_context: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AdvanceStoryStatusParams {
    #[schemars(description = "目标状态：context_ready / decomposed / executing")]
    pub target_status: String,
    #[schemars(description = "状态变更原因说明")]
    pub reason: String,
}

// ─── Server 定义 ──────────────────────────────────────────────

/// Story 层 MCP Server
///
/// 绑定到具体 Story 实例，暴露上下文管理与 Task 编排工具。
/// 工具操作范围限定在所绑定的 Story 内。
#[derive(Clone)]
pub struct StoryMcpServer {
    services: Arc<McpServices>,
    story_id: Uuid,
    #[allow(dead_code)]
    project_id: Uuid,
    tool_router: ToolRouter<Self>,
}

impl StoryMcpServer {
    pub fn new(services: Arc<McpServices>, project_id: Uuid, story_id: Uuid) -> Self {
        Self {
            services,
            story_id,
            project_id,
            tool_router: Self::tool_router(),
        }
    }

    async fn load_story(&self) -> Result<agentdash_domain::story::Story, McpError> {
        self.services
            .story_repo
            .get_by_id(self.story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", self.story_id))
    }
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl StoryMcpServer {
    #[tool(description = "获取当前 Story 的完整上下文信息（PRD、规范引用、资源清单）")]
    async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let story = self.load_story().await?;

        let result = serde_json::json!({
            "story_id": story.id.to_string(),
            "title": story.title,
            "description": story.description,
            "status": story.status,
            "context": {
                "prd_doc": story.context.prd_doc,
                "spec_refs": story.context.spec_refs,
                "resource_list": story.context.resource_list,
            },
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "更新 Story 上下文：可设置 PRD、追加规范引用和资源清单")]
    async fn update_story_context(
        &self,
        Parameters(params): Parameters<UpdateStoryContextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut story = self.load_story().await?;

        if let Some(prd) = params.prd_doc {
            story.context.prd_doc = Some(prd);
        }

        if let Some(refs) = params.add_spec_refs {
            story.context.spec_refs.extend(refs);
        }

        if let Some(resources) = params.add_resources {
            for r in resources {
                story
                    .context
                    .resource_list
                    .push(agentdash_domain::story::Resource {
                        name: r.name,
                        uri: r.uri,
                        resource_type: r.resource_type,
                    });
            }
        }

        story.updated_at = chrono::Utc::now();

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Story {} 上下文已更新（spec_refs: {} 项, resources: {} 项）",
            self.story_id,
            story.context.spec_refs.len(),
            story.context.resource_list.len(),
        ))]))
    }

    #[tool(description = "在当前 Story 下创建一个新的 Task（执行单元）")]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use agentdash_domain::task::{AgentBinding, Task};

        let workspace_id = params
            .workspace_id
            .as_deref()
            .map(|s| {
                Uuid::parse_str(s)
                    .map_err(|_| McpError::invalid_param("workspace_id", "无效的 UUID"))
            })
            .transpose()?;

        let mut task = Task::new(self.story_id, params.title, params.description);
        task.workspace_id = workspace_id;
        task.agent_binding = AgentBinding {
            agent_type: params.agent_type,
            initial_context: params.initial_context,
            ..Default::default()
        };

        self.services
            .task_repo
            .create(&task)
            .await
            .map_err(McpError::from)?;

        let result = serde_json::json!({
            "task_id": task.id.to_string(),
            "story_id": self.story_id.to_string(),
            "status": "pending",
            "message": "Task 已创建",
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "在当前 Story 下批量创建多个 Task（通常用于 Story 拆解完成后一次性创建）")]
    async fn batch_create_tasks(
        &self,
        Parameters(params): Parameters<BatchCreateTasksParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use agentdash_domain::task::{AgentBinding, Task};

        let mut created_ids = Vec::new();

        for input in &params.tasks {
            let workspace_id = input
                .workspace_id
                .as_deref()
                .map(|s| {
                    Uuid::parse_str(s)
                        .map_err(|_| McpError::invalid_param("workspace_id", "无效的 UUID"))
                })
                .transpose()?;

            let mut task =
                Task::new(self.story_id, input.title.clone(), input.description.clone());
            task.workspace_id = workspace_id;
            task.agent_binding = AgentBinding {
                agent_type: input.agent_type.clone(),
                initial_context: input.initial_context.clone(),
                ..Default::default()
            };

            self.services
                .task_repo
                .create(&task)
                .await
                .map_err(McpError::from)?;

            created_ids.push(task.id.to_string());
        }

        let result = serde_json::json!({
            "story_id": self.story_id.to_string(),
            "created_count": created_ids.len(),
            "task_ids": created_ids,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "列出当前 Story 下的所有 Task 及其状态")]
    async fn list_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = self
            .services
            .task_repo
            .list_by_story(self.story_id)
            .await
            .map_err(McpError::from)?;

        let result: Vec<serde_json::Value> = tasks
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "title": t.title,
                    "description": t.description,
                    "status": t.status,
                    "workspace_id": t.workspace_id.map(|w| w.to_string()),
                    "agent_type": t.agent_binding.agent_type,
                    "session_id": t.session_id,
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "推进 Story 生命周期状态（如从 created 到 context_ready，或到 decomposed）")]
    async fn advance_story_status(
        &self,
        Parameters(params): Parameters<AdvanceStoryStatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut story = self.load_story().await?;

        let new_status: agentdash_domain::story::StoryStatus =
            serde_json::from_value(serde_json::Value::String(params.target_status.clone()))
                .map_err(|_| {
                    McpError::invalid_param(
                        "target_status",
                        format!("无效的状态值: {}", params.target_status),
                    )
                })?;

        tracing::info!(
            story_id = %self.story_id,
            from = ?story.status,
            to = ?new_status,
            reason = %params.reason,
            "推进 Story 状态"
        );

        story.status = new_status;
        story.updated_at = chrono::Utc::now();

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Story {} 状态已推进为 {}（原因: {}）",
            self.story_id, params.target_status, params.reason
        ))]))
    }
}

// ─── ServerHandler 实现 ──────────────────────────────────────

#[tool_handler]
impl ServerHandler for StoryMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            format!(
                "Story 编排工具（Story: {}）。可用于管理 Story 上下文、创建 Task、推进状态。",
                self.story_id
            ),
        )
    }
}
