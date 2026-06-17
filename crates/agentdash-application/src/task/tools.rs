use std::collections::{BTreeMap, HashSet};
use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleTaskPlanItem, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch,
    SubjectRef, TaskPlanStatus, TaskPriority,
};
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ExecutionContext, ToolUpdateCallback,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::task::plan::{
    RunTaskPlanFilter, StoryTaskProjectionItemView, StoryTaskProjectionSourceKind,
    archive_run_task, build_story_task_projection, create_run_task, list_run_tasks,
    reorder_run_tasks, transition_run_task_status, update_run_task,
};

#[derive(Debug, Clone)]
pub struct TaskToolScope {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct TaskToolContext {
    runtime_session_id: Option<String>,
}

impl TaskToolContext {
    pub fn from_execution_context(context: &ExecutionContext) -> Self {
        Self {
            runtime_session_id: context
                .turn
                .hook_runtime
                .as_ref()
                .map(|runtime| runtime.session_id().to_string()),
        }
    }
}

impl TaskToolScope {
    pub async fn from_tool_context(
        repos: &RepositorySet,
        context: &TaskToolContext,
    ) -> Result<Self, AgentToolError> {
        let session_id = context.runtime_session_id.clone().ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 缺少 hook runtime，无法定位 Task scope".to_string(),
                )
            })?;
        let anchor = repos
            .execution_anchor_repo
            .find_by_session(&session_id)
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "查询 runtime session `{session_id}` 的执行锚点失败: {error}"
                ))
            })?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "runtime session `{session_id}` 缺少执行锚点，无法定位 Task scope"
                ))
            })?;
        let agent = repos
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "查询 LifecycleAgent `{}` 失败: {error}",
                    anchor.agent_id
                ))
            })?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "LifecycleAgent `{}` 不存在，无法定位 Task scope",
                    anchor.agent_id
                ))
            })?;
        if agent.run_id != anchor.run_id {
            return Err(AgentToolError::ExecutionFailed(format!(
                "执行锚点 run_id `{}` 与 LifecycleAgent run_id `{}` 不一致",
                anchor.run_id, agent.run_id
            )));
        }
        Ok(Self {
            project_id: agent.project_id,
            run_id: agent.run_id,
            agent_id: Some(agent.id),
        })
    }
}

#[derive(Clone)]
pub struct TaskReadTool {
    repos: RepositorySet,
    tool_context: TaskToolContext,
}

impl TaskReadTool {
    pub fn new(repos: RepositorySet, tool_context: TaskToolContext) -> Self {
        Self {
            repos,
            tool_context,
        }
    }
}

#[derive(Clone)]
pub struct TaskWriteTool {
    repos: RepositorySet,
    tool_context: TaskToolContext,
}

impl TaskWriteTool {
    pub fn new(repos: RepositorySet, tool_context: TaskToolContext) -> Self {
        Self {
            repos,
            tool_context,
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
    Execution,
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

const COMPACT_BODY_MAX_CHARS: usize = 160;

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
    pub kind: String,
    pub locator: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub delivery: Option<String>,
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
    ReorderTasks { task_ids: Vec<Uuid> },
    DropTask { task_id: String },
    ReplaceContextRefs {
        task_id: String,
        context_refs: Vec<ContextSourceRefInput>,
    },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskCreateInput {
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

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskPatchInput {
    pub task_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<Option<String>>,
    #[serde(default)]
    pub priority: Option<Option<TaskPriorityInput>>,
    #[serde(default)]
    pub owner_agent_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub assigned_agent_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub source_task_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub context_refs: Option<Vec<ContextSourceRefInput>>,
    #[serde(default)]
    pub story_ref: Option<Option<SubjectRefInput>>,
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

#[async_trait]
impl AgentTool for TaskReadTool {
    fn name(&self) -> &str {
        "task_read"
    }

    fn description(&self) -> &str {
        "读取当前 AgentRun / LifecycleRun 的 Task view。mode 支持 overview、list、detail、context、execution、projection；Task facts 来自 LifecycleRun.tasks，Story 只返回 projection。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<TaskReadParams>()
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
        let scope = TaskToolScope::from_tool_context(&self.repos, &self.tool_context).await?;
        let view = build_read_view(&self.repos, &scope, params).await?;
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

    async fn execute(
        &self,
        _tool_use_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: TaskWriteParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let scope = TaskToolScope::from_tool_context(&self.repos, &self.tool_context).await?;
        let run_id = params.run_id.unwrap_or(scope.run_id);
        ensure_run_scope(&self.repos, &scope, run_id).await?;
        let mut changes: Vec<TaskChange> = Vec::new();
        match params.mode {
            TaskWriteMode::Patch => {
                for operation in params.operations {
                    apply_operation(&self.repos, &scope, run_id, operation, &mut changes).await?;
                }
            }
            TaskWriteMode::Snapshot => {
                apply_snapshot(
                    &self.repos,
                    &scope,
                    run_id,
                    params.snapshot,
                    params.drop_missing,
                    &mut changes,
                )
                .await?;
            }
        }

        // 写后回传等价于一次 task_read，且额外带上本次变更清单，供模型与 UI 卡片消费。
        // 不按单个 task_id 过滤：批量写入时模型期望拿到完整 plan，具体变更由 changes 表达。
        let mut view = build_read_view(
            &self.repos,
            &scope,
            TaskReadParams {
                mode: params.return_mode,
                format: TaskReadFormat::Compact,
                run_id: Some(run_id),
                task_id: None,
                story_id: None,
                include_archived: false,
                owner_agent_id: None,
                assigned_agent_id: None,
                statuses: Vec::new(),
            },
        )
        .await?;
        if let Some(object) = view.as_object_mut() {
            object.insert(
                "changes".to_string(),
                serde_json::to_value(&changes).unwrap_or(serde_json::Value::Null),
            );
        }
        let message = format!("Task 写入完成，变更 {} 个 Task。", changes.len());
        Ok(result_with_view(&message, view, false))
    }
}

/// 单次 task_write operation 产生的变更摘要。既进 tool result `content`（模型可读），
/// 也进 `details`（前端卡片渲染），是 R2 自定义卡片的数据源。
#[derive(Debug, Clone, Serialize)]
pub struct TaskChange {
    pub task_id: Uuid,
    pub title: String,
    pub change_kind: TaskChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_from: Option<TaskPlanStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_to: Option<TaskPlanStatus>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskChangeKind {
    Created,
    Updated,
    StatusChanged,
    Reordered,
    Dropped,
    ContextRefsReplaced,
}

async fn apply_operation(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    run_id: Uuid,
    operation: TaskWriteOperation,
    changes: &mut Vec<TaskChange>,
) -> Result<(), AgentToolError> {
    match operation {
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
        } => {
            let result = create_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                draft_from_create(
                    TaskCreateInput {
                        title,
                        body,
                        status,
                        priority,
                        owner_agent_id,
                        assigned_agent_id,
                        source_task_id,
                        context_refs,
                        story_ref,
                    },
                    scope,
                )?,
            )
            .await
            .map_err(tool_error)?;
            changes.push(TaskChange::simple(&result.task, TaskChangeKind::Created));
        }
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
        } => {
            let task_id = resolve_task_selector(repos, run_id, &task_id).await?;
            let result = update_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                task_id,
                patch_from_input(TaskPatchInput {
                    task_id: task_id.to_string(),
                    title,
                    body,
                    priority,
                    owner_agent_id,
                    assigned_agent_id,
                    source_task_id,
                    context_refs,
                    story_ref,
                })?,
            )
            .await
            .map_err(tool_error)?;
            changes.push(TaskChange::simple(&result.task, TaskChangeKind::Updated));
        }
        TaskWriteOperation::SetStatus { task_id, status } => {
            let task_id = resolve_task_selector(repos, run_id, &task_id).await?;
            let status_from = read_task_status(repos, run_id, task_id).await?;
            let result = transition_run_task_status(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                task_id,
                status.into(),
            )
            .await
            .map_err(tool_error)?;
            changes.push(TaskChange {
                task_id: result.task.id,
                title: result.task.title.clone(),
                change_kind: TaskChangeKind::StatusChanged,
                status_from,
                status_to: Some(result.task.status),
            });
        }
        TaskWriteOperation::ReorderTasks { task_ids } => {
            reorder_run_tasks(repos.lifecycle_run_repo.as_ref(), run_id, task_ids.clone())
                .await
                .map_err(tool_error)?;
            // 排序后读回标题，避免回传里出现裸 UUID。
            let run = ensure_run_scope(repos, scope, run_id).await?;
            for task_id in task_ids {
                let title = run
                    .task_by_id(task_id)
                    .map(|task| task.title.clone())
                    .unwrap_or_default();
                changes.push(TaskChange {
                    task_id,
                    title,
                    change_kind: TaskChangeKind::Reordered,
                    status_from: None,
                    status_to: None,
                });
            }
        }
        TaskWriteOperation::DropTask { task_id } => {
            let task_id = resolve_task_selector(repos, run_id, &task_id).await?;
            let result = archive_run_task(repos.lifecycle_run_repo.as_ref(), run_id, task_id)
                .await
                .map_err(tool_error)?;
            changes.push(TaskChange::simple(&result.task, TaskChangeKind::Dropped));
        }
        TaskWriteOperation::ReplaceContextRefs {
            task_id,
            context_refs,
        } => {
            let task_id = resolve_task_selector(repos, run_id, &task_id).await?;
            let result = update_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                task_id,
                LifecycleTaskPlanItemPatch {
                    context_refs: Some(convert_context_refs(context_refs)?),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .await
            .map_err(tool_error)?;
            changes.push(TaskChange::simple(
                &result.task,
                TaskChangeKind::ContextRefsReplaced,
            ));
        }
    }
    Ok(())
}

impl TaskChange {
    fn simple(task: &LifecycleTaskPlanItem, change_kind: TaskChangeKind) -> Self {
        Self {
            task_id: task.id,
            title: task.title.clone(),
            change_kind,
            status_from: None,
            status_to: None,
        }
    }
}

async fn read_task_status(
    repos: &RepositorySet,
    run_id: Uuid,
    task_id: Uuid,
) -> Result<Option<TaskPlanStatus>, AgentToolError> {
    let run = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(tool_error)?;
    Ok(run
        .and_then(|run| run.task_by_id(task_id).map(|task| task.status)))
}

async fn apply_snapshot(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    run_id: Uuid,
    snapshot: Vec<TaskSnapshotItem>,
    drop_missing: bool,
    changes: &mut Vec<TaskChange>,
) -> Result<(), AgentToolError> {
    let existing = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(tool_error)?
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在")))?;
    let existing_ids = existing.tasks.iter().map(|task| task.id).collect::<Vec<_>>();
    let mut title_matches = existing
        .tasks
        .iter()
        .filter(|task| task.archived_at.is_none())
        .fold(BTreeMap::<String, Vec<Uuid>>::new(), |mut acc, task| {
            acc.entry(task.title.clone()).or_default().push(task.id);
            acc
        });
    let mut matched_ids = HashSet::new();
    let mut ordered_ids = Vec::new();

    for item in snapshot {
        let normalized_title = normalize_title(&item.title)?;
        let status = item.status;
        let maybe_id = item.id.or_else(|| {
            title_matches
                .get_mut(&normalized_title)
                .and_then(|ids| {
                    while let Some(task_id) = ids.pop() {
                        if matched_ids.insert(task_id) {
                            return Some(task_id);
                        }
                    }
                    None
                })
        });
        let change = if let Some(task_id) = maybe_id {
            if let Some(prior) = existing.task_by_id(task_id) {
                let status_from = prior.status;
                let result = update_run_task(
                    repos.lifecycle_run_repo.as_ref(),
                    run_id,
                    task_id,
                    patch_from_snapshot(item)?,
                )
                .await
                .map_err(tool_error)?;
                let mut task = result.task;
                let mut status_changed = false;
                if let Some(status) = status {
                    let transitioned = transition_run_task_status(
                        repos.lifecycle_run_repo.as_ref(),
                        run_id,
                        task_id,
                        status.into(),
                    )
                    .await
                    .map_err(tool_error)?;
                    status_changed = transitioned.task.status != status_from;
                    task = transitioned.task;
                }
                if status_changed {
                    TaskChange {
                        task_id: task.id,
                        title: task.title.clone(),
                        change_kind: TaskChangeKind::StatusChanged,
                        status_from: Some(status_from),
                        status_to: Some(task.status),
                    }
                } else {
                    TaskChange::simple(&task, TaskChangeKind::Updated)
                }
            } else {
                let result = create_run_task(
                    repos.lifecycle_run_repo.as_ref(),
                    run_id,
                    draft_from_snapshot(item, scope)?,
                )
                .await
                .map_err(tool_error)?;
                TaskChange::simple(&result.task, TaskChangeKind::Created)
            }
        } else {
            let result = create_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                draft_from_snapshot(item, scope)?,
            )
            .await
            .map_err(tool_error)?;
            TaskChange::simple(&result.task, TaskChangeKind::Created)
        };
        ordered_ids.push(change.task_id);
        changes.push(change);
    }

    reorder_run_tasks(repos.lifecycle_run_repo.as_ref(), run_id, ordered_ids.clone())
        .await
        .map_err(tool_error)?;
    if drop_missing {
        for task_id in existing_ids {
            if !ordered_ids.contains(&task_id) {
                let result = archive_run_task(repos.lifecycle_run_repo.as_ref(), run_id, task_id)
                    .await
                    .map_err(tool_error)?;
                changes.push(TaskChange::simple(&result.task, TaskChangeKind::Dropped));
            }
        }
    }
    Ok(())
}

async fn build_read_view(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    params: TaskReadParams,
) -> Result<serde_json::Value, AgentToolError> {
    let run_id = params.run_id.unwrap_or(scope.run_id);
    ensure_run_scope(repos, scope, run_id).await?;
    if params.mode == TaskReadMode::Projection {
        if let Some(story_id) = params.story_id {
            let projection = build_story_task_projection(
                repos.lifecycle_run_repo.as_ref(),
                repos.lifecycle_subject_association_repo.as_ref(),
                scope.project_id,
                story_id,
            )
            .await
            .map_err(tool_error)?;
            return Ok(serde_json::json!({
                "mode": "projection",
                "scope": scope_json(scope, run_id),
                "story_id": story_id,
                "tasks": projection.tasks.iter().map(projection_item_json).collect::<Vec<_>>(),
            }));
        }
    }

    let filter = RunTaskPlanFilter {
        created_by_agent_id: None,
        owner_agent_id: params.owner_agent_id.or(scope.agent_id),
        assigned_agent_id: params.assigned_agent_id,
        include_archived: params.include_archived,
    };
    let view = list_run_tasks(repos.lifecycle_run_repo.as_ref(), run_id, filter)
        .await
        .map_err(tool_error)?;
    let mut tasks = view.tasks;
    if !params.statuses.is_empty() {
        let statuses = params
            .statuses
            .iter()
            .copied()
            .map(TaskPlanStatus::from)
            .collect::<Vec<_>>();
        tasks.retain(|task| statuses.contains(&task.status));
    }
    if let Some(task_id) = params.task_id {
        tasks.retain(|task| task.id == task_id);
    }

    Ok(match params.mode {
        TaskReadMode::Overview => overview_view(scope, run_id, &tasks, params.format),
        TaskReadMode::List => serde_json::json!({
            "mode": "list",
            "scope": scope_json(scope, run_id),
            "tasks": render_tasks(&tasks, params.format),
        }),
        // detail 始终 full：它本就是为读取单个 Task 的完整内容设计。
        TaskReadMode::Detail => serde_json::json!({
            "mode": "detail",
            "scope": scope_json(scope, run_id),
            "tasks": render_tasks(&tasks, TaskReadFormat::Full),
        }),
        TaskReadMode::Context => context_view(scope, run_id, &tasks),
        TaskReadMode::Execution => serde_json::json!({
            "mode": "execution",
            "scope": scope_json(scope, run_id),
            "tasks": tasks.iter().map(execution_stub).collect::<Vec<_>>(),
            "source": "SubjectExecutionView / linked run projection",
        }),
        TaskReadMode::Projection => serde_json::json!({
            "mode": "projection",
            "scope": scope_json(scope, run_id),
            "tasks": render_tasks(&tasks, params.format),
            "source": "run",
        }),
    })
}

/// 按 format 把 Task 列表投影成 JSON：compact 仅核心字段（body 截断、context_refs 计数），
/// full 序列化完整 Task。
fn render_tasks(tasks: &[LifecycleTaskPlanItem], format: TaskReadFormat) -> Vec<serde_json::Value> {
    tasks.iter().map(|task| task_json(task, format)).collect()
}

fn task_json(task: &LifecycleTaskPlanItem, format: TaskReadFormat) -> serde_json::Value {
    match format {
        TaskReadFormat::Full => serde_json::to_value(task).unwrap_or(serde_json::Value::Null),
        TaskReadFormat::Compact => {
            let body_preview = task.body.as_ref().map(|body| {
                if body.chars().count() > COMPACT_BODY_MAX_CHARS {
                    let truncated: String = body.chars().take(COMPACT_BODY_MAX_CHARS).collect();
                    format!("{truncated}…")
                } else {
                    body.clone()
                }
            });
            serde_json::json!({
                "id": task.id,
                "title": task.title,
                "status": task.status,
                "priority": task.priority,
                "assigned_agent_id": task.assigned_agent_id,
                "body_preview": body_preview,
                "context_refs_count": task.context_refs.len(),
                "archived": task.archived_at.is_some(),
            })
        }
    }
}

fn overview_view(
    scope: &TaskToolScope,
    run_id: Uuid,
    tasks: &[LifecycleTaskPlanItem],
    format: TaskReadFormat,
) -> serde_json::Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for task in tasks {
        *counts.entry(status_key(task.status).to_string()).or_default() += 1;
    }
    // current_items = 进行中（active/review/blocked），命名避免与单一 active 状态混淆。
    let current = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskPlanStatus::Active | TaskPlanStatus::Review | TaskPlanStatus::Blocked
            )
        })
        .map(|task| task_json(task, format))
        .collect::<Vec<_>>();
    let done = tasks
        .iter()
        .filter(|task| task.status == TaskPlanStatus::Done)
        .count();
    serde_json::json!({
        "mode": "overview",
        "scope": scope_json(scope, run_id),
        "counts": counts,
        "current_items": current,
        "done": done,
        "total": tasks.len(),
    })
}

fn context_view(scope: &TaskToolScope, run_id: Uuid, tasks: &[LifecycleTaskPlanItem]) -> serde_json::Value {
    serde_json::json!({
        "mode": "context",
        "scope": scope_json(scope, run_id),
        "tasks": tasks.iter().map(|task| serde_json::json!({
            "id": task.id,
            "title": task.title,
            "status": task.status,
            "context_refs": task.context_refs,
            "story_ref": task.story_ref,
        })).collect::<Vec<_>>(),
    })
}

fn execution_stub(task: &LifecycleTaskPlanItem) -> serde_json::Value {
    serde_json::json!({
        "task_id": task.id,
        "title": task.title,
        "status": task.status,
        "assigned_agent_id": task.assigned_agent_id,
        "source_task_id": task.source_task_id,
        "execution_summary": null,
        "note": "Task facts 不保存 runtime execution；执行事实由 SubjectExecutionView / linked run projection 读取。",
    })
}

fn projection_item_json(item: &StoryTaskProjectionItemView) -> serde_json::Value {
    serde_json::json!({
        "project_id": item.project_id,
        "owning_run_id": item.owning_run_id,
        "task": item.task,
        "sources": item.sources.iter().map(|source| serde_json::json!({
            "kind": match source.kind {
                StoryTaskProjectionSourceKind::OwningRun => "owning_run",
                StoryTaskProjectionSourceKind::LinkedRun => "linked_run",
                StoryTaskProjectionSourceKind::StoryRef => "story_ref",
            },
            "run_id": source.run_id,
            "agent_id": source.agent_id,
            "story_ref": source.story_ref,
            "reason": source.reason,
        })).collect::<Vec<_>>(),
    })
}

fn scope_json(scope: &TaskToolScope, run_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "project_id": scope.project_id,
        "run_id": run_id,
        "agent_id": scope.agent_id,
    })
}

async fn ensure_run_scope(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    run_id: Uuid,
) -> Result<LifecycleRun, AgentToolError> {
    let run = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(tool_error)?
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在")))?;
    if run.project_id != scope.project_id {
        return Err(AgentToolError::ExecutionFailed(format!(
            "LifecycleRun {run_id} 不属于当前 project {}",
            scope.project_id
        )));
    }
    Ok(run)
}

fn draft_from_create(
    input: TaskCreateInput,
    scope: &TaskToolScope,
) -> Result<LifecycleTaskPlanItemDraft, AgentToolError> {
    let title = normalize_title(&input.title)?;
    Ok(LifecycleTaskPlanItemDraft {
        id: None,
        title,
        body: input.body,
        status: input.status.map(TaskPlanStatus::from).unwrap_or_default(),
        priority: input.priority.map(TaskPriority::from),
        created_by_agent_id: scope.agent_id,
        owner_agent_id: input.owner_agent_id.or(scope.agent_id),
        assigned_agent_id: input.assigned_agent_id,
        source_task_id: input.source_task_id,
        context_refs: convert_context_refs(input.context_refs)?,
        story_ref: input.story_ref.map(SubjectRef::from),
    })
}

fn draft_from_snapshot(
    item: TaskSnapshotItem,
    scope: &TaskToolScope,
) -> Result<LifecycleTaskPlanItemDraft, AgentToolError> {
    let title = normalize_title(&item.title)?;
    Ok(LifecycleTaskPlanItemDraft {
        id: item.id,
        title,
        body: item.body,
        status: item.status.map(TaskPlanStatus::from).unwrap_or_default(),
        priority: item.priority.map(TaskPriority::from),
        created_by_agent_id: scope.agent_id,
        owner_agent_id: item.owner_agent_id.or(scope.agent_id),
        assigned_agent_id: item.assigned_agent_id,
        source_task_id: item.source_task_id,
        context_refs: convert_context_refs(item.context_refs)?,
        story_ref: item.story_ref.map(SubjectRef::from),
    })
}

fn patch_from_snapshot(item: TaskSnapshotItem) -> Result<LifecycleTaskPlanItemPatch, AgentToolError> {
    Ok(LifecycleTaskPlanItemPatch {
        title: Some(normalize_title(&item.title)?),
        body: Some(item.body),
        priority: Some(item.priority.map(TaskPriority::from)),
        owner_agent_id: Some(item.owner_agent_id),
        assigned_agent_id: Some(item.assigned_agent_id),
        source_task_id: Some(item.source_task_id),
        context_refs: Some(convert_context_refs(item.context_refs)?),
        story_ref: Some(item.story_ref.map(SubjectRef::from)),
    })
}

fn patch_from_input(input: TaskPatchInput) -> Result<LifecycleTaskPlanItemPatch, AgentToolError> {
    Ok(LifecycleTaskPlanItemPatch {
        title: input.title.map(|title| normalize_title(&title)).transpose()?,
        body: input.body,
        priority: input.priority.map(|value| value.map(TaskPriority::from)),
        owner_agent_id: input.owner_agent_id,
        assigned_agent_id: input.assigned_agent_id,
        source_task_id: input.source_task_id,
        context_refs: input.context_refs.map(convert_context_refs).transpose()?,
        story_ref: input
            .story_ref
            .map(|value| value.map(SubjectRef::from)),
    })
}

async fn resolve_task_selector(
    repos: &RepositorySet,
    run_id: Uuid,
    selector: &str,
) -> Result<Uuid, AgentToolError> {
    if let Ok(task_id) = Uuid::parse_str(selector) {
        return Ok(task_id);
    }

    let title = normalize_title(selector)?;
    let run = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(tool_error)?
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在")))?;
    let matches = run
        .tasks
        .iter()
        .filter(|task| task.archived_at.is_none() && task.title == title)
        .map(|task| task.id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [task_id] => Ok(*task_id),
        [] => Err(AgentToolError::InvalidArguments(format!(
            "未找到标题为 `{title}` 的未归档 Task"
        ))),
        ids => Err(AgentToolError::InvalidArguments(format!(
            "标题 `{title}` 匹配到 {} 个未归档 Task，请改用 task_id: {}",
            ids.len(),
            ids.iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn normalize_title(title: &str) -> Result<String, AgentToolError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AgentToolError::InvalidArguments(
            "Task 标题不能为空".to_string(),
        ));
    }
    Ok(title.to_string())
}

fn convert_context_refs(
    refs: Vec<ContextSourceRefInput>,
) -> Result<Vec<ContextSourceRef>, AgentToolError> {
    refs.into_iter().map(ContextSourceRef::try_from).collect()
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

impl From<SubjectRefInput> for SubjectRef {
    fn from(value: SubjectRefInput) -> Self {
        SubjectRef::new(value.kind, value.id)
    }
}

impl TryFrom<ContextSourceRefInput> for ContextSourceRef {
    type Error = AgentToolError;

    fn try_from(value: ContextSourceRefInput) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: parse_context_kind(&value.kind)?,
            locator: value.locator,
            label: value.label,
            slot: value
                .slot
                .as_deref()
                .map(parse_context_slot)
                .transpose()?
                .unwrap_or_default(),
            priority: value.priority.unwrap_or_default(),
            required: value.required,
            max_chars: value.max_chars,
            delivery: value
                .delivery
                .as_deref()
                .map(parse_context_delivery)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

fn parse_context_kind(value: &str) -> Result<ContextSourceKind, AgentToolError> {
    match value {
        "manual_text" => Ok(ContextSourceKind::ManualText),
        "file" => Ok(ContextSourceKind::File),
        "project_snapshot" => Ok(ContextSourceKind::ProjectSnapshot),
        "http_fetch" => Ok(ContextSourceKind::HttpFetch),
        "mcp_resource" => Ok(ContextSourceKind::McpResource),
        "entity_ref" => Ok(ContextSourceKind::EntityRef),
        other => Err(AgentToolError::InvalidArguments(format!(
            "未知 context kind `{other}`，可用值: manual_text | file | project_snapshot | http_fetch | mcp_resource | entity_ref"
        ))),
    }
}

fn parse_context_slot(value: &str) -> Result<ContextSlot, AgentToolError> {
    match value {
        "requirements" => Ok(ContextSlot::Requirements),
        "constraints" => Ok(ContextSlot::Constraints),
        "codebase" => Ok(ContextSlot::Codebase),
        "references" => Ok(ContextSlot::References),
        "instruction_append" => Ok(ContextSlot::InstructionAppend),
        other => Err(AgentToolError::InvalidArguments(format!(
            "未知 context slot `{other}`，可用值: requirements | constraints | codebase | references | instruction_append"
        ))),
    }
}

fn parse_context_delivery(value: &str) -> Result<ContextDelivery, AgentToolError> {
    match value {
        "inline" => Ok(ContextDelivery::Inline),
        "resource" => Ok(ContextDelivery::Resource),
        "lazy" => Ok(ContextDelivery::Lazy),
        other => Err(AgentToolError::InvalidArguments(format!(
            "未知 context delivery `{other}`，可用值: inline | resource | lazy"
        ))),
    }
}

fn status_key(status: TaskPlanStatus) -> &'static str {
    match status {
        TaskPlanStatus::Open => "open",
        TaskPlanStatus::Active => "active",
        TaskPlanStatus::Review => "review",
        TaskPlanStatus::Blocked => "blocked",
        TaskPlanStatus::Done => "done",
        TaskPlanStatus::Dropped => "dropped",
    }
}

/// 构造工具结果：把 view JSON 既写进 `content`（模型可读 —— 所有 connector bridge 仅从
/// content 构造模型可见 output，details 永不进模型），也写进 `details`（持久化/调试）。
fn result_with_view(summary: &str, view: serde_json::Value, is_error: bool) -> AgentToolResult {
    let body = serde_json::to_string_pretty(&view).unwrap_or_else(|_| view.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(format!("{summary}\n{body}"))],
        is_error,
        details: Some(view),
    }
}

fn tool_error(error: impl std::fmt::Display) -> AgentToolError {
    AgentToolError::ExecutionFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_task(body: Option<String>) -> LifecycleTaskPlanItem {
        LifecycleTaskPlanItem {
            id: Uuid::nil(),
            title: "示例 Task".to_string(),
            body,
            status: TaskPlanStatus::Active,
            priority: Some(TaskPriority::P1),
            created_by_agent_id: None,
            owner_agent_id: None,
            assigned_agent_id: None,
            source_task_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
            context_refs: Vec::new(),
            story_ref: None,
        }
    }

    #[test]
    fn result_with_view_puts_json_into_content_for_model() {
        // P0 回归保护：模型可见 output 仅来自 content，因此 view 必须进 content。
        let view = serde_json::json!({ "mode": "list", "tasks": [{ "title": "示例 Task" }] });
        let result = result_with_view("Task view 已读取", view, false);
        let text = match &result.content[0] {
            ContentPart::Text { text } => text,
            other => panic!("expected text content, got {other:?}"),
        };
        assert!(text.contains("示例 Task"), "content 应包含 Task 数据: {text}");
        assert!(text.starts_with("Task view 已读取"));
        assert!(result.details.is_some(), "details 仍保留供持久化");
    }

    #[test]
    fn compact_projection_truncates_body_and_counts_refs() {
        let long_body = "字".repeat(COMPACT_BODY_MAX_CHARS + 50);
        let task = sample_task(Some(long_body));
        let compact = task_json(&task, TaskReadFormat::Compact);
        let preview = compact["body_preview"].as_str().unwrap();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), COMPACT_BODY_MAX_CHARS + 1); // +1 = 省略号
        assert_eq!(compact["context_refs_count"], 0);
        assert!(compact.get("created_at").is_none(), "compact 不带审计时间戳");
    }

    #[test]
    fn full_projection_keeps_all_fields() {
        let task = sample_task(Some("短内容".to_string()));
        let full = task_json(&task, TaskReadFormat::Full);
        assert!(full.get("created_at").is_some(), "full 应保留完整字段");
        assert_eq!(full["body"], "短内容");
    }

    #[test]
    fn task_change_serializes_status_transition() {
        let change = TaskChange {
            task_id: Uuid::nil(),
            title: "示例 Task".to_string(),
            change_kind: TaskChangeKind::StatusChanged,
            status_from: Some(TaskPlanStatus::Open),
            status_to: Some(TaskPlanStatus::Active),
        };
        let value = serde_json::to_value(&change).unwrap();
        assert_eq!(value["change_kind"], "status_changed");
        assert_eq!(value["status_from"], "open");
        assert_eq!(value["status_to"], "active");
    }

    #[test]
    fn task_change_simple_omits_status_fields() {
        let task = sample_task(None);
        let value = serde_json::to_value(TaskChange::simple(&task, TaskChangeKind::Created)).unwrap();
        assert_eq!(value["change_kind"], "created");
        assert!(value.get("status_from").is_none());
        assert!(value.get("status_to").is_none());
    }
}
