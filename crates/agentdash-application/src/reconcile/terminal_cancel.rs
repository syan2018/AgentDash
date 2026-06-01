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

use agentdash_domain::story::{StoryRepository, StoryStatus};
use agentdash_domain::task::TaskStatus;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleSubjectAssociationRepository,
    SubjectRef,
};

use crate::session::SessionRuntimeService;

/// 业务终态取消协调器 — 在 Task/Story 状态变更路径上被调用。
///
/// 当业务状态进入终态时，通过 lifecycle association → agent → frame → runtime session
/// 路径查找并 cancel 关联 session。
pub struct TerminalCancelCoordinator {
    session_runtime: SessionRuntimeService,
    story_repo: Arc<dyn StoryRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
}

impl TerminalCancelCoordinator {
    pub fn new(
        session_runtime: SessionRuntimeService,
        story_repo: Arc<dyn StoryRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
    ) -> Self {
        Self {
            session_runtime,
            story_repo,
            association_repo,
            agent_repo,
            frame_repo,
        }
    }

    /// Task 状态变更后调用。如果新状态是终态且 task 有关联的 running session，取消之。
    pub async fn on_task_status_changed(&self, task_id: Uuid, new_status: &TaskStatus) {
        if !is_task_terminal(new_status) {
            return;
        }

        let session_id = match self.resolve_task_runtime_session(task_id).await {
            Some(sid) => sid,
            None => return,
        };

        if let Err(err) = self.session_runtime.cancel(&session_id).await {
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
            let session_id = match self.resolve_task_runtime_session(task.id).await {
                Some(sid) => sid,
                None => continue,
            };
            if let Err(err) = self.session_runtime.cancel(&session_id).await {
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

    /// 通过 LifecycleSubjectAssociation → agent → frame 路径解析 task 的 runtime session。
    async fn resolve_task_runtime_session(&self, task_id: Uuid) -> Option<String> {
        let subject = SubjectRef::new("task", task_id);
        let associations = self.association_repo.list_by_subject(&subject).await.ok()?;
        let assoc = associations.first()?;

        let agents = self
            .agent_repo
            .list_by_run(assoc.anchor_run_id)
            .await
            .ok()?;
        let active_agent = agents.into_iter().find(|a| a.status == "active")?;

        let frame = self
            .frame_repo
            .get_current(active_agent.id)
            .await
            .ok()
            .flatten()?;

        let refs_json = frame.runtime_session_refs_json.as_ref()?;
        let arr = refs_json.as_array()?;
        arr.first().and_then(|v| v.as_str()).map(|s| s.to_string())
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
