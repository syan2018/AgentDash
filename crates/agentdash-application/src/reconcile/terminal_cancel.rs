//! 业务终态 → session cancel 指令通道。
//!
//! **方向**：业务决策（Task/Story 被外部写入 Completed/Failed/Cancelled）
//! → 取消关联的 running session。属于 command 方向。
//!
//! 对应启动期反向（session/lifecycle 真相源 → Task view 只读投影）的
//! projection 通道见 [`crate::task::view_projector`]。
//!
//! 这是"安全网"行为：确保业务状态与 session 生命周期一致。

use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::story::{StoryRepository, StoryStatus};
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository, SubjectRef,
};

use crate::lifecycle::SubjectExecutionControlService;
use crate::session::SessionRuntimeService;

/// 业务终态取消协调器 — 在 Task/Story 状态变更路径上被调用。
///
/// 当业务状态进入终态时，通过 lifecycle association → agent → frame → runtime session
/// 路径查找并 cancel 关联 session。
pub struct TerminalCancelCoordinator {
    session_runtime: SessionRuntimeService,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
}

impl TerminalCancelCoordinator {
    pub fn new(
        session_runtime: SessionRuntimeService,
        _story_repo: Arc<dyn StoryRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    ) -> Self {
        Self {
            session_runtime,
            lifecycle_run_repo,
            association_repo,
            agent_repo,
            frame_repo,
            execution_anchor_repo,
        }
    }

    /// Story 状态变更后调用。如果新状态是终态，取消 Story subject 关联的 session。
    pub async fn on_story_status_changed(&self, story_id: Uuid, new_status: &StoryStatus) {
        if !is_story_terminal(new_status) {
            return;
        }

        let command = match self
            .resolve_subject_runtime_cancel_delivery("story", story_id)
            .await
        {
            Some(command) => command,
            None => return,
        };
        if let Err(err) = self
            .session_runtime
            .cancel(&command.runtime_session_id)
            .await
        {
            diag!(Warn, Subsystem::Reconcile,

                story_id = %story_id,
                session_id = %command.runtime_session_id,
                frame_ref = %command.runtime_refs.frame_ref,
                orchestration_ref = ?command.runtime_refs.orchestration_ref(),
                node_path = ?command.runtime_refs.node_path(),
                error = %err,
                "终态取消协调器：Story 进入终态后取消关联 session 失败"
            );
        } else {
            diag!(Info, Subsystem::Reconcile,

                story_id = %story_id,
                new_status = ?new_status,
                session_id = %command.runtime_session_id,
                "终态取消协调器：Story 进入终态，已取消关联 session"
            );
        }
    }

    async fn resolve_subject_runtime_cancel_delivery(
        &self,
        subject_kind: &str,
        subject_id: Uuid,
    ) -> Option<crate::lifecycle::RuntimeCancelDeliveryCommand> {
        let subject = SubjectRef::new(subject_kind, subject_id);
        let service = SubjectExecutionControlService::new(
            self.lifecycle_run_repo.as_ref(),
            self.association_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
            self.execution_anchor_repo.as_ref(),
        );
        match service
            .prepare_runtime_cancel_delivery(&subject, Some("terminal_status_cancel".to_string()))
            .await
        {
            Ok(command) => command,
            Err(err) => {
                diag!(Debug, Subsystem::Reconcile,

                    subject_kind,
                    subject_id = %subject_id,
                    error = %err,
                    "终态取消协调器：未能解析 subject runtime cancel delivery"
                );
                None
            }
        }
    }
}

fn is_story_terminal(status: &StoryStatus) -> bool {
    matches!(
        status,
        StoryStatus::Completed | StoryStatus::Failed | StoryStatus::Cancelled
    )
}
