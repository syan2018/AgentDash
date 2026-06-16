//! Task 层 MCP Server — Task 粒度执行工具
//!
//! 面向执行 Agent，在 Task 粒度上提供状态更新和产物上报能力。
//! 典型场景：Agent 完成代码编写后，通过工具上报产物、更新状态。
//!
//! 每个 TaskMcpServer 实例绑定到一个具体的 Task，工具操作范围受限于该 Task。

use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use uuid::Uuid;

use crate::authz::{McpProjectPermission, require_project_permission};
use crate::error::McpError;
use crate::services::McpServices;
use agentdash_domain::workflow::{LifecycleRun, LifecycleTaskPlanItem, SubjectRef};
use agentdash_spi::platform::auth::AuthIdentity;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskStatusParams {
    #[schemars(
        description = "目标状态：pending / assigned / running / awaiting_verification / completed / failed / cancelled"
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
    project_id: Uuid,
    identity: AuthIdentity,
}

impl TaskMcpServer {
    pub fn new(
        services: Arc<McpServices>,
        project_id: Uuid,
        story_id: Uuid,
        task_id: Uuid,
        identity: AuthIdentity,
    ) -> Self {
        Self {
            services,
            task_id,
            story_id,
            project_id,
            identity,
        }
    }

    async fn require_project(
        &self,
        permission: McpProjectPermission,
    ) -> Result<agentdash_domain::project::Project, McpError> {
        require_project_permission(&self.services, &self.identity, self.project_id, permission)
            .await
    }

    async fn load_task_plan(&self) -> Result<(LifecycleRun, LifecycleTaskPlanItem), McpError> {
        let subject = SubjectRef::new("task", self.task_id);
        let associations = self
            .services
            .lifecycle_subject_association_repo
            .list_by_subject(&subject)
            .await
            .map_err(McpError::from)?;
        let mut run_ids = Vec::new();
        for assoc in associations {
            if !run_ids.contains(&assoc.anchor_run_id) {
                run_ids.push(assoc.anchor_run_id);
            }
        }
        let runs = self
            .services
            .lifecycle_run_repo
            .list_by_ids(&run_ids)
            .await
            .map_err(McpError::from)?;
        for run in runs {
            if run.project_id != self.project_id {
                continue;
            }
            if let Some(task) = run.task_by_id(self.task_id).cloned() {
                return Ok((run, task));
            }
        }
        Err(McpError::not_found("Task", self.task_id))
    }
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl TaskMcpServer {
    #[tool(description = "获取当前绑定 Task 的完整信息")]
    async fn get_task_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::View).await?;
        let (run, task) = self.load_task_plan().await?;

        let result = serde_json::json!({
            "task_id": task.id.to_string(),
            "owning_run_id": run.id.to_string(),
            "story_ref": task.story_ref,
            "title": task.title,
            "body": task.body,
            "status": task.status,
            "priority": task.priority,
            "created_by_agent_id": task.created_by_agent_id.map(|id| id.to_string()),
            "owner_agent_id": task.owner_agent_id.map(|id| id.to_string()),
            "assigned_agent_id": task.assigned_agent_id.map(|id| id.to_string()),
            "context_refs": task.context_refs,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "记录当前 Task 的状态变更意图")]
    async fn update_task_status(
        &self,
        Parameters(params): Parameters<UpdateTaskStatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Edit).await?;
        let new_status: agentdash_domain::task::TaskStatus =
            serde_json::from_value(serde_json::Value::String(params.status.clone())).map_err(
                |_| McpError::invalid_param("status", format!("无效的状态值: {}", params.status)),
            )?;

        tracing::info!(
            task_id = %self.task_id,
            new_status = %params.status,
            reason = ?params.reason,
            "Task 状态变更意图"
        );

        let (run, task_before) = self.load_task_plan().await?;
        self.services
            .state_change_repo
            .append_change(
                run.project_id,
                self.task_id,
                agentdash_domain::story::ChangeKind::TaskUpdated,
                serde_json::json!({
                    "reason": params.reason.as_deref().unwrap_or("mcp_update_task_status"),
                    "task_id": self.task_id,
                    "run_id": run.id,
                    "source": "mcp_command",
                    "current_status": task_before.status,
                    "requested_status": new_status,
                }),
                None,
            )
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} 状态变更意图已记录：{}",
            self.task_id, params.status,
        ))]))
    }

    #[tool(description = "上报 Task 执行产物（代码变更、测试结果、日志等）")]
    async fn report_artifact(
        &self,
        Parameters(params): Parameters<ReportArtifactParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use agentdash_domain::task::ArtifactType;

        self.require_project(McpProjectPermission::Edit).await?;
        let _artifact_type: ArtifactType =
            serde_json::from_value(serde_json::Value::String(params.artifact_type.clone()))
                .map_err(|_| {
                    McpError::invalid_param(
                        "artifact_type",
                        format!("无效的产物类型: {}", params.artifact_type),
                    )
                })?;

        let (run, _) = self.load_task_plan().await?;
        let artifact_id = Uuid::new_v4();
        self.services
            .state_change_repo
            .append_change(
                run.project_id,
                self.task_id,
                agentdash_domain::story::ChangeKind::TaskArtifactAdded,
                serde_json::json!({
                    "reason": "mcp_report_artifact",
                    "task_id": self.task_id,
                    "run_id": run.id,
                    "source": "mcp_command",
                    "artifact_id": artifact_id,
                    "artifact_type": params.artifact_type,
                    "content": parse_artifact_content(&params.content),
                }),
                None,
            )
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "产物已上报（类型: {}, ID: {}）",
            params.artifact_type, artifact_id,
        ))]))
    }

    #[tool(description = "查看同一 Story 下的其它 Task 及其状态（只读，用于协调）")]
    async fn get_sibling_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::View).await?;
        let (run, _) = self.load_task_plan().await?;
        let tasks = &run.tasks;

        let result: Vec<serde_json::Value> = tasks
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "title": t.title,
                    "status": t.status,
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
        self.require_project(McpProjectPermission::View).await?;
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
        self.require_project(McpProjectPermission::Edit).await?;
        let (run, _) = self.load_task_plan().await?;
        self.services
            .state_change_repo
            .append_change(
                run.project_id,
                self.task_id,
                agentdash_domain::story::ChangeKind::TaskUpdated,
                serde_json::json!({
                    "reason": "mcp_append_task_description",
                    "task_id": self.task_id,
                    "run_id": run.id,
                    "source": "mcp_command",
                    "append_text": params.append_text,
                }),
                None,
            )
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

#[tool_handler(router = Self::tool_router())]
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
