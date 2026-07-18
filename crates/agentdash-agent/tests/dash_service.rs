use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentSessionId, AgentTurnId, BranchId, CommandId, CompactionId,
    ContextRevision, DashAgentRepository, DashAgentRepositoryState, DashAgentService,
    DashCommandRequest, DashCompactionRequest, DashCompactionResult, DashCompactor, DashCoreError,
    DashCoreEvent, DashExecutionCallbacks, DashExecutionConsistency, DashExecutionDependencies,
    DashFinishReason, DashProvider, DashProviderEvent, DashProviderEventStream,
    DashProviderRequest, DashPublicCommand, DashReceiptState, DashServiceError, DashSurface,
    DashTerminalOutcome, DashToolCall, DashToolCallbacks, DashToolResult, EffectId,
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

struct NoCallbacks;

#[async_trait]
impl DashExecutionCallbacks for NoCallbacks {
    async fn emit(&self, _: DashCoreEvent) -> Result<(), DashCoreError> {
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
            compactor: Arc::new(NoCompaction),
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
    let serialized = serde_json::to_value(service.read().await.unwrap().state).unwrap();
    let object = serialized.as_object().unwrap();
    assert!(!object.contains_key("effects"));
    assert!(!object.contains_key("commands"));
}

struct CountingProvider {
    calls: AtomicUsize,
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
        compactor: Arc::new(NoCompaction),
    }
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
            system_prompt: "persisted instructions".into(),
            tools: vec![],
        })
        .await
        .unwrap();
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
            compactor,
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
