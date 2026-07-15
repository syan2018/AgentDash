use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    AgentRuntimeGateway, ContextCompactionId, ContextCompactionTrigger, EventSequence,
    IdempotencyKey, ImmutablePresentationEvent, InteractionResponse, OperationMeta,
    OperationReceipt, PresentationDurability, PresentationThreadId, RuntimeActor, RuntimeCommand,
    RuntimeCommandEnvelope, RuntimeEventStream, RuntimeExecuteError, RuntimeInput,
    RuntimeInteractionId, RuntimeOperationId, RuntimePresentationCoordinate,
    RuntimePresentationInput, RuntimeSnapshot, RuntimeSnapshotError, RuntimeSnapshotQuery,
    RuntimeSnapshotResult, RuntimeSubscribeError, RuntimeThreadId, RuntimeThreadStatus,
    RuntimeTurnId,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeForkSource, AgentRunRuntimePresentationPlanStore,
    AgentRunRuntimeProvisionRequest, AgentRunRuntimeProvisioner, AgentRunRuntimeRecoveryState,
    AgentRunRuntimeTarget, AgentRunTurnStartContextSource,
};
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_spi::AuthIdentity;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunRuntimeView {
    pub target: AgentRunRuntimeTarget,
    pub binding: Option<AgentRunRuntimeBinding>,
    pub snapshot: Option<RuntimeSnapshot>,
    pub binding_epoch: Option<agentdash_agent_runtime_contract::BindingEpoch>,
    pub recovery: AgentRunRuntimeRecoverySummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunRuntimeRecoverySummary {
    Active,
    Lost,
    Recovering,
    RecoveryFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunCommandGuard {
    pub thread_id: RuntimeThreadId,
    pub expected_revision: agentdash_agent_runtime_contract::RuntimeRevision,
    pub expected_active_turn_id: Option<RuntimeTurnId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendAgentRunMessage {
    pub target: AgentRunRuntimeTarget,
    pub presentation_thread_id: PresentationThreadId,
    pub presentation_input: AgentRunPresentationInput,
    pub client_command_id: String,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub identity: Option<AuthIdentity>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForkAgentRunRuntime {
    pub source_target: AgentRunRuntimeTarget,
    pub child_target: AgentRunRuntimeTarget,
    pub child_presentation_thread_id: PresentationThreadId,
    pub through_source_turn_id: Option<agentdash_agent_runtime_contract::DriverTurnId>,
    pub identity: Option<AuthIdentity>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunPresentationInput {
    UserSubmission {
        turn_id: agentdash_agent_runtime_contract::PresentationTurnId,
        item_id: agentdash_agent_runtime_contract::PresentationItemId,
        content: Vec<agentdash_agent_protocol::UserInputBlock>,
        source: agentdash_agent_protocol::UserInputSource,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind,
        started_at_seconds: i64,
    },
    SystemDelivery {
        turn_id: agentdash_agent_runtime_contract::PresentationTurnId,
        launch_source: LaunchPresentationSource,
        message: String,
        started_at_seconds: i64,
    },
}

impl AgentRunPresentationInput {
    pub fn turn_id(&self) -> &agentdash_agent_runtime_contract::PresentationTurnId {
        match self {
            Self::UserSubmission { turn_id, .. } | Self::SystemDelivery { turn_id, .. } => turn_id,
        }
    }

    fn started_at_seconds(&self) -> i64 {
        match self {
            Self::UserSubmission {
                started_at_seconds, ..
            }
            | Self::SystemDelivery {
                started_at_seconds, ..
            } => *started_at_seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchPresentationSource {
    HttpPrompt,
    LifecycleAgentUserMessage,
    HookAutoResume,
    CompanionDispatch,
    CompanionParentResume,
    SystemDelivery,
    WorkflowOrchestrator,
    RoutineExecutor,
    LocalRelayPrompt,
    ContextCompaction,
}

impl LaunchPresentationSource {
    fn tag(self) -> &'static str {
        match self {
            Self::HttpPrompt => "http_prompt",
            Self::LifecycleAgentUserMessage => "lifecycle_agent_user_message",
            Self::HookAutoResume => "hook_auto_resume",
            Self::CompanionDispatch => "companion_dispatch",
            Self::CompanionParentResume => "companion_dispatch",
            Self::SystemDelivery => "system_delivery",
            Self::WorkflowOrchestrator => "workflow_orchestrator",
            Self::RoutineExecutor => "routine_executor",
            Self::LocalRelayPrompt => "local_relay_prompt",
            Self::ContextCompaction => "context_compaction",
        }
    }

    pub(crate) fn emits_user_submission(self) -> bool {
        matches!(
            self,
            Self::HttpPrompt
                | Self::LifecycleAgentUserMessage
                | Self::CompanionDispatch
                | Self::LocalRelayPrompt
        )
    }

    fn system_delivery_kind(self) -> &'static str {
        match self {
            Self::CompanionDispatch => "companion_delivery",
            Self::CompanionParentResume => "subagent_notification",
            Self::SystemDelivery => "system_delivery",
            Self::HookAutoResume => "hook_auto_resume",
            Self::WorkflowOrchestrator => "workflow_delivery",
            Self::RoutineExecutor => "routine_delivery",
            Self::ContextCompaction => "context_compaction",
            Self::HttpPrompt | Self::LifecycleAgentUserMessage | Self::LocalRelayPrompt => {
                "system_delivery"
            }
        }
    }

    fn system_delivery_actor(self) -> &'static str {
        match self {
            Self::CompanionDispatch | Self::CompanionParentResume => "agent",
            Self::HttpPrompt | Self::LifecycleAgentUserMessage | Self::LocalRelayPrompt => "user",
            Self::HookAutoResume
            | Self::SystemDelivery
            | Self::WorkflowOrchestrator
            | Self::RoutineExecutor
            | Self::ContextCompaction => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AgentRunPresentationDraft {
    pub content: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub source: agentdash_agent_protocol::UserInputSource,
    pub launch_source: LaunchPresentationSource,
    pub submission_kind: agentdash_agent_protocol::UserInputSubmissionKind,
    pub started_at_seconds: i64,
}

impl AgentRunPresentationDraft {
    pub(crate) fn emits_user_submission(&self) -> bool {
        self.launch_source.emits_user_submission()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardedAgentRunCommand {
    pub target: AgentRunRuntimeTarget,
    pub client_command_id: String,
    pub guard: AgentRunCommandGuard,
    pub actor: RuntimeActor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteerAgentRunTurn {
    pub command: GuardedAgentRunCommand,
    pub presentation_input: AgentRunPresentationInput,
    pub input: Vec<RuntimeInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolveAgentRunInteraction {
    pub command: GuardedAgentRunCommand,
    pub interaction_id: RuntimeInteractionId,
    pub response: InteractionResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAgentRunEvents {
    pub target: AgentRunRuntimeTarget,
    pub after: Option<EventSequence>,
    pub include_transient: bool,
    pub transient_after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    pub stream_generation: Option<agentdash_agent_runtime_contract::RuntimeDriverGeneration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppendAgentRunPresentation {
    pub target: AgentRunRuntimeTarget,
    pub producer: String,
    pub idempotency_key: agentdash_agent_runtime_contract::IdempotencyKey,
    pub events: Vec<agentdash_agent_runtime_contract::RuntimePresentationInput>,
}

#[derive(Debug, Error)]
pub enum AgentRunRuntimeError {
    #[error("AgentRun has no runtime binding")]
    BindingNotFound,
    #[error("AgentRun runtime binding failed: {0}")]
    Binding(#[from] AgentRunRuntimeBindingError),
    #[error("AgentRun runtime command failed: {0}")]
    Execute(#[from] RuntimeExecuteError),
    #[error("AgentRun runtime snapshot failed: {0}")]
    Snapshot(#[from] RuntimeSnapshotError),
    #[error("AgentRun runtime event read failed: {0}")]
    Events(#[from] RuntimeSubscribeError),
    #[error("AgentRun presentation append failed: {0}")]
    PresentationAppend(#[from] agentdash_agent_runtime_contract::RuntimePresentationAppendError),
    #[error("AgentRun runtime command guard targets another thread")]
    StaleThread,
    #[error("AgentRun active turn changed")]
    StaleActiveTurn,
    #[error("AgentRun active presentation turn changed")]
    StalePresentationTurn,
    #[error("AgentRun presentation input does not match the command")]
    InvalidPresentationInput,
    #[error("AgentRun runtime returned an unexpected snapshot result")]
    UnexpectedSnapshot,
    #[error("AgentRun client command id is empty")]
    EmptyClientCommandId,
    #[error("AgentRun client command id is already bound to a different command")]
    ClientCommandConflict,
}

#[async_trait]
pub trait AgentRunRuntime: Send + Sync {
    async fn inspect(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError>;

    async fn send_message(
        &self,
        command: SendAgentRunMessage,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn fork_runtime(
        &self,
        command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError>;

    async fn compact_context(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    /// Persist the compaction request in the canonical runtime work queue. The durable worker
    /// only claims it after the active turn has reached a terminal state.
    async fn schedule_context_compaction(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.compact_context(command).await
    }

    async fn inspect_operation_terminal(
        &self,
        _operation_id: RuntimeOperationId,
    ) -> Result<
        Option<agentdash_agent_runtime_contract::RuntimeOperationTerminal>,
        AgentRunRuntimeError,
    > {
        Ok(None)
    }

    async fn steer_active_turn(
        &self,
        command: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn interrupt_active_turn(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn resolve_interaction(
        &self,
        command: ResolveAgentRunInteraction,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn read_context(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeContextView, AgentRunRuntimeError>;

    async fn read_events(
        &self,
        query: ReadAgentRunEvents,
    ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError>;

    async fn append_presentation(
        &self,
        command: AppendAgentRunPresentation,
    ) -> Result<
        agentdash_agent_runtime_contract::RuntimePresentationAppendReceipt,
        AgentRunRuntimeError,
    >;
}

pub struct ManagedAgentRunRuntime {
    gateway: Arc<dyn AgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    presentation_plans: Arc<dyn AgentRunRuntimePresentationPlanStore>,
    turn_start_context: Arc<dyn AgentRunTurnStartContextSource>,
}

impl ManagedAgentRunRuntime {
    pub fn new(
        gateway: Arc<dyn AgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
        presentation_plans: Arc<dyn AgentRunRuntimePresentationPlanStore>,
        turn_start_context: Arc<dyn AgentRunTurnStartContextSource>,
    ) -> Self {
        Self {
            gateway,
            bindings,
            provisioner,
            presentation_plans,
            turn_start_context,
        }
    }

    async fn pending_turn_start_presentation(
        &self,
        binding: &AgentRunRuntimeBinding,
        client_command_id: &str,
        turn_id: &agentdash_agent_runtime_contract::PresentationTurnId,
    ) -> Result<(Vec<RuntimePresentationInput>, Vec<String>), AgentRunRuntimeError> {
        let facts = self
            .turn_start_context
            .take_turn_start_context(&binding.binding_id)
            .await?;
        let notice_ids = facts
            .notices
            .iter()
            .map(|notice| notice.id.clone())
            .collect::<Vec<_>>();
        let runtime_revision = facts
            .runtime_snapshot
            .as_ref()
            .map_or(0, |snapshot| snapshot.revision);
        let owners = facts
            .runtime_snapshot
            .as_ref()
            .and_then(|runtime| runtime.snapshot.run_context.as_ref())
            .map(|context| {
                vec![format!(
                    "- scope: {} project: {}",
                    context.scope, context.project_id
                )]
            })
            .unwrap_or_default();
        let mut frames = facts
            .notices
            .into_iter()
            .filter_map(|notice| {
                if let Some(facts) = notice.presentation {
                    let source = match notice.source {
                        agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate => {
                            agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate
                        }
                        agentdash_spi::hooks::RuntimeEventSource::CompanionResult => {
                            agentdash_agent_protocol::ContextFrameSource::CompanionResult
                        }
                    };
                    let facts = match facts {
                        agentdash_spi::hooks::HookContextPresentationFacts::SystemNotice {
                            title,
                            summary,
                            body,
                        } => agentdash_agent_runtime::HookSemanticPresentationFacts::SystemNotice {
                            title,
                            summary,
                            body,
                        },
                        agentdash_spi::hooks::HookContextPresentationFacts::AssignmentInjection {
                            title,
                            summary,
                            injections,
                        } => agentdash_agent_runtime::HookSemanticPresentationFacts::AssignmentInjection {
                            title,
                            summary,
                            injections: injections
                                .into_iter()
                                .map(|injection| agentdash_agent_protocol::RuntimeHookInjectionEntry {
                                    slot: injection.slot,
                                    content: injection.content,
                                    source: injection.source,
                                    context_usage_kind: None,
                                })
                                .collect(),
                        },
                    };
                    return agentdash_agent_runtime::project_hook_presentation(
                            &agentdash_agent_runtime::ContextProjectionIdentity {
                                operation_id: notice.id.clone(),
                                source_frame_id: notice.id.clone(),
                                source_frame_revision: runtime_revision,
                                recorded_at_ms: notice.created_at_ms,
                            },
                            source,
                            facts,
                        )
                        .ok();
                }
                agentdash_agent_runtime::project_system_notice(
                    &agentdash_agent_runtime::SystemNoticePresentationFacts {
                        id: notice.id,
                        source: match notice.source {
                            agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate => {
                                agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate
                            }
                            agentdash_spi::hooks::RuntimeEventSource::CompanionResult => {
                                agentdash_agent_protocol::ContextFrameSource::CompanionResult
                            }
                        },
                        content: notice.content,
                        created_at_ms: notice.created_at_ms,
                    },
                )
            })
            .collect::<Vec<_>>();
        frames.extend(
            facts
                .pending_actions
                .into_iter()
                .filter(|action| !action.summary.trim().is_empty() || !action.injections.is_empty())
                .enumerate()
                .filter_map(|(index, action)| {
                    agentdash_agent_runtime::project_pending_action(
                        &agentdash_agent_runtime::ContextProjectionIdentity {
                            operation_id: format!(
                                "{}:pending:{index}",
                                Self::operation_identity(&binding.target, client_command_id)
                                    .expect("validated operation")
                            ),
                            source_frame_id: action.id.clone(),
                            source_frame_revision: if runtime_revision == 0 {
                                binding.surface.surface_revision.0
                            } else {
                                runtime_revision
                            },
                            recorded_at_ms: action.created_at_ms,
                        },
                        &agentdash_agent_runtime::PendingActionPresentationFacts {
                            source: match action.source {
                                agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate => agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
                                agentdash_spi::hooks::RuntimeEventSource::CompanionResult => agentdash_agent_protocol::ContextFrameSource::CompanionResult,
                            },
                            title: action.title,
                            summary: action.summary,
                            action_id: action.id,
                            action_type: action.action_type,
                            status: match action.status {
                                agentdash_spi::hooks::HookPendingActionStatus::Pending => "pending",
                                agentdash_spi::hooks::HookPendingActionStatus::Resolved => "resolved",
                            }.to_string(),
                            runtime_revision,
                            turn_id: action.turn_id,
                            owners: owners.clone(),
                            injections: action
                            .injections
                            .into_iter()
                            .map(|injection| {
                                let context_usage_kind = agentdash_spi::ASSIGNMENT_CONTEXT_SLOTS
                                    .contains(&injection.slot.as_str())
                                    .then(|| {
                                        agentdash_spi::context_usage_kind::SYSTEM_DEVELOPER
                                            .to_string()
                                    });
                                agentdash_agent_protocol::RuntimeHookInjectionEntry {
                                    slot: injection.slot,
                                    source: injection.source,
                                    content: injection.content,
                                    context_usage_kind,
                                }
                            })
                            .collect(),
                        },
                    )
                }),
        );
        let presentation = frames
            .into_iter()
            .enumerate()
            .map(|(index, frame)| RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(binding.presentation_thread_id.to_string()),
                    source_turn_id: Some(turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some(client_command_id.to_string()),
                    source_entry_index: Some(
                        u32::try_from(index + 2).expect("turn-start context is bounded"),
                    ),
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(Box::new(
                            agentdash_agent_protocol::ContextFrameChanged { frame },
                        )),
                    ),
                ),
            })
            .collect();
        Ok((presentation, notice_ids))
    }

    async fn bootstrap_presentation(
        &self,
        binding: &AgentRunRuntimeBinding,
        client_command_id: &str,
        turn_id: &agentdash_agent_runtime_contract::PresentationTurnId,
    ) -> Result<Vec<RuntimePresentationInput>, AgentRunRuntimeError> {
        use agentdash_agent_protocol::{BackboneEvent, ContextFrameChanged, PlatformEvent};
        let plan = self
            .presentation_plans
            .load_exact_presentation_plan(
                &binding.binding_id,
                binding.surface.surface_revision,
                &binding.surface.surface_digest,
            )
            .await?;
        Ok(plan
            .bootstrap_frames
            .into_iter()
            .enumerate()
            .map(|(index, frame)| RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(binding.presentation_thread_id.to_string()),
                    source_turn_id: Some(turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some(client_command_id.to_string()),
                    source_entry_index: Some(
                        u32::try_from(index + 1).expect("presentation plan is bounded"),
                    ),
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
                        ContextFrameChanged { frame },
                    ))),
                ),
            })
            .collect())
    }

    async fn fork_runtime_inner(
        &self,
        command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        if self.bindings.load(&command.source_target).await?.is_none() {
            return Err(AgentRunRuntimeError::BindingNotFound);
        }
        Ok(self
            .provisioner
            .provision(&AgentRunRuntimeProvisionRequest {
                target: command.child_target,
                presentation_thread_id: command.child_presentation_thread_id,
                identity: command.identity,
                backend_selection: command.backend_selection,
                fork: Some(AgentRunRuntimeForkSource {
                    source_target: command.source_target,
                    through_source_turn_id: command.through_source_turn_id,
                }),
                terminal_hook_effect_binding: None,
            })
            .await?)
    }

    fn operation_identity(
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
    ) -> Result<String, AgentRunRuntimeError> {
        let client_command_id = client_command_id.trim();
        if client_command_id.is_empty() {
            return Err(AgentRunRuntimeError::EmptyClientCommandId);
        }
        Ok(format!(
            "agentrun-{}-{}-{client_command_id}",
            target.run_id, target.agent_id
        ))
    }

    async fn replay_existing<F>(
        &self,
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
        actor: &RuntimeActor,
        presentation: &[RuntimePresentationInput],
        matches_command: F,
    ) -> Result<Option<OperationReceipt>, AgentRunRuntimeError>
    where
        F: FnOnce(&RuntimeCommand) -> bool,
    {
        let operation_id =
            RuntimeOperationId::new(Self::operation_identity(target, client_command_id)?)
                .expect("non-empty AgentRun operation identity");
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Operation { operation_id })
            .await
        {
            Ok(RuntimeSnapshotResult::Operation { operation }) => {
                if &operation.actor != actor
                    || !operation.presentation.starts_with(presentation)
                    || !matches_command(&operation.command)
                {
                    return Err(AgentRunRuntimeError::ClientCommandConflict);
                }
                Ok(Some(
                    self.gateway
                        .execute(RuntimeCommandEnvelope {
                            presentation: operation.presentation,
                            meta: OperationMeta {
                                operation_id: operation.operation_id,
                                idempotency_key: operation.idempotency_key,
                                expected_thread_revision: None,
                                actor: operation.actor,
                            },
                            command: operation.command,
                        })
                        .await?,
                ))
            }
            Err(RuntimeSnapshotError::NotFound) => Ok(None),
            Ok(_) => Err(AgentRunRuntimeError::UnexpectedSnapshot),
            Err(error) => Err(error.into()),
        }
    }

    async fn binding(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        self.bindings
            .load(target)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)
    }

    async fn snapshot_for(
        &self,
        binding: &AgentRunRuntimeBinding,
    ) -> Result<Option<RuntimeSnapshot>, AgentRunRuntimeError> {
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: binding.thread_id.clone(),
                at_revision: None,
            })
            .await
        {
            Ok(RuntimeSnapshotResult::Thread { snapshot }) => Ok(Some(*snapshot)),
            Ok(_) => Err(AgentRunRuntimeError::UnexpectedSnapshot),
            Err(RuntimeSnapshotError::NotFound) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn reconcile_committed_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
        binding: &AgentRunRuntimeBinding,
    ) -> Result<(), AgentRunRuntimeError> {
        let Some(intent) = self.bindings.load_active_recovery(target).await? else {
            return Ok(());
        };
        if intent.state == AgentRunRuntimeRecoveryState::HostBound
            && intent.proposed_binding_id == binding.binding_id
        {
            self.bindings
                .advance_recovery(
                    &intent.id,
                    AgentRunRuntimeRecoveryState::HostBound,
                    AgentRunRuntimeRecoveryState::Committed,
                    None,
                )
                .await?;
        }
        Ok(())
    }

    fn envelope(
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
        expected_thread_revision: Option<agentdash_agent_runtime_contract::RuntimeRevision>,
        actor: RuntimeActor,
        command: RuntimeCommand,
    ) -> Result<RuntimeCommandEnvelope, AgentRunRuntimeError> {
        let identity = Self::operation_identity(target, client_command_id)?;
        Ok(RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: RuntimeOperationId::new(identity.clone())
                    .expect("non-empty AgentRun operation identity"),
                idempotency_key: IdempotencyKey::new(identity)
                    .expect("non-empty AgentRun idempotency identity"),
                expected_thread_revision,
                actor,
            },
            command,
        })
    }

    fn submission_presentation(
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
        presentation_thread_id: &PresentationThreadId,
        source_frame_revision: u64,
        input: AgentRunPresentationInput,
    ) -> Result<Vec<RuntimePresentationInput>, AgentRunRuntimeError> {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
        use agentdash_agent_protocol::{
            BackboneEvent, PlatformEvent, UserInputSubmittedNotification,
        };

        Self::operation_identity(target, client_command_id)?;
        let turn_id = input.turn_id().to_string();
        let started_at_seconds = input.started_at_seconds();
        let thread_id = presentation_thread_id.to_string();
        let system_delivery = match &input {
            AgentRunPresentationInput::SystemDelivery {
                launch_source,
                message,
                ..
            } => Some((*launch_source, message.clone())),
            _ => None,
        };
        let (first_event, source_item_id) = match input {
            AgentRunPresentationInput::UserSubmission {
                item_id,
                content,
                source,
                submission_kind,
                ..
            } => {
                let item_id = item_id.to_string();
                (
                    BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                        thread_id.clone(),
                        turn_id.clone(),
                        item_id.clone(),
                        submission_kind,
                        source,
                        content,
                    )),
                    Some(item_id),
                )
            }
            AgentRunPresentationInput::SystemDelivery {
                turn_id,
                launch_source,
                message,
                ..
            } => (
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "system_message".to_string(),
                    value: system_delivery_value(launch_source, turn_id.as_str(), &message),
                }),
                None,
            ),
        };
        let turn_started = codex::TurnStartedNotification {
            thread_id: thread_id.clone(),
            turn: codex::Turn {
                completed_at: Some(None),
                duration_ms: Some(None),
                error: Some(None),
                id: turn_id.clone(),
                items: Vec::new(),
                items_view: agentdash_agent_protocol::generated::codex_v2::server_notification::TurnItemsView::NotLoaded,
                started_at: Some(Some(started_at_seconds)),
                status: codex::TurnStatus::InProgress,
            },
        };
        let coordinate = |source_item_id, source_entry_index| RuntimePresentationCoordinate {
            runtime_turn_id: None,
            runtime_item_id: None,
            interaction_id: None,
            source_thread_id: Some(thread_id.clone()),
            source_turn_id: Some(turn_id.clone()),
            source_item_id,
            source_request_id: Some(client_command_id.to_string()),
            source_entry_index,
        };
        let mut presentation = vec![
            RuntimePresentationInput {
                coordinate: coordinate(source_item_id, Some(0)),
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    first_event,
                ),
            },
            RuntimePresentationInput {
                coordinate: coordinate(None, None),
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    BackboneEvent::TurnStarted(turn_started),
                ),
            },
        ];
        if let Some((launch_source, message)) = system_delivery
            && let Some(frame) = agentdash_agent_runtime::project_system_delivery(
                &agentdash_agent_runtime::ContextProjectionIdentity {
                    operation_id: Self::operation_identity(target, client_command_id)?,
                    source_frame_id: turn_id.clone(),
                    source_frame_revision,
                    recorded_at_ms: started_at_seconds.saturating_mul(1_000),
                },
                &agentdash_agent_runtime::SystemDeliveryPresentationFacts {
                    source: if matches!(
                        launch_source,
                        LaunchPresentationSource::CompanionDispatch
                            | LaunchPresentationSource::CompanionParentResume
                    ) {
                        agentdash_agent_protocol::ContextFrameSource::CompanionResult
                    } else {
                        agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate
                    },
                    session_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    delivery_kind: launch_source.system_delivery_kind().to_string(),
                    source_kind: launch_source.tag().to_string(),
                    content: message,
                },
            )
        {
            presentation.push(RuntimePresentationInput {
                coordinate: coordinate(None, Some(1)),
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
                        agentdash_agent_protocol::ContextFrameChanged { frame },
                    ))),
                ),
            });
        }
        Ok(presentation)
    }

    fn user_steer_presentation(
        client_command_id: &str,
        presentation_thread_id: &PresentationThreadId,
        input: AgentRunPresentationInput,
    ) -> Result<Vec<RuntimePresentationInput>, AgentRunRuntimeError> {
        use agentdash_agent_protocol::{BackboneEvent, UserInputSubmittedNotification};

        let AgentRunPresentationInput::UserSubmission {
            turn_id,
            item_id,
            content,
            source,
            submission_kind,
            ..
        } = input
        else {
            return Err(AgentRunRuntimeError::InvalidPresentationInput);
        };
        let thread_id = presentation_thread_id.to_string();
        let turn_id = turn_id.to_string();
        let item_id = item_id.to_string();
        Ok(vec![RuntimePresentationInput {
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some(thread_id.clone()),
                source_turn_id: Some(turn_id.clone()),
                source_item_id: Some(item_id.clone()),
                source_request_id: Some(client_command_id.to_string()),
                source_entry_index: Some(0),
            },
            event: ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                    thread_id,
                    turn_id,
                    item_id,
                    submission_kind,
                    source,
                    content,
                )),
            ),
        }])
    }

    async fn guarded_binding(
        &self,
        command: &GuardedAgentRunCommand,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        let binding = self.coordinate_binding(command).await?;
        let snapshot = self
            .snapshot_for(&binding)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)?;
        if snapshot.active_turn_id != command.guard.expected_active_turn_id {
            return Err(AgentRunRuntimeError::StaleActiveTurn);
        }
        Ok(binding)
    }

    async fn coordinate_binding(
        &self,
        command: &GuardedAgentRunCommand,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        let binding = self.binding(&command.target).await?;
        if binding.thread_id != command.guard.thread_id {
            return Err(AgentRunRuntimeError::StaleThread);
        }
        Ok(binding)
    }

    async fn enqueue_context_compaction(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command).await?;
        let compaction_id = ContextCompactionId::new(format!(
            "compaction-{}-{}-{}",
            command.target.run_id, command.target.agent_id, command.client_command_id
        ))
        .expect("non-empty compaction identity");
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                &[],
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::ContextCompact {
                            thread_id,
                            compaction_id: existing_id,
                            ..
                        } if thread_id == &command.guard.thread_id && existing_id == &compaction_id
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command).await?;
        let snapshot = self
            .snapshot_for(&binding)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.target,
                &command.client_command_id,
                Some(command.guard.expected_revision),
                command.actor,
                RuntimeCommand::ContextCompact {
                    thread_id: binding.thread_id,
                    compaction_id,
                    trigger: ContextCompactionTrigger::Manual,
                    base_checkpoint_id: snapshot.active_checkpoint_id,
                    expected_context_revision: snapshot.context_revision,
                },
            )?)
            .await?)
    }
}

fn bounded_system_delivery_summary(message: &str) -> String {
    const MAX_CHARS: usize = 2_000;
    let trimmed = message.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let mut summary = trimmed.chars().take(MAX_CHARS).collect::<String>();
    summary.push_str("...");
    summary
}

fn system_delivery_value(
    source: LaunchPresentationSource,
    turn_id: &str,
    message: &str,
) -> serde_json::Value {
    let summary = bounded_system_delivery_summary(message);
    serde_json::json!({
        "kind": source.system_delivery_kind(),
        "origin": "system",
        "source": {
            "namespace": "runtime_launch",
            "kind": source.tag(),
            "actor": source.system_delivery_actor(),
        },
        "status": "delivered",
        "summary": summary,
        "message": summary,
        "turn_id": turn_id,
    })
}

fn finalize_launch_context_frames(
    presentation: &mut Vec<RuntimePresentationInput>,
    target: &agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget,
) {
    use agentdash_agent_protocol::{ContextAgentConsumptionMode, ContextConnectorProfile};

    let mut non_frames = Vec::with_capacity(presentation.len());
    let mut frames = Vec::new();
    for mut input in presentation.drain(..) {
        if let Some(changed) = context_frame_changed_mut(&mut input) {
            let profile_id = target.profile_id();
            let mode =
                launch_consumption_mode(target, changed.frame.delivery_metadata.model_channel);
            changed.frame.delivery_metadata.connector_profile = ContextConnectorProfile {
                profile_id: profile_id.clone(),
                declared_consumption_modes: vec![
                    ContextAgentConsumptionMode::Consume,
                    ContextAgentConsumptionMode::Ignore,
                    ContextAgentConsumptionMode::ConnectorNative,
                    ContextAgentConsumptionMode::SystemAppend,
                ],
            };
            changed.frame.delivery_metadata.agent_consumption.target = profile_id.clone();
            changed.frame.delivery_metadata.agent_consumption.mode = mode;
            changed.frame.delivery_metadata.agent_consumption.reason =
                format!("{profile_id}_{}_delivery", changed.frame.kind.as_key());
            let kind = changed.frame.kind;
            frames.push((context_frame_order(kind), input));
        } else {
            non_frames.push(input);
        }
    }
    frames.sort_by_key(|(order, _)| *order);
    for (index, (_, mut input)) in frames.into_iter().enumerate() {
        input.coordinate.source_entry_index = Some(
            u32::try_from(index + 1).expect("turn ContextFrame presentation batch is bounded"),
        );
        non_frames.push(input);
    }
    *presentation = non_frames;
}

fn launch_consumption_mode(
    target: &agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget,
    model_channel: agentdash_agent_protocol::ContextModelChannel,
) -> agentdash_agent_protocol::ContextAgentConsumptionMode {
    use agentdash_agent_protocol::{ContextAgentConsumptionMode, ContextModelChannel};
    if target.connector_id == "pi-agent" {
        return ContextAgentConsumptionMode::Consume;
    }
    match model_channel {
        ContextModelChannel::System | ContextModelChannel::Developer => {
            ContextAgentConsumptionMode::SystemAppend
        }
        ContextModelChannel::Ignored => ContextAgentConsumptionMode::Ignore,
        ContextModelChannel::AuditOnly => ContextAgentConsumptionMode::AuditOnly,
        ContextModelChannel::Context | ContextModelChannel::User => {
            ContextAgentConsumptionMode::Consume
        }
    }
}

fn context_frame_changed_mut(
    input: &mut RuntimePresentationInput,
) -> Option<&mut agentdash_agent_protocol::ContextFrameChanged> {
    let agentdash_agent_protocol::BackboneEvent::Platform(
        agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(changed),
    ) = &mut input.event.event
    else {
        return None;
    };
    Some(changed)
}

const fn context_frame_order(kind: agentdash_agent_protocol::ContextFrameKind) -> u8 {
    use agentdash_agent_protocol::ContextFrameKind;
    match kind {
        ContextFrameKind::CapabilityStateDelta => 0,
        ContextFrameKind::AssignmentContext => 1,
        ContextFrameKind::SystemDelivery => 2,
        ContextFrameKind::Identity => 3,
        ContextFrameKind::UserContext => 4,
        ContextFrameKind::Environment => 5,
        ContextFrameKind::SystemGuidelines => 6,
        ContextFrameKind::MemoryContext => 7,
        ContextFrameKind::SystemNotice => 8,
        ContextFrameKind::PendingAction => 9,
        ContextFrameKind::CompactionSummary | ContextFrameKind::AutoResume => 10,
    }
}

#[async_trait]
impl AgentRunRuntime for ManagedAgentRunRuntime {
    async fn inspect(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError> {
        let binding = self.bindings.load(&target).await?;
        let snapshot = match &binding {
            Some(binding) => self.snapshot_for(binding).await?,
            None => None,
        };
        let latest_recovery = self.bindings.load_latest_recovery(&target).await?;
        let recovery = match latest_recovery.as_ref().map(|intent| intent.state) {
            Some(
                AgentRunRuntimeRecoveryState::Prepared | AgentRunRuntimeRecoveryState::HostBound,
            ) => AgentRunRuntimeRecoverySummary::Recovering,
            Some(AgentRunRuntimeRecoveryState::Failed) => {
                AgentRunRuntimeRecoverySummary::RecoveryFailed
            }
            _ if snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.status == RuntimeThreadStatus::Lost) =>
            {
                AgentRunRuntimeRecoverySummary::Lost
            }
            _ => AgentRunRuntimeRecoverySummary::Active,
        };
        let binding_epoch = binding.as_ref().map(|binding| binding.binding_epoch);
        Ok(AgentRunRuntimeView {
            target,
            binding,
            snapshot,
            binding_epoch,
            recovery,
        })
    }

    async fn append_presentation(
        &self,
        command: AppendAgentRunPresentation,
    ) -> Result<
        agentdash_agent_runtime_contract::RuntimePresentationAppendReceipt,
        AgentRunRuntimeError,
    > {
        let binding = self.binding(&command.target).await?;
        Ok(self
            .gateway
            .append_presentation(
                agentdash_agent_runtime_contract::RuntimePresentationAppendRequest {
                    runtime_thread_id: binding.thread_id,
                    producer: command.producer,
                    idempotency_key: command.idempotency_key,
                    events: command.events,
                },
            )
            .await?)
    }

    async fn fork_runtime(
        &self,
        command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        self.fork_runtime_inner(command).await
    }

    async fn send_message(
        &self,
        command: SendAgentRunMessage,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Self::operation_identity(&command.target, &command.client_command_id)?;
        let mut binding = match self.bindings.load(&command.target).await? {
            Some(binding) => binding,
            None => {
                self.provisioner
                    .provision(&AgentRunRuntimeProvisionRequest {
                        target: command.target.clone(),
                        presentation_thread_id: command.presentation_thread_id.clone(),
                        identity: command.identity.clone(),
                        backend_selection: command.backend_selection.clone(),
                        fork: None,
                        terminal_hook_effect_binding: None,
                    })
                    .await?
            }
        };
        if binding.presentation_thread_id != command.presentation_thread_id {
            return Err(AgentRunRuntimeError::StaleThread);
        }
        let mut presentation = Self::submission_presentation(
            &command.target,
            &command.client_command_id,
            &binding.presentation_thread_id,
            binding.surface.surface_revision.0,
            command.presentation_input.clone(),
        )?;
        self.reconcile_committed_recovery(&command.target, &binding)
            .await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                &presentation,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::ThreadStart { thread_id, input, .. }
                            | RuntimeCommand::TurnStart { thread_id, input, .. }
                            if thread_id == &binding.thread_id && input == &command.input
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut snapshot = self.snapshot_for(&binding).await?;
        if snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.status == RuntimeThreadStatus::Lost)
        {
            let revision = snapshot.as_ref().expect("lost snapshot exists").revision;
            binding = self.provisioner.recover(&binding, revision).await?;
            snapshot = self.snapshot_for(&binding).await?;
        }
        let expected = snapshot.as_ref().map(|snapshot| snapshot.revision);
        let acknowledged_notice_ids;
        if snapshot.is_none() {
            presentation.extend(
                self.bootstrap_presentation(
                    &binding,
                    &command.client_command_id,
                    command.presentation_input.turn_id(),
                )
                .await?,
            );
        }
        let (turn_start_presentation, notice_ids) = self
            .pending_turn_start_presentation(
                &binding,
                &command.client_command_id,
                command.presentation_input.turn_id(),
            )
            .await?;
        presentation.extend(turn_start_presentation);
        acknowledged_notice_ids = notice_ids;
        finalize_launch_context_frames(&mut presentation, &binding.context_delivery_target);
        let acknowledged_binding_id = binding.binding_id.clone();
        let runtime_command = match snapshot {
            None => RuntimeCommand::ThreadStart {
                thread_id: binding.thread_id.clone(),
                presentation_thread_id: binding.presentation_thread_id.clone(),
                presentation_turn_id: Some(command.presentation_input.turn_id().clone()),
                binding_id: binding.binding_id.clone(),
                driver_generation: binding.driver_generation,
                source_thread_id: binding.source_thread_id.clone(),
                profile_digest: binding.profile_digest.clone(),
                bound_profile: Box::new(binding.bound_profile.clone()),
                input: command.input,
                surface: Box::new(binding.surface),
                settings_revision: binding.settings_revision,
            },
            Some(_) => RuntimeCommand::TurnStart {
                thread_id: binding.thread_id,
                presentation_turn_id: command.presentation_input.turn_id().clone(),
                input: command.input,
            },
        };
        let mut envelope = Self::envelope(
            &command.target,
            &command.client_command_id,
            expected,
            command.actor,
            runtime_command,
        )?;
        envelope.presentation = presentation;
        let receipt = self.gateway.execute(envelope).await?;
        self.turn_start_context
            .acknowledge_turn_start_context(&acknowledged_binding_id, &acknowledged_notice_ids)
            .await?;
        Ok(receipt)
    }

    async fn compact_context(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.enqueue_context_compaction(command).await
    }

    async fn schedule_context_compaction(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.enqueue_context_compaction(command).await
    }

    async fn inspect_operation_terminal(
        &self,
        operation_id: RuntimeOperationId,
    ) -> Result<
        Option<agentdash_agent_runtime_contract::RuntimeOperationTerminal>,
        AgentRunRuntimeError,
    > {
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Operation { operation_id })
            .await
        {
            Ok(RuntimeSnapshotResult::Operation { operation }) => Ok(operation.terminal),
            Err(RuntimeSnapshotError::NotFound) => Ok(None),
            Ok(_) => Err(AgentRunRuntimeError::UnexpectedSnapshot),
            Err(error) => Err(error.into()),
        }
    }

    async fn steer_active_turn(
        &self,
        command: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        let binding = self.coordinate_binding(&command.command).await?;
        let presentation = Self::user_steer_presentation(
            &command.command.client_command_id,
            &binding.presentation_thread_id,
            command.presentation_input.clone(),
        )?;
        if let Some(receipt) = self
            .replay_existing(
                &command.command.target,
                &command.command.client_command_id,
                &command.command.actor,
                &presentation,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::TurnSteer {
                            thread_id,
                            expected_turn_id,
                            input,
                        } if thread_id == &command.command.guard.thread_id
                            && Some(expected_turn_id) == command.command.guard.expected_active_turn_id.as_ref()
                            && input == &command.input
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command.command).await?;
        let snapshot = self
            .snapshot_for(&binding)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)?;
        if snapshot.active_presentation_turn_id.as_ref()
            != Some(command.presentation_input.turn_id())
        {
            return Err(AgentRunRuntimeError::StalePresentationTurn);
        }
        let turn_id = command
            .command
            .guard
            .expected_active_turn_id
            .clone()
            .ok_or(AgentRunRuntimeError::StaleActiveTurn)?;
        let mut envelope = Self::envelope(
            &command.command.target,
            &command.command.client_command_id,
            Some(command.command.guard.expected_revision),
            command.command.actor,
            RuntimeCommand::TurnSteer {
                thread_id: binding.thread_id,
                expected_turn_id: turn_id,
                input: command.input,
            },
        )?;
        envelope.presentation = presentation;
        Ok(self.gateway.execute(envelope).await?)
    }

    async fn interrupt_active_turn(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command).await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                &[],
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::TurnInterrupt {
                            thread_id,
                            expected_turn_id,
                        } if thread_id == &command.guard.thread_id
                            && Some(expected_turn_id) == command.guard.expected_active_turn_id.as_ref()
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command).await?;
        let turn_id = command
            .guard
            .expected_active_turn_id
            .clone()
            .ok_or(AgentRunRuntimeError::StaleActiveTurn)?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.target,
                &command.client_command_id,
                Some(command.guard.expected_revision),
                command.actor,
                RuntimeCommand::TurnInterrupt {
                    thread_id: binding.thread_id,
                    expected_turn_id: turn_id,
                },
            )?)
            .await?)
    }

    async fn resolve_interaction(
        &self,
        command: ResolveAgentRunInteraction,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command.command).await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.command.target,
                &command.command.client_command_id,
                &command.command.actor,
                &[],
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::InteractionRespond {
                            thread_id,
                            interaction_id,
                            response,
                        } if thread_id == &command.command.guard.thread_id
                            && interaction_id == &command.interaction_id
                            && response == &command.response
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command.command).await?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.command.target,
                &command.command.client_command_id,
                Some(command.command.guard.expected_revision),
                command.command.actor,
                RuntimeCommand::InteractionRespond {
                    thread_id: binding.thread_id,
                    interaction_id: command.interaction_id,
                    response: command.response,
                },
            )?)
            .await?)
    }

    async fn read_context(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeContextView, AgentRunRuntimeError> {
        let binding = self.binding(&target).await?;
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Context {
                thread_id: binding.thread_id,
                at_context_revision: None,
            })
            .await?
        {
            RuntimeSnapshotResult::Context { context } => Ok(*context),
            _ => Err(AgentRunRuntimeError::UnexpectedSnapshot),
        }
    }

    async fn read_events(
        &self,
        query: ReadAgentRunEvents,
    ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError> {
        let binding = self.binding(&query.target).await?;
        Ok(self
            .gateway
            .events(agentdash_agent_runtime_contract::RuntimeEventSubscription {
                thread_id: binding.thread_id,
                after: query.after,
                include_transient: query.include_transient,
                transient_after: query.transient_after,
                stream_generation: query.stream_generation,
            })
            .await?)
    }
}

#[cfg(test)]
mod launch_delivery_tests {
    use super::launch_consumption_mode;
    use agentdash_agent_protocol::{ContextAgentConsumptionMode, ContextModelChannel};
    use agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget;

    #[test]
    fn pi_agent_consumes_every_model_channel() {
        let target = AgentRunContextDeliveryTarget {
            connector_id: "pi-agent".to_string(),
            executor: "PI_AGENT".to_string(),
        };
        for channel in [
            ContextModelChannel::System,
            ContextModelChannel::Developer,
            ContextModelChannel::Context,
            ContextModelChannel::User,
            ContextModelChannel::AuditOnly,
            ContextModelChannel::Ignored,
        ] {
            assert_eq!(
                launch_consumption_mode(&target, channel),
                ContextAgentConsumptionMode::Consume
            );
        }
    }

    #[test]
    fn codex_maps_model_channels_to_declared_consumption() {
        let target = AgentRunContextDeliveryTarget {
            connector_id: "codex".to_string(),
            executor: "CODEX".to_string(),
        };
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::System),
            ContextAgentConsumptionMode::SystemAppend
        );
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::Developer),
            ContextAgentConsumptionMode::SystemAppend
        );
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::Ignored),
            ContextAgentConsumptionMode::Ignore
        );
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::AuditOnly),
            ContextAgentConsumptionMode::AuditOnly
        );
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::Context),
            ContextAgentConsumptionMode::Consume
        );
        assert_eq!(
            launch_consumption_mode(&target, ContextModelChannel::User),
            ContextAgentConsumptionMode::Consume
        );
    }
}
