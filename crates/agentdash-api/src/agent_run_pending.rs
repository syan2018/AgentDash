use async_trait::async_trait;

use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::{
    PendingMessage, PendingQueueService, QueuePauseReason, SessionLaunchService,
    SessionTerminalCallback,
};
use agentdash_application::workflow::{
    AgentRunMessageCommand, AgentRunMessageDispatch, AgentRunMessageLaunchDeliveryPort,
    AgentRunMessageService, WorkflowApplicationError,
};

#[derive(Clone)]
pub(crate) struct AgentRunPendingDispatcher {
    repos: RepositorySet,
    pending_queue: PendingQueueService,
    session_launch: SessionLaunchService,
}

impl AgentRunPendingDispatcher {
    pub(crate) fn new(
        repos: RepositorySet,
        pending_queue: PendingQueueService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            repos,
            pending_queue,
            session_launch,
        }
    }

    pub(crate) async fn dispatch_next_pending(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentRunMessageDispatch>, WorkflowApplicationError> {
        let Some(message) = self.pending_queue.dequeue_front(runtime_session_id).await else {
            return Ok(None);
        };
        let message_id = message.id.clone();
        let delivery = AgentRunMessageLaunchDeliveryPort::new(self.session_launch.clone());
        let service = AgentRunMessageService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.execution_anchor_repo.as_ref(),
            self.repos.agent_run_delivery_command_receipt_repo.as_ref(),
            delivery,
        );
        let command = pending_message_command(runtime_session_id, message.clone());
        match service.dispatch_user_message(command).await {
            Ok(dispatch) => Ok(Some(dispatch)),
            Err(error) => {
                self.pending_queue
                    .requeue_front(runtime_session_id, message)
                    .await;
                tracing::warn!(
                    runtime_session_id = %runtime_session_id,
                    pending_message_id = %message_id,
                    error = %error,
                    "AgentRun pending message 派发失败，已放回队首"
                );
                Err(error)
            }
        }
    }

    pub(crate) async fn resume_queue(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentRunMessageDispatch>, WorkflowApplicationError> {
        let pause_reason = self.pending_queue.is_paused(runtime_session_id).await;
        self.pending_queue.resume(runtime_session_id).await;
        match self.dispatch_next_pending(runtime_session_id).await {
            Ok(dispatch) => Ok(dispatch),
            Err(error) => {
                if let Some(reason) = pause_reason {
                    self.pending_queue.pause(runtime_session_id, reason).await;
                }
                Err(error)
            }
        }
    }

    pub(crate) async fn pause_queue(&self, runtime_session_id: &str, reason: QueuePauseReason) {
        self.pending_queue.pause(runtime_session_id, reason).await;
    }
}

#[derive(Clone)]
pub(crate) struct AgentRunPendingTerminalCallback {
    dispatcher: AgentRunPendingDispatcher,
}

impl AgentRunPendingTerminalCallback {
    pub(crate) fn new(dispatcher: AgentRunPendingDispatcher) -> Self {
        Self { dispatcher }
    }
}

#[async_trait]
impl SessionTerminalCallback for AgentRunPendingTerminalCallback {
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str) {
        match terminal_state {
            "completed" => {
                if let Err(error) = self.dispatcher.dispatch_next_pending(session_id).await {
                    tracing::warn!(
                        runtime_session_id = %session_id,
                        error = %error,
                        "AgentRun pending queue completed-drain 失败"
                    );
                }
            }
            "failed" => {
                self.dispatcher
                    .pause_queue(session_id, QueuePauseReason::TurnFailed)
                    .await;
            }
            "interrupted" => {
                self.dispatcher
                    .pause_queue(session_id, QueuePauseReason::TurnInterrupted)
                    .await;
            }
            _ => {}
        }
    }
}

fn pending_message_command(session_id: &str, message: PendingMessage) -> AgentRunMessageCommand {
    AgentRunMessageCommand {
        delivery_runtime_session_id: session_id.to_string(),
        input: message.input,
        client_command_id: format!("pending:{}:{}", message.id, uuid::Uuid::new_v4()),
        executor_config: message.executor_config,
        identity: None,
    }
}
