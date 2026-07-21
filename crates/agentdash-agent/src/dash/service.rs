use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryState, AgentItemId, AgentTurnId, CommandId, CommandOutcome,
    CompactionId, CompactionMode, ContextRevision, DashAgentChange, DashAgentCommit,
    DashAgentStore, DashCancellation, DashCommand, DashCommandKind, DashCoreContext, DashCoreError,
    DashCoreTurn, DashExecutionCallbacks, DashExecutionInspection, DashMessage, DashMessageRole,
    DashProvider, DashToolCall, DashToolCallbacks, DashToolDefinition, EffectId, EffectOutcome,
    EffectSettlement, ForkCutoff, HistoryContribution, HistoryEntryId, HistoryPayload,
    InitialContextInstallation, InteractionId, SessionStatus, StoreError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashSurface {
    pub revision: u64,
    pub digest: String,
    pub system_prompt: String,
    pub tools: Vec<DashToolDefinition>,
}

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
    surface: Option<DashSurface>,
    active: Option<DashActiveExecutionState>,
}

impl DashAgentRepositoryState {
    pub fn history(&self) -> &AgentHistory {
        self.store.history()
    }

    pub fn new(store: DashAgentStore) -> Self {
        Self {
            store,
            effects: BTreeMap::new(),
            surface: None,
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

#[derive(Clone)]
pub struct DashExecutionDependencies {
    pub provider: Arc<dyn DashProvider>,
    pub tools: Arc<dyn DashToolCallbacks>,
    pub callbacks: Arc<dyn DashExecutionCallbacks>,
    pub compactor: Arc<dyn DashCompactor>,
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
    cancellation: Arc<tokio::sync::Mutex<Option<(AgentTurnId, DashCancellation)>>>,
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
        execution: DashExecutionDependencies,
    ) -> Self {
        Self {
            repository,
            execution: Arc::new(tokio::sync::RwLock::new(execution)),
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
        self.execution.write().await.tools = tools;
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
        let mut state = DashAgentRepositoryState::new(DashAgentStore::new(child)?);
        state.surface = current.surface;
        Ok(state)
    }

    pub async fn read(&self) -> Result<DashAgentRead, DashServiceError> {
        let state = self.repository.load().await?;
        Ok(DashAgentRead {
            state: state.store.history().state()?,
            history: state.store.history().clone(),
            history_digest: state.store.history().digest(),
            surface: state.surface,
        })
    }

    pub async fn changes(
        &self,
        after: Option<super::DashChangeCursor>,
        limit: usize,
    ) -> Result<Vec<DashAgentChange>, DashServiceError> {
        let state = self.repository.load().await?;
        Ok(state
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
            .collect())
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
        self.repository
            .compare_and_swap(expected, replacement)
            .await
    }

    pub async fn revoke_surface(&self, expected_revision: u64) -> Result<(), DashServiceError> {
        let (expected, replacement) = self.stage_surface_revoke(expected_revision).await?;
        self.repository
            .compare_and_swap(expected, replacement)
            .await
    }

    pub async fn stage_surface_apply(
        &self,
        surface: DashSurface,
    ) -> Result<(DashAgentRepositoryState, DashAgentRepositoryState), DashServiceError> {
        let expected = self.repository.load().await?;
        let mut replacement = expected.clone();
        if replacement
            .surface
            .as_ref()
            .is_some_and(|existing| surface.revision < existing.revision)
        {
            return Err(DashServiceError::Conflict {
                message: "Dash Agent surface revision moved backwards".into(),
            });
        }
        replacement.surface = Some(surface);
        Ok((expected, replacement))
    }

    pub async fn stage_surface_revoke(
        &self,
        expected_revision: u64,
    ) -> Result<(DashAgentRepositoryState, DashAgentRepositoryState), DashServiceError> {
        let expected = self.repository.load().await?;
        let mut replacement = expected.clone();
        if replacement
            .surface
            .as_ref()
            .is_some_and(|surface| surface.revision != expected_revision)
        {
            return Err(DashServiceError::Conflict {
                message: "Dash Agent surface revision does not match".into(),
            });
        }
        replacement.surface = None;
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
        let result = DashCoreTurn {
            turn_id: turn_id.clone(),
            input: content.clone(),
            context,
            output_item_id: AgentItemId::new(format!("{effect_prefix}:assistant")),
            output_started_entry_id: HistoryEntryId::new(format!(
                "{effect_prefix}:assistant-started"
            )),
            output_entry_id: HistoryEntryId::new(format!("{effect_prefix}:assistant-output")),
            output_completed_entry_id: HistoryEntryId::new(format!(
                "{effect_prefix}:assistant-completed"
            )),
            terminal_entry_id: HistoryEntryId::new(format!("{effect_prefix}:turn-completed")),
        }
        .run(
            execution.provider.as_ref(),
            execution.tools.as_ref(),
            execution.callbacks.as_ref(),
            cancellation,
        )
        .await;

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
                self.finish_failed_turn(&request, &turn_id, terminal, Some(error.failure()))
                    .await?
            }
        };
        self.clear_active(&turn_id).await;
        Ok(receipt)
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
        let continuation = DashCoreTurn {
            turn_id: continuation_turn_id.clone(),
            input: content,
            context: self
                .materialize_context(&AgentTurnId::new(format!(
                    "turn:{}:C",
                    request.command_id.0
                )))
                .await?,
            output_item_id: AgentItemId::new(format!("{prefix}:C-assistant")),
            output_started_entry_id: HistoryEntryId::new(format!("{prefix}:C-assistant-started")),
            output_entry_id: HistoryEntryId::new(format!("{prefix}:C-assistant-output")),
            output_completed_entry_id: HistoryEntryId::new(format!(
                "{prefix}:C-assistant-completed"
            )),
            terminal_entry_id: HistoryEntryId::new(format!("{prefix}:C-completed")),
        }
        .run(
            execution.provider.as_ref(),
            execution.tools.as_ref(),
            execution.callbacks.as_ref(),
            continuation_cancellation,
        )
        .await;
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
        let surface = repository.surface.clone();
        let entries = repository.store.history().entries();
        let mut applied_compactions = BTreeMap::new();
        let mut latest_compaction = None;
        for (index, entry) in entries.iter().enumerate() {
            match &entry.payload {
                HistoryPayload::CompactionApplied {
                    compaction_id,
                    summary,
                    retained_from,
                    ..
                } => {
                    applied_compactions.insert(
                        compaction_id.clone(),
                        (summary.clone(), retained_from.clone()),
                    );
                }
                HistoryPayload::CompactionCompleted { compaction_id } => {
                    if let Some((summary, retained_from)) =
                        applied_compactions.get(compaction_id).cloned()
                    {
                        latest_compaction = Some((index, summary, retained_from));
                    }
                }
                _ => {}
            }
        }
        let (compaction_summary, history_start) = latest_compaction
            .map(|(completed_index, summary, retained_from)| {
                let start = retained_from
                    .as_ref()
                    .and_then(|id| entries.iter().position(|entry| &entry.entry_id == id))
                    .unwrap_or(completed_index.saturating_add(1));
                (Some(summary), start)
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
                            content: content.clone(),
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
        let mut system_prompt = surface
            .as_ref()
            .map(|surface| surface.system_prompt.clone())
            .unwrap_or_default();
        if let Some(summary) = compaction_summary {
            if !system_prompt.is_empty() {
                system_prompt.push_str("\n\n");
            }
            system_prompt.push_str("<compacted_context>\n");
            system_prompt.push_str(&summary);
            system_prompt.push_str("\n</compacted_context>");
        }
        Ok(DashCoreContext {
            system_prompt,
            history,
            tools: surface.map(|surface| surface.tools).unwrap_or_default(),
            max_provider_rounds: 8,
        })
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
                            }
                        } else {
                            HistoryPayload::TurnFailed {
                                turn_id: turn_id.clone(),
                                error: failure
                                    .clone()
                                    .expect("failed turn requires failure evidence"),
                                lost,
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
        let mut replacement = expected.clone();
        let result = mutate(&mut replacement.store)?;
        self.repository
            .compare_and_swap(expected, replacement.clone())
            .await?;
        Ok((replacement.store, result))
    }

    async fn update_repository<T>(
        &self,
        mutate: impl FnOnce(&mut DashAgentRepositoryState) -> Result<T, DashServiceError>,
    ) -> Result<(DashAgentRepositoryState, T), DashServiceError> {
        let expected = self.repository.load().await?;
        let mut replacement = expected.clone();
        let result = mutate(&mut replacement)?;
        self.repository
            .compare_and_swap(expected, replacement.clone())
            .await?;
        Ok((replacement, result))
    }
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
