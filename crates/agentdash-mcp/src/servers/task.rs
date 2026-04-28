//! Task 层 MCP Server — Task 粒度执行工具
//!
//! 面向执行 Agent，在 Task 粒度上提供状态更新和产物上报能力。
//! 典型场景：Agent 完成代码编写后，通过工具上报产物、更新状态。
//!
//! 每个 TaskMcpServer 实例绑定到一个具体的 Task，工具操作范围受限于该 Task。

use std::sync::Arc;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::McpError;
use crate::services::McpServices;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskStatusParams {
    #[schemars(
        description = "目标状态：pending / assigned / running / awaiting_verification / completed / failed"
    )]
    pub status: String,
    #[schemars(description = "状态变更原因说明")]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReportArtifactParams {
    #[schemars(
        description = "产物类型：code_change / test_result / log_output / file / tool_execution"
    )]
    pub artifact_type: String,
    #[schemars(description = "产物内容。优先传 JSON 字符串；若为纯文本，可直接传文本")]
    pub content: String,
}

fn parse_artifact_content(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AppendTaskDescriptionParams {
    #[schemars(description = "追加到描述末尾的内容（用于记录执行过程发现的信息）")]
    pub append_text: String,
}

// ─── Server 定义 ──────────────────────────────────────────────

/// Task 层 MCP Server
///
/// 绑定到具体 Task 实例，暴露执行粒度的操作工具。
#[derive(Clone)]
pub struct TaskMcpServer {
    services: Arc<McpServices>,
    task_id: Uuid,
    story_id: Uuid,
    _project_id: Uuid,
    tool_router: ToolRouter<Self>,
}

impl TaskMcpServer {
    pub fn new(
        services: Arc<McpServices>,
        project_id: Uuid,
        story_id: Uuid,
        task_id: Uuid,
    ) -> Self {
        Self {
            services,
            task_id,
            story_id,
            _project_id: project_id,
            tool_router: Self::tool_router(),
        }
    }

    async fn load_task(&self) -> Result<agentdash_domain::task::Task, McpError> {
        // M1-b：Task 查询经 Story aggregate（find_by_task_id）
        let story = self
            .services
            .story_repo
            .find_by_task_id(self.task_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Task", self.task_id))?;
        story
            .find_task(self.task_id)
            .cloned()
            .ok_or_else(|| McpError::not_found("Task", self.task_id))
    }

    async fn load_story_with_task(
        &self,
    ) -> Result<(agentdash_domain::story::Story, agentdash_domain::task::Task), McpError> {
        let story = self
            .services
            .story_repo
            .find_by_task_id(self.task_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Task", self.task_id))?;
        let task = story
            .find_task(self.task_id)
            .cloned()
            .ok_or_else(|| McpError::not_found("Task", self.task_id))?;
        Ok((story, task))
    }
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl TaskMcpServer {
    #[tool(description = "获取当前绑定 Task 的完整信息")]
    async fn get_task_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let task = self.load_task().await?;

        let result = serde_json::json!({
            "task_id": task.id.to_string(),
            "story_id": task.story_id.to_string(),
            "title": task.title,
            "description": task.description,
            "status": task.status(),
            "workspace_id": task.workspace_id.map(|w| w.to_string()),
            "agent_binding": {
                "agent_type": task.agent_binding.agent_type,
                "preset_name": task.agent_binding.preset_name,
                "initial_context": task.agent_binding.initial_context,
            },
            "artifact_count": task.artifacts().len(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "更新当前 Task 的执行状态")]
    async fn update_task_status(
        &self,
        Parameters(params): Parameters<UpdateTaskStatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let new_status: agentdash_domain::task::TaskStatus =
            serde_json::from_value(serde_json::Value::String(params.status.clone())).map_err(
                |_| McpError::invalid_param("status", format!("无效的状态值: {}", params.status)),
            )?;

        tracing::info!(
            task_id = %self.task_id,
            new_status = %params.status,
            reason = ?params.reason,
            "Task 状态更新"
        );

        // M2-c：MCP "用户 / agent 主动标记状态" 属于业务命令路径，走
        // `Story::force_set_task_status`（命令级写入）。命令路径与 runtime projector
        // 并行共同产出 `TaskStatusChanged` 投影索引（D-M2-4 方案 2）；
        // [UNRESOLVED] 命令路径完全走 workflow step transition 的方案留待后续任务。
        let (mut story, task_before) = self.load_story_with_task().await?;
        let previous_status = task_before.status().clone();
        let applied = story.force_set_task_status(self.task_id, new_status.clone());
        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        if matches!(applied, Some(true)) {
            if let Err(err) = self
                .services
                .state_change_repo
                .append_change(
                    story.project_id,
                    self.task_id,
                    agentdash_domain::story::ChangeKind::TaskStatusChanged,
                    serde_json::json!({
                        "reason": params.reason.as_deref().unwrap_or("mcp_update_task_status"),
                        "task_id": self.task_id,
                        "story_id": story.id,
                        "source": "mcp_command",
                        "from": previous_status,
                        "to": new_status,
                    }),
                    None,
                )
                .await
            {
                tracing::warn!(
                    task_id = %self.task_id,
                    error = %err,
                    "MCP update_task_status：state_change 追加失败（story 已更新）"
                );
            }
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} 状态已更新为 {}",
            self.task_id, params.status,
        ))]))
    }

    #[tool(description = "上报 Task 执行产物（代码变更、测试结果、日志等）")]
    async fn report_artifact(
        &self,
        Parameters(params): Parameters<ReportArtifactParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use agentdash_domain::task::{Artifact, ArtifactType};

        let artifact_type: ArtifactType =
            serde_json::from_value(serde_json::Value::String(params.artifact_type.clone()))
                .map_err(|_| {
                    McpError::invalid_param(
                        "artifact_type",
                        format!("无效的产物类型: {}", params.artifact_type),
                    )
                })?;

        let artifact = Artifact {
            id: Uuid::new_v4(),
            artifact_type,
            content: parse_artifact_content(&params.content),
            created_at: chrono::Utc::now(),
        };

        let (mut story, _) = self.load_story_with_task().await?;
        let artifact_id = artifact.id;
        // M2：MCP 主动上报 artifact 走命令级 `Story::push_task_artifact` 入口。
        story.push_task_artifact(self.task_id, artifact.clone());

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "产物已上报（类型: {}, ID: {}）",
            params.artifact_type, artifact_id,
        ))]))
    }

    #[tool(description = "查看同一 Story 下的其它 Task 及其状态（只读，用于协调）")]
    async fn get_sibling_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let story = self
            .services
            .story_repo
            .get_by_id(self.story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", self.story_id))?;
        let tasks = &story.tasks;

        let result: Vec<serde_json::Value> = tasks
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "title": t.title,
                    "status": t.status(),
                    "is_self": t.id == self.task_id,
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "获取所属 Story 的上下文信息（PRD、规范引用），用于理解任务背景")]
    async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let story = self
            .services
            .story_repo
            .get_by_id(self.story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", self.story_id))?;

        let result = serde_json::json!({
            "story_id": story.id.to_string(),
            "title": story.title,
            "description": story.description,
            "context": {
                "source_refs": story.context.source_refs,
            },
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "向 Task 描述中追加内容（记录执行过程发现的关键信息）")]
    async fn append_task_description(
        &self,
        Parameters(params): Parameters<AppendTaskDescriptionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let (mut story, task) = self.load_story_with_task().await?;
        let new_description = format!("{}\n\n---\n{}", task.description, params.append_text);

        story.update_task(self.task_id, |task| {
            *task.description = new_description.clone();
        });

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} 描述已更新",
            self.task_id,
        ))]))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_artifact_content;

    #[test]
    fn parse_artifact_content_accepts_json_string() {
        let value = parse_artifact_content(r#"{"ok":true,"items":[1,2]}"#);
        assert_eq!(value["ok"], true);
        assert_eq!(value["items"][0], 1);
    }

    #[test]
    fn parse_artifact_content_falls_back_to_plain_text() {
        let value = parse_artifact_content("plain text");
        assert_eq!(value, serde_json::Value::String("plain text".to_string()));
    }
}

// ─── ServerHandler 实现 ──────────────────────────────────────

#[tool_handler]
impl ServerHandler for TaskMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            format!(
                "Task 执行工具（Task: {}）。可用于更新状态、上报产物、查看上下文。",
                self.task_id
            ),
        )
    }
}
