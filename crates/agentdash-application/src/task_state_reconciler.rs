use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{ChangeKind, Story, StoryRepository};
use agentdash_domain::task::{Task, TaskRepository, TaskStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionState {
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
    },
}

#[async_trait]
pub trait SessionExecutionStateReader: Send + Sync {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<SessionExecutionState, String>;
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
    execution_state: SessionExecutionState,
) -> Option<StatusReconcilePlan> {
    match execution_state {
        SessionExecutionState::Running { .. } => None,
        SessionExecutionState::Completed { turn_id } => Some(StatusReconcilePlan {
            next_status: TaskStatus::AwaitingVerification,
            reason: "boot_reconcile_turn_completed",
            context: json!({
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
            }),
        }),
        SessionExecutionState::Failed { turn_id, message } => {
            let mut context = json!({
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
            });
            if let Some(msg) = message {
                context["error"] = json!(msg);
            }
            Some(StatusReconcilePlan {
                next_status: TaskStatus::Failed,
                reason: "boot_reconcile_turn_failed",
                context,
            })
        }
        SessionExecutionState::Interrupted { turn_id } => Some(StatusReconcilePlan {
            next_status: TaskStatus::Failed,
            reason: "boot_reconcile_turn_interrupted",
            context: json!({
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "turn_id": turn_id,
            }),
        }),
        SessionExecutionState::Idle => Some(StatusReconcilePlan {
            next_status: TaskStatus::Failed,
            reason: "boot_reconcile_session_idle",
            context: json!({
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
            }),
        }),
    }
}

async fn apply_reconcile_update(
    story_repo: &Arc<dyn StoryRepository>,
    task_repo: &Arc<dyn TaskRepository>,
    story: &Story,
    task: &mut Task,
    plan: StatusReconcilePlan,
) -> Result<bool, TaskStateReconcileError> {
    if task.status == plan.next_status {
        return Ok(false);
    }

    let previous_status = task.status.clone();
    task.status = plan.next_status.clone();
    task_repo.update(task).await?;

    story_repo
        .append_change(
            task.id,
            ChangeKind::TaskStatusChanged,
            json!({
                "reason": plan.reason,
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status,
                "to": plan.next_status,
                "context": plan.context,
            }),
            &story.backend_id,
        )
        .await?;

    tracing::info!(
        task_id = %task.id,
        story_id = %task.story_id,
        reason = plan.reason,
        from = "running",
        to = ?task.status,
        "启动阶段已回收 Task 运行状态"
    );

    Ok(true)
}

pub async fn reconcile_running_tasks_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    task_repo: &Arc<dyn TaskRepository>,
    session_state_reader: &dyn SessionExecutionStateReader,
) -> Result<(), TaskStateReconcileError> {
    let projects = project_repo.list_all().await?;
    let mut touched = 0usize;

    for project in projects {
        let stories = story_repo.list_by_project(project.id).await?;

        for story in stories {
            let tasks = task_repo.list_by_story(story.id).await?;

            for mut task in tasks {
                if task.status != TaskStatus::Running {
                    continue;
                }

                let execution_state = match task.session_id.as_deref() {
                    None => SessionExecutionState::Interrupted { turn_id: None },
                    Some(session_id) => session_state_reader
                        .inspect_session_execution_state(session_id)
                        .await
                        .map_err(TaskStateReconcileError::SessionState)?,
                };

                let Some(plan) = plan_for_running_task(&task, execution_state) else {
                    continue;
                };

                if apply_reconcile_update(story_repo, task_repo, &story, &mut task, plan).await? {
                    touched += 1;
                }
            }
        }
    }

    tracing::info!(reconciled_count = touched, "启动阶段 Task 状态回收完成");
    Ok(())
}
