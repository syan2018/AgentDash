use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentSessionId, AgentTurnId, BranchId, CommandId, CompactionId,
    ContextDeliveryFidelity, ContextRevision, DashAgentRepository, DashAgentRepositoryState,
    DashAgentService, DashCommandRequest, DashCompactionRequest, DashCompactionResult,
    DashCompactor, DashConversationNamer, DashConversationNamingRequest, DashCoreError,
    DashExecutionCallbacks, DashExecutionConsistency, DashExecutionDependencies,
    DashExecutionEvent, DashFinishReason, DashProvider, DashProviderEvent, DashProviderEventStream,
    DashProviderRequest, DashPublicCommand, DashReceiptState, DashServiceError, DashSurface,
    DashSurfaceInstruction, DashTerminalOutcome, DashToolCall, DashToolCallbacks,
    DashToolDefinition, DashToolResult, EffectId, HistoryPayload, InitialContextContribution,
    InitialContextInstallation, InitialContextMode, NoopDashConversationNamer,
    NoopDashHistoryCallbacks,
};
use async_trait::async_trait;
use futures::stream;

#[derive(Default)]
struct RecordingDashRepository {
    state: tokio::sync::RwLock<Option<DashAgentRepositoryState>>,
}

#[async_trait]
impl DashAgentRepository for RecordingDashRepository {
    async fn initialize(&self, initial: DashAgentRepositoryState) -> Result<(), DashServiceError> {
        let mut state = self.state.write().await;
        if state.is_some() {
            return Err(DashServiceError::Conflict {
                message: "test repository already initialized".into(),
            });
        }
        *state = Some(initial);
        Ok(())
    }

    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError> {
        self.state
            .read()
            .await
            .clone()
            .ok_or_else(|| DashServiceError::InvalidState {
                message: "test repository is not initialized".into(),
            })
    }

    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError> {
        let mut state = self.state.write().await;
        if state.as_ref() != Some(&expected) {
            return Err(DashServiceError::Conflict {
                message: "test repository revision changed".into(),
            });
        }
        *state = Some(replacement);
        Ok(())
    }
}

async fn create_service(
    history: AgentHistory,
    execution: DashExecutionDependencies,
) -> DashAgentService {
    DashAgentService::create_with_repository(
        Arc::new(RecordingDashRepository::default()),
        history,
        None,
        execution,
    )
    .await
    .unwrap()
}

struct RetryableProvider;

#[async_trait]
impl DashProvider for RetryableProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(DashCoreError::Provider {
            code: "rate_limit".into(),
            message: "temporary provider failure".into(),
            retryable: true,
        })
    }
}

struct NoTools;

#[async_trait]
impl DashToolCallbacks for NoTools {
    async fn invoke(
        &self,
        _: &AgentTurnId,
        _: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        Err(DashCoreError::Tool {
            message: "provider fails before requesting a tool".into(),
            retryable: false,
        })
    }
}

struct SurfaceChangingProvider;

#[async_trait]
impl DashProvider for SurfaceChangingProvider {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        let events = match request.round {
            1 => vec![
                Ok(DashProviderEvent::ToolCall {
                    call: DashToolCall {
                        call_id: "create-canvas".into(),
                        name: "workspace_module_operate".into(),
                        arguments: serde_json::json!({"operation": "canvas.create"}),
                    },
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::ToolCalls,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ],
            2 => vec![
                Ok(DashProviderEvent::ToolCall {
                    call: DashToolCall {
                        call_id: "edit-canvas".into(),
                        name: "fs_apply_patch".into(),
                        arguments: serde_json::json!({"mount": "canvas:cvs-demo"}),
                    },
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::ToolCalls,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ],
            _ => vec![
                Ok(DashProviderEvent::TextDelta {
                    delta: "done".into(),
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::Stop,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ],
        };
        Ok(Box::pin(stream::iter(events)))
    }
}

struct FailureAfterEightToolRoundsProvider;

#[async_trait]
impl DashProvider for FailureAfterEightToolRoundsProvider {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        if request.round > 8 {
            return Err(DashCoreError::Provider {
                code: "scripted_provider_failure".into(),
                message: "provider failed after completed tool rounds".into(),
                retryable: false,
            });
        }
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: format!("partial answer {}", request.round),
            }),
            Ok(DashProviderEvent::ToolCall {
                call: DashToolCall {
                    call_id: format!("call-{}", request.round),
                    name: "inspect_capability".into(),
                    arguments: serde_json::json!({"round": request.round}),
                },
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::ToolCalls,
                input_tokens: 1,
                output_tokens: 1,
            }),
        ])))
    }
}

struct FailedTurnConversationNamer;

#[async_trait]
impl DashConversationNamer for FailedTurnConversationNamer {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError> {
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].content, "name the failed conversation");
        assert!(request.messages[1].content.starts_with("partial answer"));
        Ok("失败回合会话".to_owned())
    }
}

struct BlockingToolCallbacks {
    calls: tokio::sync::Mutex<Vec<String>>,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl BlockingToolCallbacks {
    fn new() -> Self {
        Self {
            calls: tokio::sync::Mutex::new(Vec::new()),
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        }
    }
}

#[async_trait]
impl DashToolCallbacks for BlockingToolCallbacks {
    async fn invoke(
        &self,
        _: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        self.calls.lock().await.push(call.call_id.clone());
        if call.call_id == "create-canvas" {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(DashToolResult {
            call_id: call.call_id,
            content: vec![agentdash_agent::ContentPart::text("ok")],
            is_error: false,
            details: None,
        })
    }
}

#[derive(Default)]
struct RecordingToolCallbacks {
    calls: tokio::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl DashToolCallbacks for RecordingToolCallbacks {
    async fn invoke(
        &self,
        _: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        self.calls.lock().await.push(call.call_id.clone());
        Ok(DashToolResult {
            call_id: call.call_id,
            content: vec![agentdash_agent::ContentPart::text("ok")],
            is_error: false,
            details: None,
        })
    }
}

#[tokio::test]
async fn active_turn_adopts_replaced_tool_callbacks_between_tool_invocations() {
    let original = Arc::new(BlockingToolCallbacks::new());
    let replacement = Arc::new(RecordingToolCallbacks::default());
    let service = create_service(
        AgentHistory::empty(
            AgentSessionId::new("surface-change-session"),
            BranchId::new("surface-change-branch"),
        ),
        DashExecutionDependencies {
            provider: Arc::new(SurfaceChangingProvider),
            tools: original.clone(),
            callbacks: Arc::new(NoCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(NoCompaction),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
    )
    .await;
    service
        .apply_surface(DashSurface {
            revision: 1,
            digest: "canvas-surface".into(),
            instructions: Vec::new(),
            tools: vec![
                DashToolDefinition {
                    name: "workspace_module_operate".into(),
                    description: "Operate a workspace module".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
                },
                DashToolDefinition {
                    name: "fs_apply_patch".into(),
                    description: "Apply a file patch".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FileChange,
                },
            ],
        })
        .await
        .unwrap();

    let executing = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .execute(DashCommandRequest {
                    command_id: CommandId::new("surface-change-command"),
                    effect_id: EffectId::new("surface-change-effect"),
                    command: DashPublicCommand::SubmitInput {
                        content: "create and edit a canvas".into(),
                    },
                })
                .await
        }
    });

    original.entered.notified().await;
    service.replace_tool_callbacks(replacement.clone()).await;
    original.release.notify_one();
    executing.await.unwrap().unwrap();

    assert_eq!(
        original.calls.lock().await.as_slice(),
        ["create-canvas"],
        "the accepted invocation must finish on its admitted callback route"
    );
    assert_eq!(
        replacement.calls.lock().await.as_slice(),
        ["edit-canvas"],
        "the next invocation in the same turn must observe the newly applied surface route"
    );
}

#[tokio::test]
async fn failed_turn_retains_each_completed_tool_call_in_native_history() {
    let service = create_service(
        AgentHistory::empty(
            AgentSessionId::new("failed-tool-history-session"),
            BranchId::new("failed-tool-history-branch"),
        ),
        DashExecutionDependencies {
            provider: Arc::new(FailureAfterEightToolRoundsProvider),
            tools: Arc::new(RecordingToolCallbacks::default()),
            callbacks: Arc::new(NoCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(NoCompaction),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
    )
    .await;
    service
        .apply_surface(DashSurface {
            revision: 1,
            digest: "round-limit-surface".into(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "inspect_capability".into(),
                description: "Inspect a capability".into(),
                input_schema: serde_json::json!({"type": "object"}),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
            }],
        })
        .await
        .unwrap();

    let receipt = service
        .execute(DashCommandRequest {
            command_id: CommandId::new("round-limit-command"),
            effect_id: EffectId::new("round-limit-effect"),
            command: DashPublicCommand::SubmitInput {
                content: "keep completed tool evidence".into(),
            },
        })
        .await
        .unwrap();

    assert!(matches!(
        receipt.state,
        DashReceiptState::Terminal(DashTerminalOutcome::Failed)
    ));
    let history = service.history().await.unwrap();
    assert_eq!(
        history
            .entries()
            .iter()
            .filter(|entry| matches!(entry.payload, HistoryPayload::ToolCall { .. }))
            .count(),
        8
    );
    assert_eq!(
        history
            .entries()
            .iter()
            .filter(|entry| matches!(entry.payload, HistoryPayload::ToolResult { .. }))
            .count(),
        8
    );
}

#[tokio::test]
async fn failed_terminal_turn_with_agent_output_still_initializes_thread_name() {
    let service = create_service(
        AgentHistory::empty(
            AgentSessionId::new("failed-turn-naming-session"),
            BranchId::new("failed-turn-naming-branch"),
        ),
        DashExecutionDependencies {
            provider: Arc::new(FailureAfterEightToolRoundsProvider),
            tools: Arc::new(RecordingToolCallbacks::default()),
            callbacks: Arc::new(NoCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(NoCompaction),
            conversation_namer: Arc::new(FailedTurnConversationNamer),
        },
    )
    .await;
    service
        .apply_surface(DashSurface {
            revision: 1,
            digest: "failed-turn-naming-surface".into(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "inspect_capability".into(),
                description: "Inspect a capability".into(),
                input_schema: serde_json::json!({"type": "object"}),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
            }],
        })
        .await
        .unwrap();

    let receipt = service
        .execute(DashCommandRequest {
            command_id: CommandId::new("failed-turn-naming-command"),
            effect_id: EffectId::new("failed-turn-naming-effect"),
            command: DashPublicCommand::SubmitInput {
                content: "name the failed conversation".into(),
            },
        })
        .await
        .unwrap();

    assert!(matches!(
        receipt.state,
        DashReceiptState::Terminal(DashTerminalOutcome::Failed)
    ));
    let history = service.history().await.unwrap();
    assert_eq!(
        history.state().unwrap().thread_name.as_deref(),
        Some("失败回合会话")
    );
    assert_eq!(
        history
            .entries()
            .iter()
            .filter(|entry| matches!(entry.payload, HistoryPayload::ThreadNameChanged { .. }))
            .count(),
        1
    );
}

struct NoCallbacks;

#[async_trait]
impl DashExecutionCallbacks for NoCallbacks {
    async fn emit(&self, _: DashExecutionEvent) -> Result<(), DashCoreError> {
        Ok(())
    }
}

struct NoCompaction;

#[async_trait]
impl DashCompactor for NoCompaction {
    async fn compact(
        &self,
        _: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        Ok(DashCompactionResult {
            revision: ContextRevision::new("unused"),
            summary: "unused".into(),
            retained_from: None,
        })
    }
}

#[tokio::test]
async fn retryable_provider_failure_is_terminal_and_inspectable_outside_session() {
    let service = create_service(
        AgentHistory::empty(
            AgentSessionId::new("retry-session"),
            BranchId::new("retry-branch"),
        ),
        DashExecutionDependencies {
            provider: Arc::new(RetryableProvider),
            tools: Arc::new(NoTools),
            callbacks: Arc::new(NoCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(NoCompaction),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
    )
    .await;
    let effect_id = EffectId::new("retry-effect");
    let receipt = service
        .execute(DashCommandRequest {
            command_id: CommandId::new("retry-command"),
            effect_id: effect_id.clone(),
            command: DashPublicCommand::SubmitInput {
                content: "question".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        DashReceiptState::Terminal(DashTerminalOutcome::Failed)
    );
    let inspection = service.inspect(&effect_id).await.unwrap().unwrap();
    assert!(inspection.retryable);
    assert_eq!(
        inspection.state,
        DashReceiptState::Terminal(DashTerminalOutcome::Failed)
    );
    let read = service.read().await.unwrap();
    let failure = read
        .history
        .entries()
        .iter()
        .find_map(|entry| match &entry.payload {
            HistoryPayload::TurnFailed { error, .. } => Some(error),
            _ => None,
        })
        .expect("failed turn must retain its execution failure");
    assert_eq!(failure.code, "rate_limit");
    assert_eq!(
        failure.message,
        "Dash Agent provider failed (rate_limit): temporary provider failure"
    );
    assert!(failure.retryable);
    let serialized = serde_json::to_value(read.state).unwrap();
    let object = serialized.as_object().unwrap();
    assert!(!object.contains_key("effects"));
    assert!(!object.contains_key("commands"));
}

struct CountingProvider {
    calls: AtomicUsize,
}

#[derive(Default)]
struct CapturingProvider {
    requests: tokio::sync::Mutex<Vec<DashProviderRequest>>,
}

#[async_trait]
impl DashProvider for CapturingProvider {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.requests.lock().await.push(request);
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "answer".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            }),
        ])))
    }
}

#[async_trait]
impl DashProvider for CountingProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "persisted answer".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            }),
        ])))
    }
}

fn dependencies(provider: Arc<dyn DashProvider>) -> DashExecutionDependencies {
    DashExecutionDependencies {
        provider,
        tools: Arc::new(NoTools),
        callbacks: Arc::new(NoCallbacks),
        history_callbacks: Arc::new(NoopDashHistoryCallbacks),
        compactor: Arc::new(NoCompaction),
        conversation_namer: Arc::new(NoopDashConversationNamer),
    }
}

#[tokio::test]
async fn installed_initial_context_is_materialized_into_the_provider_prompt() {
    let provider = Arc::new(CapturingProvider::default());
    let installation = InitialContextInstallation {
        package_id: "package-1".into(),
        package_digest: "sha256:package-1".into(),
        mode: InitialContextMode::Compact,
        fidelity: ContextDeliveryFidelity::TypedNative,
        contributions: vec![InitialContextContribution {
            kind: "compact_summary".into(),
            payload: "the durable parent summary".into(),
            authority: "agent_history".into(),
            source_revision: "revision-7".into(),
            digest: "sha256:summary".into(),
        }],
    };
    let service = DashAgentService::create_with_repository(
        Arc::new(RecordingDashRepository::default()),
        AgentHistory::empty(
            AgentSessionId::new("initial-context-session"),
            BranchId::new("initial-context-branch"),
        ),
        Some(installation),
        dependencies(provider.clone()),
    )
    .await
    .unwrap();

    service
        .execute(DashCommandRequest {
            command_id: CommandId::new("initial-context-command"),
            effect_id: EffectId::new("initial-context-effect"),
            command: DashPublicCommand::SubmitInput {
                content: "continue".into(),
            },
        })
        .await
        .unwrap();

    let requests = provider.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .system_prompt
            .contains("the durable parent summary"),
        "accepted initial context must be part of the actual provider prompt"
    );
}

#[tokio::test]
async fn repository_reopen_preserves_surface_inspect_and_idempotency_without_provider_replay() {
    let provider = Arc::new(CountingProvider {
        calls: AtomicUsize::new(0),
    });
    let repository = Arc::new(RecordingDashRepository::default());
    let service = DashAgentService::create_with_repository(
        repository.clone(),
        AgentHistory::empty(
            AgentSessionId::new("reopen-session"),
            BranchId::new("reopen-branch"),
        ),
        None,
        dependencies(provider.clone()),
    )
    .await
    .unwrap();
    service
        .apply_surface(DashSurface {
            revision: 7,
            digest: "surface-r7".into(),
            instructions: vec![DashSurfaceInstruction {
                key: "instruction:test:persisted".into(),
                channel: "system".into(),
                text: "persisted instructions".into(),
                presentation:
                    agentdash_agent_protocol::AgentSurfaceInstructionPresentation::SystemGuidelines,
            }],
            tools: vec![],
        })
        .await
        .unwrap();
    let persisted = repository.load().await.unwrap();
    assert!(
        persisted
            .history()
            .entries()
            .iter()
            .any(|entry| matches!(entry.payload, HistoryPayload::SurfaceApplied { .. }))
    );
    assert!(
        serde_json::to_value(&persisted)
            .unwrap()
            .get("surface")
            .is_none(),
        "current Dash surface must only be recoverable from native history"
    );
    let request = DashCommandRequest {
        command_id: CommandId::new("reopen-command"),
        effect_id: EffectId::new("reopen-effect"),
        command: DashPublicCommand::SubmitInput {
            content: "question".into(),
        },
    };
    let first = service.execute(request.clone()).await.unwrap();
    let reopened =
        DashAgentService::open_with_repository(repository, dependencies(provider.clone()));
    let replayed = reopened.execute(request.clone()).await.unwrap();

    assert_eq!(replayed, first);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reopened.read().await.unwrap().surface.unwrap().revision, 7);
    assert_eq!(
        reopened
            .inspect(&request.effect_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        DashReceiptState::Terminal(DashTerminalOutcome::Succeeded)
    );
}

struct OverflowProvider;

#[async_trait]
impl DashProvider for OverflowProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(DashCoreError::ContextOverflow)
    }
}

struct FailingCompactor {
    lost: bool,
}

#[async_trait]
impl DashCompactor for FailingCompactor {
    async fn compact(
        &self,
        _: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        if self.lost {
            Err(DashServiceError::Lost {
                message: "compaction outcome unknown".into(),
            })
        } else {
            Err(DashServiceError::Unavailable {
                message: "compactor unavailable".into(),
                retryable: true,
            })
        }
    }
}

struct OverflowThenErrorProvider {
    calls: AtomicUsize,
    error: DashCoreError,
}

#[async_trait]
impl DashProvider for OverflowThenErrorProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(DashCoreError::ContextOverflow)
        } else {
            Err(self.error.clone())
        }
    }
}

async fn automatic_service(
    provider: Arc<dyn DashProvider>,
    compactor: Arc<dyn DashCompactor>,
    suffix: &str,
) -> DashAgentService {
    create_service(
        AgentHistory::empty(
            AgentSessionId::new(format!("automatic-{suffix}-session")),
            BranchId::new(format!("automatic-{suffix}-branch")),
        ),
        DashExecutionDependencies {
            provider,
            tools: Arc::new(NoTools),
            callbacks: Arc::new(NoCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor,
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
    )
    .await
}

fn submit_request(suffix: &str) -> DashCommandRequest {
    DashCommandRequest {
        command_id: CommandId::new(format!("automatic-{suffix}-command")),
        effect_id: EffectId::new(format!("automatic-{suffix}-effect")),
        command: DashPublicCommand::SubmitInput {
            content: "question".into(),
        },
    }
}

#[tokio::test]
async fn automatic_compaction_b_failure_matrix_terminalizes_dependent_c_and_clears_active() {
    for (suffix, lost, terminal, c_status, b_effect, consistency, retryable) in [
        (
            "b-failed",
            false,
            DashTerminalOutcome::Failed,
            "failed",
            "failed",
            DashExecutionConsistency::Current,
            true,
        ),
        (
            "b-lost",
            true,
            DashTerminalOutcome::Lost,
            "blocked",
            "lost",
            DashExecutionConsistency::Lost,
            false,
        ),
    ] {
        let service = automatic_service(
            Arc::new(OverflowProvider),
            Arc::new(FailingCompactor { lost }),
            suffix,
        )
        .await;
        let request = submit_request(suffix);
        let receipt = service.execute(request.clone()).await.unwrap();

        assert_eq!(receipt.state, DashReceiptState::Terminal(terminal.clone()));
        let inspection = service.inspect(&request.effect_id).await.unwrap().unwrap();
        assert_eq!(
            inspection.state,
            DashReceiptState::Terminal(terminal.clone())
        );
        assert_eq!(inspection.retryable, retryable);
        assert_eq!(inspection.execution.consistency, consistency);

        let read = service.read().await.unwrap();
        assert!(read.state.active_turn.is_none());
        assert!(read.state.active_compaction.is_none());
        let compaction_id = CompactionId::new(format!("{}:B", request.command_id.0));
        assert_eq!(
            read.state.compactions[&compaction_id].status,
            if lost {
                ActivityStatus::Lost
            } else {
                ActivityStatus::Failed
            }
        );

        let repository =
            serde_json::to_value(service.export_repository_state().await.unwrap()).unwrap();
        assert!(repository["active"].is_null());
        let continuation_id = format!("{}:C", request.command_id.0);
        let commands = repository["store"]["lifecycle"]["commands"]
            .as_object()
            .unwrap();
        assert_eq!(
            commands
                .keys()
                .filter(|command_id| command_id.as_str() == continuation_id)
                .count(),
            1
        );
        assert_eq!(commands[&continuation_id]["status"], c_status);
        let compaction_effect_id = format!("{}:B", request.effect_id.0);
        assert_eq!(
            repository["store"]["lifecycle"]["effects"][&compaction_effect_id],
            b_effect
        );
        assert_eq!(
            repository["store"]["lifecycle"]["effects"][&request.effect_id.0],
            if lost { "lost" } else { "failed" }
        );
    }
}

#[tokio::test]
async fn automatic_continuation_c_failure_matrix_terminalizes_effect_and_clears_active() {
    for (suffix, error, terminal, c_status, effect, retryable) in [
        (
            "c-failed",
            DashCoreError::Provider {
                code: "continuation_failed".into(),
                message: "continuation failed".into(),
                retryable: true,
            },
            DashTerminalOutcome::Failed,
            ActivityStatus::Failed,
            "failed",
            true,
        ),
        (
            "c-lost",
            DashCoreError::ProviderStreamDisconnected,
            DashTerminalOutcome::Lost,
            ActivityStatus::Lost,
            "lost",
            false,
        ),
    ] {
        let service = automatic_service(
            Arc::new(OverflowThenErrorProvider {
                calls: AtomicUsize::new(0),
                error,
            }),
            Arc::new(NoCompaction),
            suffix,
        )
        .await;
        let request = submit_request(suffix);
        let receipt = service.execute(request.clone()).await.unwrap();

        assert_eq!(receipt.state, DashReceiptState::Terminal(terminal.clone()));
        let inspection = service.inspect(&request.effect_id).await.unwrap().unwrap();
        assert_eq!(inspection.state, DashReceiptState::Terminal(terminal));
        assert_eq!(inspection.retryable, retryable);
        assert_eq!(
            inspection.execution.consistency,
            if c_status == ActivityStatus::Lost {
                DashExecutionConsistency::Lost
            } else {
                DashExecutionConsistency::Current
            }
        );

        let read = service.read().await.unwrap();
        assert!(read.state.active_turn.is_none());
        assert!(read.state.active_compaction.is_none());
        let continuation_turn_id = AgentTurnId::new(format!("turn:{}:C", request.command_id.0));
        assert_eq!(read.state.turns[&continuation_turn_id].status, c_status);

        let repository =
            serde_json::to_value(service.export_repository_state().await.unwrap()).unwrap();
        assert!(repository["active"].is_null());
        let continuation_command_id = format!("{}:C", request.command_id.0);
        assert_eq!(
            repository["store"]["lifecycle"]["commands"][&continuation_command_id]["status"],
            effect
        );
        let continuation_effect_id = format!("{}:C", request.effect_id.0);
        assert_eq!(
            repository["store"]["lifecycle"]["effects"][&continuation_effect_id],
            effect
        );
        assert_eq!(
            repository["store"]["lifecycle"]["effects"][&request.effect_id.0],
            effect
        );
    }
}
