use std::collections::BTreeMap;
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
use serde::Deserialize;
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskReadParams {
    #[serde(default = "default_read_mode")]
    pub mode: TaskReadMode,
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
    CreateTask(TaskCreateInput),
    PatchTask(TaskPatchInput),
    SetStatus { task_id: Uuid, status: TaskStatusInput },
    ReorderTasks { task_ids: Vec<Uuid> },
    DropTask { task_id: Uuid },
    ReplaceContextRefs {
        task_id: Uuid,
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
    pub task_id: Uuid,
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
        let details = build_read_view(&self.repos, &scope, params).await?;
        Ok(result_from_details("Task view 已读取", details, false))
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
        let mut changed_task_ids = Vec::new();
        match params.mode {
            TaskWriteMode::Patch => {
                for operation in params.operations {
                    apply_operation(&self.repos, &scope, run_id, operation, &mut changed_task_ids)
                        .await?;
                }
            }
            TaskWriteMode::Snapshot => {
                apply_snapshot(
                    &self.repos,
                    &scope,
                    run_id,
                    params.snapshot,
                    params.drop_missing,
                    &mut changed_task_ids,
                )
                .await?;
            }
        }

        let details = build_read_view(
            &self.repos,
            &scope,
            TaskReadParams {
                mode: params.return_mode,
                run_id: Some(run_id),
                task_id: changed_task_ids.first().copied(),
                story_id: None,
                include_archived: true,
                owner_agent_id: None,
                assigned_agent_id: None,
                statuses: Vec::new(),
            },
        )
        .await?;
        let message = format!("Task 写入完成，变更 {} 个 Task。", changed_task_ids.len());
        Ok(result_from_details(&message, details, false))
    }
}

async fn apply_operation(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    run_id: Uuid,
    operation: TaskWriteOperation,
    changed_task_ids: &mut Vec<Uuid>,
) -> Result<(), AgentToolError> {
    match operation {
        TaskWriteOperation::CreateTask(input) => {
            let result = create_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                draft_from_create(input, scope)?,
            )
            .await
            .map_err(tool_error)?;
            changed_task_ids.push(result.task.id);
        }
        TaskWriteOperation::PatchTask(input) => {
            let task_id = input.task_id;
            let result = update_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                task_id,
                patch_from_input(input)?,
            )
            .await
            .map_err(tool_error)?;
            changed_task_ids.push(result.task.id);
        }
        TaskWriteOperation::SetStatus { task_id, status } => {
            let result = transition_run_task_status(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                task_id,
                status.into(),
            )
            .await
            .map_err(tool_error)?;
            changed_task_ids.push(result.task.id);
        }
        TaskWriteOperation::ReorderTasks { task_ids } => {
            reorder_run_tasks(repos.lifecycle_run_repo.as_ref(), run_id, task_ids.clone())
                .await
                .map_err(tool_error)?;
            changed_task_ids.extend(task_ids);
        }
        TaskWriteOperation::DropTask { task_id } => {
            let result = archive_run_task(repos.lifecycle_run_repo.as_ref(), run_id, task_id)
                .await
                .map_err(tool_error)?;
            changed_task_ids.push(result.task.id);
        }
        TaskWriteOperation::ReplaceContextRefs {
            task_id,
            context_refs,
        } => {
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
            changed_task_ids.push(result.task.id);
        }
    }
    Ok(())
}

async fn apply_snapshot(
    repos: &RepositorySet,
    scope: &TaskToolScope,
    run_id: Uuid,
    snapshot: Vec<TaskSnapshotItem>,
    drop_missing: bool,
    changed_task_ids: &mut Vec<Uuid>,
) -> Result<(), AgentToolError> {
    let existing = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(tool_error)?
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在")))?;
    let existing_ids = existing.tasks.iter().map(|task| task.id).collect::<Vec<_>>();
    let mut ordered_ids = Vec::new();

    for item in snapshot {
        let maybe_id = item.id;
        let task_id = if let Some(task_id) = maybe_id {
            if existing.task_by_id(task_id).is_some() {
                update_run_task(
                    repos.lifecycle_run_repo.as_ref(),
                    run_id,
                    task_id,
                    patch_from_snapshot(item)?,
                )
                .await
                .map_err(tool_error)?;
                task_id
            } else {
                create_run_task(
                    repos.lifecycle_run_repo.as_ref(),
                    run_id,
                    draft_from_snapshot(item, scope)?,
                )
                .await
                .map_err(tool_error)?
                .task
                .id
            }
        } else {
            create_run_task(
                repos.lifecycle_run_repo.as_ref(),
                run_id,
                draft_from_snapshot(item, scope)?,
            )
            .await
            .map_err(tool_error)?
            .task
            .id
        };
        ordered_ids.push(task_id);
        changed_task_ids.push(task_id);
    }

    reorder_run_tasks(repos.lifecycle_run_repo.as_ref(), run_id, ordered_ids.clone())
        .await
        .map_err(tool_error)?;
    if drop_missing {
        for task_id in existing_ids {
            if !ordered_ids.contains(&task_id) {
                archive_run_task(repos.lifecycle_run_repo.as_ref(), run_id, task_id)
                    .await
                    .map_err(tool_error)?;
                changed_task_ids.push(task_id);
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
        TaskReadMode::Overview => overview_view(scope, run_id, &tasks),
        TaskReadMode::List => serde_json::json!({
            "mode": "list",
            "scope": scope_json(scope, run_id),
            "tasks": tasks,
        }),
        TaskReadMode::Detail => serde_json::json!({
            "mode": "detail",
            "scope": scope_json(scope, run_id),
            "tasks": tasks,
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
            "tasks": tasks,
            "source": "run",
        }),
    })
}

fn overview_view(scope: &TaskToolScope, run_id: Uuid, tasks: &[LifecycleTaskPlanItem]) -> serde_json::Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for task in tasks {
        *counts.entry(status_key(task.status).to_string()).or_default() += 1;
    }
    let active = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskPlanStatus::Active | TaskPlanStatus::Review | TaskPlanStatus::Blocked
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    serde_json::json!({
        "mode": "overview",
        "scope": scope_json(scope, run_id),
        "counts": counts,
        "active_items": active,
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
            "未知 context kind: {other}"
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
            "未知 context slot: {other}"
        ))),
    }
}

fn parse_context_delivery(value: &str) -> Result<ContextDelivery, AgentToolError> {
    match value {
        "inline" => Ok(ContextDelivery::Inline),
        "resource" => Ok(ContextDelivery::Resource),
        "lazy" => Ok(ContextDelivery::Lazy),
        other => Err(AgentToolError::InvalidArguments(format!(
            "未知 context delivery: {other}"
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

fn result_from_details(message: &str, details: serde_json::Value, is_error: bool) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(message.to_string())],
        is_error,
        details: Some(details),
    }
}

fn tool_error(error: impl std::fmt::Display) -> AgentToolError {
    AgentToolError::ExecutionFailed(error.to_string())
}
