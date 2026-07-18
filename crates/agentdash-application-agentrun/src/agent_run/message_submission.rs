use std::sync::Arc;

use agentdash_agent_runtime_contract::{PresentationThreadId, RuntimeActor, RuntimeInput};
use agentdash_application_ports::agent_run_message_submission::{
    AgentRunMessageAcceptanceResults, AgentRunMessageSubmissionAdmission,
    AgentRunMessageSubmissionCompletion, AgentRunMessageSubmissionReservation,
    AgentRunMessageSubmissionStore, CompleteAgentRunMessageSubmission,
    NewAgentRunMessageSubmission,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_application_ports::request_digest::canonical_request_digest;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage;
use agentdash_domain::agent_run_mailbox::{MailboxMessageOrigin, MailboxSourceIdentity};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandReceipt, AgentRunCommandStatus,
    NewAgentRunCommandReceipt,
};
use agentdash_platform_spi::{AgentConfig, AuthIdentity};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::{AgentRunPresentationDraft, EnqueueRuntimeMailboxMessage, RuntimeAgentRunMailbox};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunMessageDeliveryAttempt {
    Deferred,
    Accepted { mailbox_message_id: Uuid },
    Failed { mailbox_message_id: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunAcceptedProductResultKind {
    Started,
    Steered,
}

pub type AgentRunProductDeliveryResults = AgentRunMessageAcceptanceResults;

pub trait AgentRunMessageProductResultProjector: Send + Sync {
    fn accepted_result(
        &self,
        kind: AgentRunAcceptedProductResultKind,
    ) -> Result<Value, WorkflowApplicationError>;

    fn failed_result(&self) -> Result<Value, WorkflowApplicationError>;

    fn queued_result(
        &self,
        message: &AgentRunMailboxMessage,
    ) -> Result<Value, WorkflowApplicationError>;
}

pub type AcceptedResultProjector = dyn Fn(AgentRunAcceptedProductResultKind) -> Result<Value, WorkflowApplicationError>
    + Send
    + Sync;
pub type QueuedResultProjector =
    dyn Fn(&AgentRunMailboxMessage) -> Result<Value, WorkflowApplicationError> + Send + Sync;
pub type FailedResultProjector = dyn Fn() -> Result<Value, WorkflowApplicationError> + Send + Sync;

pub struct FnAgentRunMessageProductResultProjector {
    accepted: Arc<AcceptedResultProjector>,
    queued: Arc<QueuedResultProjector>,
    failed: Arc<FailedResultProjector>,
}

impl FnAgentRunMessageProductResultProjector {
    pub fn new(
        accepted: Arc<AcceptedResultProjector>,
        queued: Arc<QueuedResultProjector>,
        failed: Arc<FailedResultProjector>,
    ) -> Self {
        Self {
            accepted,
            queued,
            failed,
        }
    }
}

impl AgentRunMessageProductResultProjector for FnAgentRunMessageProductResultProjector {
    fn accepted_result(
        &self,
        kind: AgentRunAcceptedProductResultKind,
    ) -> Result<Value, WorkflowApplicationError> {
        (self.accepted)(kind)
    }

    fn failed_result(&self) -> Result<Value, WorkflowApplicationError> {
        (self.failed)()
    }

    fn queued_result(
        &self,
        message: &AgentRunMailboxMessage,
    ) -> Result<Value, WorkflowApplicationError> {
        (self.queued)(message)
    }
}

pub fn project_product_delivery_results(
    projector: &dyn AgentRunMessageProductResultProjector,
) -> Result<AgentRunProductDeliveryResults, WorkflowApplicationError> {
    Ok(AgentRunProductDeliveryResults {
        started: projector.accepted_result(AgentRunAcceptedProductResultKind::Started)?,
        steered: projector.accepted_result(AgentRunAcceptedProductResultKind::Steered)?,
        failed: projector.failed_result()?,
    })
}

#[async_trait::async_trait]
pub trait AgentRunMessageDeliveryCoordinator: Send + Sync {
    /// Advances at most one mailbox message. The returned id is authoritative:
    /// callers must not attribute a different message's delivery to their own
    /// product command.
    async fn try_deliver(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunMessageDeliveryAttempt, WorkflowApplicationError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitAgentRunMessage {
    pub client_command_id: String,
    pub mailbox_message: agentdash_domain::agent_run_mailbox::NewAgentRunMailboxMessage,
    pub acceptance_results: AgentRunMessageAcceptanceResults,
    pub reserved_receipt_id: Option<Uuid>,
}

/// Stable product semantics for one AgentRun message submission.
///
/// Mutable command preconditions are deliberately absent: they gate only a
/// newly claimed command and cannot change the identity of a network retry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentRunBackendSelectionSemantic {
    pub mode: String,
    pub backend_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentRunMessageSemanticRequest {
    pub target: AgentRunRuntimeTarget,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub executor_config: Option<Value>,
    pub backend_selection: Option<AgentRunBackendSelectionSemantic>,
    pub delivery_intent: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunMessageSubmissionResult {
    pub receipt_id: Uuid,
    pub result_json: Value,
    pub error_message: Option<String>,
    /// Replay metadata is deliberately outside the immutable stored result.
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunMessageSubmissionOwnership {
    Unattached { reserved_receipt_id: Uuid },
    Attached { receipt_id: Uuid },
    Unknown { reserved_receipt_id: Uuid },
}

#[derive(Debug)]
pub struct AgentRunMessageSubmissionFailure {
    pub ownership: AgentRunMessageSubmissionOwnership,
    pub source: WorkflowApplicationError,
}

impl std::fmt::Display for AgentRunMessageSubmissionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for AgentRunMessageSubmissionFailure {}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectAgentInitialMessageSubmission {
    pub reserved_receipt_id: Uuid,
    pub target: AgentRunRuntimeTarget,
    pub presentation_thread_id: PresentationThreadId,
    pub client_command_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub identity: Option<AuthIdentity>,
    pub execution_profile_override: AgentConfig,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[async_trait::async_trait]
pub trait ProjectAgentInitialMessageSubmissionPort: Send + Sync {
    async fn submit_initial_message(
        &self,
        command: ProjectAgentInitialMessageSubmission,
        projector: Arc<dyn AgentRunMessageProductResultProjector>,
    ) -> Result<AgentRunMessageSubmissionResult, AgentRunMessageSubmissionFailure>;
}

#[derive(Clone)]
pub struct ProjectAgentInitialMessageSubmissionService {
    store: Arc<dyn AgentRunMessageSubmissionStore>,
    mailbox: RuntimeAgentRunMailbox,
}

impl ProjectAgentInitialMessageSubmissionService {
    pub fn new(
        store: Arc<dyn AgentRunMessageSubmissionStore>,
        mailbox: RuntimeAgentRunMailbox,
    ) -> Self {
        Self { store, mailbox }
    }
}

#[async_trait::async_trait]
impl ProjectAgentInitialMessageSubmissionPort for ProjectAgentInitialMessageSubmissionService {
    async fn submit_initial_message(
        &self,
        command: ProjectAgentInitialMessageSubmission,
        projector: Arc<dyn AgentRunMessageProductResultProjector>,
    ) -> Result<AgentRunMessageSubmissionResult, AgentRunMessageSubmissionFailure> {
        let reserved_receipt_id = command.reserved_receipt_id;
        let acceptance_results = match project_product_delivery_results(projector.as_ref()) {
            Ok(results) => results,
            Err(error) => {
                return Err(submission_failure_for_reservation(
                    self.store.as_ref(),
                    reserved_receipt_id,
                    error,
                )
                .await);
            }
        };
        let runtime_input = command
            .input
            .iter()
            .cloned()
            .map(RuntimeInput::user_input)
            .collect();
        let actor = command
            .identity
            .as_ref()
            .map(|identity| RuntimeActor::User {
                subject: identity.user_id.clone(),
            })
            .unwrap_or_else(|| RuntimeActor::System {
                component: "project_agent_start".to_string(),
            });
        let mailbox_message = match self.mailbox.prepare_message(EnqueueRuntimeMailboxMessage {
            target: command.target,
            presentation_thread_id: command.presentation_thread_id,
            presentation: AgentRunPresentationDraft {
                content: command.input,
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source: super::LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: command.client_command_id.clone(),
            input: runtime_input,
            actor,
            identity: command.identity,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::draft_start(),
            delivery_intent: None,
            executor_config: Some(command.execution_profile_override),
            backend_selection: command.backend_selection,
        }) {
            Ok(message) => message,
            Err(error) => {
                return Err(submission_failure_for_reservation(
                    self.store.as_ref(),
                    reserved_receipt_id,
                    WorkflowApplicationError::Internal(error.to_string()),
                )
                .await);
            }
        };
        AgentRunMessageSubmissionService::new(
            self.store.clone(),
            Arc::new(self.mailbox.clone()),
            projector,
        )
        .submit_reserved_project_agent_start(SubmitAgentRunMessage {
            client_command_id: command.client_command_id,
            mailbox_message,
            acceptance_results,
            reserved_receipt_id: Some(reserved_receipt_id),
        })
        .await
    }
}

#[derive(Clone)]
pub struct AgentRunMessageSubmissionService {
    store: Arc<dyn AgentRunMessageSubmissionStore>,
    delivery: Arc<dyn AgentRunMessageDeliveryCoordinator>,
    results: Arc<dyn AgentRunMessageProductResultProjector>,
}

#[derive(Clone)]
pub struct AgentRunMessageReservationService {
    store: Arc<dyn AgentRunMessageSubmissionStore>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgentRunStartReceiptRequest {
    pub project_id: Uuid,
    pub project_agent_id: Uuid,
    pub client_command_id: String,
    pub request_digest: String,
}

#[async_trait::async_trait]
pub trait ProjectAgentRunStartReceiptPort: Send + Sync {
    async fn reserve_project_agent_start(
        &self,
        request: ProjectAgentRunStartReceiptRequest,
    ) -> Result<AgentRunMessageSubmissionReservation, WorkflowApplicationError>;

    async fn abandon_project_agent_start(
        &self,
        receipt_id: Uuid,
    ) -> Result<bool, WorkflowApplicationError>;

    async fn fail_project_agent_start(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, WorkflowApplicationError>;
}

impl AgentRunMessageReservationService {
    pub fn new(store: Arc<dyn AgentRunMessageSubmissionStore>) -> Self {
        Self { store }
    }

    pub async fn reserve_project_agent_start<T: Serialize>(
        &self,
        project_id: Uuid,
        project_agent_id: Uuid,
        client_command_id: String,
        request: &T,
    ) -> Result<AgentRunMessageSubmissionReservation, WorkflowApplicationError> {
        self.store
            .reserve(NewAgentRunCommandReceipt {
                scope_kind: "project_agent_run_start".to_string(),
                scope_key: format!("{project_id}:{project_agent_id}"),
                command_kind: AgentRunCommandKind::ProjectAgentStart,
                client_command_id,
                request_digest: canonical_request_digest(request)
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
            })
            .await
            .map_err(map_domain_error)
    }

    pub async fn reserve_agent_run_message(
        &self,
        kind: AgentRunCommandKind,
        client_command_id: String,
        request: &AgentRunMessageSemanticRequest,
    ) -> Result<AgentRunMessageSubmissionReservation, WorkflowApplicationError> {
        self.store
            .reserve(NewAgentRunCommandReceipt {
                scope_kind: "agent_run".to_string(),
                scope_key: format!("{}:{}", request.target.run_id, request.target.agent_id),
                command_kind: kind,
                client_command_id,
                request_digest: canonical_request_digest(request)
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
            })
            .await
            .map_err(map_domain_error)
    }

    pub async fn abandon(&self, receipt_id: Uuid) -> Result<bool, WorkflowApplicationError> {
        self.store
            .abandon_reservation(receipt_id)
            .await
            .map_err(map_domain_error)
    }

    pub async fn load_receipt(
        &self,
        receipt_id: Uuid,
    ) -> Result<Option<AgentRunCommandReceipt>, WorkflowApplicationError> {
        self.store
            .load_receipt(receipt_id)
            .await
            .map_err(map_domain_error)
    }

    pub async fn fail(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, WorkflowApplicationError> {
        self.store
            .fail_reservation(receipt_id, error_message)
            .await
            .map_err(map_domain_error)
    }
}

#[async_trait::async_trait]
impl ProjectAgentRunStartReceiptPort for AgentRunMessageReservationService {
    async fn reserve_project_agent_start(
        &self,
        request: ProjectAgentRunStartReceiptRequest,
    ) -> Result<AgentRunMessageSubmissionReservation, WorkflowApplicationError> {
        self.store
            .reserve(NewAgentRunCommandReceipt {
                scope_kind: "project_agent_run_start".to_string(),
                scope_key: format!("{}:{}", request.project_id, request.project_agent_id),
                command_kind: AgentRunCommandKind::ProjectAgentStart,
                client_command_id: request.client_command_id,
                request_digest: request.request_digest,
            })
            .await
            .map_err(map_domain_error)
    }

    async fn abandon_project_agent_start(
        &self,
        receipt_id: Uuid,
    ) -> Result<bool, WorkflowApplicationError> {
        self.abandon(receipt_id).await
    }

    async fn fail_project_agent_start(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, WorkflowApplicationError> {
        self.fail(receipt_id, error_message).await
    }
}

impl AgentRunMessageSubmissionService {
    pub fn new(
        store: Arc<dyn AgentRunMessageSubmissionStore>,
        delivery: Arc<dyn AgentRunMessageDeliveryCoordinator>,
        results: Arc<dyn AgentRunMessageProductResultProjector>,
    ) -> Self {
        Self {
            store,
            delivery,
            results,
        }
    }

    pub async fn submit_message(
        &self,
        command: SubmitAgentRunMessage,
        request: &AgentRunMessageSemanticRequest,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        self.submit_agent_run_command(AgentRunCommandKind::MessageSubmit, command, request)
            .await
    }

    pub async fn submit_fork_message(
        &self,
        command: SubmitAgentRunMessage,
        request: &AgentRunMessageSemanticRequest,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        self.submit_agent_run_command(AgentRunCommandKind::AgentRunForkSubmit, command, request)
            .await
    }

    pub async fn submit_project_agent_start<T: Serialize>(
        &self,
        project_id: Uuid,
        project_agent_id: Uuid,
        command: SubmitAgentRunMessage,
        request: &T,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        let receipt = NewAgentRunCommandReceipt {
            scope_kind: "project_agent_run_start".to_string(),
            scope_key: format!("{project_id}:{project_agent_id}"),
            command_kind: AgentRunCommandKind::ProjectAgentStart,
            client_command_id: command.client_command_id.clone(),
            request_digest: canonical_request_digest(request)
                .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
        };
        self.submit_prepared(receipt, command).await
    }

    pub async fn submit_reserved_project_agent_start(
        &self,
        mut command: SubmitAgentRunMessage,
    ) -> Result<AgentRunMessageSubmissionResult, AgentRunMessageSubmissionFailure> {
        let reserved_receipt_id =
            command
                .reserved_receipt_id
                .ok_or_else(|| AgentRunMessageSubmissionFailure {
                    ownership: AgentRunMessageSubmissionOwnership::Unknown {
                        reserved_receipt_id: Uuid::nil(),
                    },
                    source: WorkflowApplicationError::Internal(
                        "reserved Project Agent initial submission requires receipt ownership"
                            .to_string(),
                    ),
                })?;
        let receipt = match self.store.load_receipt(reserved_receipt_id).await {
            Ok(Some(receipt)) => receipt,
            Ok(None) => {
                return Err(AgentRunMessageSubmissionFailure {
                    ownership: AgentRunMessageSubmissionOwnership::Unattached {
                        reserved_receipt_id,
                    },
                    source: WorkflowApplicationError::NotFound(format!(
                        "Project Agent start reservation {reserved_receipt_id} does not exist"
                    )),
                });
            }
            Err(error) => {
                return Err(AgentRunMessageSubmissionFailure {
                    ownership: AgentRunMessageSubmissionOwnership::Unknown {
                        reserved_receipt_id,
                    },
                    source: map_domain_error(error),
                });
            }
        };
        if receipt.command_kind != AgentRunCommandKind::ProjectAgentStart
            || receipt.client_command_id != command.client_command_id
        {
            return Err(AgentRunMessageSubmissionFailure {
                ownership: ownership_from_receipt(&receipt),
                source: WorkflowApplicationError::Conflict(
                    "reserved receipt does not own this Project Agent initial submission"
                        .to_string(),
                ),
            });
        }
        let expected = NewAgentRunCommandReceipt {
            scope_kind: receipt.scope_kind,
            scope_key: receipt.scope_key,
            command_kind: receipt.command_kind,
            client_command_id: receipt.client_command_id,
            request_digest: receipt.request_digest,
        };
        command.reserved_receipt_id = Some(reserved_receipt_id);
        match self.submit_prepared(expected, command).await {
            Ok(result) => Ok(result),
            Err(source) => Err(submission_failure_for_reservation(
                self.store.as_ref(),
                reserved_receipt_id,
                source,
            )
            .await),
        }
    }

    async fn submit_agent_run_command(
        &self,
        kind: AgentRunCommandKind,
        command: SubmitAgentRunMessage,
        request: &AgentRunMessageSemanticRequest,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        let receipt = NewAgentRunCommandReceipt {
            scope_kind: "agent_run".to_string(),
            scope_key: format!(
                "{}:{}",
                command.mailbox_message.run_id, command.mailbox_message.agent_id
            ),
            command_kind: kind,
            client_command_id: command.client_command_id.clone(),
            request_digest: canonical_request_digest(request)
                .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
        };
        self.submit_prepared(receipt, command).await
    }

    async fn submit_prepared(
        &self,
        receipt: NewAgentRunCommandReceipt,
        command: SubmitAgentRunMessage,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        let submission = NewAgentRunMessageSubmission {
            receipt,
            reserved_receipt_id: command.reserved_receipt_id,
            mailbox_message: command.mailbox_message,
            acceptance_results: command.acceptance_results,
        };
        let admission = self
            .store
            .admit(submission)
            .await
            .map_err(map_domain_error)?;
        let (receipt_id, mailbox_message, duplicate) = match admission {
            AgentRunMessageSubmissionAdmission::Replay { receipt } => {
                return replay_agent_run_message_submission(receipt, true);
            }
            AgentRunMessageSubmissionAdmission::Created {
                receipt_id,
                mailbox_message,
            } => (receipt_id, mailbox_message, false),
            AgentRunMessageSubmissionAdmission::ReconcileRequired {
                receipt,
                mailbox_message,
            } => (receipt.id, mailbox_message, true),
        };
        let target = AgentRunRuntimeTarget {
            run_id: mailbox_message.run_id,
            agent_id: mailbox_message.agent_id,
        };

        match self.delivery.try_deliver(&target).await? {
            AgentRunMessageDeliveryAttempt::Accepted { mailbox_message_id }
            | AgentRunMessageDeliveryAttempt::Failed { mailbox_message_id }
                if mailbox_message_id == mailbox_message.id =>
            {
                let receipt = self
                    .store
                    .load_receipt_by_mailbox_message(mailbox_message_id)
                    .await
                    .map_err(map_domain_error)?
                    .ok_or_else(|| {
                    WorkflowApplicationError::Internal(format!(
                        "mailbox message {mailbox_message_id} settled without its product receipt"
                    ))
                })?;
                replay_agent_run_message_submission(receipt, duplicate)
            }
            AgentRunMessageDeliveryAttempt::Deferred
            | AgentRunMessageDeliveryAttempt::Accepted { .. }
            | AgentRunMessageDeliveryAttempt::Failed { .. } => {
                let queued_result_json = self.results.queued_result(&mailbox_message)?;
                let completion = self
                    .store
                    .complete_submission(CompleteAgentRunMessageSubmission {
                        receipt_id,
                        mailbox_message_id: mailbox_message.id,
                        accepted_refs: AgentRunAcceptedRefs {
                            run_id: mailbox_message.run_id,
                            agent_id: mailbox_message.agent_id,
                            frame_id: None,
                            frame_revision: None,
                            runtime_thread_id: None,
                            runtime_operation_id: None,
                        },
                        result_json: queued_result_json,
                    })
                    .await
                    .map_err(map_domain_error)?;
                let receipt = match completion {
                    AgentRunMessageSubmissionCompletion::Completed { receipt }
                    | AgentRunMessageSubmissionCompletion::Replayed { receipt } => receipt,
                };
                replay_agent_run_message_submission(receipt, duplicate)
            }
        }
    }
}

fn ownership_from_receipt(receipt: &AgentRunCommandReceipt) -> AgentRunMessageSubmissionOwnership {
    match receipt.mailbox_message_id {
        Some(_) => AgentRunMessageSubmissionOwnership::Attached {
            receipt_id: receipt.id,
        },
        None => AgentRunMessageSubmissionOwnership::Unattached {
            reserved_receipt_id: receipt.id,
        },
    }
}

async fn submission_failure_for_reservation(
    store: &dyn AgentRunMessageSubmissionStore,
    reserved_receipt_id: Uuid,
    source: WorkflowApplicationError,
) -> AgentRunMessageSubmissionFailure {
    match store.load_receipt(reserved_receipt_id).await {
        Ok(Some(receipt)) => AgentRunMessageSubmissionFailure {
            ownership: ownership_from_receipt(&receipt),
            source,
        },
        Ok(None) => AgentRunMessageSubmissionFailure {
            ownership: AgentRunMessageSubmissionOwnership::Unattached {
                reserved_receipt_id,
            },
            source,
        },
        Err(error) => AgentRunMessageSubmissionFailure {
            ownership: AgentRunMessageSubmissionOwnership::Unknown {
                reserved_receipt_id,
            },
            source: WorkflowApplicationError::Internal(format!(
                "{source}; unable to establish Project Agent initial submission ownership: {error}"
            )),
        },
    }
}

pub fn replay_agent_run_message_submission(
    receipt: AgentRunCommandReceipt,
    duplicate: bool,
) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
    match receipt.status {
        AgentRunCommandStatus::Accepted | AgentRunCommandStatus::TerminalFailed => {
            let result_json = receipt.result_json.ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "product command receipt {} has no observable result",
                    receipt.id
                ))
            })?;
            Ok(AgentRunMessageSubmissionResult {
                receipt_id: receipt.id,
                result_json,
                error_message: receipt.error_message,
                duplicate,
            })
        }
        AgentRunCommandStatus::Pending => Err(WorkflowApplicationError::Conflict(format!(
            "product command receipt {} is still reconciling delivery",
            receipt.id
        ))),
    }
}

fn map_domain_error(error: DomainError) -> WorkflowApplicationError {
    match error {
        DomainError::NotFound { .. } => WorkflowApplicationError::NotFound(error.to_string()),
        DomainError::Conflict { .. } => WorkflowApplicationError::Conflict(error.to_string()),
        DomainError::InvalidConfig(_) => WorkflowApplicationError::Internal(error.to_string()),
        _ => WorkflowApplicationError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_request(text: &str) -> AgentRunMessageSemanticRequest {
        AgentRunMessageSemanticRequest {
            target: AgentRunRuntimeTarget {
                run_id: Uuid::nil(),
                agent_id: Uuid::from_u128(1),
            },
            input: agentdash_agent_protocol::text_user_input_blocks(text),
            executor_config: Some(serde_json::json!({
                "executor": "PI_AGENT",
                "provider_id": "provider-a",
                "model_id": "model-a"
            })),
            backend_selection: Some(AgentRunBackendSelectionSemantic {
                mode: "auto_idle".to_string(),
                backend_id: None,
            }),
            delivery_intent: None,
        }
    }

    #[test]
    fn semantic_request_digest_is_stable_across_mutable_precondition_refreshes() {
        let original = semantic_request("hello");
        // A command snapshot/stale guard is intentionally not representable in
        // this type, so refreshing it cannot change the product identity.
        let refreshed_precondition = original.clone();
        assert_eq!(
            canonical_request_digest(&original).expect("original digest"),
            canonical_request_digest(&refreshed_precondition).expect("refreshed digest")
        );

        let changed_input = semantic_request("different");
        assert_ne!(
            canonical_request_digest(&original).expect("original digest"),
            canonical_request_digest(&changed_input).expect("changed digest")
        );
    }
}
