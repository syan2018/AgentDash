//! Story 层 MCP Server — Story 上下文编排工具
//!
//! 面向编排 Agent（如 PlanAgent），在 Story 粒度上提供上下文管理能力。
//! 典型场景：PlanAgent 探索项目代码后，更新 Story 上下文、拆解 Task。
//!
//! 每个 StoryMcpServer 实例绑定到一个具体的 Story，工具操作范围受限于该 Story。

use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use crate::authz::{McpProjectPermission, require_project_permission};
use crate::error::McpError;
use crate::services::McpServices;
use agentdash_domain::context_container::{
    validate_context_containers, validate_disabled_container_ids,
};
use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::session_composition::validate_session_composition;
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleTaskPlanItemDraft, SubjectRef, TaskPriority,
};
use agentdash_spi::platform::auth::AuthIdentity;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateStoryContextParams {
    #[schemars(description = "追加的声明式上下文来源")]
    pub add_source_refs: Option<Vec<ContextSourceRefInput>>,
    #[schemars(description = "完整替换声明式上下文来源")]
    pub replace_source_refs: Option<Vec<ContextSourceRefInput>>,
    #[schemars(description = "完整替换 Story 级 context_containers")]
    pub replace_context_containers: Option<Value>,
    #[schemars(description = "完整替换 disabled_container_ids")]
    pub replace_disabled_container_ids: Option<Vec<String>>,
    #[schemars(description = "覆盖 session_composition")]
    pub session_composition: Option<Value>,
    #[schemars(description = "是否清空 session_composition")]
    pub clear_session_composition: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateStoryDetailsParams {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub story_type: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ContextSourceRefInput {
    pub kind: String,
    pub locator: String,
    pub label: Option<String>,
    pub slot: Option<String>,
    pub priority: Option<i32>,
    pub required: Option<bool>,
    pub max_chars: Option<usize>,
    pub delivery: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskParams {
    #[schemars(description = "Story-bound LifecycleRun UUID，Task 将创建到该 run 的计划项集合中")]
    pub run_id: String,
    #[schemars(description = "Task 标题")]
    pub title: String,
    #[schemars(description = "Task 描述（包含执行指令和上下文）")]
    pub description: String,
    #[schemars(description = "Task 优先级：p0 / p1 / p2 / p3（可选）")]
    pub priority: Option<String>,
    #[schemars(description = "Task 专属声明式上下文来源")]
    pub context_sources: Option<Vec<ContextSourceRefInput>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BatchCreateTasksParams {
    #[schemars(description = "Story-bound LifecycleRun UUID，Task 将创建到该 run 的计划项集合中")]
    pub run_id: String,
    #[schemars(description = "批量创建的 Task 列表")]
    pub tasks: Vec<TaskInput>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskInput {
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub context_sources: Option<Vec<ContextSourceRefInput>>,
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
    project_id: Uuid,
    identity: AuthIdentity,
}

impl StoryMcpServer {
    pub fn new(
        services: Arc<McpServices>,
        project_id: Uuid,
        story_id: Uuid,
        identity: AuthIdentity,
    ) -> Self {
        Self {
            services,
            story_id,
            project_id,
            identity,
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

    async fn require_project(
        &self,
        permission: McpProjectPermission,
    ) -> Result<agentdash_domain::project::Project, McpError> {
        self.services
            .story_repo
            .get_by_id(self.story_id)
            .await
            .map_err(McpError::from)?
            .filter(|story| story.project_id == self.project_id)
            .ok_or_else(|| McpError::not_found("Story", self.story_id))?;
        require_project_permission(&self.services, &self.identity, self.project_id, permission)
            .await
    }

    fn into_context_source(source: ContextSourceRefInput) -> Result<ContextSourceRef, McpError> {
        let kind = match source.kind.as_str() {
            "manual_text" => ContextSourceKind::ManualText,
            "file" => ContextSourceKind::File,
            "project_snapshot" => ContextSourceKind::ProjectSnapshot,
            other => {
                return Err(McpError::invalid_param(
                    "kind",
                    format!("不支持的来源类型: {other}"),
                ));
            }
        };

        let slot = match source.slot.as_deref().unwrap_or("references") {
            "requirements" => ContextSlot::Requirements,
            "constraints" => ContextSlot::Constraints,
            "codebase" => ContextSlot::Codebase,
            "references" => ContextSlot::References,
            "instruction_append" => ContextSlot::InstructionAppend,
            other => {
                return Err(McpError::invalid_param(
                    "slot",
                    format!("不支持的 slot: {other}"),
                ));
            }
        };

        let delivery = match source.delivery.as_deref().unwrap_or("resource") {
            "inline" => ContextDelivery::Inline,
            "resource" => ContextDelivery::Resource,
            "lazy" => ContextDelivery::Lazy,
            other => {
                return Err(McpError::invalid_param(
                    "delivery",
                    format!("不支持的 delivery: {other}"),
                ));
            }
        };

        Ok(ContextSourceRef {
            kind,
            locator: source.locator,
            label: source.label,
            slot,
            priority: source.priority.unwrap_or_default(),
            required: source.required.unwrap_or(false),
            max_chars: source.max_chars,
            delivery,
        })
    }

    fn normalize_tags(tags: Vec<String>) -> Vec<String> {
        tags.into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }

    fn parse_uuid(field: &'static str, value: &str) -> Result<Uuid, McpError> {
        Uuid::parse_str(value)
            .map_err(|_| McpError::invalid_param(field, format!("无效的 UUID: {value}")))
    }

    fn parse_priority(value: Option<String>) -> Result<Option<TaskPriority>, McpError> {
        value
            .map(|priority| {
                serde_json::from_value(serde_json::Value::String(priority.clone())).map_err(|_| {
                    McpError::invalid_param("priority", format!("无效的 Task 优先级: {priority}"))
                })
            })
            .transpose()
    }

    async fn load_story_bound_run(&self, run_id: Uuid) -> Result<LifecycleRun, McpError> {
        let run = self
            .services
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("LifecycleRun", run_id))?;
        if run.project_id != self.project_id {
            return Err(McpError::invalid_param(
                "run_id",
                "LifecycleRun 不属于当前 Project",
            ));
        }
        let story_subject = SubjectRef::new("story", self.story_id);
        let is_story_bound = self
            .services
            .lifecycle_subject_association_repo
            .list_by_subject(&story_subject)
            .await
            .map_err(McpError::from)?
            .into_iter()
            .any(|association| association.anchor_run_id == run_id);
        if !is_story_bound {
            return Err(McpError::invalid_param(
                "run_id",
                "Task 只能通过 Story-bound LifecycleRun 创建",
            ));
        }
        Ok(run)
    }

    fn build_task_draft(
        &self,
        title: String,
        description: String,
        priority: Option<String>,
        context_sources: Option<Vec<ContextSourceRefInput>>,
    ) -> Result<LifecycleTaskPlanItemDraft, McpError> {
        let mut draft = LifecycleTaskPlanItemDraft::new(title);
        draft.body = Some(description);
        draft.priority = Self::parse_priority(priority)?;
        draft.story_ref = Some(SubjectRef::new("story", self.story_id));
        draft.context_refs = context_sources
            .unwrap_or_default()
            .into_iter()
            .map(Self::into_context_source)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(draft)
    }
}

fn parse_domain_input<T: DeserializeOwned>(
    field: &'static str,
    value: Value,
) -> Result<T, McpError> {
    serde_json::from_value(value)
        .map_err(|error| McpError::invalid_param(field, format!("参数结构无效: {error}")))
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl StoryMcpServer {
    #[tool(description = "获取当前 Story 的完整上下文信息（声明式来源与容器）")]
    async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Use).await?;
        let story = self.load_story().await?;

        let result = serde_json::json!({
            "story_id": story.id.to_string(),
            "title": story.title,
            "description": story.description,
            "status": story.status,
            "context": {
                "source_refs": story.context.source_refs,
                "context_containers": story.context.context_containers,
                "disabled_container_ids": story.context.disabled_container_ids,
                "session_composition": story.context.session_composition,
            },
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "更新 Story 上下文：声明式 source_refs / 容器 / 会话编排")]
    async fn update_story_context(
        &self,
        Parameters(params): Parameters<UpdateStoryContextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let project = self
            .require_project(McpProjectPermission::Configure)
            .await?;
        let mut story = self.load_story().await?;

        if let Some(source_refs) = params.replace_source_refs {
            story.context.source_refs = source_refs
                .into_iter()
                .map(Self::into_context_source)
                .collect::<Result<Vec<_>, _>>()?;
        }

        if let Some(source_refs) = params.add_source_refs {
            for source in source_refs {
                story
                    .context
                    .source_refs
                    .push(Self::into_context_source(source)?);
            }
        }

        if let Some(context_containers) = params.replace_context_containers {
            story.context.context_containers =
                parse_domain_input("replace_context_containers", context_containers)?;
        }

        if let Some(disabled_ids) = params.replace_disabled_container_ids {
            story.context.disabled_container_ids = disabled_ids
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }

        if let Some(session_composition) = params.session_composition {
            story.context.session_composition = Some(parse_domain_input(
                "session_composition",
                session_composition,
            )?);
        }
        if params.clear_session_composition.unwrap_or(false) {
            story.context.session_composition = None;
        }

        validate_context_containers(&story.context.context_containers)
            .map_err(|error| McpError::invalid_param("replace_context_containers", error))?;
        validate_disabled_container_ids(
            &story.context.disabled_container_ids,
            &project.config.context_containers,
        )
        .map_err(|error| McpError::invalid_param("replace_disabled_container_ids", error))?;
        if let Some(session_composition) = &story.context.session_composition {
            validate_session_composition(session_composition)
                .map_err(|error| McpError::invalid_param("session_composition", error))?;
        }

        story.updated_at = chrono::Utc::now();

        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Story {} 上下文已更新（source_refs: {} 项, containers: {} 项, session_composition: {}）",
            self.story_id,
            story.context.source_refs.len(),
            story.context.context_containers.len(),
            if story.context.session_composition.is_some() {
                "yes"
            } else {
                "no"
            },
        ))]))
    }

    #[tool(description = "更新 Story 基本信息（标题、描述、优先级、类型、标签）")]
    async fn update_story_details(
        &self,
        Parameters(params): Parameters<UpdateStoryDetailsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let mut story = self.load_story().await?;

        if let Some(title) = params.title {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                return Err(McpError::invalid_param("title", "标题不能为空").into());
            }
            story.title = trimmed.to_string();
        }
        if let Some(description) = params.description {
            story.description = description;
        }
        if let Some(priority) = params.priority {
            story.priority = serde_json::from_value(serde_json::Value::String(priority.clone()))
                .map_err(|_| {
                    McpError::invalid_param("priority", format!("无效的优先级: {priority}"))
                })?;
        }
        if let Some(story_type) = params.story_type {
            story.story_type =
                serde_json::from_value(serde_json::Value::String(story_type.clone())).map_err(
                    |_| McpError::invalid_param("story_type", format!("无效的类型: {story_type}")),
                )?;
        }
        if let Some(tags) = params.tags {
            story.tags = Self::normalize_tags(tags);
        }

        story.updated_at = chrono::Utc::now();
        self.services
            .story_repo
            .update(&story)
            .await
            .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Story {} 基本信息已更新",
            self.story_id,
        ))]))
    }

    #[tool(description = "通过 Story-bound LifecycleRun 创建一个 run-scoped Task 计划项")]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let run_id = Self::parse_uuid("run_id", &params.run_id)?;
        let run = self.load_story_bound_run(run_id).await?;
        let draft = self.build_task_draft(
            params.title,
            params.description,
            params.priority,
            params.context_sources,
        )?;
        let created = agentdash_application::task::plan::create_run_task(
            self.services.lifecycle_run_repo.as_ref(),
            run.id,
            draft,
        )
        .await
        .map_err(McpError::from)?;

        let result = serde_json::json!({
            "project_id": created.project_id.to_string(),
            "run_id": created.run_id.to_string(),
            "task": created.task,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "通过 Story-bound LifecycleRun 批量创建 run-scoped Task 计划项")]
    async fn batch_create_tasks(
        &self,
        Parameters(params): Parameters<BatchCreateTasksParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let run_id = Self::parse_uuid("run_id", &params.run_id)?;
        let run = self.load_story_bound_run(run_id).await?;
        let mut created = Vec::new();
        for task in params.tasks {
            let draft = self.build_task_draft(
                task.title,
                task.description,
                task.priority,
                task.context_sources,
            )?;
            let result = agentdash_application::task::plan::create_run_task(
                self.services.lifecycle_run_repo.as_ref(),
                run.id,
                draft,
            )
            .await
            .map_err(McpError::from)?;
            created.push(result.task);
        }

        let result = serde_json::json!({
            "project_id": self.project_id.to_string(),
            "run_id": run.id.to_string(),
            "tasks": created,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "查询当前 Story 的 Task projection")]
    async fn list_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Use).await?;
        let projection = agentdash_application::task::plan::build_story_task_projection(
            self.services.lifecycle_run_repo.as_ref(),
            self.services.lifecycle_subject_association_repo.as_ref(),
            self.project_id,
            self.story_id,
        )
        .await
        .map_err(McpError::from)?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&projection_to_json(projection)).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "推进 Story 生命周期状态（如从 created 到 context_ready，或到 decomposed）"
    )]
    async fn advance_story_status(
        &self,
        Parameters(params): Parameters<AdvanceStoryStatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let mut story = self.load_story().await?;

        let new_status: agentdash_domain::story::StoryStatus =
            serde_json::from_value(serde_json::Value::String(params.target_status.clone()))
                .map_err(|_| {
                    McpError::invalid_param(
                        "target_status",
                        format!("无效的状态值: {}", params.target_status),
                    )
                })?;

        diag!(Info, Subsystem::Mcp,

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

#[tool_handler(router = Self::tool_router())]
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

fn projection_to_json(
    projection: agentdash_application::task::plan::StoryTaskProjectionView,
) -> serde_json::Value {
    serde_json::json!({
        "story_id": projection.story_id.to_string(),
        "tasks": projection.tasks.into_iter().map(|item| {
            serde_json::json!({
                "project_id": item.project_id.to_string(),
                "owning_run_id": item.owning_run_id.to_string(),
                "task": item.task,
                "sources": item.sources.into_iter().map(|source| {
                    serde_json::json!({
                        "kind": story_projection_source_kind(source.kind),
                        "run_id": source.run_id.to_string(),
                        "agent_id": source.agent_id.map(|id| id.to_string()),
                        "story_ref": source.story_ref,
                        "reason": source.reason,
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

fn story_projection_source_kind(
    kind: agentdash_application::task::plan::StoryTaskProjectionSourceKind,
) -> &'static str {
    match kind {
        agentdash_application::task::plan::StoryTaskProjectionSourceKind::OwningRun => "owning_run",
        agentdash_application::task::plan::StoryTaskProjectionSourceKind::LinkedRun => "linked_run",
        agentdash_application::task::plan::StoryTaskProjectionSourceKind::StoryRef => "story_ref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::context::tool_schema_sanitizer::sanitize_tool_schema;
    use serde_json::Value;

    #[test]
    fn update_story_context_schema_is_openai_compatible() {
        let tool = StoryMcpServer::tool_router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name.as_ref() == "update_story_context")
            .expect("update_story_context tool should exist");
        let schema = sanitize_tool_schema(Value::Object((*tool.input_schema).clone()));

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);

        assert_schema_objects_have_type(&schema);
    }

    fn assert_schema_objects_have_type(value: &Value) {
        let Some(object) = value.as_object() else {
            if let Some(items) = value.as_array() {
                for item in items {
                    assert_schema_objects_have_type(item);
                }
            }
            return;
        };

        if object.contains_key("properties") {
            assert!(
                object.get("type").and_then(Value::as_str) == Some("object"),
                "带 properties 的 schema 必须显式声明 type=object: {}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }

        for key in [
            "items",
            "additionalProperties",
            "contains",
            "if",
            "then",
            "else",
        ] {
            if let Some(child) = object.get(key) {
                assert_schema_objects_have_type(child);
            }
        }

        for key in [
            "$defs",
            "definitions",
            "dependentSchemas",
            "patternProperties",
        ] {
            if let Some(children) = object.get(key).and_then(Value::as_object) {
                for child in children.values() {
                    assert_schema_objects_have_type(child);
                }
            }
        }

        for key in ["anyOf", "allOf", "oneOf", "prefixItems"] {
            if let Some(children) = object.get(key).and_then(Value::as_array) {
                for child in children {
                    assert_schema_objects_have_type(child);
                }
            }
        }
    }
}
