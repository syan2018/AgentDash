//! 业务终态 → session cancel 指令通道。
//!
//! **方向**：业务决策（Task/Story 被外部写入 Completed/Failed/Cancelled）
//! → 取消关联的 running session。属于 command 方向。
//!
//! 对应启动期反向（session/lifecycle 真相源 → Task view 只读投影）的
//! projection 通道见 [`crate::task::view_projector`]。
//!
//! 这是"安全网"行为：确保业务状态与 session 生命周期一致。

use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::{StoryRepository, StoryStatus};
use agentdash_domain::task::TaskStatus;

use crate::session::SessionHub;

/// 业务终态取消协调器 — 在 Task/Story 状态变更路径上被调用。
///
/// 当业务状态进入终态时，触发关联 session 的 cancel 指令。
/// 不做反向 projection（projection 方向见 [`crate::task::view_projector`]）。
pub struct TerminalCancelCoordinator {
    session_hub: SessionHub,
    story_repo: Arc<dyn StoryRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
}

impl TerminalCancelCoordinator {
    pub fn new(
        session_hub: SessionHub,
        story_repo: Arc<dyn StoryRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
    ) -> Self {
        Self {
            session_hub,
            story_repo,
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
                "终态取消协调器：Task 进入终态后取消关联 session 失败"
            );
        } else {
            tracing::info!(
                task_id = %task_id,
                session_id = %session_id,
                new_status = ?new_status,
                "终态取消协调器：Task 进入终态，已取消关联 session"
            );
        }
    }

    /// Story 状态变更后调用。如果新状态是终态，取消其下所有 running task 的 session。
    pub async fn on_story_status_changed(&self, story_id: Uuid, new_status: &StoryStatus) {
        if !is_story_terminal(new_status) {
            return;
        }

        let tasks = match self.story_repo.get_by_id(story_id).await {
            Ok(Some(story)) => story.tasks,
            Ok(None) => {
                tracing::warn!(
                    story_id = %story_id,
                    "终态取消协调器：Story 不存在，跳过级联取消"
                );
                return;
            }
            Err(err) => {
                tracing::warn!(
                    story_id = %story_id,
                    error = %err,
                    "终态取消协调器：查询 Story 下属 Task 失败"
                );
                return;
            }
        };

        let mut cancelled = 0usize;
        for task in tasks {
            if task.status() != &TaskStatus::Running {
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
                    "终态取消协调器：Story 终态级联取消 session 失败"
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
                "终态取消协调器：Story 进入终态，已级联取消关联 session"
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
