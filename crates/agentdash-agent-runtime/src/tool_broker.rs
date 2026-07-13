use std::{collections::BTreeMap, sync::Arc, time::Duration};

use agentdash_agent_runtime_contract::{
    RuntimeBindingId, RuntimeDriverGeneration, RuntimeEvent, RuntimeInteractionId, RuntimeItemId,
    RuntimeItemTerminal, RuntimeThreadId, RuntimeTurnId, ToolChannel, ToolSetRevision,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{EntityPhase, RuntimeCommit, RuntimeRepository, RuntimeStoreError, RuntimeUnitOfWork};
use crate::{ToolCatalogRevision, ToolContribution};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallCoordinates {
    pub thread_id: RuntimeThreadId,
    pub turn_id: RuntimeTurnId,
    pub item_id: RuntimeItemId,
    pub binding_id: RuntimeBindingId,
    pub binding_generation: RuntimeDriverGeneration,
    pub tool_set_revision: ToolSetRevision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBrokerInvocation {
    pub coordinates: ToolCallCoordinates,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolBrokerCallStatus {
    Accepted,
    AwaitingApproval,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl ToolBrokerCallStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBrokerResult {
    pub output: serde_json::Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBrokerCall {
    pub invocation: ToolBrokerInvocation,
    pub invocation_digest: String,
    pub capability_key: String,
    pub tool_path: String,
    pub tool: ToolContribution,
    pub channel: ToolChannel,
    pub status: ToolBrokerCallStatus,
    /// The arguments acknowledged by the synchronous BeforeTool boundary. They are persisted
    /// before execution so a crashed Running call can be replayed with identical input.
    pub effective_arguments: Option<serde_json::Value>,
    pub pending_interaction_id: Option<RuntimeInteractionId>,
    pub result: Option<ToolBrokerResult>,
    pub terminal_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallAdmission {
    Accepted,
    Existing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBrokerTransition {
    pub expected: Vec<ToolBrokerCallStatus>,
    pub next: ToolBrokerCallStatus,
    pub effective_arguments: Option<serde_json::Value>,
    pub pending_interaction_id: Option<RuntimeInteractionId>,
    pub result: Option<ToolBrokerResult>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicyStage {
    Binding,
    Capability,
    Permission,
    Vfs,
    Hook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPolicyCheck {
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolGuardDecision {
    Allowed(ToolPolicyCheck),
    Denied { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermissionDecision {
    Allowed(ToolPolicyCheck),
    Denied {
        reason: String,
    },
    ApprovalRequired {
        interaction_id: RuntimeInteractionId,
        reason: String,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialMaterial {
    values: BTreeMap<String, String>,
}

impl CredentialMaterial {
    pub fn new(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }

    pub fn expose_to_local_executor(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

#[derive(Clone)]
pub struct ToolExecutionRequest {
    /// Canonical ToolCall Item identity. Executors must use this key to deduplicate retries.
    pub idempotency_key: RuntimeItemId,
    pub invocation: ToolBrokerInvocation,
    pub credentials: CredentialMaterial,
    pub cancellation: CancellationToken,
    pub updates: tokio::sync::mpsc::UnboundedSender<
        Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    >,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolBrokerOutcome {
    Terminal {
        status: ToolBrokerCallStatus,
        result: ToolBrokerResult,
        duplicate: bool,
    },
    ApprovalRequired {
        interaction_id: RuntimeInteractionId,
        reason: String,
    },
    Denied {
        stage: ToolPolicyStage,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolBrokerStoreError {
    #[error("tool broker store unavailable: {0}")]
    Unavailable(String),
    #[error("tool call transition conflict")]
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolBrokerError {
    #[error("tool `{0}` is not present in the bound catalog")]
    UnknownTool(String),
    #[error("tool `{tool}` does not support channel {channel:?}")]
    UnsupportedChannel { tool: String, channel: ToolChannel },
    #[error("tool call coordinates do not match the bound catalog or runtime binding")]
    StaleCoordinates,
    #[error("tool call id was reused with different immutable input")]
    IdempotencyConflict,
    #[error("tool timeout must be greater than zero")]
    InvalidTimeout,
    #[error("tool credentials could not be resolved: {0}")]
    Credential(String),
    #[error("tool executor failed: {0}")]
    Execution(String),
    #[error(transparent)]
    Store(#[from] ToolBrokerStoreError),
}

#[async_trait]
pub trait ToolBrokerRepository: Send + Sync {
    async fn load(
        &self,
        item_id: &RuntimeItemId,
    ) -> Result<Option<ToolBrokerCall>, ToolBrokerStoreError>;

    async fn accept(&self, call: ToolBrokerCall)
    -> Result<ToolCallAdmission, ToolBrokerStoreError>;

    /// Returns calls that a recovery worker can safely replay. Running execution is at-least-once
    /// and relies on the canonical Item idempotency key at the executor boundary.
    async fn recoverable(&self) -> Result<Vec<ToolBrokerCall>, ToolBrokerStoreError>;

    async fn transition(
        &self,
        item_id: &RuntimeItemId,
        transition: ToolBrokerTransition,
    ) -> Result<ToolBrokerCall, ToolBrokerStoreError>;
}

#[async_trait]
pub trait ToolBrokerRuntimeJournal: Send + Sync {
    /// Ensures the canonical ToolCall Item exists before broker acceptance and any side effect.
    async fn accept_tool_call(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
    ) -> Result<(), ToolBrokerError>;

    /// Converges the canonical ToolCall Item to the broker's durable terminal.
    async fn record_tool_terminal(&self, call: &ToolBrokerCall) -> Result<(), ToolBrokerError>;

    /// Ensures the canonical approval Interaction exists before the broker references it.
    async fn request_tool_approval(
        &self,
        invocation: &ToolBrokerInvocation,
        interaction_id: &RuntimeInteractionId,
        reason: &str,
    ) -> Result<(), ToolBrokerError>;

    async fn record_tool_update(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    ) -> Result<(), ToolBrokerError>;
}

/// Canonical Runtime journal used by production ToolBroker callbacks.
///
/// The Driver must already have committed `ItemStarted(ToolCall)` before entering the broker.
/// This adapter validates that authoritative fact, then owns terminal and approval interaction
/// convergence through the same Runtime projection/event UoW.
pub struct ManagedRuntimeToolJournal<S> {
    store: Arc<S>,
}

impl<S> ManagedRuntimeToolJournal<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl<S> ToolBrokerRuntimeJournal for ManagedRuntimeToolJournal<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + crate::RuntimeTransientEvents + 'static,
{
    async fn accept_tool_call(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
    ) -> Result<(), ToolBrokerError> {
        let initial_content = tool
            .project_started(
                invocation.coordinates.item_id.as_str(),
                invocation.arguments.clone(),
            )
            .map_err(|error| ToolBrokerError::Execution(error.to_string()))?;
        loop {
            let mut thread = self.load_matching_thread(invocation).await?;
            if let Some(item) = thread.items.get(&invocation.coordinates.item_id) {
                return if item.turn_id == invocation.coordinates.turn_id
                    && item.initial_content == initial_content
                {
                    Ok(())
                } else {
                    Err(ToolBrokerError::IdempotencyConflict)
                };
            }
            let expected = thread.revision;
            let recorded_at_ms = crate::model::current_time_ms();
            let events = thread
                .append_events([RuntimeEvent::ItemStarted {
                    turn_id: invocation.coordinates.turn_id.clone(),
                    item_id: invocation.coordinates.item_id.clone(),
                    initial_content: initial_content.clone(),
                }])
                .map_err(transition_tool_error)?;
            let mut records = crate::internal_journal_records(events).map_err(store_tool_error)?;
            if tool.presentation_emitter
                == agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker
            {
                records.push(
                    thread
                        .append_durable_fact(
                            agentdash_agent_runtime_contract::RuntimeJournalFact::Presentation(
                                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                                    agentdash_agent_runtime_contract::PresentationDurability::Durable,
                                    agentdash_agent_protocol::BackboneEvent::ItemStarted(
                                        agentdash_agent_protocol::ItemStartedNotification {
                                            item: initial_content.item().clone(),
                                            thread_id: thread.presentation_thread_id.to_string(),
                                            turn_id: invocation.coordinates.turn_id.to_string(),
                                            started_at_ms: timestamp_i64(recorded_at_ms),
                                        },
                                    ),
                                ),
                            ),
                            recorded_at_ms,
                            Some(invocation.coordinates.binding_id.clone()),
                            None,
                            tool_presentation_coordinate(invocation),
                        )
                        .map_err(transition_tool_error)?,
                );
            }
            match self
                .commit_projection_records_store_result(thread, expected, records)
                .await
            {
                Ok(()) => return Ok(()),
                Err(RuntimeStoreError::ProjectionConflict { .. }) => continue,
                Err(error) => return Err(store_tool_error(error)),
            }
        }
    }

    async fn record_tool_terminal(&self, call: &ToolBrokerCall) -> Result<(), ToolBrokerError> {
        let mut thread = self.load_matching_thread(&call.invocation).await?;
        let terminal = broker_terminal(call)?;
        let item = thread
            .items
            .get(&call.invocation.coordinates.item_id)
            .ok_or(ToolBrokerError::StaleCoordinates)?;
        if item.turn_id != call.invocation.coordinates.turn_id {
            return Err(ToolBrokerError::StaleCoordinates);
        }
        match &item.phase {
            EntityPhase::Terminal(existing) if existing == &terminal => return Ok(()),
            EntityPhase::Terminal(_) => return Err(ToolBrokerError::IdempotencyConflict),
            EntityPhase::Active => {}
        }
        let expected = thread.revision;
        let recorded_at_ms = crate::model::current_time_ms();
        let events = thread
            .append_events([RuntimeEvent::ItemTerminal {
                turn_id: call.invocation.coordinates.turn_id.clone(),
                item_id: call.invocation.coordinates.item_id.clone(),
                terminal: terminal.clone(),
            }])
            .map_err(transition_tool_error)?;
        let mut records = crate::internal_journal_records(events).map_err(store_tool_error)?;
        let final_content = match &terminal {
            RuntimeItemTerminal::Completed { final_content } => final_content.clone(),
            RuntimeItemTerminal::Failed { .. }
            | RuntimeItemTerminal::Cancelled { .. }
            | RuntimeItemTerminal::Lost { .. } => broker_terminal_content(call)?,
        };
        if call.tool.presentation_emitter
            == agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker
        {
            records.push(
                thread
                    .append_durable_fact(
                        agentdash_agent_runtime_contract::RuntimeJournalFact::Presentation(
                            agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                                agentdash_agent_runtime_contract::PresentationDurability::Durable,
                                agentdash_agent_protocol::BackboneEvent::ItemCompleted(
                                    agentdash_agent_protocol::ItemCompletedNotification {
                                        item: final_content.item().clone(),
                                        thread_id: thread.presentation_thread_id.to_string(),
                                        turn_id: call.invocation.coordinates.turn_id.to_string(),
                                        completed_at_ms: timestamp_i64(recorded_at_ms),
                                    },
                                ),
                            ),
                        ),
                        recorded_at_ms,
                        Some(call.invocation.coordinates.binding_id.clone()),
                        None,
                        tool_presentation_coordinate(&call.invocation),
                    )
                    .map_err(transition_tool_error)?,
            );
        }
        self.commit_projection_records(thread, expected, records)
            .await
    }

    async fn request_tool_approval(
        &self,
        invocation: &ToolBrokerInvocation,
        interaction_id: &RuntimeInteractionId,
        reason: &str,
    ) -> Result<(), ToolBrokerError> {
        let mut thread = self.load_matching_thread(invocation).await?;
        if thread.interactions.contains_key(interaction_id) {
            return Ok(());
        }
        let expected = thread.revision;
        let request = agentdash_agent_runtime_contract::RuntimeInteractionRequest::temporary_permission_approval(
            thread.thread_id.as_str(), invocation.coordinates.turn_id.as_str(),
            invocation.coordinates.item_id.as_str(), reason.to_string(),
        );
        let events = thread
            .append_events([RuntimeEvent::InteractionRequested {
                turn_id: invocation.coordinates.turn_id.clone(),
                item_id: Some(invocation.coordinates.item_id.clone()),
                interaction_id: interaction_id.clone(),
                request,
            }])
            .map_err(transition_tool_error)?;
        self.commit_projection(thread, expected, events).await
    }

    async fn record_tool_update(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    ) -> Result<(), ToolBrokerError> {
        if tool.presentation_emitter
            == agentdash_agent_runtime_contract::ToolPresentationEmitter::VendorStream
        {
            return Ok(());
        }
        let thread = self.load_matching_thread(invocation).await?;
        let event = if matches!(
            tool.protocol_projection,
            agentdash_agent_runtime_contract::ToolProtocolProjection::Command
        ) {
            let delta = content_items
                .into_iter()
                .filter_map(|item| match item {
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                        text,
                    } => Some(text),
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage {
                        ..
                    } => None,
                })
                .collect::<String>();
            agentdash_agent_protocol::BackboneEvent::CommandOutputDelta(
                agentdash_agent_protocol::codex_app_server_protocol::CommandExecutionOutputDeltaNotification {
                    thread_id: thread.presentation_thread_id.to_string(),
                    turn_id: invocation.coordinates.turn_id.to_string(),
                    item_id: invocation.coordinates.item_id.to_string(),
                    delta,
                },
            )
        } else {
            let item = tool
                .project_updated(
                    invocation.coordinates.item_id.as_str(),
                    invocation.arguments.clone(),
                    content_items,
                )
                .map_err(|error| ToolBrokerError::Execution(error.to_string()))?;
            agentdash_agent_protocol::BackboneEvent::ItemUpdated(
                agentdash_agent_protocol::ItemUpdatedNotification {
                    item: item.item().clone(),
                    thread_id: thread.presentation_thread_id.to_string(),
                    turn_id: invocation.coordinates.turn_id.to_string(),
                    updated_at_ms: timestamp_i64(crate::model::current_time_ms()),
                },
            )
        };
        self.store
            .publish_transient_presentation(
                thread.thread_id,
                invocation.coordinates.binding_id.clone(),
                invocation.coordinates.binding_generation,
                Some(invocation.coordinates.turn_id.clone()),
                thread.revision,
                tool_presentation_coordinate(invocation),
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    agentdash_agent_runtime_contract::PresentationDurability::Ephemeral,
                    event,
                ),
            )
            .await;
        Ok(())
    }
}

impl<S> ManagedRuntimeToolJournal<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork,
{
    async fn load_matching_thread(
        &self,
        invocation: &ToolBrokerInvocation,
    ) -> Result<crate::RuntimeThreadState, ToolBrokerError> {
        let thread = self
            .store
            .load_thread(&invocation.coordinates.thread_id)
            .await
            .map_err(store_tool_error)?
            .ok_or(ToolBrokerError::StaleCoordinates)?;
        if thread.binding_id != invocation.coordinates.binding_id
            || thread.driver_generation != invocation.coordinates.binding_generation
            || thread.tool_set_revision != invocation.coordinates.tool_set_revision
        {
            return Err(ToolBrokerError::StaleCoordinates);
        }
        Ok(thread)
    }

    async fn commit_projection(
        &self,
        projection: crate::RuntimeThreadState,
        expected_projection_revision: agentdash_agent_runtime_contract::RuntimeRevision,
        events: Vec<agentdash_agent_runtime_contract::RuntimeEventEnvelope>,
    ) -> Result<(), ToolBrokerError> {
        self.commit_projection_store_result(projection, expected_projection_revision, events)
            .await
            .map_err(store_tool_error)
    }

    async fn commit_projection_store_result(
        &self,
        projection: crate::RuntimeThreadState,
        expected_projection_revision: agentdash_agent_runtime_contract::RuntimeRevision,
        events: Vec<agentdash_agent_runtime_contract::RuntimeEventEnvelope>,
    ) -> Result<(), RuntimeStoreError> {
        self.commit_projection_records_store_result(
            projection,
            expected_projection_revision,
            crate::internal_journal_records(events)?,
        )
        .await
    }

    async fn commit_projection_records(
        &self,
        projection: crate::RuntimeThreadState,
        expected_projection_revision: agentdash_agent_runtime_contract::RuntimeRevision,
        records: Vec<agentdash_agent_runtime_contract::RuntimeJournalRecord>,
    ) -> Result<(), ToolBrokerError> {
        self.commit_projection_records_store_result(
            projection,
            expected_projection_revision,
            records,
        )
        .await
        .map_err(store_tool_error)
    }

    async fn commit_projection_records_store_result(
        &self,
        projection: crate::RuntimeThreadState,
        expected_projection_revision: agentdash_agent_runtime_contract::RuntimeRevision,
        records: Vec<agentdash_agent_runtime_contract::RuntimeJournalRecord>,
    ) -> Result<(), RuntimeStoreError> {
        self.store
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection,
                operation: None,
                operation_terminals: Vec::new(),
                records,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
    }
}

fn timestamp_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn tool_presentation_coordinate(
    invocation: &ToolBrokerInvocation,
) -> agentdash_agent_runtime_contract::RuntimePresentationCoordinate {
    agentdash_agent_runtime_contract::RuntimePresentationCoordinate {
        runtime_turn_id: Some(invocation.coordinates.turn_id.clone()),
        runtime_item_id: Some(invocation.coordinates.item_id.clone()),
        interaction_id: None,
        source_thread_id: None,
        source_turn_id: None,
        source_item_id: Some(invocation.coordinates.item_id.to_string()),
        source_request_id: None,
        source_entry_index: None,
    }
}

fn broker_terminal_content(
    call: &ToolBrokerCall,
) -> Result<agentdash_agent_runtime_contract::RuntimeItemContent, ToolBrokerError> {
    let result = call.result.as_ref().ok_or(ToolBrokerStoreError::Conflict)?;
    call.tool
        .project_completed(
            call.invocation.coordinates.item_id.as_str(),
            call.effective_arguments
                .clone()
                .unwrap_or_else(|| call.invocation.arguments.clone()),
            &result.output,
            call.status != ToolBrokerCallStatus::Completed,
        )
        .map_err(|error| ToolBrokerError::Execution(error.to_string()))
}

fn broker_terminal(call: &ToolBrokerCall) -> Result<RuntimeItemTerminal, ToolBrokerError> {
    let result = call.result.as_ref().ok_or(ToolBrokerStoreError::Conflict)?;
    Ok(match call.status {
        ToolBrokerCallStatus::Completed => RuntimeItemTerminal::Completed {
            final_content: call
                .tool
                .project_completed(
                    call.invocation.coordinates.item_id.as_str(),
                    call.effective_arguments
                        .clone()
                        .unwrap_or_else(|| call.invocation.arguments.clone()),
                    &result.output,
                    false,
                )
                .map_err(|error| ToolBrokerError::Execution(error.to_string()))?,
        },
        ToolBrokerCallStatus::Cancelled => RuntimeItemTerminal::Cancelled {
            message: call.terminal_message.clone(),
        },
        ToolBrokerCallStatus::Failed | ToolBrokerCallStatus::TimedOut => {
            RuntimeItemTerminal::Completed {
                final_content: call
                    .tool
                    .project_completed(
                        call.invocation.coordinates.item_id.as_str(),
                        call.effective_arguments
                            .clone()
                            .unwrap_or_else(|| call.invocation.arguments.clone()),
                        &result.output,
                        true,
                    )
                    .map_err(|error| ToolBrokerError::Execution(error.to_string()))?,
            }
        }
        ToolBrokerCallStatus::Accepted
        | ToolBrokerCallStatus::AwaitingApproval
        | ToolBrokerCallStatus::Running => return Err(ToolBrokerStoreError::Conflict.into()),
    })
}

fn transition_tool_error(error: crate::TransitionError) -> ToolBrokerError {
    ToolBrokerError::Execution(error.to_string())
}

fn store_tool_error(error: RuntimeStoreError) -> ToolBrokerError {
    ToolBrokerStoreError::Unavailable(error.to_string()).into()
}

#[async_trait]
pub trait ToolBrokerPolicyPort: Send + Sync {
    async fn validate_binding(
        &self,
        invocation: &ToolBrokerInvocation,
    ) -> Result<ToolGuardDecision, ToolBrokerError>;

    async fn authorize_capability(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
    ) -> Result<ToolGuardDecision, ToolBrokerError>;

    async fn authorize_permission(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
    ) -> Result<ToolPermissionDecision, ToolBrokerError>;

    async fn authorize_vfs(
        &self,
        invocation: &ToolBrokerInvocation,
        tool: &ToolContribution,
    ) -> Result<ToolGuardDecision, ToolBrokerError>;
}

#[async_trait]
pub trait ToolCredentialResolver: Send + Sync {
    async fn resolve(
        &self,
        credential_refs: &[String],
    ) -> Result<CredentialMaterial, ToolBrokerError>;
}

#[async_trait]
pub trait ToolExecutionPort: Send + Sync {
    async fn execute(
        &self,
        request: ToolExecutionRequest,
    ) -> Result<ToolBrokerResult, ToolBrokerError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolBrokerHookDecision {
    Continue {
        arguments: serde_json::Value,
    },
    Block {
        reason: String,
    },
    ApprovalRequired {
        interaction_id: RuntimeInteractionId,
        reason: String,
    },
}

#[async_trait]
pub trait ToolBrokerHookPort: Send + Sync {
    /// Implementations execute the selected ToolBroker-site definitions through the canonical
    /// Managed Runtime HookRun journal. Replays must converge by Item/Hook definition identity.
    async fn before_tool(
        &self,
        invocation: &ToolBrokerInvocation,
    ) -> Result<ToolBrokerHookDecision, ToolBrokerError>;

    async fn after_tool(
        &self,
        invocation: &ToolBrokerInvocation,
        result: ToolBrokerResult,
    ) -> Result<ToolBrokerResult, ToolBrokerError>;
}

#[derive(Debug, Default)]
pub struct ToolBrokerRepositoryFixture {
    calls: Mutex<BTreeMap<RuntimeItemId, ToolBrokerCall>>,
}

#[async_trait]
impl ToolBrokerRepository for ToolBrokerRepositoryFixture {
    async fn load(
        &self,
        item_id: &RuntimeItemId,
    ) -> Result<Option<ToolBrokerCall>, ToolBrokerStoreError> {
        Ok(self.calls.lock().await.get(item_id).cloned())
    }

    async fn accept(
        &self,
        call: ToolBrokerCall,
    ) -> Result<ToolCallAdmission, ToolBrokerStoreError> {
        let mut calls = self.calls.lock().await;
        if calls.contains_key(&call.invocation.coordinates.item_id) {
            return Ok(ToolCallAdmission::Existing);
        }
        calls.insert(call.invocation.coordinates.item_id.clone(), call);
        Ok(ToolCallAdmission::Accepted)
    }

    async fn recoverable(&self) -> Result<Vec<ToolBrokerCall>, ToolBrokerStoreError> {
        let mut calls = self
            .calls
            .lock()
            .await
            .values()
            .filter(|call| {
                matches!(
                    call.status,
                    ToolBrokerCallStatus::Accepted | ToolBrokerCallStatus::Running
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        calls.sort_by(|left, right| {
            left.invocation
                .coordinates
                .item_id
                .cmp(&right.invocation.coordinates.item_id)
        });
        Ok(calls)
    }

    async fn transition(
        &self,
        item_id: &RuntimeItemId,
        transition: ToolBrokerTransition,
    ) -> Result<ToolBrokerCall, ToolBrokerStoreError> {
        let ToolBrokerTransition {
            expected,
            next,
            effective_arguments,
            pending_interaction_id,
            result,
            message,
        } = transition;
        let mut calls = self.calls.lock().await;
        let call = calls
            .get_mut(item_id)
            .ok_or(ToolBrokerStoreError::Conflict)?;
        if call.status == next
            && call.effective_arguments == effective_arguments
            && call.pending_interaction_id == pending_interaction_id
            && call.result == result
            && call.terminal_message == message
        {
            return Ok(call.clone());
        }
        if !expected.contains(&call.status)
            || !valid_transition(call.status, next)
            || (call.status != ToolBrokerCallStatus::Accepted
                && call.effective_arguments != effective_arguments)
        {
            return Err(ToolBrokerStoreError::Conflict);
        }
        call.status = next;
        call.effective_arguments = effective_arguments;
        call.pending_interaction_id = pending_interaction_id;
        call.result = result;
        call.terminal_message = message;
        Ok(call.clone())
    }
}

#[derive(Clone)]
pub struct PlatformToolBroker {
    catalog: ToolCatalogRevision,
    binding_id: RuntimeBindingId,
    binding_generation: RuntimeDriverGeneration,
    repository: Arc<dyn ToolBrokerRepository>,
    journal: Arc<dyn ToolBrokerRuntimeJournal>,
    policy: Arc<dyn ToolBrokerPolicyPort>,
    credentials: Arc<dyn ToolCredentialResolver>,
    executor: Arc<dyn ToolExecutionPort>,
    hooks: Option<Arc<dyn ToolBrokerHookPort>>,
}

#[derive(Clone)]
pub struct PlatformToolBrokerDeps {
    pub repository: Arc<dyn ToolBrokerRepository>,
    pub journal: Arc<dyn ToolBrokerRuntimeJournal>,
    pub policy: Arc<dyn ToolBrokerPolicyPort>,
    pub credentials: Arc<dyn ToolCredentialResolver>,
    pub executor: Arc<dyn ToolExecutionPort>,
}

impl PlatformToolBroker {
    pub fn new(
        catalog: ToolCatalogRevision,
        binding_id: RuntimeBindingId,
        binding_generation: RuntimeDriverGeneration,
        deps: PlatformToolBrokerDeps,
    ) -> Self {
        Self {
            catalog,
            binding_id,
            binding_generation,
            repository: deps.repository,
            journal: deps.journal,
            policy: deps.policy,
            credentials: deps.credentials,
            executor: deps.executor,
            hooks: None,
        }
    }

    pub fn with_hooks(mut self, hooks: Arc<dyn ToolBrokerHookPort>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    pub fn published_tools(&self, channel: ToolChannel) -> Vec<PublishedToolSchema> {
        self.catalog
            .tools
            .iter()
            .filter(|tool| tool.allowed_channels.contains(&channel))
            .map(PublishedToolSchema::from)
            .collect()
    }

    pub async fn invoke(
        &self,
        channel: ToolChannel,
        invocation: ToolBrokerInvocation,
        cancellation: CancellationToken,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError> {
        if invocation.timeout_ms == 0 {
            return Err(ToolBrokerError::InvalidTimeout);
        }
        if invocation.coordinates.binding_id != self.binding_id
            || invocation.coordinates.binding_generation != self.binding_generation
            || invocation.coordinates.tool_set_revision != self.catalog.revision
        {
            return Err(ToolBrokerError::StaleCoordinates);
        }
        let tool = self
            .catalog
            .tools
            .iter()
            .find(|tool| tool.runtime_name == invocation.tool_name)
            .ok_or_else(|| ToolBrokerError::UnknownTool(invocation.tool_name.clone()))?;
        if !tool.allowed_channels.contains(&channel) {
            return Err(ToolBrokerError::UnsupportedChannel {
                tool: invocation.tool_name.clone(),
                channel,
            });
        }

        let invocation_digest = invocation_digest(&invocation, channel)?;
        self.journal.accept_tool_call(&invocation, tool).await?;
        let initial = ToolBrokerCall {
            invocation: invocation.clone(),
            invocation_digest: invocation_digest.clone(),
            capability_key: tool.capability_key.clone(),
            tool_path: tool.tool_path.clone(),
            tool: tool.clone(),
            channel,
            status: ToolBrokerCallStatus::Accepted,
            effective_arguments: None,
            pending_interaction_id: None,
            result: None,
            terminal_message: None,
        };
        self.repository.accept(initial).await?;
        let existing = self
            .repository
            .load(&invocation.coordinates.item_id)
            .await?
            .ok_or(ToolBrokerStoreError::Conflict)?;
        if existing.invocation_digest != invocation_digest
            || existing.channel != channel
            || existing.capability_key != tool.capability_key
            || existing.tool_path != tool.tool_path
        {
            return Err(ToolBrokerError::IdempotencyConflict);
        }
        if existing.status.is_terminal() {
            return self.terminal_outcome(existing, true).await;
        }
        if existing.status == ToolBrokerCallStatus::Running {
            return self.execute_running(existing, cancellation, true).await;
        }

        let mut invocation = invocation;
        if let ToolGuardDecision::Denied { reason } =
            self.policy.validate_binding(&invocation).await?
        {
            return self
                .persist_denial(&invocation, ToolPolicyStage::Binding, reason)
                .await;
        }
        if let ToolGuardDecision::Denied { reason } =
            self.policy.authorize_capability(&invocation, tool).await?
        {
            return self
                .persist_denial(&invocation, ToolPolicyStage::Capability, reason)
                .await;
        }
        if let Some(hooks) = &self.hooks {
            match hooks.before_tool(&invocation).await? {
                ToolBrokerHookDecision::Continue { arguments } => {
                    invocation.arguments = arguments;
                }
                ToolBrokerHookDecision::Block { reason } => {
                    return self
                        .persist_denial(&invocation, ToolPolicyStage::Hook, reason)
                        .await;
                }
                ToolBrokerHookDecision::ApprovalRequired {
                    interaction_id,
                    reason,
                } => {
                    if existing.status == ToolBrokerCallStatus::AwaitingApproval
                        && (existing.pending_interaction_id.as_ref() != Some(&interaction_id)
                            || existing.effective_arguments.as_ref() != Some(&invocation.arguments))
                    {
                        return Err(ToolBrokerError::IdempotencyConflict);
                    }
                    self.journal
                        .request_tool_approval(&invocation, &interaction_id, &reason)
                        .await?;
                    if existing.status == ToolBrokerCallStatus::Accepted {
                        self.repository
                            .transition(
                                &invocation.coordinates.item_id,
                                ToolBrokerTransition {
                                    expected: vec![ToolBrokerCallStatus::Accepted],
                                    next: ToolBrokerCallStatus::AwaitingApproval,
                                    effective_arguments: Some(invocation.arguments.clone()),
                                    pending_interaction_id: Some(interaction_id.clone()),
                                    result: None,
                                    message: Some(reason.clone()),
                                },
                            )
                            .await?;
                    }
                    return Ok(ToolBrokerOutcome::ApprovalRequired {
                        interaction_id,
                        reason,
                    });
                }
            }
        }
        match self.policy.authorize_permission(&invocation, tool).await? {
            ToolPermissionDecision::Denied { reason } => {
                return self
                    .persist_denial(&invocation, ToolPolicyStage::Permission, reason)
                    .await;
            }
            ToolPermissionDecision::ApprovalRequired {
                interaction_id,
                reason,
            } => {
                if existing.status == ToolBrokerCallStatus::AwaitingApproval
                    && (existing.pending_interaction_id.as_ref() != Some(&interaction_id)
                        || existing.effective_arguments.as_ref() != Some(&invocation.arguments))
                {
                    return Err(ToolBrokerError::IdempotencyConflict);
                }
                self.journal
                    .request_tool_approval(&invocation, &interaction_id, &reason)
                    .await?;
                if existing.status == ToolBrokerCallStatus::Accepted {
                    self.repository
                        .transition(
                            &invocation.coordinates.item_id,
                            ToolBrokerTransition {
                                expected: vec![ToolBrokerCallStatus::Accepted],
                                next: ToolBrokerCallStatus::AwaitingApproval,
                                effective_arguments: Some(invocation.arguments.clone()),
                                pending_interaction_id: Some(interaction_id.clone()),
                                result: None,
                                message: Some(reason.clone()),
                            },
                        )
                        .await?;
                }
                return Ok(ToolBrokerOutcome::ApprovalRequired {
                    interaction_id,
                    reason,
                });
            }
            ToolPermissionDecision::Allowed(_) => {}
        }
        if let ToolGuardDecision::Denied { reason } =
            self.policy.authorize_vfs(&invocation, tool).await?
        {
            return self
                .persist_denial(&invocation, ToolPolicyStage::Vfs, reason)
                .await;
        }

        self.repository
            .transition(
                &invocation.coordinates.item_id,
                ToolBrokerTransition {
                    expected: vec![
                        ToolBrokerCallStatus::Accepted,
                        ToolBrokerCallStatus::AwaitingApproval,
                    ],
                    next: ToolBrokerCallStatus::Running,
                    effective_arguments: Some(invocation.arguments.clone()),
                    pending_interaction_id: None,
                    result: None,
                    message: None,
                },
            )
            .await?;
        let running = self
            .repository
            .load(&invocation.coordinates.item_id)
            .await?
            .ok_or(ToolBrokerStoreError::Conflict)?;
        self.execute_running(running, cancellation, false).await
    }

    async fn execute_running(
        &self,
        call: ToolBrokerCall,
        cancellation: CancellationToken,
        duplicate: bool,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError> {
        if call.status != ToolBrokerCallStatus::Running {
            return Err(ToolBrokerStoreError::Conflict.into());
        }
        let mut invocation = call.invocation.clone();
        invocation.arguments = call
            .effective_arguments
            .clone()
            .ok_or(ToolBrokerStoreError::Conflict)?;
        let tool = self
            .catalog
            .tools
            .iter()
            .find(|tool| tool.runtime_name == invocation.tool_name)
            .ok_or_else(|| ToolBrokerError::UnknownTool(invocation.tool_name.clone()))?;
        let credential_refs = self
            .catalog
            .mcp_servers
            .iter()
            .filter(|server| tool.capability_key == server.server_key)
            .flat_map(|server| server.credential_refs.iter().cloned())
            .collect::<Vec<_>>();
        let credentials = self.credentials.resolve(&credential_refs).await?;

        if cancellation.is_cancelled() {
            let result = self
                .apply_after_tool_hook(&invocation, cancelled_result())
                .await?;
            let terminal = self
                .persist_running_terminal(
                    &invocation.coordinates.item_id,
                    ToolBrokerCallStatus::Cancelled,
                    call.effective_arguments.clone(),
                    result,
                    Some("cancelled before execution".to_string()),
                )
                .await?;
            return self.terminal_outcome(terminal, duplicate).await;
        }

        let (update_sender, mut update_receiver) = tokio::sync::mpsc::unbounded_channel();
        let request = ToolExecutionRequest {
            idempotency_key: invocation.coordinates.item_id.clone(),
            invocation: invocation.clone(),
            credentials,
            cancellation: cancellation.clone(),
            updates: update_sender,
        };
        let execution_future = tokio::time::timeout(
            Duration::from_millis(invocation.timeout_ms),
            self.executor.execute(request),
        );
        tokio::pin!(execution_future);
        let execution = loop {
            tokio::select! {
                _ = cancellation.cancelled() => break ToolExecutionCompletion::Cancelled,
                result = &mut execution_future => break match result { Ok(result) => ToolExecutionCompletion::Finished(result), Err(_) => ToolExecutionCompletion::TimedOut },
                Some(content_items) = update_receiver.recv() => self.journal.record_tool_update(&invocation, tool, content_items).await?,
            }
        };
        let execution = match execution {
            ToolExecutionCompletion::Finished(Ok(result)) => (None, result, None),
            ToolExecutionCompletion::Finished(Err(error)) => (
                Some(ToolBrokerCallStatus::Failed),
                ToolBrokerResult {
                    output: serde_json::json!({"error": error.to_string()}),
                    is_error: true,
                },
                Some(error.to_string()),
            ),
            ToolExecutionCompletion::TimedOut => (
                Some(ToolBrokerCallStatus::TimedOut),
                ToolBrokerResult {
                    output: serde_json::json!({"error": "tool execution timed out"}),
                    is_error: true,
                },
                Some("tool execution timed out".to_string()),
            ),
            ToolExecutionCompletion::Cancelled => {
                let result = self
                    .apply_after_tool_hook(&invocation, cancelled_result())
                    .await?;
                let terminal = self
                    .persist_running_terminal(
                        &invocation.coordinates.item_id,
                        ToolBrokerCallStatus::Cancelled,
                        call.effective_arguments.clone(),
                        result,
                        Some("tool execution cancelled".to_string()),
                    )
                    .await?;
                return self.terminal_outcome(terminal, duplicate).await;
            }
        };
        let (forced_status, result, message) = execution;
        let result = self.apply_after_tool_hook(&invocation, result).await?;
        let status = forced_status.unwrap_or(if result.is_error {
            ToolBrokerCallStatus::Failed
        } else {
            ToolBrokerCallStatus::Completed
        });
        let message = message.or_else(|| {
            (status == ToolBrokerCallStatus::Failed)
                .then(|| "tool returned an error result".to_string())
        });
        let terminal = self
            .persist_running_terminal(
                &invocation.coordinates.item_id,
                status,
                call.effective_arguments,
                result,
                message,
            )
            .await?;
        self.terminal_outcome(terminal, duplicate).await
    }

    async fn apply_after_tool_hook(
        &self,
        invocation: &ToolBrokerInvocation,
        result: ToolBrokerResult,
    ) -> Result<ToolBrokerResult, ToolBrokerError> {
        match &self.hooks {
            Some(hooks) => hooks.after_tool(invocation, result).await,
            None => Ok(result),
        }
    }

    async fn persist_running_terminal(
        &self,
        item_id: &RuntimeItemId,
        status: ToolBrokerCallStatus,
        effective_arguments: Option<serde_json::Value>,
        result: ToolBrokerResult,
        message: Option<String>,
    ) -> Result<ToolBrokerCall, ToolBrokerError> {
        match self
            .repository
            .transition(
                item_id,
                ToolBrokerTransition {
                    expected: vec![ToolBrokerCallStatus::Running],
                    next: status,
                    effective_arguments,
                    pending_interaction_id: None,
                    result: Some(result),
                    message,
                },
            )
            .await
        {
            Ok(terminal) => Ok(terminal),
            Err(ToolBrokerStoreError::Conflict) => self
                .repository
                .load(item_id)
                .await?
                .filter(|call| call.status.is_terminal())
                .ok_or(ToolBrokerStoreError::Conflict.into()),
            Err(error) => Err(error.into()),
        }
    }

    async fn persist_denial(
        &self,
        invocation: &ToolBrokerInvocation,
        stage: ToolPolicyStage,
        reason: String,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError> {
        let result = ToolBrokerResult {
            output: serde_json::json!({"error": reason}),
            is_error: true,
        };
        let terminal = self
            .repository
            .transition(
                &invocation.coordinates.item_id,
                ToolBrokerTransition {
                    expected: vec![
                        ToolBrokerCallStatus::Accepted,
                        ToolBrokerCallStatus::AwaitingApproval,
                    ],
                    next: ToolBrokerCallStatus::Failed,
                    effective_arguments: Some(invocation.arguments.clone()),
                    pending_interaction_id: None,
                    result: Some(result),
                    message: Some(reason.clone()),
                },
            )
            .await?;
        self.journal.record_tool_terminal(&terminal).await?;
        Ok(ToolBrokerOutcome::Denied { stage, reason })
    }

    async fn terminal_outcome(
        &self,
        call: ToolBrokerCall,
        duplicate: bool,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError> {
        self.journal.record_tool_terminal(&call).await?;
        let result = call.result.ok_or(ToolBrokerStoreError::Conflict)?;
        Ok(ToolBrokerOutcome::Terminal {
            status: call.status,
            result,
            duplicate,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishedToolSchema {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub capability_key: String,
    pub tool_path: String,
}

impl From<&ToolContribution> for PublishedToolSchema {
    fn from(tool: &ToolContribution) -> Self {
        Self {
            name: tool.runtime_name.clone(),
            description: tool.description.clone(),
            parameters_schema: tool.parameters_schema.clone(),
            capability_key: tool.capability_key.clone(),
            tool_path: tool.tool_path.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SessionToolMcpFacade {
    broker: PlatformToolBroker,
    thread_id: RuntimeThreadId,
    turn_id: RuntimeTurnId,
}

impl SessionToolMcpFacade {
    pub fn new(
        broker: PlatformToolBroker,
        thread_id: RuntimeThreadId,
        turn_id: RuntimeTurnId,
    ) -> Self {
        Self {
            broker,
            thread_id,
            turn_id,
        }
    }

    pub fn list_tools(&self) -> Vec<PublishedToolSchema> {
        self.broker.published_tools(ToolChannel::McpFacade)
    }

    pub async fn call(
        &self,
        item_id: RuntimeItemId,
        name: String,
        arguments: serde_json::Value,
        timeout_ms: u64,
        cancellation: CancellationToken,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError> {
        self.broker
            .invoke(
                ToolChannel::McpFacade,
                ToolBrokerInvocation {
                    coordinates: ToolCallCoordinates {
                        thread_id: self.thread_id.clone(),
                        turn_id: self.turn_id.clone(),
                        item_id,
                        binding_id: self.broker.binding_id.clone(),
                        binding_generation: self.broker.binding_generation,
                        tool_set_revision: self.broker.catalog.revision,
                    },
                    tool_name: name,
                    arguments,
                    timeout_ms,
                },
                cancellation,
            )
            .await
    }
}

fn invocation_digest(
    invocation: &ToolBrokerInvocation,
    channel: ToolChannel,
) -> Result<String, ToolBrokerError> {
    let value = serde_json::to_value((invocation, channel))
        .map_err(|error| ToolBrokerError::Execution(error.to_string()))?;
    Ok(crate::hook_effect_payload_digest(&value))
}

fn cancelled_result() -> ToolBrokerResult {
    ToolBrokerResult {
        output: serde_json::json!({"error": "tool execution cancelled"}),
        is_error: true,
    }
}

enum ToolExecutionCompletion {
    Finished(Result<ToolBrokerResult, ToolBrokerError>),
    TimedOut,
    Cancelled,
}

fn valid_transition(current: ToolBrokerCallStatus, next: ToolBrokerCallStatus) -> bool {
    use ToolBrokerCallStatus::{
        Accepted, AwaitingApproval, Cancelled, Completed, Failed, Running, TimedOut,
    };
    matches!(
        (current, next),
        (Accepted, AwaitingApproval | Running | Failed | Cancelled)
            | (AwaitingApproval, Running | Failed | Cancelled)
            | (Running, Completed | Failed | Cancelled | TimedOut)
    )
}
