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
    ActivityExecutionClaimRepository, AgentAssignmentRepository, AgentFrameRepository,
    LifecycleAgentRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, RuntimeSessionSelectionPolicy, SubjectRef,
    WorkflowGraphInstanceRepository, WorkflowGraphRepository,
};

use crate::session::SessionRuntimeService;
use crate::workflow::SubjectExecutionControlService;

/// 业务终态取消协调器 — 在 Task/Story 状态变更路径上被调用。
///
/// 当业务状态进入终态时，通过 lifecycle association → agent → frame → runtime session
/// 路径查找并 cancel 关联 session。
pub struct TerminalCancelCoordinator {
    session_runtime: SessionRuntimeService,
    story_repo: Arc<dyn StoryRepository>,
    workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository>,
    activity_execution_claim_repo: Arc<dyn ActivityExecutionClaimRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    assignment_repo: Arc<dyn AgentAssignmentRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
}

impl TerminalCancelCoordinator {
    pub fn new(
        session_runtime: SessionRuntimeService,
        story_repo: Arc<dyn StoryRepository>,
        workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository>,
        activity_execution_claim_repo: Arc<dyn ActivityExecutionClaimRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        assignment_repo: Arc<dyn AgentAssignmentRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    ) -> Self {
        Self {
            session_runtime,
            story_repo,
            workflow_graph_repo,
            lifecycle_run_repo,
            workflow_graph_instance_repo,
            activity_execution_claim_repo,
            association_repo,
            agent_repo,
            frame_repo,
            assignment_repo,
            execution_anchor_repo,
        }
    }

    /// Task 状态变更后调用。如果新状态是终态且 task 有关联的 running session，取消之。
    pub async fn on_task_status_changed(&self, task_id: Uuid, new_status: &TaskStatus) {
        if !is_task_terminal(new_status) {
            return;
        }

        let command = match self.resolve_task_runtime_cancel_delivery(task_id).await {
            Some(command) => command,
            None => return,
        };

        if let Err(err) = self
            .session_runtime
            .cancel(&command.runtime_session_id)
            .await
        {
            tracing::warn!(
                task_id = %task_id,
                session_id = %command.runtime_session_id,
                frame_ref = %command.runtime_refs.frame_ref,
                assignment_ref = ?command.runtime_refs.assignment_ref(),
                error = %err,
                "终态取消协调器：Task 进入终态后取消关联 session 失败"
            );
        } else {
            tracing::info!(
                task_id = %task_id,
                session_id = %command.runtime_session_id,
                frame_ref = %command.runtime_refs.frame_ref,
                assignment_ref = ?command.runtime_refs.assignment_ref(),
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
            let command = match self.resolve_task_runtime_cancel_delivery(task.id).await {
                Some(command) => command,
                None => continue,
            };
            if let Err(err) = self
                .session_runtime
                .cancel(&command.runtime_session_id)
                .await
            {
                tracing::warn!(
                    task_id = %task.id,
                    session_id = %command.runtime_session_id,
                    frame_ref = %command.runtime_refs.frame_ref,
                    assignment_ref = ?command.runtime_refs.assignment_ref(),
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

    /// 通过 SubjectExecution 控制面解析 runtime cancel delivery。
    async fn resolve_task_runtime_cancel_delivery(
        &self,
        task_id: Uuid,
    ) -> Option<crate::workflow::RuntimeCancelDeliveryCommand> {
        let subject = SubjectRef::new("task", task_id);
        let service = SubjectExecutionControlService::new(
            self.workflow_graph_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
            self.workflow_graph_instance_repo.as_ref(),
            self.activity_execution_claim_repo.as_ref(),
            self.association_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
            self.assignment_repo.as_ref(),
            self.execution_anchor_repo.as_ref(),
        );
        match service
            .prepare_runtime_cancel_delivery(
                &subject,
                RuntimeSessionSelectionPolicy::LatestAttached,
                Some("terminal_status_cancel".to_string()),
            )
            .await
        {
            Ok(command) => command,
            Err(err) => {
                tracing::debug!(
                    task_id = %task_id,
                    error = %err,
                    "终态取消协调器：未能解析 task runtime cancel delivery"
                );
                None
            }
        }
    }
}

fn is_task_terminal(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

fn is_story_terminal(status: &StoryStatus) -> bool {
    matches!(
        status,
        StoryStatus::Completed | StoryStatus::Failed | StoryStatus::Cancelled
    )
}
