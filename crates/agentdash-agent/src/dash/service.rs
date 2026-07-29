use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentTurnId, CommandId, CommandOutcome,
    CommandStatus, CompactionId, CompactionMode, ContextRevision, DashAgentChange, DashAgentCommit,
    DashAgentStore, DashCancellation, DashCommand, DashCommandKind, DashCoreContext, DashCoreError,
    DashCoreEvent, DashCoreTurn, DashExecutionCallbacks, DashExecutionEvent,
    DashExecutionInspection, DashFinishReason, DashMessage, DashMessageRole, DashProvider,
    DashProviderRequest, DashProviderRoundMaterializer, DashProviderRoundSnapshots, DashSurface,
    DashToolCall, DashToolCallbacks, DashToolDefinition, DashToolResult, EffectId, EffectOutcome,
    EffectSettlement, ForkCutoff, HistoryContribution, HistoryEntryId, HistoryPayload,
    InitialContextInstallation, InteractionId, ItemKind, SessionStatus, StoreError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashTerminalOutcome {
    Succeeded,
    Failed,
    Interrupted,
    Closed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashReceiptState {
    Accepted,
    Terminal(DashTerminalOutcome),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashCommandReceipt {
    pub command_id: CommandId,
    pub effect_id: EffectId,
    pub state: DashReceiptState,
    pub history_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashPublicCommand {
    SubmitInput {
        content: String,
    },
    Steer {
        turn_id: AgentTurnId,
        content: String,
    },
    Interrupt {
        turn_id: AgentTurnId,
    },
    RequestCompaction {
        mode: CompactionMode,
    },
    ResolveInteraction {
        interaction_id: InteractionId,
        response: String,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashCommandRequest {
    pub command_id: CommandId,
    pub effect_id: EffectId,
    pub command: DashPublicCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashEffectInspection {
    pub command_id: CommandId,
    pub effect_id: EffectId,
    pub state: DashReceiptState,
    pub retryable: bool,
    pub execution: DashExecutionInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DashEffectRecord {
    request: DashCommandRequest,
    receipt: DashCommandReceipt,
    retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DashActiveExecutionState {
    turn_id: AgentTurnId,
    request: DashCommandRequest,
    lease: Option<DashWorkerLease>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DashWorkerLease {
    owner_id: String,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashAgentRepositoryState {
    store: DashAgentStore,
    effects: BTreeMap<EffectId, DashEffectRecord>,
    active: Option<DashActiveExecutionState>,
}

impl DashAgentRepositoryState {
    pub fn history(&self) -> &AgentHistory {
        self.store.history()
    }

    pub fn store(&self) -> &DashAgentStore {
        &self.store
    }

    pub fn service_effect_ids(&self) -> impl Iterator<Item = &EffectId> {
        self.effects.keys()
    }

    pub fn new(store: DashAgentStore) -> Self {
        Self {
            store,
            effects: BTreeMap::new(),
            active: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashAgentRead {
    pub state: AgentHistoryState,
    pub history: AgentHistory,
    pub history_digest: String,
    pub surface: Option<DashSurface>,
    pub context_recipe: DashContextRecipe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashContextRecipeMessage {
    pub source_entry_id: HistoryEntryId,
    pub message: DashMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashContextRecipe {
    pub snapshot_revision: u64,
    pub context_revision: Option<ContextRevision>,
    pub frames: Vec<agentdash_agent_protocol::ContextFrame>,
    pub messages: Vec<DashContextRecipeMessage>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashAgentChanges {
    pub changes: Vec<DashAgentChange>,
    pub history: AgentHistory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashCompactionRequest {
    pub compaction_id: CompactionId,
    pub mode: CompactionMode,
    pub source_head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub context: DashCoreContext,
    pub message_entry_ids: Vec<HistoryEntryId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashCompactionResult {
    pub revision: ContextRevision,
    pub summary: String,
    pub retained_from: Option<HistoryEntryId>,
}

#[async_trait]
pub trait DashCompactor: Send + Sync {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashConversationNamingRequest {
    pub messages: Vec<DashMessage>,
}

#[async_trait]
pub trait DashConversationNamer: Send + Sync {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopDashConversationNamer;

#[async_trait]
impl DashConversationNamer for NoopDashConversationNamer {
    async fn generate(
        &self,
        _request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError> {
        Err(DashServiceError::Unavailable {
            message: "Dash conversation naming is not configured".to_owned(),
            retryable: false,
        })
    }
}

#[derive(Clone)]
pub struct DashExecutionDependencies {
    pub provider: Arc<dyn DashProvider>,
    pub tools: Arc<dyn DashToolCallbacks>,
    pub callbacks: Arc<dyn DashExecutionCallbacks>,
    pub history_callbacks: Arc<dyn DashHistoryCallbacks>,
    pub compactor: Arc<dyn DashCompactor>,
    pub conversation_namer: Arc<dyn DashConversationNamer>,
}

type DashToolInvocationKey = (String, String);

struct RoutableDashToolCallbacks {
    current: tokio::sync::RwLock<Arc<dyn DashToolCallbacks>>,
    admitted: tokio::sync::Mutex<HashMap<DashToolInvocationKey, Arc<dyn DashToolCallbacks>>>,
}

impl RoutableDashToolCallbacks {
    fn new(current: Arc<dyn DashToolCallbacks>) -> Self {
        Self {
            current: tokio::sync::RwLock::new(current),
            admitted: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn replace(&self, replacement: Arc<dyn DashToolCallbacks>) {
        *self.current.write().await = replacement;
    }

    async fn current(&self) -> Arc<dyn DashToolCallbacks> {
        self.current.read().await.clone()
    }

    async fn clear_turn(&self, turn_id: &AgentTurnId) {
        self.admitted
            .lock()
            .await
            .retain(|(admitted_turn_id, _), _| admitted_turn_id != &turn_id.0);
    }

    fn key(turn_id: &AgentTurnId, call_id: &str) -> DashToolInvocationKey {
        (turn_id.0.clone(), call_id.to_owned())
    }
}

#[async_trait]
impl DashToolCallbacks for RoutableDashToolCallbacks {
    async fn before_tool(
        &self,
        turn_id: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<super::DashBeforeToolDecision, DashCoreError> {
        let admitted = self.current().await;
        match admitted.before_tool(turn_id, call).await? {
            super::DashBeforeToolDecision::Invoke { call } => {
                self.admitted
                    .lock()
                    .await
                    .insert(Self::key(turn_id, &call.call_id), admitted);
                Ok(super::DashBeforeToolDecision::Invoke { call })
            }
            decision @ super::DashBeforeToolDecision::Deny { .. } => Ok(decision),
        }
    }

    async fn invoke(
        &self,
        turn_id: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        let key = Self::key(turn_id, &call.call_id);
        let admitted = self
            .admitted
            .lock()
            .await
            .get(&key)
            .cloned()
            .unwrap_or(self.current().await);
        let result = admitted.invoke(turn_id, call).await;
        if result.is_err() {
            self.admitted.lock().await.remove(&key);
        }
        result
    }

    async fn after_tool(
        &self,
        turn_id: &AgentTurnId,
        call: &DashToolCall,
        result: DashToolResult,
    ) -> Result<DashToolResult, DashCoreError> {
        let key = Self::key(turn_id, &call.call_id);
        let admitted = self
            .admitted
            .lock()
            .await
            .remove(&key)
            .unwrap_or(self.current().await);
        admitted.after_tool(turn_id, call, result).await
    }
}

#[derive(Debug, Clone)]
pub struct DashHistoryCommit {
    pub history: AgentHistory,
    pub entries: Vec<AgentHistoryEntry>,
}

#[async_trait]
pub trait DashHistoryCallbacks: Send + Sync {
    async fn committed(&self, commit: DashHistoryCommit) -> Result<(), DashCoreError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopDashHistoryCallbacks;

#[async_trait]
impl DashHistoryCallbacks for NoopDashHistoryCallbacks {
    async fn committed(&self, _commit: DashHistoryCommit) -> Result<(), DashCoreError> {
        Ok(())
    }
}

#[async_trait]
pub trait DashAgentRepository: Send + Sync {
    async fn initialize(&self, initial: DashAgentRepositoryState) -> Result<(), DashServiceError>;

    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError>;

    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError>;
}

#[async_trait]
pub trait DashAgentRepositoryStore: Send + Sync {
    async fn create(
        &self,
        source: &super::AgentSessionId,
        initial: DashAgentRepositoryState,
    ) -> Result<Arc<dyn DashAgentRepository>, DashServiceError>;

    async fn open(
        &self,
        source: &super::AgentSessionId,
    ) -> Result<Option<Arc<dyn DashAgentRepository>>, DashServiceError>;
}

#[derive(Clone)]
pub struct DashAgentService {
    repository: Arc<dyn DashAgentRepository>,
    execution: Arc<tokio::sync::RwLock<DashExecutionDependencies>>,
    tool_callbacks: Arc<RoutableDashToolCallbacks>,
    cancellation: Arc<tokio::sync::Mutex<Option<(AgentTurnId, DashCancellation)>>>,
    steering: Arc<tokio::sync::Mutex<DashSteeringState>>,
    effect_updates: Arc<tokio::sync::Notify>,
    worker_owner_id: Arc<str>,
}

const DASH_WORKER_LEASE_MS: u64 = 15_000;
const DASH_WORKER_HEARTBEAT_MS: u64 = 5_000;
static DASH_WORKER_OWNER_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct PendingProviderRound {
    assistant_text: String,
    tool_calls: Vec<DashToolCall>,
}

#[derive(Default)]
struct DashSteeringState {
    active_turn: Option<AgentTurnId>,
    after_sequence: u64,
}

enum DashPromotedExecution {
    Submit {
        request: DashCommandRequest,
        content: String,
        turn_id: AgentTurnId,
        effect_prefix: String,
    },
    Compaction {
        request: DashCommandRequest,
        compaction_id: CompactionId,
        mode: CompactionMode,
        effect_prefix: String,
    },
}

struct DurableDashExecutionCallbacks {
    service: DashAgentService,
    downstream: Arc<dyn DashExecutionCallbacks>,
    round_snapshots: DashProviderRoundSnapshots,
    rounds: tokio::sync::Mutex<BTreeMap<u32, PendingProviderRound>>,
}

impl DurableDashExecutionCallbacks {
    fn new(
        service: DashAgentService,
        downstream: Arc<dyn DashExecutionCallbacks>,
        round_snapshots: DashProviderRoundSnapshots,
    ) -> Self {
        Self {
            service,
            downstream,
            round_snapshots,
            rounds: tokio::sync::Mutex::new(BTreeMap::new()),
        }
    }

    async fn commit_history(&self, history: Vec<HistoryContribution>) -> Result<(), DashCoreError> {
        if history.is_empty() {
            return Ok(());
        }
        self.service
            .update_store(|store| {
                store.commit(DashAgentCommit {
                    expected_head: store.history().head().cloned(),
                    command_settlement: None,
                    effect_settlements: Vec::new(),
                    history,
                    enqueue_commands: Vec::new(),
                })?;
                Ok(())
            })
            .await
            .map(|_| ())
            .map_err(|error| DashCoreError::Callback {
                message: error.to_string(),
            })
    }

    async fn commit_provider_round(
        &self,
        turn_id: &AgentTurnId,
        round: u32,
        finish_reason: DashFinishReason,
        input_tokens: u64,
        output_tokens: u64,
        context_window: u64,
    ) -> Result<(), DashCoreError> {
        let pending = self.rounds.lock().await.remove(&round).unwrap_or_default();
        let mut history = vec![HistoryContribution {
            entry_id: provider_round_entry_id(turn_id, round, "usage", "confirmed"),
            payload: HistoryPayload::ProviderUsageConfirmed {
                turn_id: turn_id.clone(),
                round,
                input_tokens,
                output_tokens,
                context_window,
            },
        }];
        if finish_reason == DashFinishReason::Stop && pending.tool_calls.is_empty() {
            return self.commit_history(history).await;
        }
        if !pending.assistant_text.is_empty() {
            history.extend(provider_round_assistant_history(
                turn_id,
                round,
                pending.assistant_text,
            ));
        }
        for call in pending.tool_calls {
            let item_id = super::execution_tool_item_id(turn_id, &call.call_id);
            let projector = self
                .round_snapshots
                .tool_projector(round, &call.name)
                .ok_or_else(|| DashCoreError::Callback {
                    message: format!(
                        "executed Dash tool `{}` has no accepted protocol projector",
                        call.name
                    ),
                })?;
            history.extend([
                HistoryContribution {
                    entry_id: provider_round_entry_id(turn_id, round, &call.call_id, "start"),
                    payload: HistoryPayload::ItemStarted {
                        turn_id: turn_id.clone(),
                        item_id: item_id.clone(),
                        kind: ItemKind::ToolCall,
                    },
                },
                HistoryContribution {
                    entry_id: provider_round_entry_id(turn_id, round, &call.call_id, "call"),
                    payload: HistoryPayload::ToolCall {
                        turn_id: turn_id.clone(),
                        item_id,
                        call_id: call.call_id,
                        name: call.name,
                        arguments: call.arguments.to_string(),
                        protocol_projector: projector,
                    },
                },
            ]);
        }
        self.commit_history(history).await
    }

    async fn commit_tool_result(
        &self,
        turn_id: &AgentTurnId,
        round: u32,
        call: &DashToolCall,
        result: &DashToolResult,
    ) -> Result<(), DashCoreError> {
        let item_id = super::execution_tool_item_id(turn_id, &call.call_id);
        self.commit_history(vec![
            HistoryContribution {
                entry_id: provider_round_entry_id(turn_id, round, &call.call_id, "result"),
                payload: HistoryPayload::ToolResult {
                    turn_id: turn_id.clone(),
                    item_id: item_id.clone(),
                    content: result.content.clone(),
                    is_error: result.is_error,
                    details: result.details.clone(),
                },
            },
            HistoryContribution {
                entry_id: provider_round_entry_id(turn_id, round, &call.call_id, "complete"),
                payload: HistoryPayload::ItemCompleted {
                    turn_id: turn_id.clone(),
                    item_id,
                },
            },
        ])
        .await
    }
}

#[async_trait]
impl DashExecutionCallbacks for DurableDashExecutionCallbacks {
    async fn emit(&self, execution: DashExecutionEvent) -> Result<(), DashCoreError> {
        match &execution.event {
            DashCoreEvent::ProviderRoundStarted { round } => {
                self.rounds
                    .lock()
                    .await
                    .insert(*round, PendingProviderRound::default());
            }
            DashCoreEvent::TextDelta { round, delta } => {
                self.rounds
                    .lock()
                    .await
                    .entry(*round)
                    .or_default()
                    .assistant_text
                    .push_str(delta);
            }
            DashCoreEvent::ToolCallRequested { round, call } => {
                self.rounds
                    .lock()
                    .await
                    .entry(*round)
                    .or_default()
                    .tool_calls
                    .push(call.clone());
            }
            DashCoreEvent::ProviderRoundCompleted {
                round,
                finish_reason,
                input_tokens,
                output_tokens,
                context_window,
            } => {
                self.commit_provider_round(
                    &execution.turn_id,
                    *round,
                    *finish_reason,
                    *input_tokens,
                    *output_tokens,
                    *context_window,
                )
                .await?;
            }
            DashCoreEvent::ToolCallCompleted {
                round,
                call,
                result,
            } => {
                self.commit_tool_result(&execution.turn_id, *round, call, result)
                    .await?;
            }
            DashCoreEvent::ReasoningDelta { .. } => {}
        }
        self.downstream.emit(execution).await
    }

    async fn drain_steering(
        &self,
        turn_id: &AgentTurnId,
        _round: u32,
        terminal_boundary: bool,
    ) -> Result<Vec<String>, DashCoreError> {
        self.service
            .drain_steering(turn_id, terminal_boundary)
            .await
    }
}

fn provider_round_entry_id(
    turn_id: &AgentTurnId,
    round: u32,
    coordinate: &str,
    stage: &str,
) -> HistoryEntryId {
    HistoryEntryId::new(format!(
        "{}:provider-round:{round}:{coordinate}:{stage}",
        turn_id.0
    ))
}

fn provider_round_assistant_history(
    turn_id: &AgentTurnId,
    round: u32,
    content: String,
) -> Vec<HistoryContribution> {
    let item_id = super::execution_assistant_item_id(turn_id, round);
    vec![
        HistoryContribution {
            entry_id: provider_round_entry_id(turn_id, round, &item_id.0, "start"),
            payload: HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::AssistantMessage,
            },
        },
        HistoryContribution {
            entry_id: provider_round_entry_id(turn_id, round, &item_id.0, "output"),
            payload: HistoryPayload::AgentOutput {
                turn_id: turn_id.clone(),
                item_id: Some(item_id.clone()),
                content,
            },
        },
        HistoryContribution {
            entry_id: provider_round_entry_id(turn_id, round, &item_id.0, "complete"),
            payload: HistoryPayload::ItemCompleted {
                turn_id: turn_id.clone(),
                item_id,
            },
        },
    ]
}

impl DashAgentService {
    pub fn initial_repository_state(
        history: AgentHistory,
        initial_context: Option<InitialContextInstallation>,
    ) -> Result<DashAgentRepositoryState, DashServiceError> {
        let mut store = DashAgentStore::new(history)?;
        if let Some(installation) = initial_context {
            store.commit(DashAgentCommit {
                expected_head: None,
                command_settlement: None,
                effect_settlements: vec![],
                history: vec![HistoryContribution {
                    entry_id: HistoryEntryId::new(format!(
                        "initial-context:{}",
                        installation.package_id
                    )),
                    payload: HistoryPayload::InitialContextInstalled { installation },
                }],
                enqueue_commands: vec![],
            })?;
        }
        Ok(DashAgentRepositoryState::new(store))
    }

    pub async fn create_with_repository(
        repository: Arc<dyn DashAgentRepository>,
        history: AgentHistory,
        initial_context: Option<InitialContextInstallation>,
        execution: DashExecutionDependencies,
    ) -> Result<Self, DashServiceError> {
        repository
            .initialize(Self::initial_repository_state(history, initial_context)?)
            .await?;
        Ok(Self::open_with_repository(repository, execution))
    }

    pub fn open_with_repository(
        repository: Arc<dyn DashAgentRepository>,
        mut execution: DashExecutionDependencies,
    ) -> Self {
        let tool_callbacks = Arc::new(RoutableDashToolCallbacks::new(execution.tools));
        execution.tools = tool_callbacks.clone();
        Self {
            repository,
            execution: Arc::new(tokio::sync::RwLock::new(execution)),
            tool_callbacks,
            cancellation: Arc::new(tokio::sync::Mutex::new(None)),
            steering: Arc::new(tokio::sync::Mutex::new(DashSteeringState::default())),
            effect_updates: Arc::new(tokio::sync::Notify::new()),
            worker_owner_id: Arc::from(format!(
                "dash-worker:{}:{}",
                std::process::id(),
                DASH_WORKER_OWNER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )),
        }
    }

    pub async fn create_with_store(
        store: &dyn DashAgentRepositoryStore,
        history: AgentHistory,
        initial_context: Option<InitialContextInstallation>,
        execution: DashExecutionDependencies,
    ) -> Result<Self, DashServiceError> {
        let source = history.session_id.clone();
        let repository = store
            .create(
                &source,
                Self::initial_repository_state(history, initial_context)?,
            )
            .await?;
        Ok(Self::open_with_repository(repository, execution))
    }

    pub async fn open_with_store(
        store: &dyn DashAgentRepositoryStore,
        source: &super::AgentSessionId,
        execution: DashExecutionDependencies,
    ) -> Result<Option<Self>, DashServiceError> {
        Ok(store
            .open(source)
            .await?
            .map(|repository| Self::open_with_repository(repository, execution)))
    }

    pub async fn replace_tool_callbacks(&self, tools: Arc<dyn DashToolCallbacks>) {
        self.tool_callbacks.replace(tools).await;
    }

    async fn execution_dependencies(&self) -> DashExecutionDependencies {
        self.execution.read().await.clone()
    }

    pub async fn fork_with_store(
        &self,
        repository_store: &dyn DashAgentRepositoryStore,
        child_session_id: super::AgentSessionId,
        child_branch_id: super::BranchId,
        cutoff: ForkCutoff,
    ) -> Result<Self, DashServiceError> {
        let state = self
            .fork_repository_state(child_session_id.clone(), child_branch_id, cutoff)
            .await?;
        let execution = self.execution_dependencies().await;
        let repository = repository_store.create(&child_session_id, state).await?;
        Ok(Self::open_with_repository(repository, execution))
    }

    pub async fn fork_repository_state(
        &self,
        child_session_id: super::AgentSessionId,
        child_branch_id: super::BranchId,
        cutoff: ForkCutoff,
    ) -> Result<DashAgentRepositoryState, DashServiceError> {
        let current = self.repository.load().await?;
        let child = current
            .store
            .history()
            .fork(child_session_id, child_branch_id, cutoff)?;
        Ok(DashAgentRepositoryState::new(DashAgentStore::new(child)?))
    }

    pub async fn read(&self) -> Result<DashAgentRead, DashServiceError> {
        let state = self.repository.load().await?;
        let history_state = state.store.history().state()?;
        let context_recipe = context_recipe_from_repository(&state)?;
        Ok(DashAgentRead {
            surface: history_state.surface.clone(),
            state: history_state,
            history: state.store.history().clone(),
            history_digest: state.store.history().digest(),
            context_recipe,
        })
    }

    pub async fn changes(
        &self,
        after: Option<super::DashChangeCursor>,
        limit: usize,
    ) -> Result<DashAgentChanges, DashServiceError> {
        let state = self.repository.load().await?;
        let changes = state
            .store
            .changes()
            .iter()
            .filter(|change| {
                after.as_ref().is_none_or(|after| {
                    (change.cursor.revision, change.cursor.ordinal)
                        > (after.revision, after.ordinal)
                })
            })
            .take(limit)
            .cloned()
            .collect();
        Ok(DashAgentChanges {
            changes,
            history: state.store.history().clone(),
        })
    }

    pub async fn history(&self) -> Result<AgentHistory, DashServiceError> {
        Ok(self.repository.load().await?.store.history().clone())
    }

    pub async fn context_recipe(&self) -> Result<DashContextRecipe, DashServiceError> {
        let repository = self.repository.load().await?;
        context_recipe_from_repository(&repository)
    }

    pub async fn export_store(&self) -> Result<DashAgentStore, DashServiceError> {
        Ok(self.repository.load().await?.store)
    }

    pub async fn export_repository_state(
        &self,
    ) -> Result<DashAgentRepositoryState, DashServiceError> {
        self.repository.load().await
    }

    pub async fn recover_pending_execution(&self) -> Result<(), DashServiceError> {
        let repository = self.repository.load().await?;
        let history = repository.store.history().state()?;
        let Some(compaction_id) = history.active_compaction.clone() else {
            if repository.active.is_none() {
                self.promote_queued_execution().await?;
            }
            return Ok(());
        };
        let compaction = history
            .compactions
            .get(&compaction_id)
            .cloned()
            .ok_or_else(|| DashServiceError::InvalidState {
                message: "active Dash compaction has no folded state".into(),
            })?;
        let active = repository
            .active
            .clone()
            .ok_or_else(|| DashServiceError::InvalidState {
                message: "active Dash compaction has no durable execution owner".into(),
            })?;
        let lease_current = active
            .lease
            .as_ref()
            .is_some_and(|lease| lease.expires_at_ms > crate::model::message::now_millis());
        if lease_current {
            return Ok(());
        }
        let automatic = compaction.mode == CompactionMode::AutomaticOverflow;
        if compaction.side_effect_started_at_ms.is_none() && !automatic {
            self.spawn_compaction_execution(
                active.request.clone(),
                compaction_id,
                compaction.mode,
                active.request.effect_id.0.clone(),
            );
            return Ok(());
        }

        let outer_effect_id = active.request.effect_id.clone();
        let compaction_effect_id = compaction.operation_id.clone();
        let compaction_command_id = if automatic {
            CommandId::new(compaction_id.0.clone())
        } else {
            active.request.command_id.clone()
        };
        let lost = compaction.side_effect_started_at_ms.is_some();
        let (_, ()) = self
            .update_repository(|repository| {
                let current = repository.store.history().state()?;
                if current.active_compaction.as_ref() != Some(&compaction_id) {
                    return Ok(());
                }
                repository.store.fail_compaction(
                    compaction_command_id,
                    compaction_effect_id.clone(),
                    compaction_id.clone(),
                    HistoryEntryId::new(format!(
                        "{}:compaction-recovered-terminal",
                        compaction_effect_id.0
                    )),
                    if lost {
                        "compaction provider outcome is unknown after worker restart".into()
                    } else {
                        "compaction worker stopped before the provider side effect".into()
                    },
                    lost,
                )?;
                if automatic {
                    repository.store.commit(DashAgentCommit {
                        expected_head: repository.store.history().head().cloned(),
                        command_settlement: None,
                        effect_settlements: vec![EffectSettlement {
                            effect_id: outer_effect_id.clone(),
                            outcome: if lost {
                                EffectOutcome::Lost
                            } else {
                                EffectOutcome::Failed
                            },
                        }],
                        history: vec![],
                        enqueue_commands: vec![],
                    })?;
                }
                repository.active = None;
                terminalize_repository_effect(
                    repository,
                    &outer_effect_id,
                    if lost {
                        DashTerminalOutcome::Lost
                    } else {
                        DashTerminalOutcome::Failed
                    },
                    false,
                )?;
                terminalize_dependent_effects(repository)?;
                Ok(())
            })
            .await?;
        self.clear_active(&active.turn_id).await;
        self.effect_updates.notify_waiters();
        self.promote_queued_execution().await
    }

    pub async fn apply_surface(&self, surface: DashSurface) -> Result<(), DashServiceError> {
        let (expected, replacement) = self.stage_surface_apply(surface).await?;
        let previous_entry_count = expected.store.history().entries().len();
        let committed_history = replacement.store.history().clone();
        self.repository
            .compare_and_swap(expected, replacement)
            .await?;
        self.publish_committed_history_since(previous_entry_count, &committed_history)
            .await;
        Ok(())
    }

    pub async fn revoke_surface(&self, expected_revision: u64) -> Result<(), DashServiceError> {
        let (expected, replacement) = self.stage_surface_revoke(expected_revision).await?;
        let previous_entry_count = expected.store.history().entries().len();
        let committed_history = replacement.store.history().clone();
        self.repository
            .compare_and_swap(expected, replacement)
            .await?;
        self.publish_committed_history_since(previous_entry_count, &committed_history)
            .await;
        Ok(())
    }

    pub async fn stage_surface_apply(
        &self,
        surface: DashSurface,
    ) -> Result<(DashAgentRepositoryState, DashAgentRepositoryState), DashServiceError> {
        let expected = self.repository.load().await?;
        let mut replacement = expected.clone();
        let current_surface = replacement.store.history().state()?.surface;
        if current_surface
            .as_ref()
            .is_some_and(|existing| surface.revision < existing.revision)
        {
            return Err(DashServiceError::Conflict {
                message: "Dash Agent surface revision moved backwards".into(),
            });
        }
        if current_surface.as_ref() != Some(&surface) {
            let next_sequence = replacement.store.history().entries().len() as u64 + 1;
            replacement.store.commit(DashAgentCommit {
                expected_head: replacement.store.history().head().cloned(),
                command_settlement: None,
                effect_settlements: vec![],
                history: vec![HistoryContribution {
                    entry_id: HistoryEntryId::new(format!(
                        "surface-applied:{next_sequence}:{}:{}",
                        surface.revision, surface.digest
                    )),
                    payload: HistoryPayload::SurfaceApplied {
                        surface: surface.clone(),
                    },
                }],
                enqueue_commands: vec![],
            })?;
        }
        Ok((expected, replacement))
    }

    pub async fn stage_surface_revoke(
        &self,
        expected_revision: u64,
    ) -> Result<(DashAgentRepositoryState, DashAgentRepositoryState), DashServiceError> {
        let expected = self.repository.load().await?;
        let mut replacement = expected.clone();
        let current_surface = replacement.store.history().state()?.surface;
        if current_surface
            .as_ref()
            .is_some_and(|surface| surface.revision != expected_revision)
        {
            return Err(DashServiceError::Conflict {
                message: "Dash Agent surface revision does not match".into(),
            });
        }
        if let Some(surface) = current_surface {
            let next_sequence = replacement.store.history().entries().len() as u64 + 1;
            replacement.store.commit(DashAgentCommit {
                expected_head: replacement.store.history().head().cloned(),
                command_settlement: None,
                effect_settlements: vec![],
                history: vec![HistoryContribution {
                    entry_id: HistoryEntryId::new(format!(
                        "surface-revoked:{next_sequence}:{expected_revision}"
                    )),
                    payload: HistoryPayload::SurfaceRevoked { surface },
                }],
                enqueue_commands: vec![],
            })?;
        }
        Ok((expected, replacement))
    }

    pub async fn execute(
        &self,
        request: DashCommandRequest,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        if let Some(existing) = self
            .repository
            .load()
            .await?
            .effects
            .get(&request.effect_id)
        {
            return if existing.request == request {
                Ok(existing.receipt.clone())
            } else {
                Err(DashServiceError::Conflict {
                    message: "effect identity was reused by another Dash command".into(),
                })
            };
        }
        match request.command.clone() {
            DashPublicCommand::SubmitInput { content } => {
                self.execute_submit(request, content, false).await
            }
            DashPublicCommand::Steer { turn_id, content } => {
                self.execute_steer(request, turn_id, content).await
            }
            DashPublicCommand::Interrupt { turn_id } => {
                self.execute_interrupt(request, turn_id).await
            }
            DashPublicCommand::RequestCompaction { mode } => {
                self.execute_compaction(request, mode, false).await
            }
            DashPublicCommand::ResolveInteraction {
                interaction_id,
                response,
            } => {
                self.execute_resolve_interaction(request, interaction_id, response)
                    .await
            }
            DashPublicCommand::Close => self.execute_close(request).await,
        }
    }

    /// Admits a normal input turn synchronously and lets the Dash owner advance it in a
    /// source-scoped background task. Non-submit commands keep their synchronous command
    /// semantics because steer, interrupt, interaction and compaction are bounded control
    /// operations.
    pub async fn execute_admitted(
        &self,
        request: DashCommandRequest,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        if let Some(existing) = self
            .repository
            .load()
            .await?
            .effects
            .get(&request.effect_id)
        {
            return if existing.request == request {
                Ok(existing.receipt.clone())
            } else {
                Err(DashServiceError::Conflict {
                    message: "effect identity was reused by another Dash command".into(),
                })
            };
        }
        match request.command.clone() {
            DashPublicCommand::SubmitInput { content } => {
                self.execute_submit(request, content, true).await
            }
            DashPublicCommand::RequestCompaction { mode } => {
                self.execute_compaction(request, mode, true).await
            }
            _ => self.execute(request).await,
        }
    }

    pub async fn wait_for_effect_terminal(
        &self,
        effect_id: &EffectId,
    ) -> Result<DashEffectInspection, DashServiceError> {
        loop {
            let notified = self.effect_updates.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let inspection =
                self.inspect(effect_id)
                    .await?
                    .ok_or_else(|| DashServiceError::InvalidState {
                        message: "Dash Agent effect does not exist".into(),
                    })?;
            if matches!(inspection.state, DashReceiptState::Terminal(_)) {
                return Ok(inspection);
            }
            notified.as_mut().await;
        }
    }

    pub async fn inspect(
        &self,
        effect_id: &EffectId,
    ) -> Result<Option<DashEffectInspection>, DashServiceError> {
        let state = self.repository.load().await?;
        let Some(record) = state.effects.get(effect_id).cloned() else {
            return Ok(None);
        };
        Ok(Some(DashEffectInspection {
            command_id: record.request.command_id.clone(),
            effect_id: effect_id.clone(),
            state: record.receipt.state,
            retryable: record.retryable,
            execution: state
                .store
                .inspect_execution(&record.request.command_id, effect_id),
        }))
    }

    async fn execute_submit(
        &self,
        request: DashCommandRequest,
        content: String,
        background: bool,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        if content.trim().is_empty() {
            return Err(DashServiceError::InvalidArgument {
                message: "Dash Agent input must not be blank".into(),
            });
        }
        let turn_id = AgentTurnId::new(format!("turn:{}", request.command_id.0));
        let effect_prefix = request.effect_id.0.clone();
        let command = DashCommand {
            command_id: request.command_id.clone(),
            kind: DashCommandKind::SubmitInput {
                input_id: request.command_id.0.clone(),
                content: content.clone(),
            },
            dependency: None,
        };
        if let Some(compaction_command_id) =
            self.repository
                .load()
                .await?
                .active
                .as_ref()
                .and_then(|active| {
                    matches!(
                        active.request.command,
                        DashPublicCommand::RequestCompaction { .. }
                    )
                    .then(|| active.request.command_id.clone())
                })
        {
            let mut deferred_command = command;
            deferred_command.dependency = Some(super::CommandDependency {
                command_id: compaction_command_id,
            });
            let (_, accepted) = self
                .update_repository(|repository| {
                    if !repository.active.as_ref().is_some_and(|active| {
                        matches!(
                            active.request.command,
                            DashPublicCommand::RequestCompaction { .. }
                        )
                    }) {
                        return Err(DashServiceError::Conflict {
                            message: "Dash Agent compaction is no longer active".into(),
                        });
                    }
                    repository.store.commit(DashAgentCommit {
                        expected_head: repository.store.history().head().cloned(),
                        command_settlement: None,
                        effect_settlements: vec![],
                        history: vec![],
                        enqueue_commands: vec![deferred_command],
                    })?;
                    let receipt = DashCommandReceipt {
                        command_id: request.command_id.clone(),
                        effect_id: request.effect_id.clone(),
                        state: DashReceiptState::Accepted,
                        history_revision: repository.store.history().state()?.entry_count,
                    };
                    repository.effects.insert(
                        request.effect_id.clone(),
                        DashEffectRecord {
                            request: request.clone(),
                            receipt: receipt.clone(),
                            retryable: false,
                        },
                    );
                    Ok(receipt)
                })
                .await?;
            if background {
                return Ok(accepted);
            }
            self.wait_for_effect_terminal(&request.effect_id).await?;
            return self
                .repository
                .load()
                .await?
                .effects
                .get(&request.effect_id)
                .map(|record| record.receipt.clone())
                .ok_or_else(|| DashServiceError::Internal {
                    message: "deferred Dash input lost its effect record".into(),
                });
        }
        let mut steering = self.steering.lock().await;
        let (_, accepted) = self
            .update_repository(|repository| {
                if repository.active.is_some() {
                    return Err(DashServiceError::Conflict {
                        message: "another Dash Agent execution is active".into(),
                    });
                }
                let expected_head = repository.store.history().head().cloned();
                repository.store.commit(DashAgentCommit {
                    expected_head,
                    command_settlement: None,
                    effect_settlements: vec![],
                    history: vec![
                        HistoryContribution {
                            entry_id: HistoryEntryId::new(format!("{effect_prefix}:input")),
                            payload: HistoryPayload::InputAccepted {
                                input_id: request.command_id.0.clone(),
                                content: content.clone(),
                            },
                        },
                        HistoryContribution {
                            entry_id: HistoryEntryId::new(format!("{effect_prefix}:turn-started")),
                            payload: HistoryPayload::TurnStarted {
                                turn_id: turn_id.clone(),
                                started_at_ms: crate::model::message::now_millis(),
                            },
                        },
                    ],
                    enqueue_commands: vec![command],
                })?;
                let claimed = repository.store.claim_next_command()?;
                if claimed.as_ref().map(|claimed| &claimed.command_id) != Some(&request.command_id)
                {
                    return Err(DashServiceError::Conflict {
                        message: "Dash Agent command could not be claimed".into(),
                    });
                }
                let history_revision = repository.store.history().state()?.entry_count;
                let receipt = DashCommandReceipt {
                    command_id: request.command_id.clone(),
                    effect_id: request.effect_id.clone(),
                    state: DashReceiptState::Accepted,
                    history_revision,
                };
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                repository.active = Some(DashActiveExecutionState {
                    turn_id: turn_id.clone(),
                    request: request.clone(),
                    lease: None,
                });
                Ok(receipt)
            })
            .await?;
        steering.active_turn = Some(turn_id.clone());
        steering.after_sequence = accepted.history_revision;
        drop(steering);
        let cancellation = DashCancellation::new();
        {
            let mut handle = self.cancellation.lock().await;
            *handle = Some((turn_id.clone(), cancellation.clone()));
        }

        if background {
            let service = self.clone();
            let background_request = request.clone();
            let background_turn_id = turn_id.clone();
            let background_accepted = accepted.clone();
            tokio::spawn(async move {
                let execution_service = service.clone();
                let execution_request = background_request.clone();
                let execution_turn_id = background_turn_id.clone();
                let execution = tokio::spawn(async move {
                    execution_service
                        .advance_submit_execution(
                            execution_request,
                            content,
                            execution_turn_id,
                            effect_prefix,
                            cancellation,
                            background_accepted,
                        )
                        .await
                })
                .await;
                let failure = match execution {
                    Ok(Ok(_)) => None,
                    Ok(Err(error)) => Some(super::DashExecutionFailure {
                        code: "background_execution_failed".to_owned(),
                        message: error.to_string(),
                        retryable: error.retryable(),
                    }),
                    Err(error) => Some(super::DashExecutionFailure {
                        code: "background_execution_panicked".to_owned(),
                        message: error.to_string(),
                        retryable: false,
                    }),
                };
                if let Some(failure) = failure {
                    let _ = service.expire_compaction_lease().await;
                    let _ = service.recover_pending_execution().await;
                    let already_terminal = service
                        .inspect(&background_request.effect_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|inspection| {
                            matches!(inspection.state, DashReceiptState::Terminal(_))
                        });
                    if !already_terminal {
                        let _ = service
                            .finish_failed_turn(
                                &background_request,
                                &background_turn_id,
                                DashTerminalOutcome::Failed,
                                Some(failure),
                            )
                            .await;
                    }
                    service.clear_active(&background_turn_id).await;
                    let _ = service.promote_queued_execution().await;
                    service.effect_updates.notify_waiters();
                }
            });
            return Ok(accepted);
        }

        self.advance_submit_execution(
            request,
            content,
            turn_id,
            effect_prefix,
            cancellation,
            accepted,
        )
        .await
    }

    async fn advance_submit_execution(
        &self,
        request: DashCommandRequest,
        content: String,
        turn_id: AgentTurnId,
        effect_prefix: String,
        cancellation: DashCancellation,
        accepted: DashCommandReceipt,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let context = self.materialize_context(&turn_id).await?;
        let execution = self.execution_dependencies().await;
        let round_snapshots = DashProviderRoundSnapshots::default();
        let callbacks = DurableDashExecutionCallbacks::new(
            self.clone(),
            execution.callbacks.clone(),
            round_snapshots.clone(),
        );
        let result = DashCoreTurn {
            turn_id: turn_id.clone(),
            input: content.clone(),
            context,
            output_started_entry_id: HistoryEntryId::new(format!(
                "{effect_prefix}:assistant-started"
            )),
            output_entry_id: HistoryEntryId::new(format!("{effect_prefix}:assistant-output")),
            output_completed_entry_id: HistoryEntryId::new(format!(
                "{effect_prefix}:assistant-completed"
            )),
            terminal_entry_id: HistoryEntryId::new(format!("{effect_prefix}:turn-completed")),
        }
        .run_with_materializer(
            execution.provider.as_ref(),
            execution.tools.as_ref(),
            &callbacks,
            self,
            round_snapshots,
            cancellation,
        )
        .await;
        self.tool_callbacks.clear_turn(&turn_id).await;

        let receipt = match result {
            Ok(result) => {
                let (_, receipt) = self
                    .update_repository(|repository| {
                        repository.store.commit(DashAgentCommit {
                            expected_head: repository.store.history().head().cloned(),
                            command_settlement: Some(super::CommandSettlement {
                                command_id: request.command_id.clone(),
                                outcome: CommandOutcome::Succeeded,
                            }),
                            effect_settlements: vec![EffectSettlement {
                                effect_id: request.effect_id.clone(),
                                outcome: EffectOutcome::Applied,
                            }],
                            history: result.history,
                            enqueue_commands: vec![],
                        })?;
                        repository.active = None;
                        terminalize_repository_effect(
                            repository,
                            &request.effect_id,
                            DashTerminalOutcome::Succeeded,
                            false,
                        )
                    })
                    .await?;
                receipt
            }
            Err(DashCoreError::Cancelled) => {
                self.finish_failed_turn(&request, &turn_id, DashTerminalOutcome::Interrupted, None)
                    .await?
            }
            Err(DashCoreError::InteractionRequired {
                interaction_id,
                prompt,
            }) => {
                self.update_store(|store| {
                    store.commit(DashAgentCommit {
                        expected_head: store.history().head().cloned(),
                        command_settlement: None,
                        effect_settlements: vec![],
                        history: vec![HistoryContribution {
                            entry_id: HistoryEntryId::new(format!(
                                "{effect_prefix}:interaction-requested"
                            )),
                            payload: HistoryPayload::InteractionRequested {
                                turn_id,
                                item_id: None,
                                interaction_id: InteractionId::new(interaction_id),
                                prompt,
                            },
                        }],
                        enqueue_commands: vec![],
                    })?;
                    Ok(())
                })
                .await?;
                return Ok(accepted);
            }
            Err(DashCoreError::ContextOverflow) => {
                self.recover_automatic_overflow(&request, &turn_id, content)
                    .await?
            }
            Err(error) => {
                let lost = matches!(error, DashCoreError::ProviderStreamDisconnected);
                let terminal = if lost {
                    DashTerminalOutcome::Lost
                } else {
                    DashTerminalOutcome::Failed
                };
                diag!(
                    Error,
                    Subsystem::AgentRun,
                    operation = "dash.execute",
                    stage = "core_terminal_failure",
                    turn_id = %turn_id.0,
                    command_id = %request.command_id.0,
                    effect_id = %request.effect_id.0,
                    error_code = error.code(),
                    retryable = error.retryable(),
                    error = %error,
                    error_debug = ?error,
                    "Dash Agent execution reached a failed terminal"
                );
                self.finish_failed_turn(&request, &turn_id, terminal, Some(error.failure()))
                    .await?
            }
        };
        self.clear_active(&turn_id).await;
        if matches!(receipt.state, DashReceiptState::Terminal(_))
            && let Err(error) = self
                .try_assign_thread_name(
                    &turn_id,
                    HistoryEntryId::new(format!("{effect_prefix}:thread-name")),
                )
                .await
        {
            diag!(
                Warn,
                Subsystem::AgentRun,
                error = %error,
                turn_id = ?turn_id,
                "Dash conversation naming failed after a terminal turn"
            );
        }
        self.promote_queued_execution().await?;
        self.effect_updates.notify_waiters();
        Ok(receipt)
    }

    async fn try_assign_thread_name(
        &self,
        turn_id: &AgentTurnId,
        entry_id: HistoryEntryId,
    ) -> Result<(), DashServiceError> {
        let history = self.repository.load().await?.store.history().clone();
        if history.state()?.thread_name.is_some() {
            return Ok(());
        }
        let Some(request) = conversation_naming_request(&history, turn_id) else {
            return Ok(());
        };
        let thread_name = self
            .execution_dependencies()
            .await
            .conversation_namer
            .generate(request)
            .await?;
        if thread_name.trim().is_empty() {
            return Err(DashServiceError::InvalidState {
                message: "Dash conversation namer returned a blank title".to_owned(),
            });
        }
        self.update_store(|store| {
            if store.history().state()?.thread_name.is_some() {
                return Ok(());
            }
            store.commit(DashAgentCommit {
                expected_head: store.history().head().cloned(),
                command_settlement: None,
                effect_settlements: vec![],
                history: vec![HistoryContribution {
                    entry_id,
                    payload: HistoryPayload::ThreadNameChanged { thread_name },
                }],
                enqueue_commands: vec![],
            })?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn recover_automatic_overflow(
        &self,
        request: &DashCommandRequest,
        overflow_turn_id: &AgentTurnId,
        content: String,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let prefix = request.effect_id.0.clone();
        let compaction_command_id = CommandId::new(format!("{}:B", request.command_id.0));
        let continuation_command_id = CommandId::new(format!("{}:C", request.command_id.0));
        let compaction_effect_id = EffectId::new(format!("{}:B", request.effect_id.0));
        let continuation_effect_id = EffectId::new(format!("{}:C", request.effect_id.0));
        let compaction_id = CompactionId::new(format!("{}:B", request.command_id.0));
        let continuation_turn_id = AgentTurnId::new(format!("turn:{}:C", request.command_id.0));
        let compaction_command = DashCommand {
            command_id: compaction_command_id.clone(),
            kind: DashCommandKind::RequestCompaction {
                compaction_id: compaction_id.clone(),
                mode: CompactionMode::AutomaticOverflow,
            },
            dependency: None,
        };
        let continuation_command = DashCommand {
            command_id: continuation_command_id.clone(),
            kind: DashCommandKind::ContinueAfterCompaction {
                input_id: request.command_id.0.clone(),
                content: content.clone(),
            },
            dependency: Some(super::CommandDependency {
                command_id: compaction_command_id.clone(),
            }),
        };
        let (_, ()) = self
            .update_store(|store| {
                store.commit(DashAgentCommit {
                    expected_head: store.history().head().cloned(),
                    command_settlement: Some(super::CommandSettlement {
                        command_id: request.command_id.clone(),
                        outcome: CommandOutcome::Succeeded,
                    }),
                    effect_settlements: vec![],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!("{prefix}:A-overflow")),
                        payload: HistoryPayload::TurnFailed {
                            turn_id: overflow_turn_id.clone(),
                            error: DashCoreError::ContextOverflow.failure(),
                            lost: false,
                            completed_at_ms: crate::model::message::now_millis(),
                        },
                    }],
                    enqueue_commands: vec![compaction_command, continuation_command],
                })?;
                let claimed = store.claim_next_command()?;
                if claimed.as_ref().map(|command| &command.command_id)
                    != Some(&compaction_command_id)
                {
                    return Err(DashServiceError::Conflict {
                        message: "automatic compaction B was not promoted".into(),
                    });
                }
                store.commit(DashAgentCommit {
                    expected_head: store.history().head().cloned(),
                    command_settlement: None,
                    effect_settlements: vec![],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!("{prefix}:B-started")),
                        payload: HistoryPayload::CompactionStarted {
                            compaction_id: compaction_id.clone(),
                            operation_id: compaction_effect_id.clone(),
                            mode: CompactionMode::AutomaticOverflow,
                            source_head: store.history().head().cloned(),
                            source_digest: store.history().digest(),
                            started_at_ms: crate::model::message::now_millis(),
                        },
                    }],
                    enqueue_commands: vec![],
                })?;
                Ok(())
            })
            .await?;
        let lease = DashWorkerLease {
            owner_id: self.worker_owner_id.to_string(),
            expires_at_ms: crate::model::message::now_millis().saturating_add(DASH_WORKER_LEASE_MS),
        };
        self.update_repository(|repository| {
            repository.store.mark_compaction_side_effect_started(
                compaction_id.clone(),
                HistoryEntryId::new(format!("{prefix}:B-side-effect-started")),
            )?;
            repository
                .active
                .as_mut()
                .ok_or_else(|| DashServiceError::Lost {
                    message: "automatic compaction lost its owning execution".into(),
                })?
                .lease = Some(lease);
            Ok(())
        })
        .await?;
        let compactor = self.execution_dependencies().await.compactor;
        let compacted = match self
            .materialize_compaction_request(
                compaction_id.clone(),
                CompactionMode::AutomaticOverflow,
            )
            .await
        {
            Ok(compaction_request) => {
                let compact = compactor.compact(compaction_request);
                tokio::pin!(compact);
                loop {
                    tokio::select! {
                        result = &mut compact => break result,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(
                            DASH_WORKER_HEARTBEAT_MS,
                        )) => {
                            self.renew_compaction_lease(&request.effect_id).await?;
                        }
                    }
                }
            }
            Err(error) => Err(error),
        };
        let compacted = match compacted {
            Ok(compacted) => compacted,
            Err(error) => {
                let lost = matches!(error, DashServiceError::Lost { .. });
                let retryable = error.retryable();
                let terminal = if lost {
                    DashTerminalOutcome::Lost
                } else {
                    DashTerminalOutcome::Failed
                };
                let (_, receipt) = self
                    .update_repository(|repository| {
                        repository.store.fail_compaction(
                            compaction_command_id.clone(),
                            compaction_effect_id.clone(),
                            compaction_id.clone(),
                            HistoryEntryId::new(format!("{prefix}:B-failed")),
                            error.to_string(),
                            lost,
                        )?;
                        repository.store.commit(DashAgentCommit {
                            expected_head: repository.store.history().head().cloned(),
                            command_settlement: None,
                            effect_settlements: vec![EffectSettlement {
                                effect_id: request.effect_id.clone(),
                                outcome: if lost {
                                    EffectOutcome::Lost
                                } else {
                                    EffectOutcome::Failed
                                },
                            }],
                            history: vec![],
                            enqueue_commands: vec![],
                        })?;
                        repository.active = None;
                        terminalize_repository_effect(
                            repository,
                            &request.effect_id,
                            terminal.clone(),
                            retryable,
                        )
                    })
                    .await?;
                self.clear_active(overflow_turn_id).await;
                return Ok(receipt);
            }
        };
        let mut steering = self.steering.lock().await;
        self.update_repository(|repository| {
            repository.store.complete_compaction(
                compaction_command_id.clone(),
                compaction_effect_id.clone(),
                compaction_id.clone(),
                compacted.revision,
                compacted.summary,
                compacted.retained_from,
                HistoryEntryId::new(format!("{prefix}:B-applied")),
                HistoryEntryId::new(format!("{prefix}:B-completed")),
            )?;
            let claimed = repository.store.claim_next_command()?;
            if claimed.as_ref().map(|command| &command.command_id) != Some(&continuation_command_id)
            {
                return Err(DashServiceError::Conflict {
                    message: "automatic continuation C was not promoted".into(),
                });
            }
            repository.store.commit(DashAgentCommit {
                expected_head: repository.store.history().head().cloned(),
                command_settlement: None,
                effect_settlements: vec![],
                history: vec![HistoryContribution {
                    entry_id: HistoryEntryId::new(format!("{prefix}:C-started")),
                    payload: HistoryPayload::TurnStarted {
                        turn_id: continuation_turn_id.clone(),
                        started_at_ms: crate::model::message::now_millis(),
                    },
                }],
                enqueue_commands: vec![],
            })?;
            repository.active = Some(DashActiveExecutionState {
                turn_id: continuation_turn_id.clone(),
                request: request.clone(),
                lease: None,
            });
            Ok(())
        })
        .await?;
        steering.active_turn = Some(continuation_turn_id.clone());
        drop(steering);
        let continuation_cancellation = DashCancellation::new();
        {
            let mut handle = self.cancellation.lock().await;
            *handle = Some((
                continuation_turn_id.clone(),
                continuation_cancellation.clone(),
            ));
        }
        let execution = self.execution_dependencies().await;
        let continuation_context = self
            .materialize_context(&AgentTurnId::new(format!(
                "turn:{}:C",
                request.command_id.0
            )))
            .await?;
        let round_snapshots = DashProviderRoundSnapshots::default();
        let callbacks = DurableDashExecutionCallbacks::new(
            self.clone(),
            execution.callbacks.clone(),
            round_snapshots.clone(),
        );
        let continuation = DashCoreTurn {
            turn_id: continuation_turn_id.clone(),
            input: content,
            context: continuation_context,
            output_started_entry_id: HistoryEntryId::new(format!("{prefix}:C-assistant-started")),
            output_entry_id: HistoryEntryId::new(format!("{prefix}:C-assistant-output")),
            output_completed_entry_id: HistoryEntryId::new(format!(
                "{prefix}:C-assistant-completed"
            )),
            terminal_entry_id: HistoryEntryId::new(format!("{prefix}:C-completed")),
        }
        .run_with_materializer(
            execution.provider.as_ref(),
            execution.tools.as_ref(),
            &callbacks,
            self,
            round_snapshots,
            continuation_cancellation,
        )
        .await;
        self.tool_callbacks.clear_turn(&continuation_turn_id).await;
        let (_, receipt) = self
            .update_repository(|repository| match continuation {
                Ok(continuation) => {
                    repository.store.commit(DashAgentCommit {
                        expected_head: repository.store.history().head().cloned(),
                        command_settlement: Some(super::CommandSettlement {
                            command_id: continuation_command_id,
                            outcome: CommandOutcome::Succeeded,
                        }),
                        effect_settlements: vec![
                            EffectSettlement {
                                effect_id: continuation_effect_id,
                                outcome: EffectOutcome::Applied,
                            },
                            EffectSettlement {
                                effect_id: request.effect_id.clone(),
                                outcome: EffectOutcome::Applied,
                            },
                        ],
                        history: continuation.history,
                        enqueue_commands: vec![],
                    })?;
                    repository.active = None;
                    terminalize_repository_effect(
                        repository,
                        &request.effect_id,
                        DashTerminalOutcome::Succeeded,
                        false,
                    )
                }
                Err(error) => {
                    let lost = matches!(error, DashCoreError::ProviderStreamDisconnected);
                    let retryable = error.retryable();
                    let terminal = if lost {
                        DashTerminalOutcome::Lost
                    } else {
                        DashTerminalOutcome::Failed
                    };
                    repository.store.commit(DashAgentCommit {
                        expected_head: repository.store.history().head().cloned(),
                        command_settlement: Some(super::CommandSettlement {
                            command_id: continuation_command_id,
                            outcome: if lost {
                                CommandOutcome::Lost
                            } else {
                                CommandOutcome::Failed
                            },
                        }),
                        effect_settlements: vec![
                            EffectSettlement {
                                effect_id: continuation_effect_id,
                                outcome: if lost {
                                    EffectOutcome::Lost
                                } else {
                                    EffectOutcome::Failed
                                },
                            },
                            EffectSettlement {
                                effect_id: request.effect_id.clone(),
                                outcome: if lost {
                                    EffectOutcome::Lost
                                } else {
                                    EffectOutcome::Failed
                                },
                            },
                        ],
                        history: vec![HistoryContribution {
                            entry_id: HistoryEntryId::new(format!("{prefix}:C-failed")),
                            payload: HistoryPayload::TurnFailed {
                                turn_id: continuation_turn_id.clone(),
                                error: error.failure(),
                                lost,
                                completed_at_ms: crate::model::message::now_millis(),
                            },
                        }],
                        enqueue_commands: vec![],
                    })?;
                    repository.active = None;
                    terminalize_repository_effect(
                        repository,
                        &request.effect_id,
                        terminal,
                        retryable,
                    )
                }
            })
            .await?;
        self.clear_active(&continuation_turn_id).await;
        Ok(receipt)
    }

    async fn execute_steer(
        &self,
        request: DashCommandRequest,
        turn_id: AgentTurnId,
        content: String,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        if content.trim().is_empty() {
            return Err(DashServiceError::InvalidArgument {
                message: "Dash Agent steering input must not be blank".into(),
            });
        }
        self.require_active_turn(&turn_id).await?;
        let steering = self.steering.lock().await;
        if steering.active_turn.as_ref() != Some(&turn_id) {
            return Err(DashServiceError::InvalidState {
                message: "Dash Agent turn no longer accepts steering".into(),
            });
        }
        let (_, receipt) = self
            .update_repository(|repository| {
                let active =
                    repository
                        .active
                        .as_ref()
                        .ok_or_else(|| DashServiceError::InvalidState {
                            message: "Dash Agent turn completed before steering was committed"
                                .into(),
                        })?;
                if active.turn_id != turn_id {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash Agent turn is not active".into(),
                    });
                }
                let has_pending_interaction = repository
                    .store
                    .history()
                    .state()?
                    .interactions
                    .values()
                    .any(|interaction| {
                        interaction.turn_id == turn_id
                            && interaction.response.is_none()
                            && !interaction.cancelled
                    });
                if has_pending_interaction {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash Agent turn is waiting for interaction resolution".into(),
                    });
                }
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: None,
                    effect_settlements: vec![],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!("{}:steer", request.effect_id.0)),
                        payload: HistoryPayload::InputAccepted {
                            input_id: request.command_id.0.clone(),
                            content,
                        },
                    }],
                    enqueue_commands: vec![],
                })?;
                let receipt = terminal_receipt(
                    &request,
                    DashTerminalOutcome::Succeeded,
                    repository.store.history().state()?.entry_count,
                );
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                Ok(receipt)
            })
            .await?;
        drop(steering);
        Ok(receipt)
    }

    async fn execute_interrupt(
        &self,
        request: DashCommandRequest,
        turn_id: AgentTurnId,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let active =
            self.repository
                .load()
                .await?
                .active
                .ok_or_else(|| DashServiceError::InvalidState {
                    message: "Dash Agent has no active turn".into(),
                })?;
        if active.turn_id != turn_id {
            return Err(DashServiceError::InvalidState {
                message: "Dash Agent turn is not active".into(),
            });
        }
        if matches!(
            active.request.command,
            DashPublicCommand::RequestCompaction { .. }
        ) {
            return self.execute_compaction_interrupt(request, active).await;
        }
        let cancellation = self.require_active_turn(&turn_id).await?;
        cancellation.cancel();
        let (_, receipt) = self
            .update_repository(|repository| {
                let active =
                    repository
                        .active
                        .as_ref()
                        .ok_or_else(|| DashServiceError::InvalidState {
                            message: "Dash Agent turn completed before interrupt was committed"
                                .into(),
                        })?;
                if active.turn_id != turn_id {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash Agent turn is not active".into(),
                    });
                }
                let active_request = active.request.clone();
                let pending_interactions = repository
                    .store
                    .history()
                    .state()?
                    .interactions
                    .iter()
                    .filter(|(_, interaction)| {
                        interaction.turn_id == turn_id
                            && interaction.response.is_none()
                            && !interaction.cancelled
                    })
                    .map(|(interaction_id, _)| interaction_id.clone())
                    .collect::<Vec<_>>();
                let mut history = pending_interactions
                    .into_iter()
                    .map(|interaction_id| HistoryContribution {
                        entry_id: HistoryEntryId::new(format!(
                            "{}:interaction-cancelled:{}",
                            request.effect_id.0, interaction_id.0
                        )),
                        payload: HistoryPayload::InteractionCancelled { interaction_id },
                    })
                    .collect::<Vec<_>>();
                history.push(HistoryContribution {
                    entry_id: HistoryEntryId::new(format!(
                        "{}:turn-terminal",
                        active_request.effect_id.0
                    )),
                    payload: HistoryPayload::TurnInterrupted {
                        turn_id: turn_id.clone(),
                        completed_at_ms: crate::model::message::now_millis(),
                    },
                });
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: Some(super::CommandSettlement {
                        command_id: active_request.command_id.clone(),
                        outcome: CommandOutcome::Failed,
                    }),
                    effect_settlements: vec![EffectSettlement {
                        effect_id: active_request.effect_id.clone(),
                        outcome: EffectOutcome::Failed,
                    }],
                    history,
                    enqueue_commands: vec![],
                })?;
                terminalize_repository_effect(
                    repository,
                    &active_request.effect_id,
                    DashTerminalOutcome::Interrupted,
                    false,
                )?;
                let receipt = terminal_receipt(
                    &request,
                    DashTerminalOutcome::Succeeded,
                    repository.store.history().state()?.entry_count,
                );
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                repository.active = None;
                Ok(receipt)
            })
            .await?;
        self.clear_active(&turn_id).await;
        self.effect_updates.notify_waiters();
        Ok(receipt)
    }

    async fn execute_compaction_interrupt(
        &self,
        request: DashCommandRequest,
        active: DashActiveExecutionState,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let compaction_id = CompactionId::new(active.request.command_id.0.clone());
        let (_, receipt) = self
            .update_repository(|repository| {
                let current =
                    repository
                        .active
                        .as_ref()
                        .ok_or_else(|| DashServiceError::InvalidState {
                            message: "Dash compaction completed before cancellation was committed"
                                .into(),
                        })?;
                if current.request.effect_id != active.request.effect_id {
                    return Err(DashServiceError::Conflict {
                        message: "Dash compaction operation changed".into(),
                    });
                }
                let history = repository.store.history().state()?;
                let compaction = history.compactions.get(&compaction_id).ok_or_else(|| {
                    DashServiceError::InvalidState {
                        message: "active Dash compaction has no history state".into(),
                    }
                })?;
                if compaction.side_effect_started_at_ms.is_some() {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash compaction is no longer cancellable".into(),
                    });
                }
                repository.store.cancel_compaction(
                    active.request.command_id.clone(),
                    active.request.effect_id.clone(),
                    compaction_id,
                    HistoryEntryId::new(format!(
                        "{}:compaction-cancelled",
                        active.request.effect_id.0
                    )),
                )?;
                terminalize_repository_effect(
                    repository,
                    &active.request.effect_id,
                    DashTerminalOutcome::Interrupted,
                    false,
                )?;
                terminalize_dependent_effects(repository)?;
                let receipt = terminal_receipt(
                    &request,
                    DashTerminalOutcome::Succeeded,
                    repository.store.history().state()?.entry_count,
                );
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                repository.active = None;
                Ok(receipt)
            })
            .await?;
        self.clear_active(&active.turn_id).await;
        self.effect_updates.notify_waiters();
        self.promote_queued_execution().await?;
        Ok(receipt)
    }

    async fn execute_compaction(
        &self,
        request: DashCommandRequest,
        mode: CompactionMode,
        background: bool,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let compaction_id = CompactionId::new(request.command_id.0.clone());
        let effect_prefix = request.effect_id.0.clone();
        let worker_lease = DashWorkerLease {
            owner_id: self.worker_owner_id.to_string(),
            expires_at_ms: crate::model::message::now_millis().saturating_add(DASH_WORKER_LEASE_MS),
        };
        let (_, (accepted, promoted)) = self
            .update_repository(|repository| {
                let command = DashCommand {
                    command_id: request.command_id.clone(),
                    kind: DashCommandKind::RequestCompaction {
                        compaction_id: compaction_id.clone(),
                        mode,
                    },
                    dependency: None,
                };
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: None,
                    effect_settlements: vec![],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!("{effect_prefix}:compaction-queued")),
                        payload: HistoryPayload::CompactionQueued {
                            compaction_id: compaction_id.clone(),
                            operation_id: request.effect_id.clone(),
                            mode,
                            queued_at_ms: crate::model::message::now_millis(),
                        },
                    }],
                    enqueue_commands: vec![command],
                })?;
                let promoted = if repository.active.is_none() {
                    let claimed = repository.store.claim_next_command()?;
                    if claimed.as_ref().map(|value| &value.command_id) != Some(&request.command_id)
                    {
                        return Err(DashServiceError::Conflict {
                            message: "Dash Agent compaction command could not be claimed".into(),
                        });
                    }
                    repository.store.commit(DashAgentCommit {
                        expected_head: repository.store.history().head().cloned(),
                        command_settlement: None,
                        effect_settlements: vec![],
                        history: vec![HistoryContribution {
                            entry_id: HistoryEntryId::new(format!(
                                "{effect_prefix}:compaction-started"
                            )),
                            payload: HistoryPayload::CompactionStarted {
                                compaction_id: compaction_id.clone(),
                                operation_id: request.effect_id.clone(),
                                mode,
                                source_head: repository.store.history().head().cloned(),
                                source_digest: repository.store.history().digest(),
                                started_at_ms: crate::model::message::now_millis(),
                            },
                        }],
                        enqueue_commands: vec![],
                    })?;
                    repository.active = Some(DashActiveExecutionState {
                        turn_id: AgentTurnId::new(request.command_id.0.clone()),
                        request: request.clone(),
                        lease: Some(worker_lease),
                    });
                    true
                } else {
                    false
                };
                let receipt = DashCommandReceipt {
                    command_id: request.command_id.clone(),
                    effect_id: request.effect_id.clone(),
                    state: DashReceiptState::Accepted,
                    history_revision: repository.store.history().state()?.entry_count,
                };
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                Ok((receipt, promoted))
            })
            .await?;
        if !promoted {
            if background {
                return Ok(accepted);
            }
            self.wait_for_effect_terminal(&request.effect_id).await?;
            return self
                .repository
                .load()
                .await?
                .effects
                .get(&request.effect_id)
                .map(|record| record.receipt.clone())
                .ok_or_else(|| DashServiceError::Internal {
                    message: "queued Dash compaction lost its effect record".into(),
                });
        }
        if background {
            self.spawn_compaction_execution(request, compaction_id, mode, effect_prefix);
            return Ok(accepted);
        }
        self.advance_compaction_execution(request, compaction_id, mode, effect_prefix)
            .await
    }

    fn spawn_compaction_execution(
        &self,
        request: DashCommandRequest,
        compaction_id: CompactionId,
        mode: CompactionMode,
        effect_prefix: String,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            let execution_service = service.clone();
            let execution = tokio::spawn(async move {
                execution_service
                    .advance_compaction_execution(request, compaction_id, mode, effect_prefix)
                    .await
            })
            .await;
            if !matches!(execution, Ok(Ok(_))) {
                let _ = service.expire_compaction_lease().await;
                let _ = service.recover_pending_execution().await;
            }
        });
    }

    async fn renew_compaction_lease(&self, effect_id: &EffectId) -> Result<(), DashServiceError> {
        let owner_id = self.worker_owner_id.to_string();
        let expires_at_ms =
            crate::model::message::now_millis().saturating_add(DASH_WORKER_LEASE_MS);
        self.update_repository(|repository| {
            let active = repository
                .active
                .as_mut()
                .ok_or_else(|| DashServiceError::Lost {
                    message: "Dash compaction lease lost its active execution".into(),
                })?;
            if active.request.effect_id != *effect_id
                || active
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.owner_id != owner_id)
            {
                return Err(DashServiceError::Lost {
                    message: "Dash compaction lease ownership changed".into(),
                });
            }
            active.lease = Some(DashWorkerLease {
                owner_id,
                expires_at_ms,
            });
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn expire_compaction_lease(&self) -> Result<(), DashServiceError> {
        let owner_id = self.worker_owner_id.to_string();
        self.update_repository(|repository| {
            if let Some(active) = repository.active.as_mut()
                && active
                    .lease
                    .as_ref()
                    .is_some_and(|lease| lease.owner_id == owner_id)
            {
                active.lease = Some(DashWorkerLease {
                    owner_id,
                    expires_at_ms: 0,
                });
            }
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn advance_compaction_execution(
        &self,
        request: DashCommandRequest,
        compaction_id: CompactionId,
        mode: CompactionMode,
        effect_prefix: String,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let lease = DashWorkerLease {
            owner_id: self.worker_owner_id.to_string(),
            expires_at_ms: crate::model::message::now_millis().saturating_add(DASH_WORKER_LEASE_MS),
        };
        let (_, started) = self
            .update_repository(|repository| {
                let is_current = repository.active.as_ref().is_some_and(|active| {
                    active.request.effect_id == request.effect_id
                        && active.turn_id.0 == compaction_id.0
                });
                if !is_current {
                    return Ok(false);
                }
                let state = repository.store.history().state()?;
                let compaction = state.compactions.get(&compaction_id).ok_or_else(|| {
                    DashServiceError::InvalidState {
                        message: format!(
                            "active Dash compaction {} has no history state",
                            compaction_id.0
                        ),
                    }
                })?;
                if compaction.side_effect_started_at_ms.is_some() {
                    return Ok(false);
                }
                repository.store.mark_compaction_side_effect_started(
                    compaction_id.clone(),
                    HistoryEntryId::new(format!("{effect_prefix}:compaction-side-effect-started")),
                )?;
                repository
                    .active
                    .as_mut()
                    .expect("current compaction execution exists")
                    .lease = Some(lease);
                Ok(true)
            })
            .await?;
        if !started {
            return self
                .repository
                .load()
                .await?
                .effects
                .get(&request.effect_id)
                .map(|record| record.receipt.clone())
                .ok_or_else(|| DashServiceError::Internal {
                    message: "Dash compaction worker lost its effect record".into(),
                });
        }
        let compactor = self.execution_dependencies().await.compactor;
        let result = match self
            .materialize_compaction_request(compaction_id.clone(), mode)
            .await
        {
            Ok(compaction_request) => {
                let compact = compactor.compact(compaction_request);
                tokio::pin!(compact);
                loop {
                    tokio::select! {
                        result = &mut compact => break result,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(
                            DASH_WORKER_HEARTBEAT_MS,
                        )) => {
                            self.renew_compaction_lease(&request.effect_id).await?;
                        }
                    }
                }
            }
            Err(error) => Err(error),
        };
        let (_, receipt) = self
            .update_repository(|repository| {
                let (terminal, retryable) = match result {
                    Ok(result) => {
                        repository.store.complete_compaction(
                            request.command_id.clone(),
                            request.effect_id.clone(),
                            compaction_id,
                            result.revision,
                            result.summary,
                            result.retained_from,
                            HistoryEntryId::new(format!("{effect_prefix}:compaction-applied")),
                            HistoryEntryId::new(format!("{effect_prefix}:compaction-completed")),
                        )?;
                        (DashTerminalOutcome::Succeeded, false)
                    }
                    Err(error) => {
                        let retryable = error.retryable();
                        let lost = matches!(error, DashServiceError::Lost { .. });
                        repository.store.fail_compaction(
                            request.command_id.clone(),
                            request.effect_id.clone(),
                            compaction_id,
                            HistoryEntryId::new(format!("{effect_prefix}:compaction-failed")),
                            error.to_string(),
                            lost,
                        )?;
                        (
                            if lost {
                                DashTerminalOutcome::Lost
                            } else {
                                DashTerminalOutcome::Failed
                            },
                            retryable,
                        )
                    }
                };
                repository.active = None;
                let receipt = terminalize_repository_effect(
                    repository,
                    &request.effect_id,
                    terminal,
                    retryable,
                )?;
                terminalize_dependent_effects(repository)?;
                Ok(receipt)
            })
            .await?;
        self.clear_active(&AgentTurnId::new(request.command_id.0.clone()))
            .await;
        self.effect_updates.notify_waiters();
        self.promote_queued_execution().await?;
        Ok(receipt)
    }

    async fn execute_resolve_interaction(
        &self,
        request: DashCommandRequest,
        interaction_id: InteractionId,
        response: String,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let active =
            self.repository
                .load()
                .await?
                .active
                .ok_or_else(|| DashServiceError::InvalidState {
                    message: "Dash Agent has no suspended interaction turn".into(),
                })?;
        let (_, receipt) = self
            .update_repository(|repository| {
                let state = repository.store.history().state()?;
                let interaction = state.interactions.get(&interaction_id).ok_or_else(|| {
                    DashServiceError::InvalidState {
                        message: "Dash Agent interaction is not pending".into(),
                    }
                })?;
                if interaction.response.is_some() {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash Agent interaction is already resolved".into(),
                    });
                }
                if interaction.turn_id != active.turn_id {
                    return Err(DashServiceError::InvalidState {
                        message: "Dash Agent interaction does not belong to the active turn".into(),
                    });
                }
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: Some(super::CommandSettlement {
                        command_id: active.request.command_id.clone(),
                        outcome: CommandOutcome::Succeeded,
                    }),
                    effect_settlements: vec![EffectSettlement {
                        effect_id: active.request.effect_id.clone(),
                        outcome: EffectOutcome::Applied,
                    }],
                    history: vec![
                        HistoryContribution {
                            entry_id: HistoryEntryId::new(format!(
                                "{}:interaction-resolved",
                                request.effect_id.0
                            )),
                            payload: HistoryPayload::InteractionResolved {
                                interaction_id,
                                response,
                            },
                        },
                        HistoryContribution {
                            entry_id: HistoryEntryId::new(format!(
                                "{}:interaction-turn-completed",
                                request.effect_id.0
                            )),
                            payload: HistoryPayload::TurnCompleted {
                                turn_id: active.turn_id.clone(),
                                completed_at_ms: crate::model::message::now_millis(),
                            },
                        },
                    ],
                    enqueue_commands: vec![],
                })?;
                terminalize_repository_effect(
                    repository,
                    &active.request.effect_id,
                    DashTerminalOutcome::Succeeded,
                    false,
                )?;
                let receipt = terminal_receipt(
                    &request,
                    DashTerminalOutcome::Succeeded,
                    repository.store.history().state()?.entry_count,
                );
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                repository.active = None;
                Ok(receipt)
            })
            .await?;
        self.clear_active(&active.turn_id).await;
        self.effect_updates.notify_waiters();
        Ok(receipt)
    }

    async fn execute_close(
        &self,
        request: DashCommandRequest,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let (_, receipt) = self
            .update_repository(|repository| {
                let state = repository.store.history().state()?;
                if state.status == SessionStatus::Closed {
                    let receipt =
                        terminal_receipt(&request, DashTerminalOutcome::Closed, state.entry_count);
                    repository.effects.insert(
                        request.effect_id.clone(),
                        DashEffectRecord {
                            request: request.clone(),
                            receipt: receipt.clone(),
                            retryable: false,
                        },
                    );
                    return Ok(receipt);
                }
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: None,
                    effect_settlements: vec![],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!("{}:closed", request.effect_id.0)),
                        payload: HistoryPayload::Closed,
                    }],
                    enqueue_commands: vec![],
                })?;
                let receipt = terminal_receipt(
                    &request,
                    DashTerminalOutcome::Closed,
                    repository.store.history().state()?.entry_count,
                );
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt: receipt.clone(),
                        retryable: false,
                    },
                );
                Ok(receipt)
            })
            .await?;
        Ok(receipt)
    }

    async fn materialize_context(
        &self,
        active_turn: &AgentTurnId,
    ) -> Result<DashCoreContext, DashServiceError> {
        Ok(self
            .materialize_session_context(Some(active_turn), true)
            .await?
            .context)
    }

    async fn materialize_compaction_request(
        &self,
        compaction_id: CompactionId,
        mode: CompactionMode,
    ) -> Result<DashCompactionRequest, DashServiceError> {
        let repository = self.repository.load().await?;
        let (source_head, source_digest) = repository
            .store
            .history()
            .entries()
            .iter()
            .find_map(|entry| match &entry.payload {
                HistoryPayload::CompactionStarted {
                    compaction_id: started_id,
                    source_head,
                    source_digest,
                    ..
                } if started_id == &compaction_id => {
                    Some((source_head.clone(), source_digest.clone()))
                }
                _ => None,
            })
            .ok_or_else(|| DashServiceError::InvalidState {
                message: format!("compaction {} has no started history fact", compaction_id.0),
            })?;
        let materialized = materialize_session_context(&repository, None, false)?;
        Ok(DashCompactionRequest {
            compaction_id,
            mode,
            source_head,
            source_digest,
            context: materialized.context,
            message_entry_ids: materialized.message_entry_ids,
        })
    }

    async fn materialize_session_context(
        &self,
        excluded_turn: Option<&AgentTurnId>,
        drop_latest_input: bool,
    ) -> Result<MaterializedSessionContext, DashServiceError> {
        let repository = self.repository.load().await?;
        materialize_session_context(&repository, excluded_turn, drop_latest_input)
    }

    async fn materialize_provider_round_context(
        &self,
    ) -> Result<(String, Vec<DashToolDefinition>), DashServiceError> {
        let repository = self.repository.load().await?;
        let materialized = materialize_session_context(&repository, None, false)?;
        Ok((
            materialized.context.system_prompt,
            materialized.context.tools,
        ))
    }

    async fn finish_failed_turn(
        &self,
        request: &DashCommandRequest,
        turn_id: &AgentTurnId,
        terminal: DashTerminalOutcome,
        failure: Option<super::DashExecutionFailure>,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let lost = terminal == DashTerminalOutcome::Lost;
        let interrupted = terminal == DashTerminalOutcome::Interrupted;
        let retryable = failure.as_ref().is_some_and(|failure| failure.retryable);
        let (_, receipt) = self
            .update_repository(|repository| {
                repository.store.commit(DashAgentCommit {
                    expected_head: repository.store.history().head().cloned(),
                    command_settlement: Some(super::CommandSettlement {
                        command_id: request.command_id.clone(),
                        outcome: if lost {
                            CommandOutcome::Lost
                        } else {
                            CommandOutcome::Failed
                        },
                    }),
                    effect_settlements: vec![EffectSettlement {
                        effect_id: request.effect_id.clone(),
                        outcome: if lost {
                            EffectOutcome::Lost
                        } else {
                            EffectOutcome::Failed
                        },
                    }],
                    history: vec![HistoryContribution {
                        entry_id: HistoryEntryId::new(format!(
                            "{}:turn-terminal",
                            request.effect_id.0
                        )),
                        payload: if interrupted {
                            HistoryPayload::TurnInterrupted {
                                turn_id: turn_id.clone(),
                                completed_at_ms: crate::model::message::now_millis(),
                            }
                        } else {
                            HistoryPayload::TurnFailed {
                                turn_id: turn_id.clone(),
                                error: failure
                                    .clone()
                                    .expect("failed turn requires failure evidence"),
                                lost,
                                completed_at_ms: crate::model::message::now_millis(),
                            }
                        },
                    }],
                    enqueue_commands: vec![],
                })?;
                repository.active = None;
                terminalize_repository_effect(repository, &request.effect_id, terminal, retryable)
            })
            .await?;
        Ok(receipt)
    }

    async fn require_active_turn(
        &self,
        turn_id: &AgentTurnId,
    ) -> Result<DashCancellation, DashServiceError> {
        let repository = self.repository.load().await?;
        let active = repository
            .active
            .as_ref()
            .ok_or_else(|| DashServiceError::InvalidState {
                message: "Dash Agent has no active turn".into(),
            })?;
        if &active.turn_id != turn_id {
            return Err(DashServiceError::InvalidState {
                message: "Dash Agent turn is not active".into(),
            });
        }
        let handle = self.cancellation.lock().await;
        let (_, cancellation) = handle
            .as_ref()
            .filter(|(active_turn, _)| active_turn == turn_id)
            .ok_or_else(|| DashServiceError::Lost {
                message: "active Dash execution requires worker recovery after restart".into(),
            })?;
        Ok(cancellation.clone())
    }

    async fn promote_queued_execution(&self) -> Result<(), DashServiceError> {
        let worker_lease = DashWorkerLease {
            owner_id: self.worker_owner_id.to_string(),
            expires_at_ms: crate::model::message::now_millis().saturating_add(DASH_WORKER_LEASE_MS),
        };
        let (_, promoted) = self
            .update_repository(|repository| {
                if repository.active.is_some() {
                    return Ok(None);
                }
                let Some(command) = repository.store.claim_next_command()? else {
                    return Ok(None);
                };
                let record = repository
                    .effects
                    .values()
                    .find(|record| record.request.command_id == command.command_id)
                    .cloned()
                    .ok_or_else(|| DashServiceError::InvalidState {
                        message: format!(
                            "queued Dash command {} has no service effect",
                            command.command_id.0
                        ),
                    })?;
                let request = record.request;
                let effect_prefix = request.effect_id.0.clone();
                let promoted = match command.kind {
                    DashCommandKind::SubmitInput { content, .. } => {
                        let turn_id = AgentTurnId::new(format!("turn:{}", request.command_id.0));
                        repository.store.commit(DashAgentCommit {
                            expected_head: repository.store.history().head().cloned(),
                            command_settlement: None,
                            effect_settlements: vec![],
                            history: vec![
                                HistoryContribution {
                                    entry_id: HistoryEntryId::new(format!("{effect_prefix}:input")),
                                    payload: HistoryPayload::InputAccepted {
                                        input_id: request.command_id.0.clone(),
                                        content: content.clone(),
                                    },
                                },
                                HistoryContribution {
                                    entry_id: HistoryEntryId::new(format!(
                                        "{effect_prefix}:turn-started"
                                    )),
                                    payload: HistoryPayload::TurnStarted {
                                        turn_id: turn_id.clone(),
                                        started_at_ms: crate::model::message::now_millis(),
                                    },
                                },
                            ],
                            enqueue_commands: vec![],
                        })?;
                        repository.active = Some(DashActiveExecutionState {
                            turn_id: turn_id.clone(),
                            request: request.clone(),
                            lease: None,
                        });
                        DashPromotedExecution::Submit {
                            request,
                            content,
                            turn_id,
                            effect_prefix,
                        }
                    }
                    DashCommandKind::RequestCompaction {
                        compaction_id,
                        mode,
                    } => {
                        repository.store.commit(DashAgentCommit {
                            expected_head: repository.store.history().head().cloned(),
                            command_settlement: None,
                            effect_settlements: vec![],
                            history: vec![HistoryContribution {
                                entry_id: HistoryEntryId::new(format!(
                                    "{effect_prefix}:compaction-started"
                                )),
                                payload: HistoryPayload::CompactionStarted {
                                    compaction_id: compaction_id.clone(),
                                    operation_id: request.effect_id.clone(),
                                    mode,
                                    source_head: repository.store.history().head().cloned(),
                                    source_digest: repository.store.history().digest(),
                                    started_at_ms: crate::model::message::now_millis(),
                                },
                            }],
                            enqueue_commands: vec![],
                        })?;
                        repository.active = Some(DashActiveExecutionState {
                            turn_id: AgentTurnId::new(request.command_id.0.clone()),
                            request: request.clone(),
                            lease: Some(worker_lease),
                        });
                        DashPromotedExecution::Compaction {
                            request,
                            compaction_id,
                            mode,
                            effect_prefix,
                        }
                    }
                    DashCommandKind::ContinueAfterCompaction { .. } | DashCommandKind::Close => {
                        return Err(DashServiceError::InvalidState {
                            message: format!(
                                "queued Dash command {} requires its owning workflow",
                                command.command_id.0
                            ),
                        });
                    }
                };
                let revision = repository.store.history().state()?.entry_count;
                repository
                    .effects
                    .get_mut(&record.receipt.effect_id)
                    .expect("promoted effect record exists")
                    .receipt
                    .history_revision = revision;
                Ok(Some(promoted))
            })
            .await?;
        match promoted {
            Some(DashPromotedExecution::Submit {
                request,
                content,
                turn_id,
                effect_prefix,
            }) => {
                let revision = self
                    .repository
                    .load()
                    .await?
                    .store
                    .history()
                    .state()?
                    .entry_count;
                let mut steering = self.steering.lock().await;
                steering.active_turn = Some(turn_id.clone());
                steering.after_sequence = revision;
                drop(steering);
                let cancellation = DashCancellation::new();
                *self.cancellation.lock().await = Some((turn_id.clone(), cancellation.clone()));
                self.spawn_submit_execution(request, content, turn_id, effect_prefix, cancellation);
            }
            Some(DashPromotedExecution::Compaction {
                request,
                compaction_id,
                mode,
                effect_prefix,
            }) => {
                self.spawn_compaction_execution(request, compaction_id, mode, effect_prefix);
            }
            None => {}
        }
        Ok(())
    }

    fn spawn_submit_execution(
        &self,
        request: DashCommandRequest,
        content: String,
        turn_id: AgentTurnId,
        effect_prefix: String,
        cancellation: DashCancellation,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            let accepted = match service.repository.load().await {
                Ok(repository) => repository
                    .effects
                    .get(&request.effect_id)
                    .map(|record| record.receipt.clone()),
                Err(_) => None,
            };
            let Some(accepted) = accepted else {
                return;
            };
            let execution = service
                .advance_submit_execution(
                    request.clone(),
                    content,
                    turn_id.clone(),
                    effect_prefix,
                    cancellation,
                    accepted,
                )
                .await;
            if let Err(error) = execution {
                let already_terminal = service
                    .inspect(&request.effect_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|inspection| {
                        matches!(inspection.state, DashReceiptState::Terminal(_))
                    });
                if !already_terminal {
                    let _ = service
                        .finish_failed_turn(
                            &request,
                            &turn_id,
                            DashTerminalOutcome::Failed,
                            Some(super::DashExecutionFailure {
                                code: "background_execution_failed".to_owned(),
                                message: error.to_string(),
                                retryable: error.retryable(),
                            }),
                        )
                        .await;
                    let _ = service.promote_queued_execution().await;
                }
                service.clear_active(&turn_id).await;
                service.effect_updates.notify_waiters();
            }
        });
    }

    async fn clear_active(&self, turn_id: &AgentTurnId) {
        let mut handle = self.cancellation.lock().await;
        if handle
            .as_ref()
            .is_some_and(|(active_turn, _)| active_turn == turn_id)
        {
            *handle = None;
        }
        drop(handle);
        let mut steering = self.steering.lock().await;
        if steering.active_turn.as_ref() == Some(turn_id) {
            steering.active_turn = None;
        }
    }

    async fn drain_steering(
        &self,
        turn_id: &AgentTurnId,
        terminal_boundary: bool,
    ) -> Result<Vec<String>, DashCoreError> {
        let mut steering = self.steering.lock().await;
        if steering.active_turn.as_ref() != Some(turn_id) {
            return Ok(Vec::new());
        }
        let repository = self
            .repository
            .load()
            .await
            .map_err(|error| DashCoreError::Callback {
                message: error.to_string(),
            })?;
        let mut inputs = Vec::new();
        for entry in repository
            .store
            .history()
            .entries()
            .iter()
            .filter(|entry| entry.sequence > steering.after_sequence)
        {
            if let HistoryPayload::InputAccepted { content, .. } = &entry.payload {
                inputs.push(content.clone());
            }
        }
        steering.after_sequence = repository
            .store
            .history()
            .state()
            .map_err(|error| DashCoreError::Callback {
                message: error.to_string(),
            })?
            .entry_count;
        if terminal_boundary && inputs.is_empty() {
            steering.active_turn = None;
        }
        Ok(inputs)
    }

    async fn update_store<T>(
        &self,
        mutate: impl FnOnce(&mut DashAgentStore) -> Result<T, DashServiceError>,
    ) -> Result<(DashAgentStore, T), DashServiceError> {
        let expected = self.repository.load().await?;
        let previous_entry_count = expected.store.history().entries().len();
        let mut replacement = expected.clone();
        let result = mutate(&mut replacement.store)?;
        let committed_history = replacement.store.history().clone();
        self.repository
            .compare_and_swap(expected, replacement.clone())
            .await?;
        self.publish_committed_history_since(previous_entry_count, &committed_history)
            .await;
        Ok((replacement.store, result))
    }

    async fn update_repository<T>(
        &self,
        mutate: impl FnOnce(&mut DashAgentRepositoryState) -> Result<T, DashServiceError>,
    ) -> Result<(DashAgentRepositoryState, T), DashServiceError> {
        let expected = self.repository.load().await?;
        let previous_entry_count = expected.store.history().entries().len();
        let mut replacement = expected.clone();
        let result = mutate(&mut replacement)?;
        let committed_history = replacement.store.history().clone();
        self.repository
            .compare_and_swap(expected, replacement.clone())
            .await?;
        self.publish_committed_history_since(previous_entry_count, &committed_history)
            .await;
        Ok((replacement, result))
    }

    /// Publishes the canonical live view of an already committed native history suffix.
    ///
    /// The Complete Agent adapter calls this after an outer transaction atomically commits the
    /// Dash repository together with source metadata. Publication is process-local and never
    /// participates in the durable commit result.
    pub async fn publish_committed_history_since(
        &self,
        previous_entry_count: usize,
        history: &AgentHistory,
    ) {
        let Some(entries) = history.entries().get(previous_entry_count..) else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let _ = self
            .execution_dependencies()
            .await
            .history_callbacks
            .committed(DashHistoryCommit {
                history: history.clone(),
                entries: entries.to_vec(),
            })
            .await;
    }
}

fn conversation_naming_request(
    history: &AgentHistory,
    turn_id: &AgentTurnId,
) -> Option<DashConversationNamingRequest> {
    let turn_start = history.entries().iter().position(|entry| {
        matches!(
            &entry.payload,
            HistoryPayload::TurnStarted {
                turn_id: candidate,
                ..
            } if candidate == turn_id
        )
    })?;
    let user = history.entries()[..turn_start]
        .iter()
        .rev()
        .find_map(|entry| match &entry.payload {
            HistoryPayload::InputAccepted { content, .. } if !content.trim().is_empty() => {
                Some(DashMessage {
                    role: DashMessageRole::User,
                    content: content.clone(),
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    is_error: false,
                })
            }
            _ => None,
        })?;
    let assistant = history
        .entries()
        .get(turn_start + 1..)?
        .iter()
        .rev()
        .find_map(|entry| match &entry.payload {
            HistoryPayload::AgentOutput {
                turn_id: candidate,
                content,
                ..
            } if candidate == turn_id && !content.trim().is_empty() => Some(DashMessage {
                role: DashMessageRole::Assistant,
                content: content.clone(),
                tool_call_id: None,
                tool_calls: Vec::new(),
                is_error: false,
            }),
            _ => None,
        })?;
    Some(DashConversationNamingRequest {
        messages: vec![user, assistant],
    })
}

#[async_trait]
impl DashProviderRoundMaterializer for DashAgentService {
    async fn materialize_provider_round(
        &self,
        _turn_id: &AgentTurnId,
        mut draft: DashProviderRequest,
    ) -> Result<DashProviderRequest, DashCoreError> {
        let (system_prompt, tools) =
            self.materialize_provider_round_context()
                .await
                .map_err(|error| DashCoreError::Callback {
                    message: format!("failed to materialize accepted ContextFrame input: {error}"),
                })?;
        draft.system_prompt = system_prompt;
        draft.tools = tools;
        Ok(draft)
    }
}

fn materialize_accepted_context_frames(
    surface: Option<&DashSurface>,
    initial_context: Option<&InitialContextInstallation>,
    compaction_frame: Option<&agentdash_agent_protocol::ContextFrame>,
    surface_append_frames: &[agentdash_agent_protocol::ContextFrame],
) -> Vec<agentdash_agent_protocol::ContextFrame> {
    let mut frames = Vec::new();
    if let Some(surface) = surface {
        frames.extend(
            surface
                .context_frames
                .iter()
                .filter(|frame| {
                    frame.delivery_metadata.agent_consumption.mode
                        != agentdash_agent_protocol::ContextAgentConsumptionMode::SystemAppend
                })
                .cloned()
                .map(|frame| (frame, None)),
        );
    }
    if let Some(initial_context) = initial_context {
        frames.extend(
            initial_context
                .context_frames
                .iter()
                .cloned()
                .map(|frame| (frame, None)),
        );
    }
    if let Some(compaction_frame) = compaction_frame {
        frames.push((compaction_frame.clone(), None));
    }
    frames.extend(
        surface_append_frames
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, frame)| (frame, Some(index))),
    );
    frames.sort_by(|left, right| {
        (
            left.0.delivery_metadata.delivery_phase,
            left.0.delivery_metadata.delivery_order,
            left.1.unwrap_or_default(),
            left.0.created_at_ms,
            left.0.id.as_str(),
        )
            .cmp(&(
                right.0.delivery_metadata.delivery_phase,
                right.0.delivery_metadata.delivery_order,
                right.1.unwrap_or_default(),
                right.0.created_at_ms,
                right.0.id.as_str(),
            ))
    });
    frames.into_iter().map(|(frame, _)| frame).collect()
}

fn accepted_surface_append_frames(
    entries: &[AgentHistoryEntry],
) -> Vec<agentdash_agent_protocol::ContextFrame> {
    let mut frames = Vec::new();
    let mut frame_ids = BTreeSet::new();
    for entry in entries {
        match &entry.payload {
            HistoryPayload::SurfaceApplied { surface } => {
                for frame in &surface.context_frames {
                    if frame.delivery_metadata.agent_consumption.mode
                        == agentdash_agent_protocol::ContextAgentConsumptionMode::SystemAppend
                        && frame_ids.insert(frame.id.clone())
                    {
                        frames.push(frame.clone());
                    }
                }
            }
            HistoryPayload::SurfaceRevoked { .. } => {
                frames.clear();
                frame_ids.clear();
            }
            _ => {}
        }
    }
    frames
}

struct MaterializedSessionContext {
    context: DashCoreContext,
    message_entry_ids: Vec<HistoryEntryId>,
    frames: Vec<agentdash_agent_protocol::ContextFrame>,
    context_revision: Option<ContextRevision>,
}

fn context_recipe_from_repository(
    repository: &DashAgentRepositoryState,
) -> Result<DashContextRecipe, DashServiceError> {
    let materialized = materialize_session_context(repository, None, false)?;
    let snapshot_revision = repository.store.history().state()?.entry_count;
    let messages = materialized
        .context
        .history
        .into_iter()
        .zip(materialized.message_entry_ids)
        .map(|(message, source_entry_id)| DashContextRecipeMessage {
            source_entry_id,
            message,
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&(
        &materialized.frames,
        &messages,
        &materialized.context_revision,
    ))
    .map_err(|error| DashServiceError::Internal {
        message: format!("encode Dash context recipe: {error}"),
    })?;
    Ok(DashContextRecipe {
        snapshot_revision,
        context_revision: materialized.context_revision,
        frames: materialized.frames,
        messages,
        digest: format!("sha256:{:x}", Sha256::digest(encoded)),
    })
}

fn materialize_session_context(
    repository: &DashAgentRepositoryState,
    excluded_turn: Option<&AgentTurnId>,
    drop_latest_input: bool,
) -> Result<MaterializedSessionContext, DashServiceError> {
    let history_state = repository.store.history().state()?;
    let surface = history_state.surface;
    let initial_context = history_state.initial_context;
    let entries = repository.store.history().entries();
    let mut applied_compactions = BTreeMap::new();
    let mut latest_compaction = None;
    for (index, entry) in entries.iter().enumerate() {
        match &entry.payload {
            HistoryPayload::CompactionApplied {
                compaction_id,
                checkpoint,
            } => {
                applied_compactions.insert(
                    compaction_id.clone(),
                    (
                        checkpoint.context_revision.clone(),
                        checkpoint.summary_frame.clone(),
                        checkpoint.retained_from.clone(),
                    ),
                );
            }
            HistoryPayload::CompactionCompleted { compaction_id, .. } => {
                if let Some((revision, context_frame, retained_from)) =
                    applied_compactions.get(compaction_id).cloned()
                {
                    latest_compaction = Some((index, revision, context_frame, retained_from));
                }
            }
            _ => {}
        }
    }
    let (context_revision, compaction_frame, history_start) = latest_compaction
        .map(
            |(completed_index, revision, context_frame, retained_from)| {
                let start = retained_from
                    .as_ref()
                    .and_then(|id| entries.iter().position(|entry| &entry.entry_id == id))
                    .unwrap_or(completed_index.saturating_add(1));
                (Some(revision), Some(context_frame), start)
            },
        )
        .unwrap_or((None, None, 0));
    let mut history = Vec::new();
    let mut message_entry_ids = Vec::new();
    let mut pending_tool_calls = Vec::new();
    let mut tool_call_ids = BTreeMap::new();
    for entry in &entries[history_start..] {
        match &entry.payload {
            HistoryPayload::InputAccepted { content, .. } => {
                flush_provider_tool_calls(
                    &mut history,
                    &mut message_entry_ids,
                    &mut pending_tool_calls,
                );
                history.push(DashMessage {
                    role: DashMessageRole::User,
                    content: content.clone(),
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    is_error: false,
                });
                message_entry_ids.push(entry.entry_id.clone());
            }
            HistoryPayload::AgentOutput {
                turn_id, content, ..
            } if excluded_turn.is_none_or(|excluded| turn_id != excluded) => {
                flush_provider_tool_calls(
                    &mut history,
                    &mut message_entry_ids,
                    &mut pending_tool_calls,
                );
                history.push(DashMessage {
                    role: DashMessageRole::Assistant,
                    content: content.clone(),
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    is_error: false,
                });
                message_entry_ids.push(entry.entry_id.clone());
            }
            HistoryPayload::ToolCall {
                item_id,
                call_id,
                name,
                arguments,
                ..
            } => {
                tool_call_ids.insert(item_id.clone(), (call_id.clone(), entry.entry_id.clone()));
                pending_tool_calls.push((
                    DashToolCall {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        arguments: serde_json::from_str(arguments)
                            .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
                    },
                    entry.entry_id.clone(),
                ));
            }
            HistoryPayload::ToolResult {
                item_id,
                content,
                is_error,
                ..
            } => {
                flush_provider_tool_calls(
                    &mut history,
                    &mut message_entry_ids,
                    &mut pending_tool_calls,
                );
                if let Some((call_id, _)) = tool_call_ids.get(item_id) {
                    history.push(DashMessage {
                        role: DashMessageRole::Tool,
                        content: content
                            .iter()
                            .filter_map(crate::ContentPart::extract_text)
                            .collect::<Vec<_>>()
                            .join("\n"),
                        tool_call_id: Some(call_id.clone()),
                        tool_calls: Vec::new(),
                        is_error: *is_error,
                    });
                    message_entry_ids.push(entry.entry_id.clone());
                }
            }
            _ => {}
        }
    }
    flush_provider_tool_calls(
        &mut history,
        &mut message_entry_ids,
        &mut pending_tool_calls,
    );
    if drop_latest_input {
        history.pop();
        message_entry_ids.pop();
    }
    let frames = materialize_accepted_context_frames(
        surface.as_ref(),
        initial_context.as_ref(),
        compaction_frame.as_ref(),
        &accepted_surface_append_frames(entries),
    );
    let system_prompt = frames
        .iter()
        .map(|frame| frame.rendered_text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(MaterializedSessionContext {
        context: DashCoreContext {
            system_prompt,
            history,
            tools: surface.map(|surface| surface.tools).unwrap_or_default(),
        },
        message_entry_ids,
        frames,
        context_revision,
    })
}

fn flush_provider_tool_calls(
    history: &mut Vec<DashMessage>,
    message_entry_ids: &mut Vec<HistoryEntryId>,
    pending: &mut Vec<(DashToolCall, HistoryEntryId)>,
) {
    if pending.is_empty() {
        return;
    }
    let Some((_, entry_id)) = pending.first() else {
        return;
    };
    let entry_id = entry_id.clone();
    history.push(DashMessage {
        role: DashMessageRole::Assistant,
        content: String::new(),
        tool_call_id: None,
        tool_calls: std::mem::take(pending)
            .into_iter()
            .map(|(call, _)| call)
            .collect(),
        is_error: false,
    });
    message_entry_ids.push(entry_id);
}

fn terminalize_repository_effect(
    repository: &mut DashAgentRepositoryState,
    effect_id: &EffectId,
    outcome: DashTerminalOutcome,
    retryable: bool,
) -> Result<DashCommandReceipt, DashServiceError> {
    let revision = repository.store.history().state()?.entry_count;
    let record =
        repository
            .effects
            .get_mut(effect_id)
            .ok_or_else(|| DashServiceError::Internal {
                message: "Dash Agent terminalized an unrecorded effect".into(),
            })?;
    record.receipt.state = DashReceiptState::Terminal(outcome);
    record.receipt.history_revision = revision;
    record.retryable = retryable;
    Ok(record.receipt.clone())
}

fn terminalize_dependent_effects(
    repository: &mut DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    let terminal = repository
        .effects
        .iter()
        .filter_map(|(effect_id, record)| {
            if matches!(record.receipt.state, DashReceiptState::Terminal(_)) {
                return None;
            }
            match repository.store.command_status(&record.request.command_id) {
                Some(CommandStatus::Failed) => Some((
                    effect_id.clone(),
                    DashTerminalOutcome::Failed,
                    EffectOutcome::Failed,
                )),
                Some(CommandStatus::Lost | CommandStatus::Blocked) => Some((
                    effect_id.clone(),
                    DashTerminalOutcome::Lost,
                    EffectOutcome::Lost,
                )),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    if terminal.is_empty() {
        return Ok(());
    }
    repository.store.commit(DashAgentCommit {
        expected_head: repository.store.history().head().cloned(),
        command_settlement: None,
        effect_settlements: terminal
            .iter()
            .map(|(effect_id, _, outcome)| EffectSettlement {
                effect_id: effect_id.clone(),
                outcome: *outcome,
            })
            .collect(),
        history: vec![],
        enqueue_commands: vec![],
    })?;
    for (effect_id, outcome, _) in terminal {
        terminalize_repository_effect(repository, &effect_id, outcome, false)?;
    }
    Ok(())
}

fn terminal_receipt(
    request: &DashCommandRequest,
    outcome: DashTerminalOutcome,
    history_revision: u64,
) -> DashCommandReceipt {
    DashCommandReceipt {
        command_id: request.command_id.clone(),
        effect_id: request.effect_id.clone(),
        state: DashReceiptState::Terminal(outcome),
        history_revision,
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DashServiceError {
    #[error("invalid Dash Agent argument: {message}")]
    InvalidArgument { message: String },
    #[error("invalid Dash Agent state: {message}")]
    InvalidState { message: String },
    #[error("Dash Agent conflict: {message}")]
    Conflict { message: String },
    #[error("Dash Agent dependency is unavailable: {message}")]
    Unavailable { message: String, retryable: bool },
    #[error("Dash Agent outcome is unknown: {message}")]
    Lost { message: String },
    #[error("Dash Agent internal failure: {message}")]
    Internal { message: String },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    History(#[from] super::HistoryError),
    #[error(transparent)]
    Core(#[from] DashCoreError),
}

impl DashServiceError {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Unavailable {
                retryable: true,
                ..
            }
        )
    }
}

impl From<tokio::task::JoinError> for DashServiceError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}
