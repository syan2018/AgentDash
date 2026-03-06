//! Relay 层 MCP Server — 面向用户的看板全局操作
//!
//! 直接面向用户（或用户侧 Agent），提供跨 Project 的全局看板管理能力。
//! 典型调用者：IDE 中的用户 Agent、Web 前端的 MCP 客户端。

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
pub struct ListProjectsParams {
    #[schemars(description = "按项目名称关键字过滤（可选）")]
    pub keyword: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetProjectParams {
    #[schemars(description = "项目 UUID")]
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateStoryParams {
    #[schemars(description = "目标项目 UUID")]
    pub project_id: String,
    #[schemars(description = "Story 标题")]
    pub title: String,
    #[schemars(description = "Story 描述（需求详情）")]
    pub description: String,
    #[schemars(description = "Story 类型：feature / bugfix / refactor / docs / test / other")]
    pub story_type: Option<String>,
    #[schemars(description = "优先级：p0 / p1 / p2 / p3")]
    pub priority: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListStoriesParams {
    #[schemars(description = "按项目 UUID 过滤")]
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetStoryDetailParams {
    #[schemars(description = "Story UUID")]
    pub story_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateStoryStatusParams {
    #[schemars(description = "Story UUID")]
    pub story_id: String,
    #[schemars(description = "目标状态：created / context_ready / decomposed / executing / completed / failed / cancelled")]
    pub status: String,
}

// ─── Server 定义 ──────────────────────────────────────────────

/// Relay 层 MCP Server
///
/// 暴露面向用户的看板全局工具：
/// - 项目列表/详情查看
/// - Story 创建、状态变更
/// - 全局看板视图操作
#[derive(Clone)]
pub struct RelayMcpServer {
    services: Arc<McpServices>,
    tool_router: ToolRouter<Self>,
}

impl RelayMcpServer {
    pub fn new(services: Arc<McpServices>) -> Self {
        Self {
            services,
            tool_router: Self::tool_router(),
        }
    }

    fn parse_uuid(s: &str, field: &'static str) -> Result<Uuid, McpError> {
        Uuid::parse_str(s).map_err(|_| McpError::invalid_param(field, format!("无效的 UUID: {s}")))
    }
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl RelayMcpServer {
    #[tool(description = "列出所有项目，可按名称关键字过滤")]
    async fn list_projects(
        &self,
        Parameters(params): Parameters<ListProjectsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let projects = self
            .services
            .project_repo
            .list_all()
            .await
            .map_err(McpError::from)?;

        let filtered: Vec<_> = match &params.keyword {
            Some(kw) => projects
                .into_iter()
                .filter(|p| p.name.contains(kw.as_str()))
                .collect(),
            None => projects,
        };

        let summary: Vec<serde_json::Value> = filtered
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "description": p.description,
                    "backend_id": p.backend_id,
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        )]))
    }

    #[tool(description = "获取指定项目的完整信息，包括配置和关联的 Story 概况")]
    async fn get_project(
        &self,
        Parameters(params): Parameters<GetProjectParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = Self::parse_uuid(&params.project_id, "project_id")?;
        let project = self
            .services
            .project_repo
            .get_by_id(id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Project", &params.project_id))?;

        let stories = self
            .services
            .story_repo
            .list_by_project(id)
            .await
            .map_err(McpError::from)?;

        let workspaces = self
            .services
            .workspace_repo
            .list_by_project(id)
            .await
            .map_err(McpError::from)?;

        let result = serde_json::json!({
            "project": {
                "id": project.id.to_string(),
                "name": project.name,
                "description": project.description,
                "backend_id": project.backend_id,
                "config": project.config,
                "created_at": project.created_at.to_rfc3339(),
            },
            "story_count": stories.len(),
            "stories": stories.iter().map(|s| serde_json::json!({
                "id": s.id.to_string(),
                "title": s.title,
                "status": s.status,
                "priority": s.priority,
            })).collect::<Vec<_>>(),
            "workspace_count": workspaces.len(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "在指定项目中创建一个新的 Story（用户价值单元）")]
    async fn create_story(
        &self,
        Parameters(params): Parameters<CreateStoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use agentdash_domain::story::Story;

        let project_id = Self::parse_uuid(&params.project_id, "project_id")?;

        let project = self
            .services
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Project", &params.project_id))?;

        let story = Story::new(
            project_id,
            project.backend_id.clone(),
            params.title,
            params.description,
        );

        self.services
            .story_repo
            .create(&story)
            .await
            .map_err(McpError::from)?;

        let result = serde_json::json!({
            "story_id": story.id.to_string(),
            "project_id": project_id.to_string(),
            "status": "created",
            "message": "Story 已创建",
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "列出指定项目下的所有 Story")]
    async fn list_stories(
        &self,
        Parameters(params): Parameters<ListStoriesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let project_id = Self::parse_uuid(&params.project_id, "project_id")?;

        let stories = self
            .services
            .story_repo
            .list_by_project(project_id)
            .await
            .map_err(McpError::from)?;

        let result: Vec<serde_json::Value> = stories
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id.to_string(),
                    "title": s.title,
                    "description": s.description,
                    "status": s.status,
                    "priority": s.priority,
                    "story_type": s.story_type,
                    "task_count": s.task_count,
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "获取 Story 的完整详情，包括上下文信息和关联的 Task 列表")]
    async fn get_story_detail(
        &self,
        Parameters(params): Parameters<GetStoryDetailParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let story_id = Self::parse_uuid(&params.story_id, "story_id")?;

        let story = self
            .services
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", &params.story_id))?;

        let tasks = self
            .services
            .task_repo
            .list_by_story(story_id)
            .await
            .map_err(McpError::from)?;

        let result = serde_json::json!({
            "story": {
                "id": story.id.to_string(),
                "project_id": story.project_id.to_string(),
                "title": story.title,
                "description": story.description,
                "status": story.status,
                "priority": story.priority,
                "story_type": story.story_type,
                "context": story.context,
                "task_count": story.task_count,
                "created_at": story.created_at.to_rfc3339(),
            },
            "tasks": tasks.iter().map(|t| serde_json::json!({
                "id": t.id.to_string(),
                "title": t.title,
                "status": t.status,
                "workspace_id": t.workspace_id.map(|w| w.to_string()),
            })).collect::<Vec<_>>(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "变更 Story 状态（如从 created 推进到 context_ready）")]
    async fn update_story_status(
        &self,
        Parameters(params): Parameters<UpdateStoryStatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let story_id = Self::parse_uuid(&params.story_id, "story_id")?;

        let mut story = self
            .services
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", &params.story_id))?;

        let new_status: agentdash_domain::story::StoryStatus =
            serde_json::from_value(serde_json::Value::String(params.status.clone())).map_err(
                |_| McpError::invalid_param("status", format!("无效的状态值: {}", params.status)),
            )?;

        story.status = new_status;
        story.updated_at = chrono::Utc::now();

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Story {} 状态已更新为 {}",
            story_id, params.status
        ))]))
    }
}

// ─── ServerHandler 实现 ──────────────────────────────────────

#[tool_handler]
impl ServerHandler for RelayMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("AgentDashboard 看板操作工具。可用于查看项目、创建和管理 Story、查看 Task 状态等。")
    }
}
