//! Story 层 MCP Server — Story 上下文编排工具
//!
//! 面向编排 Agent（如 PlanAgent），在 Story 粒度上提供上下文管理能力。
//! 典型场景：PlanAgent 探索项目代码后，更新 Story 上下文、拆解 Task。
//!
//! 每个 StoryMcpServer 实例绑定到一个具体的 Story，工具操作范围受限于该 Story。

use std::sync::Arc;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::McpError;
use crate::services::McpServices;
use agentdash_domain::context_container::{
    ContextContainerDefinition, MountDerivationPolicy, validate_context_containers,
    validate_disabled_container_ids,
};
use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::session_composition::{SessionComposition, validate_session_composition};

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateStoryContextParams {
    #[schemars(description = "PRD 文档内容（覆盖更新）")]
    pub prd_doc: Option<String>,
    #[schemars(description = "追加的规范引用列表")]
    pub add_spec_refs: Option<Vec<String>>,
    #[schemars(description = "追加的资源清单项 [{name, uri, resource_type}]")]
    pub add_resources: Option<Vec<ResourceInput>>,
    #[schemars(description = "追加的声明式上下文来源")]
    pub add_source_refs: Option<Vec<ContextSourceRefInput>>,
    #[schemars(description = "完整替换声明式上下文来源")]
    pub replace_source_refs: Option<Vec<ContextSourceRefInput>>,
    #[schemars(description = "完整替换 Story 级 context_containers")]
    pub replace_context_containers: Option<Vec<ContextContainerDefinition>>,
    #[schemars(description = "完整替换 disabled_container_ids")]
    pub replace_disabled_container_ids: Option<Vec<String>>,
    #[schemars(description = "覆盖 mount_policy_override")]
    pub mount_policy_override: Option<MountDerivationPolicy>,
    #[schemars(description = "是否清空 mount_policy_override")]
    pub clear_mount_policy_override: Option<bool>,
    #[schemars(description = "覆盖 session_composition")]
    pub session_composition: Option<SessionComposition>,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResourceInput {
    pub name: String,
    pub uri: String,
    pub resource_type: String,
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
    #[schemars(description = "Task 专属声明式上下文来源")]
    pub context_sources: Option<Vec<ContextSourceRefInput>>,
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

    async fn load_project(&self) -> Result<agentdash_domain::project::Project, McpError> {
        self.services
            .project_repo
            .get_by_id(self.project_id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::not_found("Project", self.project_id))
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
                "source_refs": story.context.source_refs,
                "context_containers": story.context.context_containers,
                "disabled_container_ids": story.context.disabled_container_ids,
                "mount_policy_override": story.context.mount_policy_override,
                "session_composition": story.context.session_composition,
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
        let project = self.load_project().await?;

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
            story.context.context_containers = context_containers;
        }

        if let Some(disabled_ids) = params.replace_disabled_container_ids {
            story.context.disabled_container_ids = disabled_ids
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }

        if let Some(mount_policy_override) = params.mount_policy_override {
            story.context.mount_policy_override = Some(mount_policy_override);
        }

        if params.clear_mount_policy_override.unwrap_or(false) {
            story.context.mount_policy_override = None;
        }
        if let Some(session_composition) = params.session_composition {
            story.context.session_composition = Some(session_composition);
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
            "Story {} 上下文已更新（spec_refs: {} 项, resources: {} 项, containers: {} 项, session_composition: {}）",
            self.story_id,
            story.context.spec_refs.len(),
            story.context.resource_list.len(),
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

        let mut task = Task::new(
            self.project_id,
            self.story_id,
            params.title,
            params.description,
        );
        task.workspace_id = workspace_id;
        task.agent_binding = AgentBinding {
            agent_type: params.agent_type,
            initial_context: params.initial_context,
            context_sources: params
                .context_sources
                .unwrap_or_default()
                .into_iter()
                .map(Self::into_context_source)
                .collect::<Result<Vec<_>, _>>()?,
            ..Default::default()
        };

        self.services
            .task_command_repo
            .create_for_story(&task)
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

            let mut task = Task::new(
                self.project_id,
                self.story_id,
                input.title.clone(),
                input.description.clone(),
            );
            task.workspace_id = workspace_id;
            task.agent_binding = AgentBinding {
                agent_type: input.agent_type.clone(),
                initial_context: input.initial_context.clone(),
                context_sources: input
                    .context_sources
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(Self::into_context_source)
                    .collect::<Result<Vec<_>, _>>()?,
                ..Default::default()
            };

            self.services
                .task_command_repo
                .create_for_story(&task)
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

    #[tool(
        description = "推进 Story 生命周期状态（如从 created 到 context_ready，或到 decomposed）"
    )]
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::schema::sanitize_tool_schema;
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

        let properties = schema["properties"]
            .as_object()
            .expect("properties should be object");
        let required = schema["required"]
            .as_array()
            .expect("required should be array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<std::collections::BTreeSet<_>>();

        for key in properties.keys() {
            assert!(
                required.contains(key.as_str()),
                "required 应包含属性 `{key}`"
            );
        }

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
