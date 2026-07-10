use std::{collections::BTreeMap, sync::Arc, time::Duration};

use agentdash_agent_runtime_contract::{
    RuntimeBindingId, RuntimeDriverGeneration, RuntimeInteractionId, RuntimeItemId,
    RuntimeThreadId, RuntimeTurnId, ToolChannel, ToolSetRevision,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

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
pub struct InMemoryToolBrokerRepository {
    calls: Mutex<BTreeMap<RuntimeItemId, ToolBrokerCall>>,
}

#[async_trait]
impl ToolBrokerRepository for InMemoryToolBrokerRepository {
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

        let request = ToolExecutionRequest {
            idempotency_key: invocation.coordinates.item_id.clone(),
            invocation: invocation.clone(),
            credentials,
            cancellation: cancellation.clone(),
        };
        let execution = tokio::select! {
            _ = cancellation.cancelled() => ToolExecutionCompletion::Cancelled,
            result = tokio::time::timeout(
                Duration::from_millis(invocation.timeout_ms),
                self.executor.execute(request),
            ) => match result {
                Ok(result) => ToolExecutionCompletion::Finished(result),
                Err(_) => ToolExecutionCompletion::TimedOut,
            },
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
