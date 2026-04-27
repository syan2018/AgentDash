use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::{ChangeKind, StateChangeRepository, StoryRepository};
use agentdash_domain::task::{Task, TaskExecutionMode, TaskStatus};

use super::restart_tracker::{RestartDecision, RestartTracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSessionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
}

#[async_trait]
pub trait TaskSessionStateReader: Send + Sync {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<TaskSessionState, String>;
}

#[derive(Debug, thiserror::Error)]
pub enum TaskStateReconcileError {
    #[error(transparent)]
    Domain(#[from] agentdash_domain::DomainError),
    #[error("读取 session 运行状态失败: {0}")]
    SessionState(String),
}

struct StatusReconcilePlan {
    next_status: TaskStatus,
    reason: &'static str,
    context: Value,
}

fn plan_for_running_task(
    task: &Task,
    session_id: Option<&str>,
    execution_state: TaskSessionState,
    restart_tracker: Option<&RestartTracker>,
) -> Option<StatusReconcilePlan> {
    match execution_state {
        TaskSessionState::Running { .. } => None,
        TaskSessionState::Completed { turn_id } => Some(StatusReconcilePlan {
            next_status: TaskStatus::AwaitingVerification,
            reason: "boot_reconcile_turn_completed",
            context: json!({
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
            }),
        }),
        TaskSessionState::Failed { turn_id, message } => {
            let mut context = json!({
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
                "execution_mode": task.execution_mode,
            });
            if let Some(msg) = &message {
                context["error"] = json!(msg);
            }

            if task.execution_mode == TaskExecutionMode::AutoRetry
                && let Some(tracker) = restart_tracker
                && let RestartDecision::Allowed { attempt, .. } = tracker.report_failure(task.id)
            {
                context["retry_attempt"] = json!(attempt);
                context["auto_retry"] = json!(true);
                return Some(StatusReconcilePlan {
                    next_status: TaskStatus::AwaitingVerification,
                    reason: "boot_reconcile_turn_failed_pending_retry",
                    context,
                });
            }

            Some(StatusReconcilePlan {
                next_status: TaskStatus::Failed,
                reason: "boot_reconcile_turn_failed",
                context,
            })
        }
        TaskSessionState::Interrupted { turn_id, message } => {
            let mut context = json!({
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
                "execution_mode": task.execution_mode,
            });
            if let Some(msg) = &message {
                context["message"] = json!(msg);
            }

            if task.execution_mode == TaskExecutionMode::AutoRetry
                && let Some(tracker) = restart_tracker
                && let RestartDecision::Allowed { attempt, .. } = tracker.report_failure(task.id)
            {
                context["retry_attempt"] = json!(attempt);
                context["auto_retry"] = json!(true);
                return Some(StatusReconcilePlan {
                    next_status: TaskStatus::AwaitingVerification,
                    reason: "boot_reconcile_turn_interrupted_pending_retry",
                    context,
                });
            }

            Some(StatusReconcilePlan {
                next_status: TaskStatus::Failed,
                reason: "boot_reconcile_turn_interrupted",
                context,
            })
        }
        TaskSessionState::Idle => Some(StatusReconcilePlan {
            next_status: TaskStatus::Failed,
            reason: "boot_reconcile_session_idle",
            context: json!({
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
            }),
        }),
    }
}

async fn apply_reconcile_update(
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    task: &mut Task,
    session_id: Option<&str>,
    plan: StatusReconcilePlan,
) -> Result<bool, TaskStateReconcileError> {
    if task.status() == &plan.next_status {
        return Ok(false);
    }

    let previous_status = task.status().clone();
    task.set_status(plan.next_status.clone());

    // M2：Task 写入经 Story aggregate `force_set_task_status`（命令型恢复路径）。
    let mut story = story_repo
        .get_by_id(task.story_id)
        .await?
        .ok_or_else(|| agentdash_domain::DomainError::NotFound {
            entity: "story",
            id: task.story_id.to_string(),
        })?;
    let applied = story.force_set_task_status(task.id, plan.next_status.clone());
    if applied.is_none() {
        return Err(TaskStateReconcileError::Domain(
            agentdash_domain::DomainError::NotFound {
                entity: "task",
                id: task.id.to_string(),
            },
        ));
    }
    story_repo.update(&story).await?;

    state_change_repo
        .append_change(
            task.project_id,
            task.id,
            ChangeKind::TaskStatusChanged,
            json!({
                "reason": plan.reason,
                "project_id": task.project_id,
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status,
                "to": plan.next_status,
                "context": plan.context,
            }),
            None,
        )
        .await?;

    tracing::info!(
        task_id = %task.id,
        story_id = %task.story_id,
        reason = plan.reason,
        from = "running",
        to = ?task.status(),
        "启动阶段已回收 Task 运行状态"
    );

    Ok(true)
}

pub async fn reconcile_running_tasks_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    session_binding_repo: &Arc<dyn SessionBindingRepository>,
    session_state_reader: &dyn TaskSessionStateReader,
    restart_tracker: Option<&RestartTracker>,
) -> Result<(), TaskStateReconcileError> {
    let projects = project_repo.list_all().await?;
    let mut touched = 0usize;
    let mut pending_retry = 0usize;

    // M1-b：Task 真相源为 `stories.tasks` JSONB；遍历 project 下 stories 再扁平化 tasks。
    for project in projects {
        let stories = story_repo.list_by_project(project.id).await?;
        let tasks: Vec<Task> = stories
            .into_iter()
            .flat_map(|story| story.tasks.into_iter())
            .collect();

        for mut task in tasks {
            if task.status() != &TaskStatus::Running {
                continue;
            }

            let session_id =
                super::find_task_execution_session_id(session_binding_repo.as_ref(), task.id)
                    .await
                    .unwrap_or(None);

            let execution_state = match session_id.as_deref() {
                None => TaskSessionState::Interrupted {
                    turn_id: None,
                    message: None,
                },
                Some(sid) => session_state_reader
                    .inspect_session_execution_state(sid)
                    .await
                    .map_err(TaskStateReconcileError::SessionState)?,
            };

            let Some(plan) = plan_for_running_task(
                &task,
                session_id.as_deref(),
                execution_state,
                restart_tracker,
            ) else {
                continue;
            };

            let is_retry = plan.next_status == TaskStatus::AwaitingVerification
                && plan.reason.contains("pending_retry");

            if apply_reconcile_update(
                state_change_repo,
                story_repo,
                &mut task,
                session_id.as_deref(),
                plan,
            )
            .await?
            {
                touched += 1;
                if is_retry {
                    pending_retry += 1;
                }
            }
        }
    }

    tracing::info!(
        reconciled_count = touched,
        pending_retry = pending_retry,
        "启动阶段 Task 状态回收完成"
    );
    Ok(())
}
