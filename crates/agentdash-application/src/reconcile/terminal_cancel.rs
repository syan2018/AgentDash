//! 业务终态 → session cancel 指令通道。
//!
//! **方向**：业务决策（Task/Story 被外部写入 Completed/Failed/Cancelled）
//! → 取消关联的 running session。属于 command 方向。
//!
//! 对应启动期反向（session/lifecycle 真相源 → Task view 只读投影）的
//! projection 通道见 [`crate::task::view_projector`]。
//!
//! 这是"安全网"行为：确保业务状态与 session 生命周期一致。

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;

use uuid::Uuid;

use agentdash_application_agentrun::agent_run::{
    AgentRunCancelCommand, AgentRunCancelCommandService,
};
use agentdash_domain::story::{StoryRepository, StoryStatus};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, AgentRunDeliveryBindingRepository,
    LifecycleAgentRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, SubjectRef,
};

use crate::lifecycle::SubjectExecutionControlService;
use crate::runtime_session_agent_run_bridge::agent_run_session_cancel_runtime;
use crate::session::SessionRuntimeService;

/// 业务终态取消协调器 — 在 Task/Story 状态变更路径上被调用。
///
/// 当业务状态进入终态时，通过 lifecycle association → agent → frame → runtime session
/// 路径查找并 cancel 关联 session。
pub struct TerminalCancelCoordinator {
    session_runtime: SessionRuntimeService,
    command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
}

impl TerminalCancelCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_runtime: SessionRuntimeService,
        _story_repo: Arc<dyn StoryRepository>,
        command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    ) -> Self {
        Self {
            session_runtime,
            command_receipt_repo,
            lifecycle_run_repo,
            association_repo,
            agent_repo,
            frame_repo,
            execution_anchor_repo,
            delivery_binding_repo,
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
        let cancel_runtime = agent_run_session_cancel_runtime(self.session_runtime.clone());
        let client_command_id = format!(
            "terminal-status-cancel:{}:{}",
            story_id, command.runtime_session_id
        );
        match AgentRunCancelCommandService::new(self.command_receipt_repo.as_ref(), &cancel_runtime)
            .cancel(AgentRunCancelCommand {
                run_id: command.runtime_refs.run_ref,
                agent_id: command.runtime_refs.agent_ref,
                frame_id: Some(command.runtime_refs.frame_ref),
                runtime_session_id: command.runtime_session_id.clone(),
                client_command_id,
                reason: Some("terminal_status_cancel".to_string()),
            })
            .await
        {
            Err(err) => {
                let context =
                    DiagnosticErrorContext::new("reconcile.terminal_cancel", "cancel_agent_run")
                        .with_field("story_id", story_id)
                        .with_field("session_id", &command.runtime_session_id)
                        .with_field("run_id", command.runtime_refs.run_ref)
                        .with_field("agent_id", command.runtime_refs.agent_ref)
                        .with_field("frame_id", command.runtime_refs.frame_ref);
                diag_error!(
                    Warn,
                    Subsystem::Reconcile,
                    context = &context,
                    error = &err,
                    story_id = %story_id,
                    session_id = %command.runtime_session_id,
                    run_id = %command.runtime_refs.run_ref,
                    agent_id = %command.runtime_refs.agent_ref,
                    frame_ref = %command.runtime_refs.frame_ref,
                    orchestration_ref = ?command.runtime_refs.orchestration_ref(),
                    node_path = ?command.runtime_refs.node_path(),
                    "终态取消协调器：Story 进入终态后取消关联 session 失败"
                );
            }
            Ok(receipt) => {
                diag!(Info, Subsystem::Reconcile,

                    story_id = %story_id,
                    new_status = ?new_status,
                    session_id = %command.runtime_session_id,
                    command_status = %receipt.status,
                    command_duplicate = receipt.duplicate,
                    "终态取消协调器：Story 进入终态，已通过 AgentRun command 取消关联 session"
                );
            }
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
            self.delivery_binding_repo.as_ref(),
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
