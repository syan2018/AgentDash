//! Task 层 MCP Server — Task 粒度执行工具
//!
//! 面向执行 Agent，在 Task 粒度上提供计划状态推进和执行投影关联能力。
//! 典型场景：Agent 完成代码编写后，通过工具推进计划状态、记录产物路径或摘要。
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
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleTaskPlanItem, LifecycleTaskPlanItemPatch, SubjectRef, TaskPlanStatus,
};
use agentdash_spi::platform::auth::AuthIdentity;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskStatusParams {
    #[schemars(description = "目标计划状态：open / active / review / blocked / done / dropped")]
    pub status: String,
    #[schemars(description = "状态变更原因说明")]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReportArtifactParams {
    #[schemars(
        description = "产物类型或关联语义，如 code_change / test_result / log_output / file / tool_execution"
    )]
    pub artifact_type: String,
    #[schemars(description = "产物内容、摘要或路径。优先传 JSON 字符串；若为纯文本，可直接传文本")]
    pub content: String,
    #[schemars(description = "可选产物路径，如 lifecycle://artifacts/report")]
    pub artifact_path: Option<String>,
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
    project_id: Uuid,
    identity: AuthIdentity,
}

impl TaskMcpServer {
    pub fn new(
        services: Arc<McpServices>,
        project_id: Uuid,
        task_id: Uuid,
        identity: AuthIdentity,
    ) -> Self {
        Self {
            services,
            task_id,
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

    async fn resolve_story_id_for_task(
        &self,
        run: &LifecycleRun,
        task: &LifecycleTaskPlanItem,
    ) -> Result<Option<Uuid>, McpError> {
        if let Some(story_id) = task
            .story_ref
            .as_ref()
            .filter(|subject| subject.kind == "story")
            .map(|subject| subject.id)
        {
            return Ok(Some(story_id));
        }
        let associations = self
            .services
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(McpError::from)?;
        Ok(associations
            .into_iter()
            .find(|association| association.subject_kind == "story")
            .map(|association| association.subject_id))
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
        let new_status: TaskPlanStatus = serde_json::from_value(serde_json::Value::String(
            params.status.clone(),
        ))
        .map_err(|_| {
            McpError::invalid_param(
                "status",
                format!(
                    "无效的计划状态: {}。允许值: open / active / review / blocked / done / dropped",
                    params.status
                ),
            )
        })?;

        tracing::info!(
            task_id = %self.task_id,
            new_status = %params.status,
            reason = ?params.reason,
            "Task 状态变更意图"
        );

        let (run, task_before) = self.load_task_plan().await?;
        let updated = agentdash_application::task::plan::transition_run_task_status(
            self.services.lifecycle_run_repo.as_ref(),
            run.id,
            self.task_id,
            new_status,
        )
        .await
        .map_err(McpError::from)?;
        self.services
            .state_change_repo
            .append_change(
                run.project_id,
                self.task_id,
                agentdash_domain::story::ChangeKind::TaskStatusChanged,
                serde_json::json!({
                    "reason": params.reason.as_deref().unwrap_or("mcp_update_task_status"),
                    "task_id": self.task_id,
                    "run_id": run.id,
                    "source": "mcp_command",
                    "current_status": task_before.status,
                    "new_status": updated.task.status,
                }),
                None,
            )
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} 计划状态已更新为：{}",
            self.task_id, params.status,
        ))]))
    }

    #[tool(description = "记录 Task 关联的 SubjectExecution 产物路径或摘要")]
    async fn report_artifact(
        &self,
        Parameters(params): Parameters<ReportArtifactParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Edit).await?;
        if params.artifact_type.trim().is_empty() {
            return Err(McpError::invalid_param("artifact_type", "产物类型不能为空").into());
        }

        let (run, _) = self.load_task_plan().await?;
        let artifact_id = Uuid::new_v4();
        self.services
            .state_change_repo
            .append_change(
                run.project_id,
                self.task_id,
                agentdash_domain::story::ChangeKind::TaskUpdated,
                serde_json::json!({
                    "event": "subject_execution_artifact_reported",
                    "reason": "mcp_report_artifact",
                    "task_id": self.task_id,
                    "run_id": run.id,
                    "source": "mcp_command",
                    "artifact_id": artifact_id,
                    "artifact_type": params.artifact_type,
                    "artifact_path": params.artifact_path,
                    "content": parse_artifact_content(&params.content),
                }),
                None,
            )
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "SubjectExecution 产物关联已记录（类型: {}, ID: {}）",
            params.artifact_type, artifact_id,
        ))]))
    }

    #[tool(description = "查看同一 LifecycleRun 内的其它 Task 计划状态（只读，用于协调）")]
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

    #[tool(description = "获取 Task 关联 Story 的上下文信息；无 Story 关联时返回空上下文")]
    async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::View).await?;
        let (run, task) = self.load_task_plan().await?;
        let Some(story_id) = self.resolve_story_id_for_task(&run, &task).await? else {
            let result = serde_json::json!({
                "task_id": self.task_id.to_string(),
                "story": null,
                "reason": "Task 没有关联 Story；Task scope 的 Story 归属是可选的",
            });
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&result).unwrap_or_default(),
            )]));
        };
        let story = self
            .services
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Story", story_id))?;

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
        let (run, task) = self.load_task_plan().await?;
        let next_body = match task.body.as_deref() {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{existing}\n\n{}", params.append_text)
            }
            _ => params.append_text.clone(),
        };
        agentdash_application::task::plan::update_run_task(
            self.services.lifecycle_run_repo.as_ref(),
            run.id,
            self.task_id,
            LifecycleTaskPlanItemPatch {
                body: Some(Some(next_body)),
                ..LifecycleTaskPlanItemPatch::default()
            },
        )
        .await
        .map_err(McpError::from)?;
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
