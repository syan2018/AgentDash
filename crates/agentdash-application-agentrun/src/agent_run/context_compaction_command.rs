use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{Duration, Instant, sleep};
use uuid::Uuid;

use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput, LaunchPromptInput};
use agentdash_application_runtime_session::session::SessionLaunchService;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandReceipt,
    AgentRunCommandReceiptRepository, AgentRunCommandStatus,
    ManualContextCompactionRequestRepository, ManualContextCompactionRequestStatus,
    ManualContextCompactionRequestedMode, NewManualContextCompactionRequest,
};

use crate::agent_run::command_receipt::{
    AgentRunCommandReceiptView, claim_agent_run_command_receipt, digest_command_request,
    mark_command_terminal_failed,
};
use crate::agent_run::{
    AgentRunExecutionState, DeliveryRuntimeSelection, DeliveryRuntimeSelectionError,
    DeliveryRuntimeSelectionPolicy, DeliveryRuntimeSelectionRepositories,
    DeliveryRuntimeSelectionService,
};
use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AgentRunContextCompactionCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub client_command_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunContextCompactionOutcome {
    ScheduledNextTurn,
    LaunchedCompactionTurn,
    Completed,
    NoEligibleMessages,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunContextCompactionCommandResult {
    pub command_receipt: AgentRunCommandReceiptView,
    pub outcome: AgentRunContextCompactionOutcome,
    pub runtime_session_id: Option<String>,
    pub request_id: Option<String>,
    pub turn_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AgentRunRuntimeCommandFulfillmentDecision {
    ScheduleForNextTurn {
        selection: DeliveryRuntimeSelection,
        active_turn_id: String,
    },
    LaunchMaintenanceTurn {
        selection: DeliveryRuntimeSelection,
    },
    Reject {
        selection: Option<DeliveryRuntimeSelection>,
        disabled_code: String,
        message: String,
    },
}

pub struct AgentRunRuntimeCommandFulfillmentService<'a> {
    delivery_selection: DeliveryRuntimeSelectionService<'a>,
}

impl<'a> AgentRunRuntimeCommandFulfillmentService<'a> {
    pub fn new(repos: DeliveryRuntimeSelectionRepositories<'a>) -> Self {
        Self {
            delivery_selection: DeliveryRuntimeSelectionService::new(repos),
        }
    }

    pub async fn decide_context_compaction(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunRuntimeCommandFulfillmentDecision, WorkflowApplicationError> {
        let selection = match self
            .delivery_selection
            .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery { run_id, agent_id })
            .await
        {
            Ok(selection) => selection,
            Err(DeliveryRuntimeSelectionError::CurrentDeliveryMissing { .. }) => {
                return Ok(AgentRunRuntimeCommandFulfillmentDecision::Reject {
                    selection: None,
                    disabled_code: "runtime_session_missing".to_string(),
                    message: "当前 AgentRun 缺少可压缩的 runtime session。".to_string(),
                });
            }
            Err(error) => return Err(workflow_error_from_selection_error(error)),
        };

        match selection.execution_state() {
            AgentRunExecutionState::Running {
                turn_id: Some(active_turn_id),
            } => Ok(
                AgentRunRuntimeCommandFulfillmentDecision::ScheduleForNextTurn {
                    selection,
                    active_turn_id,
                },
            ),
            AgentRunExecutionState::Running { turn_id: None } => {
                Ok(AgentRunRuntimeCommandFulfillmentDecision::Reject {
                    selection: Some(selection),
                    disabled_code: "starting_claimed".to_string(),
                    message: "当前 AgentRun 正在启动中，等待 active turn 建立。".to_string(),
                })
            }
            AgentRunExecutionState::Cancelling { .. } => {
                Ok(AgentRunRuntimeCommandFulfillmentDecision::Reject {
                    selection: Some(selection),
                    disabled_code: "cancelling".to_string(),
                    message: "当前 AgentRun 正在取消中。".to_string(),
                })
            }
            AgentRunExecutionState::Lost { message, .. } => {
                Ok(AgentRunRuntimeCommandFulfillmentDecision::Reject {
                    selection: Some(selection),
                    disabled_code: "lost".to_string(),
                    message: message
                        .unwrap_or_else(|| "当前 AgentRun runtime delivery 已丢失。".to_string()),
                })
            }
            AgentRunExecutionState::Idle
            | AgentRunExecutionState::Completed { .. }
            | AgentRunExecutionState::Failed { .. }
            | AgentRunExecutionState::Interrupted { .. } => {
                Ok(AgentRunRuntimeCommandFulfillmentDecision::LaunchMaintenanceTurn { selection })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunContextCompactionRuntimeLaunchCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub runtime_session_id: String,
    pub request_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunContextCompactionRuntimeLaunchOutcome {
    Launched {
        turn_id: String,
    },
    Completed {
        turn_id: String,
        message: Option<String>,
    },
    NoEligibleMessages {
        turn_id: String,
        message: Option<String>,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    NotImplemented {
        message: String,
    },
}

#[async_trait]
pub trait AgentRunContextCompactionRuntimePort: Send + Sync {
    async fn launch_compact_only_turn(
        &self,
        command: AgentRunContextCompactionRuntimeLaunchCommand,
    ) -> Result<AgentRunContextCompactionRuntimeLaunchOutcome, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct AgentRunContextCompactionSessionRuntimePort {
    session_launch: SessionLaunchService,
    compaction_request_repo: Arc<dyn ManualContextCompactionRequestRepository>,
}

impl AgentRunContextCompactionSessionRuntimePort {
    pub fn new(
        session_launch: SessionLaunchService,
        compaction_request_repo: Arc<dyn ManualContextCompactionRequestRepository>,
    ) -> Self {
        Self {
            session_launch,
            compaction_request_repo,
        }
    }
}

#[async_trait]
impl AgentRunContextCompactionRuntimePort for AgentRunContextCompactionSessionRuntimePort {
    async fn launch_compact_only_turn(
        &self,
        command: AgentRunContextCompactionRuntimeLaunchCommand,
    ) -> Result<AgentRunContextCompactionRuntimeLaunchOutcome, WorkflowApplicationError> {
        let launch = LaunchCommand::context_compaction_input(LaunchPromptInput::from_text(
            "Run AgentDash manual context compaction maintenance turn.",
        ));
        let turn_id = self
            .session_launch
            .launch_command_in_task(
                command.runtime_session_id.clone(),
                launch,
                LaunchPlanningInput::default(),
            )
            .await?;

        let deadline = Instant::now() + Duration::from_millis(750);
        loop {
            let request = self
                .compaction_request_repo
                .get_by_id(command.request_id)
                .await?;
            if let Some(request) = request {
                match request.status {
                    ManualContextCompactionRequestStatus::Completed => {
                        return Ok(AgentRunContextCompactionRuntimeLaunchOutcome::Completed {
                            turn_id,
                            message: request_result_message(&request)
                                .or_else(|| Some("context compaction completed".to_string())),
                        });
                    }
                    ManualContextCompactionRequestStatus::Noop => {
                        return Ok(
                            AgentRunContextCompactionRuntimeLaunchOutcome::NoEligibleMessages {
                                turn_id,
                                message: request_result_message(&request)
                                    .or_else(|| Some("no_eligible_messages".to_string())),
                            },
                        );
                    }
                    ManualContextCompactionRequestStatus::Failed => {
                        return Ok(AgentRunContextCompactionRuntimeLaunchOutcome::Failed {
                            turn_id,
                            message: request_result_message(&request)
                                .or_else(|| Some("context compaction failed".to_string())),
                        });
                    }
                    ManualContextCompactionRequestStatus::Requested
                    | ManualContextCompactionRequestStatus::Consumed => {}
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }

        Ok(AgentRunContextCompactionRuntimeLaunchOutcome::Launched { turn_id })
    }
}

pub struct AgentRunContextCompactionRuntimeTodoPort;

#[async_trait]
impl AgentRunContextCompactionRuntimePort for AgentRunContextCompactionRuntimeTodoPort {
    async fn launch_compact_only_turn(
        &self,
        _command: AgentRunContextCompactionRuntimeLaunchCommand,
    ) -> Result<AgentRunContextCompactionRuntimeLaunchOutcome, WorkflowApplicationError> {
        Ok(
            AgentRunContextCompactionRuntimeLaunchOutcome::NotImplemented {
                message: "compact-only turn runtime launch 尚未接入".to_string(),
            },
        )
    }
}

#[derive(Clone, Copy)]
pub struct AgentRunContextCompactionCommandDeps<'a> {
    pub command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    pub compaction_request_repo: &'a dyn ManualContextCompactionRequestRepository,
    pub delivery_selection_repos: DeliveryRuntimeSelectionRepositories<'a>,
    pub runtime: &'a dyn AgentRunContextCompactionRuntimePort,
}

pub struct AgentRunContextCompactionCommandService<'a> {
    deps: AgentRunContextCompactionCommandDeps<'a>,
}

impl<'a> AgentRunContextCompactionCommandService<'a> {
    pub fn new(deps: AgentRunContextCompactionCommandDeps<'a>) -> Self {
        Self { deps }
    }

    pub async fn compact_context(
        &self,
        command: AgentRunContextCompactionCommand,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }

        let request_digest = digest_command_request(&ContextCompactionCommandDigest {
            kind: "agent_run_context_compaction",
            run_id: command.run_id,
            agent_id: command.agent_id,
        })?;
        let claim = claim_agent_run_command_receipt(
            self.deps.command_receipt_repo,
            "agent_run_mailbox",
            format!("{}:{}", command.run_id, command.agent_id),
            AgentRunCommandKind::ContextCompact,
            command.client_command_id,
            request_digest,
        )
        .await?;
        if claim.duplicate {
            return self.replay_duplicate(claim.record).await;
        }

        let decision =
            AgentRunRuntimeCommandFulfillmentService::new(self.deps.delivery_selection_repos)
                .decide_context_compaction(command.run_id, command.agent_id)
                .await?;
        match decision {
            AgentRunRuntimeCommandFulfillmentDecision::ScheduleForNextTurn {
                selection,
                active_turn_id,
            } => {
                self.schedule_for_next_turn(claim.record.id, selection, active_turn_id)
                    .await
            }
            AgentRunRuntimeCommandFulfillmentDecision::LaunchMaintenanceTurn { selection } => {
                self.launch_maintenance_turn(claim.record.id, selection)
                    .await
            }
            AgentRunRuntimeCommandFulfillmentDecision::Reject {
                selection,
                disabled_code,
                message,
            } => {
                self.reject(claim.record.id, selection.as_ref(), disabled_code, message)
                    .await
            }
        }
    }

    async fn schedule_for_next_turn(
        &self,
        receipt_id: Uuid,
        selection: DeliveryRuntimeSelection,
        active_turn_id: String,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        let request = match self
            .deps
            .compaction_request_repo
            .create_requested(NewManualContextCompactionRequest {
                session_id: selection.runtime_session_id.clone(),
                run_id: selection.run_id,
                agent_id: selection.agent_id,
                command_receipt_id: receipt_id,
                requested_mode: ManualContextCompactionRequestedMode::NextTurn,
                keep_last_n: None,
                reserve_tokens: None,
                request_metadata: Some(serde_json::json!({
                    "trigger": "manual",
                    "fulfillment": "schedule_for_next_turn",
                    "active_turn_id": active_turn_id,
                })),
            })
            .await
        {
            Ok(request) => request,
            Err(error) => {
                let workflow_error = WorkflowApplicationError::from(error);
                mark_command_terminal_failed(
                    self.deps.command_receipt_repo,
                    receipt_id,
                    &workflow_error,
                )
                .await;
                return Err(workflow_error);
            }
        };

        let stored = self
            .accept_and_store_result(
                receipt_id,
                &selection,
                None,
                StoredContextCompactionResult {
                    outcome: AgentRunContextCompactionOutcome::ScheduledNextTurn,
                    runtime_session_id: Some(selection.runtime_session_id.clone()),
                    request_id: Some(request.id.to_string()),
                    turn_id: None,
                    message: Some("已安排在当前 turn 结束后压缩上下文。".to_string()),
                    disabled_code: None,
                },
            )
            .await?;
        self.result_from_stored_record(stored, false)
    }

    async fn launch_maintenance_turn(
        &self,
        receipt_id: Uuid,
        selection: DeliveryRuntimeSelection,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        let request = match self
            .deps
            .compaction_request_repo
            .create_requested(NewManualContextCompactionRequest {
                session_id: selection.runtime_session_id.clone(),
                run_id: selection.run_id,
                agent_id: selection.agent_id,
                command_receipt_id: receipt_id,
                requested_mode: ManualContextCompactionRequestedMode::CompactOnly,
                keep_last_n: None,
                reserve_tokens: None,
                request_metadata: Some(serde_json::json!({
                    "trigger": "manual",
                    "fulfillment": "launch_maintenance_turn",
                })),
            })
            .await
        {
            Ok(request) => request,
            Err(error) => {
                let workflow_error = WorkflowApplicationError::from(error);
                mark_command_terminal_failed(
                    self.deps.command_receipt_repo,
                    receipt_id,
                    &workflow_error,
                )
                .await;
                return Err(workflow_error);
            }
        };

        let launch_outcome = match self
            .deps
            .runtime
            .launch_compact_only_turn(AgentRunContextCompactionRuntimeLaunchCommand {
                run_id: selection.run_id,
                agent_id: selection.agent_id,
                frame_id: selection.current_frame_id,
                runtime_session_id: selection.runtime_session_id.clone(),
                request_id: request.id,
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = self
                    .deps
                    .compaction_request_repo
                    .mark_failed(
                        request.id,
                        Some(serde_json::json!({
                            "outcome": "failed",
                            "reason": "runtime_launch_failed",
                            "message": error.to_string(),
                        })),
                    )
                    .await;
                return Err(error);
            }
        };

        match launch_outcome {
            AgentRunContextCompactionRuntimeLaunchOutcome::Launched { turn_id } => {
                let stored = self
                    .accept_and_store_result(
                        receipt_id,
                        &selection,
                        Some(turn_id.clone()),
                        StoredContextCompactionResult {
                            outcome: AgentRunContextCompactionOutcome::LaunchedCompactionTurn,
                            runtime_session_id: Some(selection.runtime_session_id.clone()),
                            request_id: Some(request.id.to_string()),
                            turn_id: Some(turn_id),
                            message: None,
                            disabled_code: None,
                        },
                    )
                    .await?;
                self.result_from_stored_record(stored, false)
            }
            AgentRunContextCompactionRuntimeLaunchOutcome::Completed { turn_id, message } => {
                let stored = self
                    .accept_and_store_result(
                        receipt_id,
                        &selection,
                        Some(turn_id.clone()),
                        StoredContextCompactionResult {
                            outcome: AgentRunContextCompactionOutcome::Completed,
                            runtime_session_id: Some(selection.runtime_session_id.clone()),
                            request_id: Some(request.id.to_string()),
                            turn_id: Some(turn_id),
                            message,
                            disabled_code: None,
                        },
                    )
                    .await?;
                self.result_from_stored_record(stored, false)
            }
            AgentRunContextCompactionRuntimeLaunchOutcome::NoEligibleMessages {
                turn_id,
                message,
            } => {
                let stored = self
                    .accept_and_store_result(
                        receipt_id,
                        &selection,
                        Some(turn_id.clone()),
                        StoredContextCompactionResult {
                            outcome: AgentRunContextCompactionOutcome::NoEligibleMessages,
                            runtime_session_id: Some(selection.runtime_session_id.clone()),
                            request_id: Some(request.id.to_string()),
                            turn_id: Some(turn_id),
                            message,
                            disabled_code: None,
                        },
                    )
                    .await?;
                self.result_from_stored_record(stored, false)
            }
            AgentRunContextCompactionRuntimeLaunchOutcome::Failed { turn_id, message } => {
                let stored = self
                    .accept_and_store_result(
                        receipt_id,
                        &selection,
                        Some(turn_id.clone()),
                        StoredContextCompactionResult {
                            outcome: AgentRunContextCompactionOutcome::Failed,
                            runtime_session_id: Some(selection.runtime_session_id.clone()),
                            request_id: Some(request.id.to_string()),
                            turn_id: Some(turn_id),
                            message,
                            disabled_code: None,
                        },
                    )
                    .await?;
                self.result_from_stored_record(stored, false)
            }
            AgentRunContextCompactionRuntimeLaunchOutcome::NotImplemented { message } => {
                let _ = self
                    .deps
                    .compaction_request_repo
                    .mark_failed(
                        request.id,
                        Some(serde_json::json!({
                            "outcome": "blocked",
                            "reason": "runtime_port_not_implemented",
                            "message": message,
                        })),
                    )
                    .await?;
                self.store_terminal_failed_result(
                    receipt_id,
                    StoredContextCompactionResult {
                        outcome: AgentRunContextCompactionOutcome::Blocked,
                        runtime_session_id: Some(selection.runtime_session_id.clone()),
                        request_id: Some(request.id.to_string()),
                        turn_id: None,
                        message: Some(message),
                        disabled_code: Some("runtime_port_not_implemented".to_string()),
                    },
                    false,
                )
                .await
            }
        }
    }

    async fn reject(
        &self,
        receipt_id: Uuid,
        selection: Option<&DeliveryRuntimeSelection>,
        disabled_code: String,
        message: String,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        self.store_terminal_failed_result(
            receipt_id,
            StoredContextCompactionResult {
                outcome: AgentRunContextCompactionOutcome::Blocked,
                runtime_session_id: selection.map(|selection| selection.runtime_session_id.clone()),
                request_id: None,
                turn_id: None,
                message: Some(message),
                disabled_code: Some(disabled_code),
            },
            false,
        )
        .await
    }

    async fn accept_and_store_result(
        &self,
        receipt_id: Uuid,
        selection: &DeliveryRuntimeSelection,
        turn_id: Option<String>,
        result: StoredContextCompactionResult,
    ) -> Result<AgentRunCommandReceipt, WorkflowApplicationError> {
        let accepted = self
            .deps
            .command_receipt_repo
            .mark_accepted(
                receipt_id,
                AgentRunAcceptedRefs {
                    run_id: selection.run_id,
                    agent_id: selection.agent_id,
                    frame_id: Some(selection.current_frame_id),
                    frame_revision: None,
                    runtime_session_id: Some(selection.runtime_session_id.clone()),
                    agent_run_turn_id: turn_id,
                    protocol_turn_id: None,
                },
            )
            .await?;
        let stored = self
            .deps
            .command_receipt_repo
            .store_result_json(
                receipt_id,
                serde_json::to_value(result)
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
            )
            .await?;
        Ok(if stored.updated_at >= accepted.updated_at {
            stored
        } else {
            accepted
        })
    }

    async fn store_terminal_failed_result(
        &self,
        receipt_id: Uuid,
        result: StoredContextCompactionResult,
        duplicate: bool,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        let failed = self
            .deps
            .command_receipt_repo
            .mark_terminal_failed(
                receipt_id,
                result
                    .message
                    .clone()
                    .unwrap_or_else(|| "context compact command rejected".to_string()),
            )
            .await?;
        let stored = self
            .deps
            .command_receipt_repo
            .store_result_json(
                receipt_id,
                serde_json::to_value(result)
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
            )
            .await?;
        self.result_from_stored_record(
            if stored.updated_at >= failed.updated_at {
                stored
            } else {
                failed
            },
            duplicate,
        )
    }

    async fn replay_duplicate(
        &self,
        record: AgentRunCommandReceipt,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        if record.status == AgentRunCommandStatus::Pending {
            return Err(WorkflowApplicationError::Conflict(
                "命令仍在处理中，请刷新 AgentRun workspace 获取最新状态".to_string(),
            ));
        }
        self.result_from_stored_record(record, true)
    }

    fn result_from_stored_record(
        &self,
        record: AgentRunCommandReceipt,
        duplicate: bool,
    ) -> Result<AgentRunContextCompactionCommandResult, WorkflowApplicationError> {
        let stored = record
            .result_json
            .clone()
            .map(serde_json::from_value::<StoredContextCompactionResult>)
            .transpose()
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let fallback_runtime_session_id = record
            .accepted_refs
            .as_ref()
            .and_then(|refs| refs.runtime_session_id.clone());
        let fallback_turn_id = record
            .accepted_refs
            .as_ref()
            .and_then(|refs| refs.agent_run_turn_id.clone());
        let fallback_outcome = if record.status == AgentRunCommandStatus::TerminalFailed {
            AgentRunContextCompactionOutcome::Failed
        } else {
            AgentRunContextCompactionOutcome::Blocked
        };
        Ok(AgentRunContextCompactionCommandResult {
            command_receipt: AgentRunCommandReceiptView::from_record(&record, duplicate),
            outcome: stored
                .as_ref()
                .map(|result| result.outcome)
                .unwrap_or(fallback_outcome),
            runtime_session_id: stored
                .as_ref()
                .and_then(|result| result.runtime_session_id.clone())
                .or(fallback_runtime_session_id),
            request_id: stored.as_ref().and_then(|result| result.request_id.clone()),
            turn_id: stored
                .as_ref()
                .and_then(|result| result.turn_id.clone())
                .or(fallback_turn_id),
            message: stored
                .as_ref()
                .and_then(|result| result.message.clone())
                .or(record.error_message),
        })
    }
}

#[derive(Serialize)]
struct ContextCompactionCommandDigest {
    kind: &'static str,
    run_id: Uuid,
    agent_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct StoredContextCompactionResult {
    outcome: AgentRunContextCompactionOutcome,
    runtime_session_id: Option<String>,
    request_id: Option<String>,
    turn_id: Option<String>,
    message: Option<String>,
    disabled_code: Option<String>,
}

fn request_result_message(
    request: &agentdash_domain::workflow::ManualContextCompactionRequest,
) -> Option<String> {
    let metadata = request.result_metadata.as_ref()?;
    for key in ["reason", "message", "error"] {
        if let Some(value) = metadata
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(value.to_string());
        }
    }
    None
}

fn workflow_error_from_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> WorkflowApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => WorkflowApplicationError::from(source),
        other => WorkflowApplicationError::Conflict(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentRunDeliveryBinding,
        AgentRunDeliveryBindingRepository, AgentSource, DeliveryBindingStatus, LifecycleAgent,
        LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
        ManualContextCompactionRequestStatus, ManualContextCompactionRequestedMode,
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_test_support::workflow::{
        MemoryAgentFrameRepository, MemoryAgentRunCommandReceiptRepository,
        MemoryAgentRunDeliveryBindingRepository, MemoryLifecycleAgentRepository,
        MemoryLifecycleRunRepository, MemoryManualContextCompactionRequestRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use chrono::Utc;

    use super::*;

    struct CountingRuntime {
        calls: AtomicUsize,
        outcome: tokio::sync::Mutex<AgentRunContextCompactionRuntimeLaunchOutcome>,
    }

    impl Default for CountingRuntime {
        fn default() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                outcome: tokio::sync::Mutex::new(
                    AgentRunContextCompactionRuntimeLaunchOutcome::Launched {
                        turn_id: "maintenance-turn-1".to_string(),
                    },
                ),
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentRunContextCompactionRuntimePort for CountingRuntime {
        async fn launch_compact_only_turn(
            &self,
            _command: AgentRunContextCompactionRuntimeLaunchCommand,
        ) -> Result<AgentRunContextCompactionRuntimeLaunchOutcome, WorkflowApplicationError>
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.outcome.lock().await.clone())
        }
    }

    struct Fixture {
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
        delivery_bindings: Arc<MemoryAgentRunDeliveryBindingRepository>,
        receipts: Arc<MemoryAgentRunCommandReceiptRepository>,
        requests: Arc<MemoryManualContextCompactionRequestRepository>,
        runtime: Arc<CountingRuntime>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                runs: Arc::new(MemoryLifecycleRunRepository::default()),
                agents: Arc::new(MemoryLifecycleAgentRepository::default()),
                frames: Arc::new(MemoryAgentFrameRepository::default()),
                anchors: Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default()),
                delivery_bindings: Arc::new(MemoryAgentRunDeliveryBindingRepository::default()),
                receipts: Arc::new(MemoryAgentRunCommandReceiptRepository::default()),
                requests: Arc::new(MemoryManualContextCompactionRequestRepository::default()),
                runtime: Arc::new(CountingRuntime::default()),
            }
        }

        fn service(&self) -> AgentRunContextCompactionCommandService<'_> {
            AgentRunContextCompactionCommandService::new(AgentRunContextCompactionCommandDeps {
                command_receipt_repo: self.receipts.as_ref(),
                compaction_request_repo: self.requests.as_ref(),
                delivery_selection_repos: DeliveryRuntimeSelectionRepositories {
                    lifecycle_runs: self.runs.as_ref(),
                    lifecycle_agents: self.agents.as_ref(),
                    agent_frames: self.frames.as_ref(),
                    execution_anchors: self.anchors.as_ref(),
                    delivery_bindings: self.delivery_bindings.as_ref(),
                },
                runtime: self.runtime.as_ref(),
            })
        }

        async fn seed_delivery(
            &self,
            binding: impl FnOnce(RuntimeSessionExecutionAnchor, &AgentFrame) -> AgentRunDeliveryBinding,
        ) -> (LifecycleRun, LifecycleAgent) {
            let run = LifecycleRun::new_plain(Uuid::new_v4());
            let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
            let launch_frame = AgentFrame::new_initial(agent.id);
            let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                "runtime-session-1",
                run.id,
                launch_frame.id,
                agent.id,
            );
            let delivery_binding = binding(anchor.clone(), &current_frame);

            self.runs.create(&run).await.expect("run");
            self.agents.create(&agent).await.expect("agent");
            self.frames
                .create(&launch_frame)
                .await
                .expect("launch frame");
            self.frames
                .create(&current_frame)
                .await
                .expect("current frame");
            self.anchors.create_once(&anchor).await.expect("anchor");
            self.delivery_bindings
                .upsert(&delivery_binding)
                .await
                .expect("delivery binding");

            (run, agent)
        }

        fn command(
            run: &LifecycleRun,
            agent: &LifecycleAgent,
            client_command_id: &str,
        ) -> AgentRunContextCompactionCommand {
            AgentRunContextCompactionCommand {
                run_id: run.id,
                agent_id: agent.id,
                client_command_id: client_command_id.to_string(),
            }
        }
    }

    #[tokio::test]
    async fn running_context_compact_creates_pending_next_turn_request() {
        let fixture = Fixture::new();
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
                .mark_running("turn-1", Utc::now())
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-1"))
            .await
            .expect("compact context");

        assert_eq!(
            result.outcome,
            AgentRunContextCompactionOutcome::ScheduledNextTurn
        );
        assert_eq!(fixture.runtime.calls.load(Ordering::SeqCst), 0);
        let requests = fixture.requests.debug_list().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].requested_mode,
            ManualContextCompactionRequestedMode::NextTurn
        );
        assert_eq!(
            requests[0].status,
            ManualContextCompactionRequestStatus::Requested
        );
    }

    #[tokio::test]
    async fn idle_context_compact_launches_compact_only_request() {
        let fixture = Fixture::new();
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-idle"))
            .await
            .expect("compact context");

        assert_eq!(
            result.outcome,
            AgentRunContextCompactionOutcome::LaunchedCompactionTurn
        );
        assert_eq!(fixture.runtime.calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.turn_id.as_deref(), Some("maintenance-turn-1"));
        let requests = fixture.requests.debug_list().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].requested_mode,
            ManualContextCompactionRequestedMode::CompactOnly
        );
        assert_eq!(
            requests[0].status,
            ManualContextCompactionRequestStatus::Requested
        );
    }

    #[tokio::test]
    async fn compact_only_noop_result_preserves_maintenance_turn_id() {
        let fixture = Fixture::new();
        *fixture.runtime.outcome.lock().await =
            AgentRunContextCompactionRuntimeLaunchOutcome::NoEligibleMessages {
                turn_id: "maintenance-turn-noop".to_string(),
                message: Some("no_eligible_messages".to_string()),
            };
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-noop"))
            .await
            .expect("compact context");

        assert_eq!(
            result.outcome,
            AgentRunContextCompactionOutcome::NoEligibleMessages
        );
        assert_eq!(result.turn_id.as_deref(), Some("maintenance-turn-noop"));
        assert_eq!(result.message.as_deref(), Some("no_eligible_messages"));
        assert_eq!(result.command_receipt.status, "accepted");
    }

    #[tokio::test]
    async fn compact_only_completed_result_preserves_maintenance_turn_id() {
        let fixture = Fixture::new();
        *fixture.runtime.outcome.lock().await =
            AgentRunContextCompactionRuntimeLaunchOutcome::Completed {
                turn_id: "maintenance-turn-completed".to_string(),
                message: Some("context compaction completed".to_string()),
            };
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-completed"))
            .await
            .expect("compact context");

        assert_eq!(result.outcome, AgentRunContextCompactionOutcome::Completed);
        assert_eq!(
            result.turn_id.as_deref(),
            Some("maintenance-turn-completed")
        );
        assert_eq!(
            result.message.as_deref(),
            Some("context compaction completed")
        );
        assert_eq!(result.command_receipt.status, "accepted");
    }

    #[tokio::test]
    async fn compact_only_failed_result_is_stored_with_maintenance_turn_id() {
        let fixture = Fixture::new();
        *fixture.runtime.outcome.lock().await =
            AgentRunContextCompactionRuntimeLaunchOutcome::Failed {
                turn_id: "maintenance-turn-failed".to_string(),
                message: Some("compaction_context_empty".to_string()),
            };
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-failed"))
            .await
            .expect("compact context");

        assert_eq!(result.outcome, AgentRunContextCompactionOutcome::Failed);
        assert_eq!(result.turn_id.as_deref(), Some("maintenance-turn-failed"));
        assert_eq!(result.message.as_deref(), Some("compaction_context_empty"));
        assert_eq!(result.command_receipt.status, "accepted");
    }

    #[tokio::test]
    async fn duplicate_context_compact_replays_receipt_without_second_request() {
        let fixture = Fixture::new();
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    Utc::now(),
                )
                .mark_running("turn-1", Utc::now())
            })
            .await;
        let command = Fixture::command(&run, &agent, "compact-duplicate");

        let first = fixture
            .service()
            .compact_context(command.clone())
            .await
            .expect("first compact");
        let duplicate = fixture
            .service()
            .compact_context(command)
            .await
            .expect("duplicate compact");

        assert!(!first.command_receipt.duplicate);
        assert!(duplicate.command_receipt.duplicate);
        assert_eq!(duplicate.outcome, first.outcome);
        assert_eq!(fixture.requests.debug_list().await.len(), 1);
    }

    #[tokio::test]
    async fn starting_context_compact_is_rejected_without_request() {
        let fixture = Fixture::new();
        let (run, agent) = fixture
            .seed_delivery(|anchor, _| {
                AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Running,
                    Utc::now(),
                )
            })
            .await;

        let result = fixture
            .service()
            .compact_context(Fixture::command(&run, &agent, "compact-starting"))
            .await
            .expect("starting compact returns receipt outcome");

        assert_eq!(result.outcome, AgentRunContextCompactionOutcome::Blocked);
        assert_eq!(result.command_receipt.status, "terminal_failed");
        assert_eq!(fixture.requests.debug_list().await.len(), 0);
    }
}
