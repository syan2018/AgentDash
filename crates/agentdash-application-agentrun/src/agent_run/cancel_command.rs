use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandReceiptRepository,
};
use agentdash_spi::ConnectorError;

use crate::agent_run::command_receipt::{
    AgentRunCommandReceiptView, claim_agent_run_command_receipt, digest_command_request,
    mark_command_terminal_failed,
};
use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AgentRunCancelCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub runtime_session_id: String,
    pub client_command_id: String,
    pub reason: Option<String>,
}

#[async_trait]
pub trait AgentRunCancelRuntimePort: Send + Sync {
    async fn cancel_runtime_session(&self, session_id: &str) -> Result<(), ConnectorError>;
}

pub struct AgentRunCancelCommandService<'a> {
    command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    runtime: &'a dyn AgentRunCancelRuntimePort,
}

impl<'a> AgentRunCancelCommandService<'a> {
    pub fn new(
        command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
        runtime: &'a dyn AgentRunCancelRuntimePort,
    ) -> Self {
        Self {
            command_receipt_repo,
            runtime,
        }
    }

    pub async fn cancel(
        &self,
        command: AgentRunCancelCommand,
    ) -> Result<AgentRunCommandReceiptView, WorkflowApplicationError> {
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }
        let request_digest = digest_command_request(&CancelCommandDigest {
            kind: "agent_run_cancel",
            run_id: command.run_id,
            agent_id: command.agent_id,
            frame_id: command.frame_id,
            runtime_session_id: &command.runtime_session_id,
            reason: command.reason.as_deref(),
        })?;
        let claim = claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_mailbox",
            format!("{}:{}", command.run_id, command.agent_id),
            AgentRunCommandKind::Cancel,
            command.client_command_id,
            request_digest,
        )
        .await?;
        if claim.duplicate {
            return Ok(AgentRunCommandReceiptView::from_record(&claim.record, true));
        }

        if let Err(error) = self
            .runtime
            .cancel_runtime_session(&command.runtime_session_id)
            .await
        {
            let workflow_error = WorkflowApplicationError::Internal(error.to_string());
            mark_command_terminal_failed(
                self.command_receipt_repo,
                claim.record.id,
                &workflow_error,
            )
            .await;
            return Err(workflow_error);
        }

        let accepted = self
            .command_receipt_repo
            .mark_accepted(
                claim.record.id,
                AgentRunAcceptedRefs {
                    run_id: command.run_id,
                    agent_id: command.agent_id,
                    frame_id: command.frame_id,
                    frame_revision: None,
                    runtime_session_id: Some(command.runtime_session_id),
                    agent_run_turn_id: None,
                    protocol_turn_id: None,
                },
            )
            .await?;
        let stored = self
            .command_receipt_repo
            .store_result_json(
                claim.record.id,
                serde_json::json!({
                    "cancelled": true,
                    "reason": command.reason,
                }),
            )
            .await?;
        Ok(AgentRunCommandReceiptView::from_record(
            if stored.updated_at >= accepted.updated_at {
                &stored
            } else {
                &accepted
            },
            false,
        ))
    }
}

#[derive(Serialize)]
struct CancelCommandDigest<'a> {
    kind: &'static str,
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Option<Uuid>,
    runtime_session_id: &'a str,
    reason: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::test_support::MemoryAgentRunCommandReceiptRepository;

    #[derive(Default)]
    struct CountingCancelRuntime {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl AgentRunCancelRuntimePort for CountingCancelRuntime {
        async fn cancel_runtime_session(&self, _session_id: &str) -> Result<(), ConnectorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn command(client_command_id: &str) -> AgentRunCancelCommand {
        AgentRunCancelCommand {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Some(Uuid::new_v4()),
            runtime_session_id: "runtime-session-1".to_string(),
            client_command_id: client_command_id.to_string(),
            reason: Some("terminal_status_cancel".to_string()),
        }
    }

    #[tokio::test]
    async fn duplicate_cancel_replays_receipt_without_second_runtime_cancel() {
        let repo = MemoryAgentRunCommandReceiptRepository::default();
        let runtime = Arc::new(CountingCancelRuntime::default());
        let service = AgentRunCancelCommandService::new(&repo, runtime.as_ref());
        let command = command("cancel-command-1");

        let first = service.cancel(command.clone()).await.expect("first cancel");
        let duplicate = service.cancel(command).await.expect("duplicate cancel");

        assert_eq!(first.status, "accepted");
        assert!(!first.duplicate);
        assert_eq!(duplicate.status, "accepted");
        assert!(duplicate.duplicate);
        assert_eq!(runtime.calls.load(Ordering::SeqCst), 1);
    }
}
