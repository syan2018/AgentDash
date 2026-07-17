use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::workflow::{SubjectRef, TaskPlanStatus, TaskPriority};
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ExecutionContext, ToolUpdateCallback,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::task::scope::{
    AgentRunTaskScopeInput, AgentRunTaskScopeResolutionError, AgentRunTaskScopeResolver,
};
use crate::task::workspace::{
    TaskPlanChangeset, TaskPlanChangesetMode, TaskPlanCreateCommand, TaskPlanOperation,
    TaskPlanPatchCommand, TaskPlanReadFormat as WorkspaceTaskPlanReadFormat,
    TaskPlanReadMode as WorkspaceTaskPlanReadMode, TaskPlanReadQuery,
    TaskPlanSnapshotItem as WorkspaceTaskPlanSnapshotItem, TaskPlanWorkspace,
    TaskPlanWorkspaceError,
};

#[derive(Clone)]
pub struct TaskReadTool {
    repos: RepositorySet,
    scope_input: AgentRunTaskScopeInput,
}

impl TaskReadTool {
    pub fn new(repos: RepositorySet, context: &ExecutionContext) -> Self {
        Self {
            repos,
            scope_input: AgentRunTaskScopeInput::from_execution_context(context),
        }
    }
}

#[derive(Clone)]
pub struct TaskWriteTool {
    repos: RepositorySet,
    scope_input: AgentRunTaskScopeInput,
}

impl TaskWriteTool {
    pub fn new(repos: RepositorySet, context: &ExecutionContext) -> Self {
        Self {
            repos,
            scope_input: AgentRunTaskScopeInput::from_execution_context(context),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskReadMode {
    Overview,
    List,
    Detail,
    Context,
    Projection,
}

fn default_read_mode() -> TaskReadMode {
    TaskReadMode::Overview
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskReadFormat {
    /// 紧凑：每个 Task 仅核心字段，body 截断、context_refs 仅计数。默认。
    Compact,
    /// 完整：返回全部字段。
    Full,
}

fn default_read_format() -> TaskReadFormat {
    TaskReadFormat::Compact
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskReadParams {
    #[serde(default = "default_read_mode")]
    pub mode: TaskReadMode,
    /// compact（默认）只返回核心字段；full 返回完整 Task。detail mode 始终 full。
    #[serde(default = "default_read_format")]
    pub format: TaskReadFormat,
    #[serde(default)]
    pub run_id: Option<Uuid>,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub story_id: Option<Uuid>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default)]
    pub owner_agent_id: Option<Uuid>,
    #[serde(default)]
    pub assigned_agent_id: Option<Uuid>,
    #[serde(default)]
    pub statuses: Vec<TaskStatusInput>,
}

impl TaskReadParams {
    fn into_query(self) -> TaskPlanReadQuery {
        TaskPlanReadQuery {
            mode: self.mode.into(),
            format: self.format.into(),
            run_id: self.run_id,
            task_id: self.task_id,
            story_id: self.story_id,
            include_archived: self.include_archived,
            owner_agent_id: self.owner_agent_id,
            assigned_agent_id: self.assigned_agent_id,
            statuses: self
                .statuses
                .into_iter()
                .map(TaskPlanStatus::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatusInput {
    Open,
    Active,
    Review,
    Blocked,
    Done,
    Dropped,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriorityInput {
    P0,
    P1,
    P2,
    P3,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SubjectRefInput {
    pub kind: String,
    pub id: Uuid,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContextSourceRefInput {
    pub kind: ContextSourceKindInput,
    pub locator: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub slot: Option<ContextSlotInput>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub delivery: Option<ContextDeliveryInput>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKindInput {
    ManualText,
    File,
    ProjectSnapshot,
    HttpFetch,
    McpResource,
    EntityRef,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSlotInput {
    Requirements,
    Constraints,
    Codebase,
    References,
    InstructionAppend,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryInput {
    Inline,
    Resource,
    Lazy,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskWriteMode {
    Patch,
    Snapshot,
}

fn default_write_mode() -> TaskWriteMode {
    TaskWriteMode::Patch
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskWriteParams {
    #[serde(default = "default_write_mode")]
    pub mode: TaskWriteMode,
    #[serde(default)]
    pub run_id: Option<Uuid>,
    #[serde(default)]
    pub operations: Vec<TaskWriteOperation>,
    #[serde(default)]
    pub snapshot: Vec<TaskSnapshotItem>,
    #[serde(default)]
    pub drop_missing: bool,
    #[serde(default = "default_read_mode")]
    pub return_mode: TaskReadMode,
}

impl TaskWriteParams {
    fn into_changeset(self) -> Result<TaskPlanChangeset, AgentToolError> {
        let mode = match self.mode {
            TaskWriteMode::Patch => TaskPlanChangesetMode::Patch {
                operations: self
                    .operations
                    .into_iter()
                    .map(TaskWriteOperation::into_workspace_operation)
                    .collect::<Result<Vec<_>, _>>()?,
            },
            TaskWriteMode::Snapshot => TaskPlanChangesetMode::Snapshot {
                snapshot: self
                    .snapshot
                    .into_iter()
                    .map(TaskSnapshotItem::into_workspace_item)
                    .collect::<Result<Vec<_>, _>>()?,
                drop_missing: self.drop_missing,
            },
        };
        Ok(TaskPlanChangeset {
            run_id: self.run_id,
            mode,
            return_mode: self.return_mode.into(),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TaskWriteOperation {
    CreateTask {
        title: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        status: Option<TaskStatusInput>,
        #[serde(default)]
        priority: Option<TaskPriorityInput>,
        #[serde(default)]
        owner_agent_id: Option<Uuid>,
        #[serde(default)]
        assigned_agent_id: Option<Uuid>,
        #[serde(default)]
        source_task_id: Option<Uuid>,
        #[serde(default)]
        context_refs: Vec<ContextSourceRefInput>,
        #[serde(default)]
        story_ref: Option<SubjectRefInput>,
    },
    PatchTask {
        task_id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        body: Option<Option<String>>,
        #[serde(default)]
        priority: Option<Option<TaskPriorityInput>>,
        #[serde(default)]
        owner_agent_id: Option<Option<Uuid>>,
        #[serde(default)]
        assigned_agent_id: Option<Option<Uuid>>,
        #[serde(default)]
        source_task_id: Option<Option<Uuid>>,
        #[serde(default)]
        context_refs: Option<Vec<ContextSourceRefInput>>,
        #[serde(default)]
        story_ref: Option<Option<SubjectRefInput>>,
    },
    SetStatus {
        task_id: String,
        status: TaskStatusInput,
    },
    ReorderTasks {
        task_ids: Vec<Uuid>,
    },
    DropTask {
        task_id: String,
    },
    ReplaceContextRefs {
        task_id: String,
        context_refs: Vec<ContextSourceRefInput>,
    },
}

impl TaskWriteOperation {
    fn into_workspace_operation(self) -> Result<TaskPlanOperation, AgentToolError> {
        Ok(match self {
            TaskWriteOperation::CreateTask {
                title,
                body,
                status,
                priority,
                owner_agent_id,
                assigned_agent_id,
                source_task_id,
                context_refs,
                story_ref,
            } => TaskPlanOperation::CreateTask(TaskPlanCreateCommand {
                title,
                body,
                status: status.map(TaskPlanStatus::from),
                priority: priority.map(TaskPriority::from),
                owner_agent_id,
                assigned_agent_id,
                source_task_id,
                context_refs: convert_context_refs(context_refs)?,
                story_ref: story_ref.map(SubjectRef::from),
            }),
            TaskWriteOperation::PatchTask {
                task_id,
                title,
                body,
                priority,
                owner_agent_id,
                assigned_agent_id,
                source_task_id,
                context_refs,
                story_ref,
            } => TaskPlanOperation::PatchTask(TaskPlanPatchCommand {
                task_id,
                title,
                body,
                priority: priority.map(|value| value.map(TaskPriority::from)),
                owner_agent_id,
                assigned_agent_id,
                source_task_id,
                context_refs: context_refs.map(convert_context_refs).transpose()?,
                story_ref: story_ref.map(|value| value.map(SubjectRef::from)),
            }),
            TaskWriteOperation::SetStatus { task_id, status } => TaskPlanOperation::SetStatus {
                task_id,
                status: status.into(),
            },
            TaskWriteOperation::ReorderTasks { task_ids } => {
                TaskPlanOperation::ReorderTasks { task_ids }
            }
            TaskWriteOperation::DropTask { task_id } => TaskPlanOperation::DropTask { task_id },
            TaskWriteOperation::ReplaceContextRefs {
                task_id,
                context_refs,
            } => TaskPlanOperation::ReplaceContextRefs {
                task_id,
                context_refs: convert_context_refs(context_refs)?,
            },
        })
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskSnapshotItem {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub status: Option<TaskStatusInput>,
    #[serde(default)]
    pub priority: Option<TaskPriorityInput>,
    #[serde(default)]
    pub owner_agent_id: Option<Uuid>,
    #[serde(default)]
    pub assigned_agent_id: Option<Uuid>,
    #[serde(default)]
    pub source_task_id: Option<Uuid>,
    #[serde(default)]
    pub context_refs: Vec<ContextSourceRefInput>,
    #[serde(default)]
    pub story_ref: Option<SubjectRefInput>,
}

impl TaskSnapshotItem {
    fn into_workspace_item(self) -> Result<WorkspaceTaskPlanSnapshotItem, AgentToolError> {
        Ok(WorkspaceTaskPlanSnapshotItem {
            id: self.id,
            title: self.title,
            body: self.body,
            status: self.status.map(TaskPlanStatus::from),
            priority: self.priority.map(TaskPriority::from),
            owner_agent_id: self.owner_agent_id,
            assigned_agent_id: self.assigned_agent_id,
            source_task_id: self.source_task_id,
            context_refs: convert_context_refs(self.context_refs)?,
            story_ref: self.story_ref.map(SubjectRef::from),
        })
    }
}

#[async_trait]
impl AgentTool for TaskReadTool {
    fn name(&self) -> &str {
        "task_read"
    }

    fn description(&self) -> &str {
        "读取当前 AgentRun / LifecycleRun 的 Task view。mode 支持 overview、list、detail、context、projection；Task facts 来自 LifecycleRun.tasks，执行事实通过 SubjectExecutionView 读取。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<TaskReadParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_task_read_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        _tool_use_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: TaskReadParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let scope = AgentRunTaskScopeResolver
            .resolve(&self.scope_input)
            .map_err(scope_error)?;
        let view = TaskPlanWorkspace::from_repos(&self.repos)
            .read(&scope, params.into_query())
            .await
            .map_err(workspace_error)?;
        Ok(result_with_view("Task view 已读取", view, false))
    }
}

#[async_trait]
impl AgentTool for TaskWriteTool {
    fn name(&self) -> &str {
        "task_write"
    }

    fn description(&self) -> &str {
        "唯一 Task 写入口。通过 patch operations 或 snapshot 创建、更新、推进状态、排序、归档和替换 context refs；写入后返回 task_read 视图。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<TaskWriteParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_task_write_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        _tool_use_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: TaskWriteParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let scope = AgentRunTaskScopeResolver
            .resolve(&self.scope_input)
            .map_err(scope_error)?;
        let result = TaskPlanWorkspace::from_repos(&self.repos)
            .apply(&scope, params.into_changeset()?)
            .await
            .map_err(workspace_error)?;
        let message = format!("Task 写入完成，变更 {} 个 Task。", result.changes.len());
        Ok(result_with_view(&message, result.view, false))
    }
}

impl From<TaskStatusInput> for TaskPlanStatus {
    fn from(value: TaskStatusInput) -> Self {
        match value {
            TaskStatusInput::Open => Self::Open,
            TaskStatusInput::Active => Self::Active,
            TaskStatusInput::Review => Self::Review,
            TaskStatusInput::Blocked => Self::Blocked,
            TaskStatusInput::Done => Self::Done,
            TaskStatusInput::Dropped => Self::Dropped,
        }
    }
}

impl From<TaskReadMode> for WorkspaceTaskPlanReadMode {
    fn from(value: TaskReadMode) -> Self {
        match value {
            TaskReadMode::Overview => Self::Overview,
            TaskReadMode::List => Self::List,
            TaskReadMode::Detail => Self::Detail,
            TaskReadMode::Context => Self::Context,
            TaskReadMode::Projection => Self::Projection,
        }
    }
}

impl From<TaskReadFormat> for WorkspaceTaskPlanReadFormat {
    fn from(value: TaskReadFormat) -> Self {
        match value {
            TaskReadFormat::Compact => Self::Compact,
            TaskReadFormat::Full => Self::Full,
        }
    }
}

impl From<TaskPriorityInput> for TaskPriority {
    fn from(value: TaskPriorityInput) -> Self {
        match value {
            TaskPriorityInput::P0 => Self::P0,
            TaskPriorityInput::P1 => Self::P1,
            TaskPriorityInput::P2 => Self::P2,
            TaskPriorityInput::P3 => Self::P3,
        }
    }
}

impl From<ContextSourceKindInput> for ContextSourceKind {
    fn from(value: ContextSourceKindInput) -> Self {
        match value {
            ContextSourceKindInput::ManualText => Self::ManualText,
            ContextSourceKindInput::File => Self::File,
            ContextSourceKindInput::ProjectSnapshot => Self::ProjectSnapshot,
            ContextSourceKindInput::HttpFetch => Self::HttpFetch,
            ContextSourceKindInput::McpResource => Self::McpResource,
            ContextSourceKindInput::EntityRef => Self::EntityRef,
        }
    }
}

impl From<ContextSlotInput> for ContextSlot {
    fn from(value: ContextSlotInput) -> Self {
        match value {
            ContextSlotInput::Requirements => Self::Requirements,
            ContextSlotInput::Constraints => Self::Constraints,
            ContextSlotInput::Codebase => Self::Codebase,
            ContextSlotInput::References => Self::References,
            ContextSlotInput::InstructionAppend => Self::InstructionAppend,
        }
    }
}

impl From<ContextDeliveryInput> for ContextDelivery {
    fn from(value: ContextDeliveryInput) -> Self {
        match value {
            ContextDeliveryInput::Inline => Self::Inline,
            ContextDeliveryInput::Resource => Self::Resource,
            ContextDeliveryInput::Lazy => Self::Lazy,
        }
    }
}

impl From<SubjectRefInput> for SubjectRef {
    fn from(value: SubjectRefInput) -> Self {
        SubjectRef::new(value.kind, value.id)
    }
}

impl TryFrom<ContextSourceRefInput> for ContextSourceRef {
    type Error = AgentToolError;

    fn try_from(value: ContextSourceRefInput) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: value.kind.into(),
            locator: value.locator,
            label: value.label,
            slot: value.slot.map(ContextSlot::from).unwrap_or_default(),
            priority: value.priority.unwrap_or_default(),
            required: value.required,
            max_chars: value.max_chars,
            delivery: value
                .delivery
                .map(ContextDelivery::from)
                .unwrap_or_default(),
        })
    }
}

fn convert_context_refs(
    refs: Vec<ContextSourceRefInput>,
) -> Result<Vec<ContextSourceRef>, AgentToolError> {
    refs.into_iter().map(ContextSourceRef::try_from).collect()
}

fn result_with_view(summary: &str, view: serde_json::Value, is_error: bool) -> AgentToolResult {
    let body = serde_json::to_string_pretty(&view).unwrap_or_else(|_| view.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(format!("{summary}\n{body}"))],
        is_error,
        details: Some(view),
    }
}

fn scope_error(error: AgentRunTaskScopeResolutionError) -> AgentToolError {
    AgentToolError::ExecutionFailed(error.to_string())
}

fn workspace_error(error: TaskPlanWorkspaceError) -> AgentToolError {
    match error {
        TaskPlanWorkspaceError::InvalidArguments(message) => {
            AgentToolError::InvalidArguments(message)
        }
        TaskPlanWorkspaceError::ExecutionFailed(message) => {
            AgentToolError::ExecutionFailed(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_with_view_puts_json_into_content_for_model() {
        let view = serde_json::json!({ "mode": "list", "tasks": [{ "title": "示例 Task" }] });
        let result = result_with_view("Task view 已读取", view, false);
        let text = match &result.content[0] {
            ContentPart::Text { text } => text,
            other => panic!("expected text content, got {other:?}"),
        };
        assert!(
            text.contains("示例 Task"),
            "content 应包含 Task 数据: {text}"
        );
        assert!(text.starts_with("Task view 已读取"));
        assert!(result.details.is_some(), "details 仍保留供持久化");
    }

    #[test]
    fn task_read_mode_rejects_execution_mode() {
        let error =
            serde_json::from_value::<TaskReadParams>(serde_json::json!({ "mode": "execution" }))
                .expect_err("execution mode should not be accepted by task_read");
        assert!(
            error.to_string().contains("execution"),
            "error should mention rejected mode: {error}"
        );
    }
}
