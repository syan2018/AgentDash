use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandReceiptRepository,
    AgentRunCommandStatus, NewAgentRunCommandReceipt,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::WorkflowApplicationError;

pub struct AgentRunProductCommandService<'a> {
    receipts: &'a dyn AgentRunCommandReceiptRepository,
}

pub struct AgentRunProductCommandClaim {
    pub receipt_id: Uuid,
    pub duplicate: bool,
    pub result_json: Option<Value>,
    pub status: AgentRunCommandStatus,
    pub error_message: Option<String>,
}

impl<'a> AgentRunProductCommandService<'a> {
    pub fn new(receipts: &'a dyn AgentRunCommandReceiptRepository) -> Self {
        Self { receipts }
    }

    pub async fn claim<T: Serialize>(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        kind: AgentRunCommandKind,
        client_command_id: String,
        request: &T,
    ) -> Result<AgentRunProductCommandClaim, WorkflowApplicationError> {
        let payload = serde_json::to_vec(request)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let digest = format!("{:x}", Sha256::digest(payload));
        let claim = self
            .receipts
            .claim(NewAgentRunCommandReceipt {
                scope_kind: "agent_run".to_string(),
                scope_key: format!("{run_id}:{agent_id}"),
                command_kind: kind,
                client_command_id,
                request_digest: digest,
            })
            .await
            .map_err(map_domain_error)?;
        let receipt = claim.receipt();
        Ok(AgentRunProductCommandClaim {
            receipt_id: receipt.id,
            duplicate: claim.duplicate(),
            result_json: receipt.result_json.clone(),
            status: receipt.status,
            error_message: receipt.error_message.clone(),
        })
    }

    pub async fn accept<T: Serialize>(
        &self,
        receipt_id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
        result: &T,
    ) -> Result<(), WorkflowApplicationError> {
        let result_json = serde_json::to_value(result)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        self.receipts
            .accept_with_result(receipt_id, accepted_refs, result_json)
            .await
            .map_err(map_domain_error)?;
        Ok(())
    }

    pub async fn store_result<T: Serialize>(
        &self,
        receipt_id: Uuid,
        result: &T,
    ) -> Result<(), WorkflowApplicationError> {
        let result_json = serde_json::to_value(result)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        self.receipts
            .store_result_json(receipt_id, result_json)
            .await
            .map_err(map_domain_error)?;
        Ok(())
    }

    pub async fn accept_refs(
        &self,
        receipt_id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
    ) -> Result<(), WorkflowApplicationError> {
        self.receipts
            .mark_accepted(receipt_id, accepted_refs)
            .await
            .map_err(map_domain_error)?;
        Ok(())
    }

    pub async fn fail(
        &self,
        receipt_id: Uuid,
        error: impl Into<String>,
    ) -> Result<(), WorkflowApplicationError> {
        self.receipts
            .mark_terminal_failed(receipt_id, error.into())
            .await
            .map_err(map_domain_error)?;
        Ok(())
    }

    pub async fn fail_with_result<T: Serialize>(
        &self,
        receipt_id: Uuid,
        error: impl Into<String>,
        result: &T,
    ) -> Result<(), WorkflowApplicationError> {
        let result_json = serde_json::to_value(result)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        self.receipts
            .fail_with_result(receipt_id, error.into(), result_json)
            .await
            .map_err(map_domain_error)?;
        Ok(())
    }
}

fn map_domain_error(error: DomainError) -> WorkflowApplicationError {
    match error {
        DomainError::NotFound { .. } => WorkflowApplicationError::NotFound(error.to_string()),
        DomainError::Conflict { .. } => WorkflowApplicationError::Conflict(error.to_string()),
        _ => WorkflowApplicationError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::workflow::{
        AgentRunAcceptedRefs, AgentRunCommandClaim, AgentRunCommandReceipt,
        AgentRunCommandReceiptRepository, AgentRunCommandStatus, NewAgentRunCommandReceipt,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::{Value, json};

    use super::*;

    struct AtomicReceiptRepository {
        accepted: Mutex<Option<(Uuid, AgentRunAcceptedRefs, Value)>>,
        failed: Mutex<Option<(Uuid, String, Value)>>,
    }

    #[async_trait]
    impl AgentRunCommandReceiptRepository for AtomicReceiptRepository {
        async fn claim(
            &self,
            _receipt: NewAgentRunCommandReceipt,
        ) -> Result<AgentRunCommandClaim, DomainError> {
            unreachable!("accept test does not claim")
        }

        async fn mark_accepted(
            &self,
            _id: Uuid,
            _accepted_refs: AgentRunAcceptedRefs,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            panic!("service must not split acceptance from result persistence")
        }

        async fn attach_mailbox_message(
            &self,
            _id: Uuid,
            _mailbox_message_id: Uuid,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            unreachable!("accept test does not attach mailbox messages")
        }

        async fn store_result_json(
            &self,
            _id: Uuid,
            _result_json: Value,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            panic!("service must not split result persistence from acceptance")
        }

        async fn accept_with_result(
            &self,
            id: Uuid,
            accepted_refs: AgentRunAcceptedRefs,
            result_json: Value,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            *self.accepted.lock().expect("accepted lock") =
                Some((id, accepted_refs.clone(), result_json.clone()));
            Ok(receipt(id, accepted_refs, result_json))
        }

        async fn mark_terminal_failed(
            &self,
            _id: Uuid,
            _error_message: String,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            unreachable!("accept test does not fail")
        }

        async fn fail_with_result(
            &self,
            id: Uuid,
            error_message: String,
            result_json: Value,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            *self.failed.lock().expect("failed lock") =
                Some((id, error_message, result_json.clone()));
            let mut receipt = receipt(id, accepted_refs(), result_json);
            receipt.status = AgentRunCommandStatus::TerminalFailed;
            Ok(receipt)
        }

        async fn get(&self, _id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
            unreachable!("accept test does not read")
        }
    }

    #[tokio::test]
    async fn accept_uses_atomic_repository_operation() {
        let repository = AtomicReceiptRepository {
            accepted: Mutex::new(None),
            failed: Mutex::new(None),
        };
        let service = AgentRunProductCommandService::new(&repository);
        let receipt_id = Uuid::new_v4();
        let refs = accepted_refs();
        let result = json!({"outcome": "forked"});

        service
            .accept(receipt_id, refs.clone(), &result)
            .await
            .expect("atomic accept");

        assert_eq!(
            repository.accepted.into_inner().expect("accepted lock"),
            Some((receipt_id, refs, result))
        );
    }

    #[tokio::test]
    async fn fail_with_result_uses_atomic_repository_operation() {
        let repository = AtomicReceiptRepository {
            accepted: Mutex::new(None),
            failed: Mutex::new(None),
        };
        let service = AgentRunProductCommandService::new(&repository);
        let receipt_id = Uuid::new_v4();
        let result = json!({"outcome": "blocked"});

        service
            .fail_with_result(receipt_id, "active turn", &result)
            .await
            .expect("atomic failure result");

        assert_eq!(
            repository.failed.into_inner().expect("failed lock"),
            Some((receipt_id, "active turn".to_string(), result))
        );
    }

    struct ReplayableForkReceiptRepository {
        state: Mutex<(Option<AgentRunCommandReceipt>, bool)>,
    }

    #[async_trait]
    impl AgentRunCommandReceiptRepository for ReplayableForkReceiptRepository {
        async fn claim(
            &self,
            request: NewAgentRunCommandReceipt,
        ) -> Result<AgentRunCommandClaim, DomainError> {
            let mut state = self.state.lock().expect("state");
            if let Some(receipt) = state.0.clone() {
                return Ok(AgentRunCommandClaim::Duplicate(receipt));
            }
            let now = Utc::now();
            let receipt = AgentRunCommandReceipt {
                id: Uuid::new_v4(),
                scope_kind: request.scope_kind,
                scope_key: request.scope_key,
                command_kind: request.command_kind,
                client_command_id: request.client_command_id,
                request_digest: request.request_digest,
                status: AgentRunCommandStatus::Pending,
                mailbox_message_id: None,
                accepted_refs: None,
                result_json: None,
                error_message: None,
                created_at: now,
                updated_at: now,
                accepted_at: None,
                failed_at: None,
            };
            state.0 = Some(receipt.clone());
            Ok(AgentRunCommandClaim::Created(receipt))
        }

        async fn mark_accepted(
            &self,
            _id: Uuid,
            accepted_refs: AgentRunAcceptedRefs,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut state = self.state.lock().expect("state");
            if state.1 {
                state.1 = false;
                return Err(DomainError::Database {
                    operation: "mark fork accepted",
                    message: "injected".to_string(),
                });
            }
            let receipt = state.0.as_mut().expect("receipt");
            receipt.status = AgentRunCommandStatus::Accepted;
            receipt.accepted_refs = Some(accepted_refs);
            Ok(receipt.clone())
        }

        async fn attach_mailbox_message(
            &self,
            _id: Uuid,
            _mailbox_message_id: Uuid,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            unreachable!("fork replay does not attach mailbox messages")
        }

        async fn store_result_json(
            &self,
            _id: Uuid,
            result_json: Value,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut state = self.state.lock().expect("state");
            let receipt = state.0.as_mut().expect("receipt");
            receipt.result_json = Some(result_json);
            Ok(receipt.clone())
        }

        async fn mark_terminal_failed(
            &self,
            _id: Uuid,
            _error_message: String,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            unreachable!("fork replay succeeds on retry")
        }

        async fn get(&self, _id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
            Ok(self.state.lock().expect("state").0.clone())
        }
    }

    #[tokio::test]
    async fn fork_result_survives_acceptance_failure_and_replays_without_a_second_materialization()
    {
        let repository = ReplayableForkReceiptRepository {
            state: Mutex::new((None, true)),
        };
        let service = AgentRunProductCommandService::new(&repository);
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let request = json!({"client_command_id": "fork-1"});
        let first = service
            .claim(
                run_id,
                agent_id,
                AgentRunCommandKind::AgentRunFork,
                "fork-1".to_string(),
                &request,
            )
            .await
            .expect("first claim");
        let result = json!({
            "outcome": "forked",
            "child_refs": {"run_ref": {"run_id": run_id}, "agent_ref": {"run_id": run_id, "agent_id": agent_id}}
        });
        service
            .store_result(first.receipt_id, &result)
            .await
            .expect("persist replay intent before final acceptance");
        service
            .accept_refs(first.receipt_id, accepted_refs())
            .await
            .expect_err("injected acceptance failure");

        let replay = service
            .claim(
                run_id,
                agent_id,
                AgentRunCommandKind::AgentRunFork,
                "fork-1".to_string(),
                &request,
            )
            .await
            .expect("duplicate claim");
        assert!(replay.duplicate);
        assert_eq!(replay.status, AgentRunCommandStatus::Pending);
        assert_eq!(replay.result_json, Some(result));
        service
            .accept_refs(replay.receipt_id, accepted_refs())
            .await
            .expect("replay finalizes original fork");
    }

    fn accepted_refs() -> AgentRunAcceptedRefs {
        AgentRunAcceptedRefs {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Some(Uuid::new_v4()),
            frame_revision: Some(1),
            runtime_session_id: Some("session-1".to_string()),
            agent_run_turn_id: Some("turn-1".to_string()),
            protocol_turn_id: Some("protocol-turn-1".to_string()),
        }
    }

    fn receipt(
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
        result_json: Value,
    ) -> AgentRunCommandReceipt {
        let now = Utc::now();
        AgentRunCommandReceipt {
            id,
            scope_kind: "agent_run".to_string(),
            scope_key: "run:agent".to_string(),
            command_kind: AgentRunCommandKind::AgentRunFork,
            client_command_id: "command-1".to_string(),
            request_digest: "digest".to_string(),
            status: AgentRunCommandStatus::Accepted,
            mailbox_message_id: None,
            accepted_refs: Some(accepted_refs),
            result_json: Some(result_json),
            error_message: None,
            created_at: now,
            updated_at: now,
            accepted_at: Some(now),
            failed_at: None,
        }
    }
}
