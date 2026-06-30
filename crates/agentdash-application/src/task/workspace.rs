use std::collections::{BTreeMap, HashSet};

use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    LifecycleTaskPlanItem, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch, SubjectRef,
    TaskPlanStatus, TaskPriority,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::task::plan::{
    RunTaskPlanFilter, StoryTaskProjectionItemView, StoryTaskProjectionSourceKind,
    archive_run_task, build_story_task_projection, create_run_task, list_run_tasks,
    reorder_run_tasks, transition_run_task_status, update_run_task,
};
use crate::task::scope::TaskPlanScope;

const COMPACT_BODY_MAX_CHARS: usize = 160;

pub struct TaskPlanWorkspace<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
}

impl<'a> TaskPlanWorkspace<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_subject_association_repo,
        }
    }

    pub fn from_repos(repos: &'a RepositorySet) -> Self {
        Self::new(
            repos.lifecycle_run_repo.as_ref(),
            repos.lifecycle_subject_association_repo.as_ref(),
        )
    }

    pub async fn read(
        &self,
        scope: &TaskPlanScope,
        query: TaskPlanReadQuery,
    ) -> Result<serde_json::Value, TaskPlanWorkspaceError> {
        let run_id = query.run_id.unwrap_or(scope.run_id);
        self.ensure_run_scope(scope, run_id).await?;
        if query.mode == TaskPlanReadMode::Projection {
            if let Some(story_id) = query.story_id {
                let projection = build_story_task_projection(
                    self.lifecycle_run_repo,
                    self.lifecycle_subject_association_repo,
                    scope.project_id,
                    story_id,
                )
                .await
                .map_err(TaskPlanWorkspaceError::execution)?;
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
            owner_agent_id: query.owner_agent_id.or(scope.agent_id),
            assigned_agent_id: query.assigned_agent_id,
            include_archived: query.include_archived,
        };
        let view = list_run_tasks(self.lifecycle_run_repo, run_id, filter)
            .await
            .map_err(TaskPlanWorkspaceError::execution)?;
        let mut tasks = view.tasks;
        if !query.statuses.is_empty() {
            tasks.retain(|task| query.statuses.contains(&task.status));
        }
        if let Some(task_id) = query.task_id {
            tasks.retain(|task| task.id == task_id);
        }

        Ok(match query.mode {
            TaskPlanReadMode::Overview => overview_view(scope, run_id, &tasks, query.format),
            TaskPlanReadMode::List => serde_json::json!({
                "mode": "list",
                "scope": scope_json(scope, run_id),
                "tasks": render_tasks(&tasks, query.format),
            }),
            TaskPlanReadMode::Detail => serde_json::json!({
                "mode": "detail",
                "scope": scope_json(scope, run_id),
                "tasks": render_tasks(&tasks, TaskPlanReadFormat::Full),
            }),
            TaskPlanReadMode::Context => context_view(scope, run_id, &tasks),
            TaskPlanReadMode::Projection => serde_json::json!({
                "mode": "projection",
                "scope": scope_json(scope, run_id),
                "tasks": render_tasks(&tasks, query.format),
                "source": "run",
            }),
        })
    }

    pub async fn apply(
        &self,
        scope: &TaskPlanScope,
        changeset: TaskPlanChangeset,
    ) -> Result<TaskPlanApplyResult, TaskPlanWorkspaceError> {
        let run_id = changeset.run_id.unwrap_or(scope.run_id);
        self.ensure_run_scope(scope, run_id).await?;
        let mut changes = Vec::new();
        match changeset.mode {
            TaskPlanChangesetMode::Patch { operations } => {
                for operation in operations {
                    self.apply_operation(scope, run_id, operation, &mut changes)
                        .await?;
                }
            }
            TaskPlanChangesetMode::Snapshot {
                snapshot,
                drop_missing,
            } => {
                self.apply_snapshot(scope, run_id, snapshot, drop_missing, &mut changes)
                    .await?;
            }
        }

        let mut view = self
            .read(
                scope,
                TaskPlanReadQuery {
                    mode: changeset.return_mode,
                    format: TaskPlanReadFormat::Compact,
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
        Ok(TaskPlanApplyResult { view, changes })
    }

    async fn apply_operation(
        &self,
        scope: &TaskPlanScope,
        run_id: Uuid,
        operation: TaskPlanOperation,
        changes: &mut Vec<TaskChange>,
    ) -> Result<(), TaskPlanWorkspaceError> {
        match operation {
            TaskPlanOperation::CreateTask(input) => {
                let result = create_run_task(
                    self.lifecycle_run_repo,
                    run_id,
                    draft_from_create(input, scope)?,
                )
                .await
                .map_err(TaskPlanWorkspaceError::execution)?;
                changes.push(TaskChange::simple(&result.task, TaskChangeKind::Created));
            }
            TaskPlanOperation::PatchTask(input) => {
                let task_id = self.resolve_task_selector(run_id, &input.task_id).await?;
                let result = update_run_task(
                    self.lifecycle_run_repo,
                    run_id,
                    task_id,
                    patch_from_input(TaskPlanPatchCommand {
                        task_id: task_id.to_string(),
                        ..input
                    })?,
                )
                .await
                .map_err(TaskPlanWorkspaceError::execution)?;
                changes.push(TaskChange::simple(&result.task, TaskChangeKind::Updated));
            }
            TaskPlanOperation::SetStatus { task_id, status } => {
                let task_id = self.resolve_task_selector(run_id, &task_id).await?;
                let status_from = self.read_task_status(run_id, task_id).await?;
                let result =
                    transition_run_task_status(self.lifecycle_run_repo, run_id, task_id, status)
                        .await
                        .map_err(TaskPlanWorkspaceError::execution)?;
                changes.push(TaskChange {
                    task_id: result.task.id,
                    title: result.task.title.clone(),
                    change_kind: TaskChangeKind::StatusChanged,
                    status_from,
                    status_to: Some(result.task.status),
                });
            }
            TaskPlanOperation::ReorderTasks { task_ids } => {
                reorder_run_tasks(self.lifecycle_run_repo, run_id, task_ids.clone())
                    .await
                    .map_err(TaskPlanWorkspaceError::execution)?;
                let run = self.ensure_run_scope(scope, run_id).await?;
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
            TaskPlanOperation::DropTask { task_id } => {
                let task_id = self.resolve_task_selector(run_id, &task_id).await?;
                let result = archive_run_task(self.lifecycle_run_repo, run_id, task_id)
                    .await
                    .map_err(TaskPlanWorkspaceError::execution)?;
                changes.push(TaskChange::simple(&result.task, TaskChangeKind::Dropped));
            }
            TaskPlanOperation::ReplaceContextRefs {
                task_id,
                context_refs,
            } => {
                let task_id = self.resolve_task_selector(run_id, &task_id).await?;
                let result = update_run_task(
                    self.lifecycle_run_repo,
                    run_id,
                    task_id,
                    LifecycleTaskPlanItemPatch {
                        context_refs: Some(context_refs),
                        ..LifecycleTaskPlanItemPatch::default()
                    },
                )
                .await
                .map_err(TaskPlanWorkspaceError::execution)?;
                changes.push(TaskChange::simple(
                    &result.task,
                    TaskChangeKind::ContextRefsReplaced,
                ));
            }
        }
        Ok(())
    }

    async fn apply_snapshot(
        &self,
        scope: &TaskPlanScope,
        run_id: Uuid,
        snapshot: Vec<TaskPlanSnapshotItem>,
        drop_missing: bool,
        changes: &mut Vec<TaskChange>,
    ) -> Result<(), TaskPlanWorkspaceError> {
        let existing = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(TaskPlanWorkspaceError::execution)?
            .ok_or_else(|| {
                TaskPlanWorkspaceError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在"))
            })?;
        let existing_ids = existing
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>();
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
                title_matches.get_mut(&normalized_title).and_then(|ids| {
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
                        self.lifecycle_run_repo,
                        run_id,
                        task_id,
                        patch_from_snapshot(item)?,
                    )
                    .await
                    .map_err(TaskPlanWorkspaceError::execution)?;
                    let mut task = result.task;
                    let mut status_changed = false;
                    if let Some(status) = status {
                        let transitioned = transition_run_task_status(
                            self.lifecycle_run_repo,
                            run_id,
                            task_id,
                            status,
                        )
                        .await
                        .map_err(TaskPlanWorkspaceError::execution)?;
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
                        self.lifecycle_run_repo,
                        run_id,
                        draft_from_snapshot(item, scope)?,
                    )
                    .await
                    .map_err(TaskPlanWorkspaceError::execution)?;
                    TaskChange::simple(&result.task, TaskChangeKind::Created)
                }
            } else {
                let result = create_run_task(
                    self.lifecycle_run_repo,
                    run_id,
                    draft_from_snapshot(item, scope)?,
                )
                .await
                .map_err(TaskPlanWorkspaceError::execution)?;
                TaskChange::simple(&result.task, TaskChangeKind::Created)
            };
            ordered_ids.push(change.task_id);
            changes.push(change);
        }

        reorder_run_tasks(self.lifecycle_run_repo, run_id, ordered_ids.clone())
            .await
            .map_err(TaskPlanWorkspaceError::execution)?;
        if drop_missing {
            for task_id in existing_ids {
                if !ordered_ids.contains(&task_id) {
                    let result = archive_run_task(self.lifecycle_run_repo, run_id, task_id)
                        .await
                        .map_err(TaskPlanWorkspaceError::execution)?;
                    changes.push(TaskChange::simple(&result.task, TaskChangeKind::Dropped));
                }
            }
        }
        Ok(())
    }

    async fn ensure_run_scope(
        &self,
        scope: &TaskPlanScope,
        run_id: Uuid,
    ) -> Result<LifecycleRun, TaskPlanWorkspaceError> {
        let run = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(TaskPlanWorkspaceError::execution)?
            .ok_or_else(|| {
                TaskPlanWorkspaceError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在"))
            })?;
        if run.project_id != scope.project_id {
            return Err(TaskPlanWorkspaceError::ExecutionFailed(format!(
                "LifecycleRun {run_id} 不属于当前 project {}",
                scope.project_id
            )));
        }
        Ok(run)
    }

    async fn resolve_task_selector(
        &self,
        run_id: Uuid,
        selector: &str,
    ) -> Result<Uuid, TaskPlanWorkspaceError> {
        if let Ok(task_id) = Uuid::parse_str(selector) {
            return Ok(task_id);
        }

        let title = normalize_title(selector)?;
        let run = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(TaskPlanWorkspaceError::execution)?
            .ok_or_else(|| {
                TaskPlanWorkspaceError::ExecutionFailed(format!("LifecycleRun {run_id} 不存在"))
            })?;
        let matches = run
            .tasks
            .iter()
            .filter(|task| task.archived_at.is_none() && task.title == title)
            .map(|task| task.id)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [task_id] => Ok(*task_id),
            [] => Err(TaskPlanWorkspaceError::InvalidArguments(format!(
                "未找到标题为 `{title}` 的未归档 Task"
            ))),
            ids => Err(TaskPlanWorkspaceError::InvalidArguments(format!(
                "标题 `{title}` 匹配到 {} 个未归档 Task，请改用 task_id: {}",
                ids.len(),
                ids.iter()
                    .map(Uuid::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    async fn read_task_status(
        &self,
        run_id: Uuid,
        task_id: Uuid,
    ) -> Result<Option<TaskPlanStatus>, TaskPlanWorkspaceError> {
        let run = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(TaskPlanWorkspaceError::execution)?;
        Ok(run.and_then(|run| run.task_by_id(task_id).map(|task| task.status)))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPlanReadMode {
    Overview,
    List,
    Detail,
    Context,
    Projection,
}

impl Default for TaskPlanReadMode {
    fn default() -> Self {
        Self::Overview
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPlanReadFormat {
    Compact,
    Full,
}

impl Default for TaskPlanReadFormat {
    fn default() -> Self {
        Self::Compact
    }
}

#[derive(Debug, Clone)]
pub struct TaskPlanReadQuery {
    pub mode: TaskPlanReadMode,
    pub format: TaskPlanReadFormat,
    pub run_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub story_id: Option<Uuid>,
    pub include_archived: bool,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub statuses: Vec<TaskPlanStatus>,
}

#[derive(Debug, Clone)]
pub struct TaskPlanChangeset {
    pub run_id: Option<Uuid>,
    pub mode: TaskPlanChangesetMode,
    pub return_mode: TaskPlanReadMode,
}

#[derive(Debug, Clone)]
pub enum TaskPlanChangesetMode {
    Patch {
        operations: Vec<TaskPlanOperation>,
    },
    Snapshot {
        snapshot: Vec<TaskPlanSnapshotItem>,
        drop_missing: bool,
    },
}

#[derive(Debug, Clone)]
pub enum TaskPlanOperation {
    CreateTask(TaskPlanCreateCommand),
    PatchTask(TaskPlanPatchCommand),
    SetStatus {
        task_id: String,
        status: TaskPlanStatus,
    },
    ReorderTasks {
        task_ids: Vec<Uuid>,
    },
    DropTask {
        task_id: String,
    },
    ReplaceContextRefs {
        task_id: String,
        context_refs: Vec<ContextSourceRef>,
    },
}

#[derive(Debug, Clone)]
pub struct TaskPlanCreateCommand {
    pub title: String,
    pub body: Option<String>,
    pub status: Option<TaskPlanStatus>,
    pub priority: Option<TaskPriority>,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub context_refs: Vec<ContextSourceRef>,
    pub story_ref: Option<SubjectRef>,
}

#[derive(Debug, Clone)]
pub struct TaskPlanPatchCommand {
    pub task_id: String,
    pub title: Option<String>,
    pub body: Option<Option<String>>,
    pub priority: Option<Option<TaskPriority>>,
    pub owner_agent_id: Option<Option<Uuid>>,
    pub assigned_agent_id: Option<Option<Uuid>>,
    pub source_task_id: Option<Option<Uuid>>,
    pub context_refs: Option<Vec<ContextSourceRef>>,
    pub story_ref: Option<Option<SubjectRef>>,
}

#[derive(Debug, Clone)]
pub struct TaskPlanSnapshotItem {
    pub id: Option<Uuid>,
    pub title: String,
    pub body: Option<String>,
    pub status: Option<TaskPlanStatus>,
    pub priority: Option<TaskPriority>,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub context_refs: Vec<ContextSourceRef>,
    pub story_ref: Option<SubjectRef>,
}

#[derive(Debug, Clone)]
pub struct TaskPlanApplyResult {
    pub view: serde_json::Value,
    pub changes: Vec<TaskChange>,
}

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

#[derive(Debug, thiserror::Error)]
pub enum TaskPlanWorkspaceError {
    #[error("{0}")]
    InvalidArguments(String),
    #[error("{0}")]
    ExecutionFailed(String),
}

impl TaskPlanWorkspaceError {
    fn execution(error: impl std::fmt::Display) -> Self {
        Self::ExecutionFailed(error.to_string())
    }
}

fn render_tasks(
    tasks: &[LifecycleTaskPlanItem],
    format: TaskPlanReadFormat,
) -> Vec<serde_json::Value> {
    tasks.iter().map(|task| task_json(task, format)).collect()
}

fn task_json(task: &LifecycleTaskPlanItem, format: TaskPlanReadFormat) -> serde_json::Value {
    match format {
        TaskPlanReadFormat::Full => serde_json::to_value(task).unwrap_or(serde_json::Value::Null),
        TaskPlanReadFormat::Compact => {
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
    scope: &TaskPlanScope,
    run_id: Uuid,
    tasks: &[LifecycleTaskPlanItem],
    format: TaskPlanReadFormat,
) -> serde_json::Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for task in tasks {
        *counts
            .entry(status_key(task.status).to_string())
            .or_default() += 1;
    }
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

fn context_view(
    scope: &TaskPlanScope,
    run_id: Uuid,
    tasks: &[LifecycleTaskPlanItem],
) -> serde_json::Value {
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

fn scope_json(scope: &TaskPlanScope, run_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "project_id": scope.project_id,
        "run_id": run_id,
        "agent_id": scope.agent_id,
    })
}

fn draft_from_create(
    input: TaskPlanCreateCommand,
    scope: &TaskPlanScope,
) -> Result<LifecycleTaskPlanItemDraft, TaskPlanWorkspaceError> {
    let title = normalize_title(&input.title)?;
    Ok(LifecycleTaskPlanItemDraft {
        id: None,
        title,
        body: input.body,
        status: input.status.unwrap_or_default(),
        priority: input.priority,
        created_by_agent_id: scope.agent_id,
        owner_agent_id: input.owner_agent_id.or(scope.agent_id),
        assigned_agent_id: input.assigned_agent_id,
        source_task_id: input.source_task_id,
        context_refs: input.context_refs,
        story_ref: input.story_ref,
    })
}

fn draft_from_snapshot(
    item: TaskPlanSnapshotItem,
    scope: &TaskPlanScope,
) -> Result<LifecycleTaskPlanItemDraft, TaskPlanWorkspaceError> {
    let title = normalize_title(&item.title)?;
    Ok(LifecycleTaskPlanItemDraft {
        id: item.id,
        title,
        body: item.body,
        status: item.status.unwrap_or_default(),
        priority: item.priority,
        created_by_agent_id: scope.agent_id,
        owner_agent_id: item.owner_agent_id.or(scope.agent_id),
        assigned_agent_id: item.assigned_agent_id,
        source_task_id: item.source_task_id,
        context_refs: item.context_refs,
        story_ref: item.story_ref,
    })
}

fn patch_from_snapshot(
    item: TaskPlanSnapshotItem,
) -> Result<LifecycleTaskPlanItemPatch, TaskPlanWorkspaceError> {
    Ok(LifecycleTaskPlanItemPatch {
        title: Some(normalize_title(&item.title)?),
        body: Some(item.body),
        priority: Some(item.priority),
        owner_agent_id: Some(item.owner_agent_id),
        assigned_agent_id: Some(item.assigned_agent_id),
        source_task_id: Some(item.source_task_id),
        context_refs: Some(item.context_refs),
        story_ref: Some(item.story_ref),
    })
}

fn patch_from_input(
    input: TaskPlanPatchCommand,
) -> Result<LifecycleTaskPlanItemPatch, TaskPlanWorkspaceError> {
    Ok(LifecycleTaskPlanItemPatch {
        title: input
            .title
            .map(|title| normalize_title(&title))
            .transpose()?,
        body: input.body,
        priority: input.priority,
        owner_agent_id: input.owner_agent_id,
        assigned_agent_id: input.assigned_agent_id,
        source_task_id: input.source_task_id,
        context_refs: input.context_refs,
        story_ref: input.story_ref,
    })
}

fn normalize_title(title: &str) -> Result<String, TaskPlanWorkspaceError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(TaskPlanWorkspaceError::InvalidArguments(
            "Task 标题不能为空".to_string(),
        ));
    }
    Ok(title.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{LifecycleRun, LifecycleSubjectAssociation};
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct InMemoryLifecycleRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().await.push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().await;
            if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
                Ok(())
            } else {
                Err(DomainError::NotFound {
                    entity: "LifecycleRun",
                    id: run.id.to_string(),
                })
            }
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().await.retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAssociationRepo {
        associations: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait]
    impl LifecycleSubjectAssociationRepository for InMemoryAssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.associations.lock().await.push(assoc.clone());
            Ok(())
        }

        async fn list_by_subject(
            &self,
            subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .await
                .iter()
                .filter(|assoc| {
                    assoc.subject_kind == subject.kind && assoc.subject_id == subject.id
                })
                .cloned()
                .collect())
        }

        async fn list_by_anchor(
            &self,
            run_id: Uuid,
            agent_id: Option<Uuid>,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .await
                .iter()
                .filter(|assoc| assoc.anchor_run_id == run_id && assoc.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.associations
                .lock()
                .await
                .retain(|assoc| assoc.id != id);
            Ok(())
        }
    }

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
    fn compact_projection_truncates_body_and_counts_refs() {
        let long_body = "字".repeat(COMPACT_BODY_MAX_CHARS + 50);
        let task = sample_task(Some(long_body));
        let compact = task_json(&task, TaskPlanReadFormat::Compact);
        let preview = compact["body_preview"].as_str().unwrap();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), COMPACT_BODY_MAX_CHARS + 1);
        assert_eq!(compact["context_refs_count"], 0);
        assert!(compact.get("created_at").is_none());
    }

    #[test]
    fn full_projection_keeps_all_fields() {
        let task = sample_task(Some("短内容".to_string()));
        let full = task_json(&task, TaskPlanReadFormat::Full);
        assert!(full.get("created_at").is_some());
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
        let value =
            serde_json::to_value(TaskChange::simple(&task, TaskChangeKind::Created)).unwrap();
        assert_eq!(value["change_kind"], "created");
        assert!(value.get("status_from").is_none());
        assert!(value.get("status_to").is_none());
    }

    #[tokio::test]
    async fn workspace_apply_and_read_cover_task_plan_without_tool_json() {
        let lifecycle_repo = InMemoryLifecycleRunRepo::default();
        let association_repo = InMemoryAssociationRepo::default();
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let run_id = run.id;
        lifecycle_repo.create(&run).await.expect("seed run");
        let workspace = TaskPlanWorkspace::new(&lifecycle_repo, &association_repo);
        let scope = TaskPlanScope {
            project_id,
            run_id,
            agent_id: Some(agent_id),
        };

        let applied = workspace
            .apply(
                &scope,
                TaskPlanChangeset {
                    run_id: None,
                    mode: TaskPlanChangesetMode::Patch {
                        operations: vec![TaskPlanOperation::CreateTask(TaskPlanCreateCommand {
                            title: "  typed task  ".to_string(),
                            body: Some("body".to_string()),
                            status: Some(TaskPlanStatus::Active),
                            priority: Some(TaskPriority::P1),
                            owner_agent_id: None,
                            assigned_agent_id: None,
                            source_task_id: None,
                            context_refs: Vec::new(),
                            story_ref: None,
                        })],
                    },
                    return_mode: TaskPlanReadMode::List,
                },
            )
            .await
            .expect("apply");

        assert_eq!(applied.changes.len(), 1);
        assert_eq!(applied.view["mode"], "list");
        assert_eq!(applied.view["tasks"][0]["title"], "typed task");

        let view = workspace
            .read(
                &scope,
                TaskPlanReadQuery {
                    mode: TaskPlanReadMode::Overview,
                    format: TaskPlanReadFormat::Compact,
                    run_id: None,
                    task_id: None,
                    story_id: None,
                    include_archived: false,
                    owner_agent_id: None,
                    assigned_agent_id: None,
                    statuses: vec![TaskPlanStatus::Active],
                },
            )
            .await
            .expect("read");

        assert_eq!(view["total"], 1);
        assert_eq!(view["counts"]["active"], 1);
    }
}
