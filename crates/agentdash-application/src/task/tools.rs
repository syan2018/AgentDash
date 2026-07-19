use agentdash_agent_runtime::{
    RuntimeTaskGrantedOperation, RuntimeToolDefinition, RuntimeToolEffect, RuntimeToolExecutor,
    RuntimeToolInvocation, RuntimeToolPermission, RuntimeToolResourceGrant,
};
use agentdash_agent_service_api::{AgentToolName, AgentToolResult as RuntimeAgentToolResult};
use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::workflow::{SubjectRef, TaskPlanStatus, TaskPriority};
use agentdash_platform_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_platform_spi::{
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
    TaskPlanScope,
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
    fn protocol_projector(&self) -> Option<agentdash_platform_spi::ToolProtocolProjector> {
        Some(agentdash_platform_spi::ToolProtocolProjector::Dynamic { namespace: None })
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
    fn protocol_projector(&self) -> Option<agentdash_platform_spi::ToolProtocolProjector> {
        Some(agentdash_platform_spi::ToolProtocolProjector::Dynamic { namespace: None })
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

#[derive(Clone)]
pub struct RuntimeTaskReadTool {
    repos: RepositorySet,
}

impl RuntimeTaskReadTool {
    pub fn new(repos: RepositorySet) -> Self {
        Self { repos }
    }
}

#[async_trait]
impl RuntimeToolExecutor for RuntimeTaskReadTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("task_read").expect("static runtime tool name"),
            description: "Read the current AgentRun Task view.".to_owned(),
            parameters_schema: schema_value::<TaskReadParams>(),
            permission: RuntimeToolPermission::ProductRead,
            effect: RuntimeToolEffect::ReadOnly,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> RuntimeAgentToolResult {
        let scope = match runtime_task_scope(&invocation, RuntimeTaskGrantedOperation::Read) {
            Ok(scope) => scope,
            Err(result) => return result,
        };
        let params: TaskReadParams = match serde_json::from_value(invocation.arguments) {
            Ok(params) => params,
            Err(error) => {
                return RuntimeAgentToolResult::Rejected {
                    code: "invalid_task_read_arguments".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        match TaskPlanWorkspace::from_repos(&self.repos)
            .read(&scope, params.into_query())
            .await
        {
            Ok(view) => RuntimeAgentToolResult::Completed {
                output: serde_json::json!({"summary": "Task view 已读取", "view": view}),
            },
            Err(error) => RuntimeAgentToolResult::Failed {
                code: "task_read_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}

#[derive(Clone)]
pub struct RuntimeTaskWriteTool {
    repos: RepositorySet,
}

impl RuntimeTaskWriteTool {
    pub fn new(repos: RepositorySet) -> Self {
        Self { repos }
    }
}

#[async_trait]
impl RuntimeToolExecutor for RuntimeTaskWriteTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("task_write").expect("static runtime tool name"),
            description: "Apply changes to the current AgentRun Task plan.".to_owned(),
            parameters_schema: schema_value::<TaskWriteParams>(),
            permission: RuntimeToolPermission::ProductWrite,
            effect: RuntimeToolEffect::ProductMutation,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> RuntimeAgentToolResult {
        let scope = match runtime_task_scope(&invocation, RuntimeTaskGrantedOperation::Write) {
            Ok(scope) => scope,
            Err(result) => return result,
        };
        let params: TaskWriteParams = match serde_json::from_value(invocation.arguments) {
            Ok(params) => params,
            Err(error) => {
                return RuntimeAgentToolResult::Rejected {
                    code: "invalid_task_write_arguments".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        let changeset = match params.into_changeset() {
            Ok(changeset) => changeset,
            Err(error) => {
                return RuntimeAgentToolResult::Rejected {
                    code: "invalid_task_write_arguments".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        match TaskPlanWorkspace::from_repos(&self.repos)
            .apply(&scope, changeset)
            .await
        {
            Ok(result) => RuntimeAgentToolResult::Completed {
                output: serde_json::json!({
                    "summary": format!("Task 写入完成，变更 {} 个 Task。", result.changes.len()),
                    "view": result.view
                }),
            },
            Err(error) => RuntimeAgentToolResult::Failed {
                code: "task_write_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}

fn runtime_task_scope(
    invocation: &RuntimeToolInvocation,
    required: RuntimeTaskGrantedOperation,
) -> Result<TaskPlanScope, RuntimeAgentToolResult> {
    let RuntimeToolResourceGrant::Task(task) = &invocation.grant.resources else {
        return Err(RuntimeAgentToolResult::Rejected {
            code: "runtime_task_grant_required".to_owned(),
            message: "Task tool requires a typed Task execution grant".to_owned(),
        });
    };
    if !task.operations.contains(&required) {
        return Err(RuntimeAgentToolResult::Rejected {
            code: "runtime_task_operation_denied".to_owned(),
            message: format!("Task execution grant does not allow {required:?}"),
        });
    }
    let project_id = parse_runtime_target_uuid("project_id", &invocation.grant.target.project_id)?;
    let run_id = parse_runtime_target_uuid("run_id", &invocation.grant.target.run_id)?;
    let agent_id = parse_runtime_target_uuid("agent_id", &invocation.grant.target.agent_id)?;
    Ok(TaskPlanScope {
        project_id,
        run_id,
        agent_id: Some(agent_id),
    })
}

fn parse_runtime_target_uuid(field: &str, value: &str) -> Result<Uuid, RuntimeAgentToolResult> {
    Uuid::parse_str(value).map_err(|error| RuntimeAgentToolResult::Rejected {
        code: "invalid_runtime_tool_target".to_owned(),
        message: format!("{field} is not a valid UUID: {error}"),
    })
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
    use agentdash_agent_runtime::{
        RuntimeTaskExecutionGrant, RuntimeToolAppliedSurfaceEvidence,
        RuntimeToolAuthorizationGrant, RuntimeToolProductTarget, RuntimeToolResolvedContext,
    };
    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentProfileDigest, AgentServiceInstanceId, AgentSourceCoordinate,
        AgentSurfaceDigest, AgentSurfaceRevision,
    };

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

    #[test]
    fn task_grant_does_not_expand_read_into_write() {
        let invocation =
            runtime_invocation(Uuid::new_v4(), vec![RuntimeTaskGrantedOperation::Read]);
        assert!(runtime_task_scope(&invocation, RuntimeTaskGrantedOperation::Read).is_ok());
        let denied = runtime_task_scope(&invocation, RuntimeTaskGrantedOperation::Write)
            .expect_err("read grant must not authorize write");
        assert!(matches!(
            denied,
            RuntimeAgentToolResult::Rejected { code, .. }
                if code == "runtime_task_operation_denied"
        ));
    }

    #[test]
    fn task_target_is_taken_from_each_invocation_grant() {
        let first_run = Uuid::new_v4();
        let second_run = Uuid::new_v4();
        let first = runtime_task_scope(
            &runtime_invocation(first_run, vec![RuntimeTaskGrantedOperation::Read]),
            RuntimeTaskGrantedOperation::Read,
        )
        .unwrap();
        let second = runtime_task_scope(
            &runtime_invocation(second_run, vec![RuntimeTaskGrantedOperation::Read]),
            RuntimeTaskGrantedOperation::Read,
        )
        .unwrap();
        assert_eq!(first.run_id, first_run);
        assert_eq!(second.run_id, second_run);
        assert_ne!(first.run_id, second.run_id);
    }

    fn runtime_invocation(
        run_id: Uuid,
        operations: Vec<RuntimeTaskGrantedOperation>,
    ) -> RuntimeToolInvocation {
        RuntimeToolInvocation {
            context: RuntimeToolResolvedContext {
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
                binding_generation: AgentBindingGeneration(1),
                source: AgentSourceCoordinate::new("source-test").unwrap(),
                service_instance_id: AgentServiceInstanceId::new("service-test").unwrap(),
                profile_digest: AgentProfileDigest::new("profile-test").unwrap(),
                bound_surface_revision: AgentSurfaceRevision(1),
                bound_surface_digest: AgentSurfaceDigest::new("bound-test").unwrap(),
                applied_surface_revision: AgentSurfaceRevision(1),
                applied_surface_digest: AgentSurfaceDigest::new("applied-test").unwrap(),
            },
            tool: AgentToolName::new("task_read").unwrap(),
            arguments: serde_json::json!({}),
            grant: RuntimeToolAuthorizationGrant {
                permission: RuntimeToolPermission::ProductRead,
                effect: RuntimeToolEffect::ReadOnly,
                target: RuntimeToolProductTarget {
                    project_id: Uuid::new_v4().to_string(),
                    run_id: run_id.to_string(),
                    agent_id: Uuid::new_v4().to_string(),
                },
                applied_surface: RuntimeToolAppliedSurfaceEvidence {
                    snapshot_revision: 1,
                    revision: 1,
                    digest: "surface-test".to_owned(),
                    projection_revision: 1,
                    provenance_source: "test".to_owned(),
                    provenance_revision: 1,
                },
                resources: RuntimeToolResourceGrant::Task(RuntimeTaskExecutionGrant {
                    plan_revision: 1,
                    plan_digest: "plan-test".to_owned(),
                    operations,
                }),
            },
        }
    }
}
