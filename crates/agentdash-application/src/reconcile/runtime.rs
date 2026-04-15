//! 事件驱动的运行时对账
//!
//! 当 Story/Task 状态被外部变更为终态时，自动取消关联的 running session。
//! 这是"安全网"行为：确保业务状态与 session 生命周期一致。

use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryStatus;
use agentdash_domain::task::{TaskRepository, TaskStatus};

use crate::session::SessionHub;

/// 运行时对账服务 — 在状态变更路径上被调用
pub struct RuntimeReconciler {
    session_hub: SessionHub,
    task_repo: Arc<dyn TaskRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
}

impl RuntimeReconciler {
    pub fn new(
        session_hub: SessionHub,
        task_repo: Arc<dyn TaskRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
    ) -> Self {
        Self {
            session_hub,
            task_repo,
            session_binding_repo,
        }
    }

    /// Task 状态变更后调用。如果新状态是终态且 task 有关联的 running session，取消之。
    pub async fn on_task_status_changed(&self, task_id: Uuid, new_status: &TaskStatus) {
        if !is_task_terminal(new_status) {
            return;
        }

        let session_id = match crate::task::find_task_execution_session_id(
            self.session_binding_repo.as_ref(),
            task_id,
        )
        .await
        {
            Ok(Some(sid)) => sid,
            _ => return,
        };

        if let Err(err) = self.session_hub.cancel(&session_id).await {
            tracing::warn!(
                task_id = %task_id,
                session_id = %session_id,
                error = %err,
                "运行时对账：Task 进入终态后取消关联 session 失败"
            );
        } else {
            tracing::info!(
                task_id = %task_id,
                session_id = %session_id,
                new_status = ?new_status,
                "运行时对账：Task 进入终态，已取消关联 session"
            );
        }
    }

    /// Story 状态变更后调用。如果新状态是终态，取消其下所有 running task 的 session。
    pub async fn on_story_status_changed(&self, story_id: Uuid, new_status: &StoryStatus) {
        if !is_story_terminal(new_status) {
            return;
        }

        let tasks = match self.task_repo.list_by_story(story_id).await {
            Ok(tasks) => tasks,
            Err(err) => {
                tracing::warn!(
                    story_id = %story_id,
                    error = %err,
                    "运行时对账：查询 Story 下属 Task 失败"
                );
                return;
            }
        };

        let mut cancelled = 0usize;
        for task in tasks {
            if task.status != TaskStatus::Running {
                continue;
            }
            let session_id = match crate::task::find_task_execution_session_id(
                self.session_binding_repo.as_ref(),
                task.id,
            )
            .await
            {
                Ok(Some(sid)) => sid,
                _ => continue,
            };
            if let Err(err) = self.session_hub.cancel(&session_id).await {
                tracing::warn!(
                    task_id = %task.id,
                    session_id = %session_id,
                    error = %err,
                    "运行时对账：Story 终态级联取消 session 失败"
                );
            } else {
                cancelled += 1;
            }
        }

        if cancelled > 0 {
            tracing::info!(
                story_id = %story_id,
                new_status = ?new_status,
                cancelled_sessions = cancelled,
                "运行时对账：Story 进入终态，已级联取消关联 session"
            );
        }
    }
}

fn is_task_terminal(status: &TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed | TaskStatus::Failed)
}

fn is_story_terminal(status: &StoryStatus) -> bool {
    matches!(
        status,
        StoryStatus::Completed | StoryStatus::Failed | StoryStatus::Cancelled
    )
}
