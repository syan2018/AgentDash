use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentTurnId, CommandId, CommandOutcome,
    CompactionId, CompactionMode, ContextRevision, DashAgentChange, DashAgentCommit,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashAgentChanges {
    pub changes: Vec<DashAgentChange>,
    pub history: AgentHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashCompactionRequest {
    pub compaction_id: CompactionId,
    pub mode: CompactionMode,
    pub source_head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub history: AgentHistory,
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
}

#[derive(Default)]
struct PendingProviderRound {
    assistant_text: String,
    tool_calls: Vec<DashToolCall>,
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
    ) -> Result<(), DashCoreError> {
        let pending = self.rounds.lock().await.remove(&round).unwrap_or_default();
        if finish_reason == DashFinishReason::Stop && pending.tool_calls.is_empty() {
            return Ok(());
        }
        let mut history = Vec::new();
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
            } => {
                self.commit_provider_round(&execution.turn_id, *round, *finish_reason)
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
        Ok(DashAgentRead {
            surface: history_state.surface.clone(),
            state: history_state,
            history: state.store.history().clone(),
            history_digest: state.store.history().digest(),
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

    pub async fn export_store(&self) -> Result<DashAgentStore, DashServiceError> {
        Ok(self.repository.load().await?.store)
    }

    pub async fn export_repository_state(
        &self,
    ) -> Result<DashAgentRepositoryState, DashServiceError> {
        self.repository.load().await
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
                self.execute_submit(request, content).await
            }
            DashPublicCommand::Steer { turn_id, content } => {
                self.execute_steer(request, turn_id, content).await
            }
            DashPublicCommand::Interrupt { turn_id } => {
                self.execute_interrupt(request, turn_id).await
            }
            DashPublicCommand::RequestCompaction { mode } => {
                self.execute_compaction(request, mode).await
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
                });
                Ok(receipt)
            })
            .await?;
        let cancellation = DashCancellation::new();
        {
            let mut handle = self.cancellation.lock().await;
            *handle = Some((turn_id.clone(), cancellation.clone()));
        }

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
        let (_, history) = self
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
                            mode: CompactionMode::AutomaticOverflow,
                            source_head: store.history().head().cloned(),
                            source_digest: store.history().digest(),
                        },
                    }],
                    enqueue_commands: vec![],
                })?;
                Ok(store.history().clone())
            })
            .await?;
        let compactor = self.execution_dependencies().await.compactor;
        let compacted = match compactor
            .compact(DashCompactionRequest {
                compaction_id: compaction_id.clone(),
                mode: CompactionMode::AutomaticOverflow,
                source_head: history.head().cloned(),
                source_digest: history.digest(),
                history,
            })
            .await
        {
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
            });
            Ok(())
        })
        .await?;
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
        self.require_active_turn(&turn_id).await?;
        self.execution_dependencies()
            .await
            .provider
            .steer(&turn_id, &content)
            .await?;
        let (_, receipt) = self
            .update_repository(|repository| {
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
        Ok(receipt)
    }

    async fn execute_interrupt(
        &self,
        request: DashCommandRequest,
        turn_id: AgentTurnId,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let cancellation = self.require_active_turn(&turn_id).await?;
        cancellation.cancel();
        let (_, receipt) = self
            .update_repository(|repository| {
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
        Ok(receipt)
    }

    async fn execute_compaction(
        &self,
        request: DashCommandRequest,
        mode: CompactionMode,
    ) -> Result<DashCommandReceipt, DashServiceError> {
        let compaction_id = CompactionId::new(request.command_id.0.clone());
        let effect_prefix = request.effect_id.0.clone();
        let (_, history) = self
            .update_repository(|repository| {
                repository.store.begin_compaction(
                    DashCommand {
                        command_id: request.command_id.clone(),
                        kind: DashCommandKind::RequestCompaction {
                            compaction_id: compaction_id.clone(),
                            mode,
                        },
                        dependency: None,
                    },
                    HistoryEntryId::new(format!("{effect_prefix}:compaction-started")),
                )?;
                let history = repository.store.history().clone();
                let receipt = DashCommandReceipt {
                    command_id: request.command_id.clone(),
                    effect_id: request.effect_id.clone(),
                    state: DashReceiptState::Accepted,
                    history_revision: history.state()?.entry_count,
                };
                repository.effects.insert(
                    request.effect_id.clone(),
                    DashEffectRecord {
                        request: request.clone(),
                        receipt,
                        retryable: false,
                    },
                );
                Ok(history)
            })
            .await?;
        let compactor = self.execution_dependencies().await.compactor;
        let result = compactor
            .compact(DashCompactionRequest {
                compaction_id: compaction_id.clone(),
                mode,
                source_head: history.head().cloned(),
                source_digest: history.digest(),
                history,
            })
            .await;
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
                terminalize_repository_effect(repository, &request.effect_id, terminal, retryable)
            })
            .await?;
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
        let repository = self.repository.load().await?;
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
                    context_frame,
                    retained_from,
                    ..
                } => {
                    applied_compactions.insert(
                        compaction_id.clone(),
                        (context_frame.clone(), retained_from.clone()),
                    );
                }
                HistoryPayload::CompactionCompleted { compaction_id } => {
                    if let Some((context_frame, retained_from)) =
                        applied_compactions.get(compaction_id).cloned()
                    {
                        latest_compaction = Some((index, context_frame, retained_from));
                    }
                }
                _ => {}
            }
        }
        let (compaction_frame, history_start) = latest_compaction
            .map(|(completed_index, context_frame, retained_from)| {
                let start = retained_from
                    .as_ref()
                    .and_then(|id| entries.iter().position(|entry| &entry.entry_id == id))
                    .unwrap_or(completed_index.saturating_add(1));
                (Some(context_frame), start)
            })
            .unwrap_or((None, 0));
        let mut history = Vec::new();
        let mut pending_tool_calls = Vec::new();
        let mut tool_call_ids = BTreeMap::new();
        for entry in &entries[history_start..] {
            match &entry.payload {
                HistoryPayload::InputAccepted { content, .. } => {
                    flush_provider_tool_calls(&mut history, &mut pending_tool_calls);
                    history.push(DashMessage {
                        role: DashMessageRole::User,
                        content: content.clone(),
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        is_error: false,
                    });
                }
                HistoryPayload::AgentOutput {
                    turn_id, content, ..
                } if turn_id != active_turn => {
                    flush_provider_tool_calls(&mut history, &mut pending_tool_calls);
                    history.push(DashMessage {
                        role: DashMessageRole::Assistant,
                        content: content.clone(),
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        is_error: false,
                    });
                }
                HistoryPayload::ToolCall {
                    item_id,
                    call_id,
                    name,
                    arguments,
                    ..
                } => {
                    tool_call_ids.insert(item_id.clone(), call_id.clone());
                    pending_tool_calls.push(DashToolCall {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        arguments: serde_json::from_str(arguments)
                            .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
                    });
                }
                HistoryPayload::ToolResult {
                    item_id,
                    content,
                    is_error,
                    ..
                } => {
                    flush_provider_tool_calls(&mut history, &mut pending_tool_calls);
                    if let Some(call_id) = tool_call_ids.get(item_id) {
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
                    }
                }
                _ => {}
            }
        }
        flush_provider_tool_calls(&mut history, &mut pending_tool_calls);
        history.pop();
        let system_prompt = render_accepted_context(
            surface.as_ref(),
            initial_context.as_ref(),
            compaction_frame.as_ref(),
            &accepted_surface_append_frames(entries),
        );
        Ok(DashCoreContext {
            system_prompt,
            history,
            tools: surface.map(|surface| surface.tools).unwrap_or_default(),
        })
    }

    async fn materialize_provider_round_context(
        &self,
    ) -> Result<(String, Vec<DashToolDefinition>), DashServiceError> {
        let repository = self.repository.load().await?;
        let state = repository.store.history().state()?;
        let mut applied_compactions = BTreeMap::new();
        let mut latest_frame = None;
        for entry in repository.store.history().entries() {
            match &entry.payload {
                HistoryPayload::CompactionApplied {
                    compaction_id,
                    context_frame,
                    ..
                } => {
                    applied_compactions.insert(compaction_id.clone(), context_frame.clone());
                }
                HistoryPayload::CompactionCompleted { compaction_id } => {
                    latest_frame = applied_compactions.get(compaction_id).cloned();
                }
                _ => {}
            }
        }
        let system_prompt = render_accepted_context(
            state.surface.as_ref(),
            state.initial_context.as_ref(),
            latest_frame.as_ref(),
            &accepted_surface_append_frames(repository.store.history().entries()),
        );
        let tools = state
            .surface
            .map(|surface| surface.tools)
            .unwrap_or_default();
        Ok((system_prompt, tools))
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

    async fn clear_active(&self, turn_id: &AgentTurnId) {
        let mut handle = self.cancellation.lock().await;
        if handle
            .as_ref()
            .is_some_and(|(active_turn, _)| active_turn == turn_id)
        {
            *handle = None;
        }
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

fn render_accepted_context(
    surface: Option<&DashSurface>,
    initial_context: Option<&InitialContextInstallation>,
    compaction_frame: Option<&agentdash_agent_protocol::ContextFrame>,
    surface_append_frames: &[agentdash_agent_protocol::ContextFrame],
) -> String {
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
    frames
        .into_iter()
        .map(|(frame, _)| frame.rendered_text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
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

fn flush_provider_tool_calls(history: &mut Vec<DashMessage>, pending: &mut Vec<DashToolCall>) {
    if pending.is_empty() {
        return;
    }
    history.push(DashMessage {
        role: DashMessageRole::Assistant,
        content: String::new(),
        tool_call_id: None,
        tool_calls: std::mem::take(pending),
        is_error: false,
    });
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
