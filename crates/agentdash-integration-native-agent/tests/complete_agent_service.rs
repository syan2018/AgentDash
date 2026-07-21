use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use agentdash_agent::dash::{
    AgentSessionId, AgentTurnId as DashTurnId, ContextRevision, DashAgentRepository,
    DashAgentRepositoryState, DashAgentRepositoryStore, DashCompactionRequest,
    DashCompactionResult, DashCompactor, DashConversationNamer, DashConversationNamingRequest,
    DashCoreError, DashExecutionCallbacks, DashExecutionDependencies, DashExecutionEvent,
    DashFinishReason, DashProvider, DashProviderEvent, DashProviderEventStream,
    DashProviderRequest, DashServiceError, DashToolCall, DashToolCallbacks, DashToolResult,
    NoopDashConversationNamer, NoopDashHistoryCallbacks,
};
use agentdash_agent_protocol::{
    BackboneEvent, ContextFrameKind, ContextFrameSection, PlatformEvent, PresentationDurability,
    codex_app_server_protocol as codex,
};
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentBindingGeneration, AgentCallbackRouteId, AgentChangePayload,
    AgentChangesQuery, AgentCommand, AgentCommandEnvelope, AgentCommandId, AgentCommandMeta,
    AgentContextPackageId, AgentContextSchemaVersion, AgentContextSourceCoordinate,
    AgentContextSourceRevision, AgentEffectIdentity, AgentEffectInspectionState,
    AgentForkCutoffKind, AgentForkPoint, AgentHookAction, AgentHookBlockingSemantics,
    AgentHookDecision, AgentHookDefinitionId, AgentHookInvocation, AgentHookMutationKind,
    AgentHookPoint, AgentHookTiming, AgentHostCallbackBinding, AgentHostCallbackError,
    AgentHostCallbacks, AgentIdempotencyKey, AgentInput, AgentInputContent, AgentPayloadDigest,
    AgentProfileDigest, AgentReadQuery, AgentReceiptState, AgentServiceError,
    AgentServiceErrorCode, AgentServiceInstanceId, AgentSnapshotRevision, AgentSourceCoordinate,
    AgentSurfaceContributionPayload, AgentSurfaceDigest, AgentSurfaceRevision, AgentSurfaceRoute,
    AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentToolDelivery, AgentToolInvocation,
    AgentToolName, AgentToolResult, AgentToolSemanticFacet, AgentToolUpdateSemantics,
    ApplyBoundAgentSurface, BoundAgentSurface, BoundAgentSurfaceContribution, CompleteAgentService,
    ContextAuthorityKind, ContextProvenance, CreateAgentCommand, ForkAgentCommand,
    InitialAgentContextPackage, InitialContextAppliedEvidence, InitialContextContribution,
    InitialContextDeliveryFidelity, InitialContextMode, ResumeAgentCommand,
    RevokeBoundAgentSurface, SemanticFidelity,
};
use agentdash_integration_native_agent::{
    DashAgentCompleteService, DashCompleteAgentStore, DashCompleteAtomicCommit,
    DashCompleteEffectRecord, DashCompleteSourceMetadata, DashCompleteSourceMutation,
    native_complete_agent_registration,
};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use tokio::sync::{Notify, RwLock};

struct RecordingDashRepository {
    source: String,
    durable: Arc<RwLock<RecordingCompleteDurableState>>,
}

#[async_trait]
impl DashAgentRepository for RecordingDashRepository {
    async fn initialize(&self, initial: DashAgentRepositoryState) -> Result<(), DashServiceError> {
        let mut durable = self.durable.write().await;
        if durable.repositories.contains_key(&self.source) {
            return Err(DashServiceError::Conflict {
                message: "test Dash repository already initialized".into(),
            });
        }
        durable.repositories.insert(self.source.clone(), initial);
        Ok(())
    }

    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError> {
        self.durable
            .read()
            .await
            .repositories
            .get(&self.source)
            .cloned()
            .ok_or_else(|| DashServiceError::InvalidState {
                message: "test Dash repository is not initialized".into(),
            })
    }

    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError> {
        let mut durable = self.durable.write().await;
        if durable.repositories.get(&self.source) != Some(&expected) {
            return Err(DashServiceError::Conflict {
                message: "test Dash repository revision changed".into(),
            });
        }
        durable
            .repositories
            .insert(self.source.clone(), replacement);
        Ok(())
    }
}

#[derive(Default)]
struct RecordingCompleteDurableState {
    repositories: BTreeMap<String, DashAgentRepositoryState>,
    sources: BTreeMap<AgentSourceCoordinate, DashCompleteSourceMetadata>,
    effects: BTreeMap<AgentEffectIdentity, DashCompleteEffectRecord>,
}

#[derive(Default)]
struct RecordingCompleteStore {
    durable: Arc<RwLock<RecordingCompleteDurableState>>,
    lose_next_commit_receipt: AtomicBool,
    fail_next_terminal_commit: AtomicBool,
}

impl RecordingCompleteStore {
    fn lose_next_commit_receipt(&self) {
        self.lose_next_commit_receipt.store(true, Ordering::SeqCst);
    }

    fn fail_next_terminal_commit(&self) {
        self.fail_next_terminal_commit.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl DashAgentRepositoryStore for RecordingCompleteStore {
    async fn create(
        &self,
        source: &AgentSessionId,
        initial: DashAgentRepositoryState,
    ) -> Result<Arc<dyn DashAgentRepository>, DashServiceError> {
        let mut durable = self.durable.write().await;
        if durable.repositories.contains_key(&source.0) {
            return Err(DashServiceError::Conflict {
                message: "test Dash source already exists".into(),
            });
        }
        durable.repositories.insert(source.0.clone(), initial);
        Ok(Arc::new(RecordingDashRepository {
            source: source.0.clone(),
            durable: self.durable.clone(),
        }))
    }

    async fn open(
        &self,
        source: &AgentSessionId,
    ) -> Result<Option<Arc<dyn DashAgentRepository>>, DashServiceError> {
        if !self
            .durable
            .read()
            .await
            .repositories
            .contains_key(&source.0)
        {
            return Ok(None);
        }
        Ok(Some(Arc::new(RecordingDashRepository {
            source: source.0.clone(),
            durable: self.durable.clone(),
        })))
    }
}

#[async_trait]
impl DashCompleteAgentStore for RecordingCompleteStore {
    fn repositories(&self) -> &dyn DashAgentRepositoryStore {
        self
    }

    async fn load_source(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<DashCompleteSourceMetadata>, AgentServiceError> {
        Ok(self.durable.read().await.sources.get(source).cloned())
    }

    async fn load_effect(
        &self,
        identity: &AgentEffectIdentity,
    ) -> Result<Option<DashCompleteEffectRecord>, AgentServiceError> {
        Ok(self.durable.read().await.effects.get(identity).cloned())
    }

    async fn commit(&self, commit: DashCompleteAtomicCommit) -> Result<(), AgentServiceError> {
        if matches!(
            commit.replacement_effect.inspection.state,
            AgentEffectInspectionState::Applied { .. }
        ) && self.fail_next_terminal_commit.swap(false, Ordering::SeqCst)
        {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                "test Complete Agent crashed before the terminal commit",
                true,
            ));
        }
        let mut durable = self.durable.write().await;
        if durable.effects.get(&commit.effect_id) != commit.expected_effect.as_ref() {
            return Err(test_conflict(
                "test Complete Agent effect identity conflict",
            ));
        }

        for mutation in &commit.source_mutations {
            match mutation {
                DashCompleteSourceMutation::Create { source, .. } => {
                    if durable.sources.contains_key(source)
                        || durable.repositories.contains_key(source.as_str())
                    {
                        return Err(test_conflict("test Complete Agent source already exists"));
                    }
                }
                DashCompleteSourceMutation::CompareAndSwap {
                    source,
                    expected_repository,
                    expected_metadata,
                    ..
                } => {
                    if durable.sources.get(source) != Some(expected_metadata.as_ref())
                        || durable.repositories.get(source.as_str())
                            != Some(expected_repository.as_ref())
                    {
                        return Err(test_conflict(
                            "test Complete Agent source aggregate revision changed",
                        ));
                    }
                }
            }
        }

        for mutation in commit.source_mutations {
            match mutation {
                DashCompleteSourceMutation::Create {
                    source,
                    repository,
                    metadata,
                } => {
                    durable
                        .repositories
                        .insert(source.as_str().to_owned(), *repository);
                    durable.sources.insert(source, *metadata);
                }
                DashCompleteSourceMutation::CompareAndSwap {
                    source,
                    replacement_repository,
                    replacement_metadata,
                    ..
                } => {
                    durable
                        .repositories
                        .insert(source.as_str().to_owned(), *replacement_repository);
                    durable.sources.insert(source, *replacement_metadata);
                }
            }
        }
        durable
            .effects
            .insert(commit.effect_id, commit.replacement_effect);
        drop(durable);

        if self.lose_next_commit_receipt.swap(false, Ordering::SeqCst) {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                "test Complete Agent committed but lost the response",
                true,
            ));
        }
        Ok(())
    }
}

fn test_conflict(message: &str) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Conflict, message, false)
}

struct FixtureProvider;

struct FixtureConversationNamer;

#[async_trait]
impl DashConversationNamer for FixtureConversationNamer {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError> {
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].content, "修复消息流");
        assert_eq!(request.messages[1].content, "fixture answer");
        Ok("消息流收束".to_owned())
    }
}

#[async_trait]
impl DashProvider for FixtureProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "fixture answer".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 2,
            }),
        ])))
    }
}

struct FixtureTools;

#[async_trait]
impl DashToolCallbacks for FixtureTools {
    async fn invoke(
        &self,
        _: &DashTurnId,
        _: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        Err(DashCoreError::Tool {
            message: "fixture provider does not call tools".into(),
            retryable: false,
        })
    }
}

struct FixtureCallbacks;

#[async_trait]
impl DashExecutionCallbacks for FixtureCallbacks {
    async fn emit(&self, _: DashExecutionEvent) -> Result<(), DashCoreError> {
        Ok(())
    }
}

struct FixtureHostCallbacks;

#[async_trait]
impl AgentHostCallbacks for FixtureHostCallbacks {
    async fn invoke_tool(
        &self,
        _: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        Ok(AgentToolResult::Completed {
            output: serde_json::json!({"ok": true}),
        })
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

struct HookRoundProvider {
    calls: AtomicUsize,
    requests: Mutex<Vec<DashProviderRequest>>,
}

#[async_trait]
impl DashProvider for HookRoundProvider {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.requests.lock().unwrap().push(request);
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Ok(Box::pin(stream::iter([
                Ok(DashProviderEvent::ToolCall {
                    call: DashToolCall {
                        call_id: "hook-call-1".into(),
                        name: "read".into(),
                        arguments: serde_json::json!({"original": true}),
                    },
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::ToolCalls,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ])))
        } else {
            Ok(Box::pin(stream::iter([
                Ok(DashProviderEvent::TextDelta {
                    delta: "hooked answer".into(),
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::Stop,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ])))
        }
    }
}

struct SurfaceGenerationProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl DashProvider for SurfaceGenerationProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call.is_multiple_of(2) {
            Ok(Box::pin(stream::iter([
                Ok(DashProviderEvent::ToolCall {
                    call: DashToolCall {
                        call_id: format!("surface-call-{call}"),
                        name: "read".into(),
                        arguments: serde_json::json!({"call": call}),
                    },
                }),
                Ok(DashProviderEvent::Completed {
                    finish_reason: DashFinishReason::ToolCalls,
                    input_tokens: 1,
                    output_tokens: 1,
                }),
            ])))
        } else {
            Ok(Box::pin(stream::iter([Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            })])))
        }
    }
}

#[derive(Default)]
struct SurfaceGenerationHostCallbacks {
    generations: Mutex<Vec<AgentBindingGeneration>>,
}

#[async_trait]
impl AgentHostCallbacks for SurfaceGenerationHostCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.generations
            .lock()
            .unwrap()
            .push(call.meta.binding_generation);
        Ok(AgentToolResult::Completed {
            output: serde_json::json!({"ok": true}),
        })
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

#[derive(Default)]
struct HookExecutionCallbacks {
    before: AtomicUsize,
    after: AtomicUsize,
    tools: AtomicUsize,
    tool_arguments: Mutex<Vec<serde_json::Value>>,
}

#[async_trait]
impl AgentHostCallbacks for HookExecutionCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.tools.fetch_add(1, Ordering::SeqCst);
        self.tool_arguments.lock().unwrap().push(call.arguments);
        Ok(AgentToolResult::Completed {
            output: serde_json::json!({"content": "original-result"}),
        })
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        assert_eq!(call.meta.binding_generation, AgentBindingGeneration(11));
        assert_eq!(call.meta.source.as_str(), "dash-hook-execution");
        assert_eq!(call.meta.turn_id.as_str(), "turn:hook-input");
        assert_eq!(call.meta.item_id.as_ref().unwrap().as_str(), "hook-call-1");
        assert!(call.meta.deadline_at_ms > 0);
        match call.point {
            AgentHookPoint::BeforeTool => {
                self.before.fetch_add(1, Ordering::SeqCst);
                Ok(AgentHookDecision::ReplaceInput {
                    input: serde_json::json!({"arguments": {"rewritten": true}}),
                })
            }
            AgentHookPoint::AfterTool => {
                self.after.fetch_add(1, Ordering::SeqCst);
                Ok(AgentHookDecision::ReplaceResult {
                    result: serde_json::json!({
                        "content": "rewritten-result",
                        "is_error": false
                    }),
                })
            }
            _ => Ok(AgentHookDecision::Allow),
        }
    }
}

struct FixtureCompactor;

#[async_trait]
impl DashCompactor for FixtureCompactor {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        Ok(DashCompactionResult {
            revision: ContextRevision::new("fixture-context-r1"),
            summary: "fixture compacted summary".into(),
            retained_from: request
                .history
                .entries()
                .last()
                .map(|entry| entry.entry_id.clone()),
        })
    }
}

fn service() -> DashAgentCompleteService {
    service_with_store(Arc::new(RecordingCompleteStore::default()))
}

fn service_with_store(store: Arc<dyn DashCompleteAgentStore>) -> DashAgentCompleteService {
    DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider: Arc::new(FixtureProvider),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        Arc::new(FixtureHostCallbacks),
        store,
    )
}

#[tokio::test]
async fn production_registration_packages_the_complete_dash_service_without_registering_a_driver() {
    let registration = native_complete_agent_registration(
        AgentServiceInstanceId::new("native-complete-1").unwrap(),
        DashExecutionDependencies {
            provider: Arc::new(FixtureProvider),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        Arc::new(FixtureHostCallbacks),
        Arc::new(RecordingCompleteStore::default()),
    )
    .unwrap();

    assert_eq!(
        registration.facts().instance_id().as_str(),
        "native-complete-1"
    );
    let registration = registration.materialize().await.unwrap();
    assert_eq!(
        registration
            .service()
            .describe()
            .await
            .unwrap()
            .definition_id
            .as_str(),
        "dash-agent"
    );
}

#[tokio::test]
async fn successful_turn_commits_agent_owned_thread_name_and_projects_one_canonical_change() {
    let service = DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider: Arc::new(FixtureProvider),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(FixtureConversationNamer),
        },
        Arc::new(FixtureHostCallbacks),
        Arc::new(RecordingCompleteStore::default()),
    );
    let source = AgentSourceCoordinate::new("dash-thread-name").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-thread-name", "effect-create-thread-name"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    service
        .execute(AgentCommandEnvelope {
            meta: meta("name-input", "effect-name-input"),
            source: source.clone(),
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "修复消息流".to_owned(),
                    }],
                },
            },
        })
        .await
        .unwrap();

    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(
        snapshot
            .thread_name
            .as_ref()
            .and_then(|name| name.thread_name.as_deref()),
        Some("消息流收束")
    );
    assert!(snapshot.conversation_history.iter().any(|record| matches!(
        &record.presentation.envelope.event,
        BackboneEvent::ThreadNameUpdated(notification)
            if notification.thread_name.as_deref() == Some("消息流收束")
    )));

    let changes = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    assert!(changes.changes.iter().any(|change| matches!(
        &change.payload,
        AgentChangePayload::SourceObservation { state: Some(state), presentation }
            if matches!(state.as_ref(), AgentChangePayload::ThreadNameChanged {
                thread_name: Some(thread_name), ..
            } if thread_name == "消息流收束")
                && presentation.iter().any(|record| matches!(
                    &record.presentation.envelope.event,
                    BackboneEvent::ThreadNameUpdated(notification)
                        if notification.thread_name.as_deref() == Some("消息流收束")
                ))
    )));
}

fn service_with_provider(provider: Arc<dyn DashProvider>) -> DashAgentCompleteService {
    service_with(provider, Arc::new(FixtureCompactor))
}

fn service_with(
    provider: Arc<dyn DashProvider>,
    compactor: Arc<dyn DashCompactor>,
) -> DashAgentCompleteService {
    DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider,
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor,
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        Arc::new(FixtureHostCallbacks),
        Arc::new(RecordingCompleteStore::default()),
    )
}

struct ErrorProvider {
    error: DashCoreError,
}

#[async_trait]
impl DashProvider for ErrorProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(self.error.clone())
    }
}

struct SteerProvider {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl DashProvider for SteerProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.started.notify_one();
        let release = self.release.clone();
        Ok(Box::pin(
            stream::once(async move {
                release.notified().await;
                Ok(DashProviderEvent::TextDelta {
                    delta: "steered answer".into(),
                })
            })
            .chain(stream::iter([Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            })])),
        ))
    }

    async fn steer(&self, _: &DashTurnId, _: &str) -> Result<(), DashCoreError> {
        self.release.notify_one();
        Ok(())
    }
}

struct BlockingProvider {
    started: Arc<Notify>,
}

#[async_trait]
impl DashProvider for BlockingProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.started.notify_one();
        Ok(Box::pin(stream::pending()))
    }
}

struct InteractionProvider;

#[async_trait]
impl DashProvider for InteractionProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(DashCoreError::InteractionRequired {
            interaction_id: "interaction-1".into(),
            prompt: "approve?".into(),
        })
    }
}

struct OverflowProvider {
    calls: AtomicUsize,
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

#[async_trait]
impl DashProvider for OverflowProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(DashCoreError::ContextOverflow);
        }
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "answer after compaction".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            }),
        ])))
    }
}

fn meta(command: &str, effect: &str) -> AgentCommandMeta {
    AgentCommandMeta {
        command_id: AgentCommandId::new(command).unwrap(),
        effect_id: AgentEffectIdentity::new(effect).unwrap(),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{command}")).unwrap(),
        binding_generation: AgentBindingGeneration(1),
        expected_snapshot_revision: None,
    }
}

fn initial_package() -> InitialAgentContextPackage {
    let package_id = AgentContextPackageId::new("package-1").unwrap();
    let contribution = InitialContextContribution::CompactSummary {
        summary: "parent summary".into(),
        provenance: ContextProvenance {
            authority: ContextAuthorityKind::AgentHistory,
            source: AgentContextSourceCoordinate::new("parent-source").unwrap(),
            revision: AgentContextSourceRevision::new("parent-r7").unwrap(),
            digest: AgentPayloadDigest::new("sha256:parent-r7").unwrap(),
        },
    };
    let digest = InitialAgentContextPackage::calculated_digest(
        &package_id,
        AgentContextSchemaVersion(1),
        InitialContextMode::Compact,
        std::slice::from_ref(&contribution),
    );
    InitialAgentContextPackage {
        package_id,
        schema_version: AgentContextSchemaVersion(1),
        mode: InitialContextMode::Compact,
        contributions: vec![contribution],
        digest,
    }
}

#[tokio::test]
async fn native_complete_agent_create_input_and_fork_use_dash_history_authority() {
    let service = service();
    let descriptor = service.describe().await.unwrap();
    assert!(
        descriptor
            .profile
            .fork
            .supports_exact(AgentForkCutoffKind::Head)
    );
    assert_eq!(
        descriptor.profile.initial_context.applied_evidence,
        InitialContextAppliedEvidence::PackageDigest
    );
    let tool = descriptor
        .profile
        .surface
        .facets
        .iter()
        .find_map(|facet| match &facet.semantics {
            AgentSurfaceSemanticFacet::Tool(tool) => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.delivery, AgentToolDelivery::AgentNativeCallback);
    assert_eq!(tool.invocation, SemanticFidelity::Exact);
    assert_eq!(tool.update, AgentToolUpdateSemantics::HotUpdate);
    let before_tool = descriptor
        .profile
        .surface
        .facets
        .iter()
        .find_map(|facet| match &facet.semantics {
            AgentSurfaceSemanticFacet::Hook(hook) if hook.point == AgentHookPoint::BeforeTool => {
                Some(hook)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        before_tool.blocking,
        AgentHookBlockingSemantics::Blocking {
            fidelity: SemanticFidelity::Exact
        }
    );
    assert_eq!(
        before_tool
            .mutations
            .get(&AgentHookMutationKind::RewriteInput),
        Some(&SemanticFidelity::Exact)
    );

    let parent = AgentSourceCoordinate::new("dash-parent").unwrap();
    let package = initial_package();
    let create = service
        .create(CreateAgentCommand {
            meta: meta("create-parent", "effect-create-parent"),
            requested_source: Some(parent.clone()),
            initial_context: Some(package.clone()),
        })
        .await
        .unwrap();
    let evidence = create.initial_context.unwrap();
    assert_eq!(evidence.package_digest, package.digest);
    assert_eq!(
        evidence.fidelity,
        InitialContextDeliveryFidelity::TypedNative
    );
    assert!(evidence.satisfies(InitialContextAppliedEvidence::PackageDigest));
    assert_eq!(create.snapshot_revision, Some(AgentSnapshotRevision(1)));
    let initial_snapshot = service
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(
        initial_snapshot
            .conversation_history
            .iter()
            .any(|record| matches!(
                &record.presentation.envelope.event,
                BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed))
                    if changed.frame.kind == ContextFrameKind::CompactionSummary
                        && changed.frame.rendered_text == "parent summary"
            ))
    );

    let submit = service
        .execute(AgentCommandEnvelope {
            meta: meta("input-1", "effect-input-1"),
            source: parent.clone(),
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "first ordinary input".into(),
                    }],
                },
            },
        })
        .await
        .unwrap();
    assert_eq!(
        submit.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    assert_eq!(submit.snapshot_revision, Some(AgentSnapshotRevision(7)));
    assert!(matches!(
        service
            .inspect(AgentEffectIdentity::new("effect-input-1").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { receipt }
        } if receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
            && receipt.snapshot_revision == submit.snapshot_revision
            && receipt.source == parent
    ));

    let command_inspection = service
        .inspect(AgentEffectIdentity::new("effect-input-1").unwrap())
        .await
        .unwrap();
    assert!(
        command_inspection.validate(),
        "typed command inspection coordinates must be self-consistent"
    );

    let changes = service
        .changes(AgentChangesQuery {
            source: parent.clone(),
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(changes.changes.len(), 9);
    assert_eq!(changes.changes[0].cursor.as_str(), "1:0");
    assert_eq!(changes.changes[1].cursor.as_str(), "2:0");
    let parent_snapshot = service
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(parent_snapshot.conversation_history.iter().any(|record| {
        matches!(
            &record.presentation.envelope.event,
            BackboneEvent::UserInputSubmitted(_)
        )
    }));
    assert!(parent_snapshot.conversation_history.iter().any(|record| {
        matches!(
            &record.presentation.envelope.event,
            BackboneEvent::TurnStarted(_)
        )
    }));
    assert!(parent_snapshot.conversation_history.iter().any(|record| {
        matches!(
            &record.presentation.envelope.event,
            BackboneEvent::TurnCompleted(_)
        )
    }));

    let fork_command = ForkAgentCommand {
        meta: meta("fork-child", "effect-fork-child"),
        source: parent.clone(),
        requested_child_source: Some(AgentSourceCoordinate::new("dash-child").unwrap()),
        cutoff: AgentForkPoint::Head,
    };
    let forked = service.fork(fork_command.clone()).await.unwrap();
    assert_eq!(
        forked.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    let child = forked.child_source.clone().unwrap();
    assert!(forked.child_history_digest.is_some());

    // Replaying the stable effect returns the same child rather than creating another fork.
    let replayed = service.fork(fork_command).await.unwrap();
    assert_eq!(replayed.child_source, Some(child.clone()));
    let inspection = service
        .inspect(AgentEffectIdentity::new("effect-fork-child").unwrap())
        .await
        .unwrap();
    assert!(inspection.validate());
    assert!(matches!(
        inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt }
        } if receipt.parent_source == parent
            && receipt.child_source == child
            && receipt.cutoff == AgentForkPoint::Head
            && Some(&receipt.child_history_digest) == forked.child_history_digest.as_ref()
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));

    let child_snapshot = service
        .read(AgentReadQuery {
            source: child,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(
        child_snapshot.initial_context.unwrap().package_digest,
        package.digest
    );
}

#[tokio::test]
async fn fork_profile_only_advertises_recoverable_exact_cutoffs() {
    let store = Arc::new(RecordingCompleteStore::default());
    let service = service_with_store(store.clone());
    let parent = create_source(&service, "dash-cutoff-parent").await;
    service
        .execute(submit_envelope(
            parent.clone(),
            "cutoff-parent-input",
            "cutoff-parent-effect",
        ))
        .await
        .unwrap();
    let parent_snapshot = service
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    let completed_turn = agentdash_agent_service_api::AgentTurnId::new(
        parent_snapshot
            .conversation()
            .completed_turn(None)
            .expect("completed turn")
            .id
            .clone(),
    )
    .unwrap();
    let completed_item = agentdash_agent_service_api::AgentItemId::new(
        parent_snapshot
            .conversation()
            .completed_items()
            .next()
            .expect("completed item")
            .item
            .id()
            .to_owned(),
    )
    .unwrap();

    let descriptor = service.describe().await.unwrap();
    assert_eq!(
        descriptor
            .profile
            .fork
            .cutoffs
            .get(&AgentForkCutoffKind::CompletedTurn),
        Some(&SemanticFidelity::Exact)
    );
    assert_eq!(
        descriptor
            .profile
            .fork
            .cutoffs
            .get(&AgentForkCutoffKind::Item),
        Some(&SemanticFidelity::Unsupported)
    );

    let child = AgentSourceCoordinate::new("dash-cutoff-child").unwrap();
    let fork_command = ForkAgentCommand {
        meta: meta("cutoff-fork", "cutoff-fork-effect"),
        source: parent.clone(),
        requested_child_source: Some(child.clone()),
        cutoff: AgentForkPoint::CompletedTurn {
            turn_id: completed_turn.clone(),
        },
    };
    let forked = service.fork(fork_command.clone()).await.unwrap();
    let history_digest = forked.child_history_digest.clone().unwrap();

    let restarted = service_with_store(store.clone());
    let inspection = restarted
        .inspect(fork_command.meta.effect_id.clone())
        .await
        .unwrap();
    assert!(inspection.validate());
    assert!(matches!(
        inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt }
        } if receipt.parent_source == parent
            && receipt.child_source == child
            && receipt.cutoff == fork_command.cutoff
            && receipt.child_history_digest == history_digest
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));
    let child_snapshot = restarted
        .read(AgentReadQuery {
            source: child.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    let digest = history_digest
        .as_str()
        .strip_prefix("sha256:")
        .expect("Dash fork digest is sha256");
    let expected_source_revision = format!("history:{digest}");
    assert_eq!(
        child_snapshot
            .source_info
            .source_revision
            .as_ref()
            .map(|revision| revision.as_str()),
        Some(expected_source_revision.as_str())
    );
    assert!(child_snapshot.active_turn_id().is_none());
    assert!(matches!(
        restarted
            .execute(submit_envelope(
                child.clone(),
                "cutoff-child-input",
                "cutoff-child-effect",
            ))
            .await
            .unwrap()
            .state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));

    let repositories_before = store.durable.read().await.repositories.len();
    let item_child = AgentSourceCoordinate::new("dash-item-cutoff-child").unwrap();
    let error = restarted
        .fork(ForkAgentCommand {
            meta: meta("item-cutoff-fork", "item-cutoff-fork-effect"),
            source: parent,
            requested_child_source: Some(item_child.clone()),
            cutoff: AgentForkPoint::Item {
                item_id: completed_item,
            },
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, AgentServiceErrorCode::Unsupported);
    assert_eq!(
        store.durable.read().await.repositories.len(),
        repositories_before,
        "unsupported item cutoff must be rejected before creating a child source"
    );
    assert_eq!(
        restarted
            .read(AgentReadQuery {
                source: item_child,
                at_revision: None,
            })
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::NotFound
    );
}

#[tokio::test]
async fn surface_instructions_preserve_materialized_context_frame_boundaries() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-context-surface").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-context-surface", "effect-create-context-surface"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let instruction = |key: &str, channel: &str, text: &str| BoundAgentSurfaceContribution {
        key: key.to_owned(),
        required: true,
        route: AgentSurfaceRoute::ImmutableDelivery,
        fidelity: SemanticFidelity::Exact,
        semantics: AgentSurfaceSemanticFacet::Instruction,
        payload: AgentSurfaceContributionPayload::Instruction {
            channel: channel.to_owned(),
            text: text.to_owned(),
        },
        payload_digest: AgentPayloadDigest::new(format!("sha256:{key}")).unwrap(),
    };
    service
        .apply_surface(ApplyBoundAgentSurface {
            command_id: AgentCommandId::new("command-context-surface").unwrap(),
            effect_id: AgentEffectIdentity::new("effect-context-surface").unwrap(),
            idempotency_key: AgentIdempotencyKey::new("idem-context-surface").unwrap(),
            source: source.clone(),
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("context-surface-1").unwrap(),
                offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
                contributions: vec![
                    instruction(
                        "instruction:execution-profile:system-prompt",
                        "system",
                        "system rules",
                    ),
                    instruction("instruction:37:persona_summary", "persona", "persona"),
                    instruction("instruction:30:workspace_context", "workspace", "workspace"),
                    instruction("instruction:48:workflow_summary", "workflow", "workflow"),
                ],
            },
            callbacks: AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new("callbacks-context").unwrap(),
                binding_generation: AgentBindingGeneration(1),
                delivery: AgentSurfaceRoute::AgentNativeCallback,
                default_deadline_ms: 5_000,
            },
        })
        .await
        .unwrap();

    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    let context_kinds = snapshot
        .conversation_history
        .iter()
        .filter_map(|record| match &record.presentation.envelope.event {
            BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed)) => {
                Some(changed.frame.kind)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        context_kinds,
        vec![
            ContextFrameKind::SystemGuidelines,
            ContextFrameKind::Identity,
            ContextFrameKind::Environment,
            ContextFrameKind::AssignmentContext,
        ]
    );
}

#[tokio::test]
async fn unsupported_input_is_rejected_before_history_changes() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-text-only").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create", "effect-create"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let error = service
        .execute(AgentCommandEnvelope {
            meta: meta("structured", "effect-structured"),
            source: source.clone(),
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Structured {
                        schema: "example".into(),
                        value: serde_json::json!({"x": 1}),
                    }],
                },
            },
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Unsupported
    );
    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.revision, AgentSnapshotRevision(0));
}

#[tokio::test]
async fn surface_apply_preserves_exact_tool_semantics_and_rejects_route_substitution() {
    let store = Arc::new(RecordingCompleteStore::default());
    let service = service_with_store(store.clone());
    let source = AgentSourceCoordinate::new("dash-surface").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-surface", "effect-create-surface"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let semantics = AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
        delivery: AgentToolDelivery::AgentNativeCallback,
        invocation: SemanticFidelity::Exact,
        update: AgentToolUpdateSemantics::HotUpdate,
    });
    let contribution = BoundAgentSurfaceContribution {
        key: "tool:read".into(),
        required: true,
        route: AgentSurfaceRoute::AgentNativeCallback,
        fidelity: SemanticFidelity::Exact,
        semantics: semantics.clone(),
        payload: AgentSurfaceContributionPayload::Tool {
            name: AgentToolName::new("read").unwrap(),
            description: "read".into(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
        },
        payload_digest: AgentPayloadDigest::new("sha256:tool-read").unwrap(),
    };
    let apply = |route, effect: &str| ApplyBoundAgentSurface {
        command_id: AgentCommandId::new(format!("command-{effect}")).unwrap(),
        effect_id: AgentEffectIdentity::new(effect).unwrap(),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{effect}")).unwrap(),
        source: source.clone(),
        bound_surface: BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("surface-1").unwrap(),
            offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
            contributions: vec![BoundAgentSurfaceContribution {
                route,
                ..contribution.clone()
            }],
        },
        callbacks: AgentHostCallbackBinding {
            route_id: AgentCallbackRouteId::new("callbacks-1").unwrap(),
            binding_generation: AgentBindingGeneration(1),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 5_000,
        },
    };

    let receipt = service
        .apply_surface(apply(
            AgentSurfaceRoute::AgentNativeCallback,
            "effect-apply",
        ))
        .await
        .unwrap();
    assert_eq!(receipt.applied.contributions[0].semantics, semantics);
    assert_eq!(
        receipt.applied.contributions[0].fidelity,
        SemanticFidelity::Exact
    );
    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(snapshot.conversation_history.iter().any(|record| matches!(
        &record.presentation.envelope.event,
        BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed))
            if changed.frame.kind == ContextFrameKind::CapabilityStateDelta
                && changed.frame.sections.iter().any(|section| matches!(
                    section,
                    ContextFrameSection::ToolSchemaDelta { added_tools }
                        if added_tools.iter().any(|tool| tool.name == "read")
                ))
    )));
    let reopened = service_with_store(store.clone());
    let replayed = reopened
        .apply_surface(apply(
            AgentSurfaceRoute::AgentNativeCallback,
            "effect-apply",
        ))
        .await
        .unwrap();
    assert_eq!(replayed, receipt);
    let mut conflicting_apply = apply(AgentSurfaceRoute::AgentNativeCallback, "effect-apply");
    conflicting_apply.command_id = AgentCommandId::new("conflicting-apply").unwrap();
    assert_eq!(
        reopened
            .apply_surface(conflicting_apply)
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Conflict
    );
    let apply_inspection = reopened
        .inspect(AgentEffectIdentity::new("effect-apply").unwrap())
        .await
        .unwrap();
    assert!(apply_inspection.validate());
    assert!(matches!(
        apply_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceApply { receipt: inspected }
        } if inspected == receipt
    ));

    let error = service
        .apply_surface(apply(
            AgentSurfaceRoute::RuntimeToolBroker,
            "effect-wrong-route",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Unsupported
    );
    let revoke = RevokeBoundAgentSurface {
        command_id: AgentCommandId::new("command-revoke").unwrap(),
        effect_id: AgentEffectIdentity::new("effect-revoke").unwrap(),
        idempotency_key: AgentIdempotencyKey::new("idem-revoke").unwrap(),
        binding_generation: AgentBindingGeneration(1),
        source: source.clone(),
        expected_revision: AgentSurfaceRevision(1),
    };
    let revoked = reopened.revoke_surface(revoke.clone()).await.unwrap();
    let restarted = service_with_store(store);
    assert_eq!(restarted.revoke_surface(revoke).await.unwrap(), revoked);
    let revoke_inspection = restarted
        .inspect(AgentEffectIdentity::new("effect-revoke").unwrap())
        .await
        .unwrap();
    assert!(revoke_inspection.validate());
    assert!(matches!(
        revoke_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceRevoke { receipt }
        } if receipt.command_id == revoked.command_id
            && receipt.effect_id == revoked.effect_id
            && receipt.source == revoked.source
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
            && receipt.snapshot_revision == revoked.snapshot_revision
    ));
}

#[tokio::test]
async fn lost_surface_receipts_reconcile_live_callbacks_on_the_same_service() {
    let store = Arc::new(RecordingCompleteStore::default());
    let provider = Arc::new(SurfaceGenerationProvider {
        calls: AtomicUsize::new(0),
    });
    let host = Arc::new(SurfaceGenerationHostCallbacks::default());
    let service = DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider: provider.clone(),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        host.clone(),
        store.clone(),
    );
    let source = AgentSourceCoordinate::new("dash-live-surface-reconcile").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("live-surface-create", "live-surface-create-effect"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let apply = |revision, generation, effect: &str| ApplyBoundAgentSurface {
        command_id: AgentCommandId::new(format!("{effect}-command")).unwrap(),
        effect_id: AgentEffectIdentity::new(effect).unwrap(),
        idempotency_key: AgentIdempotencyKey::new(format!("{effect}-idem")).unwrap(),
        source: source.clone(),
        bound_surface: generation_surface(revision),
        callbacks: AgentHostCallbackBinding {
            route_id: AgentCallbackRouteId::new(format!("live-route-{generation}")).unwrap(),
            binding_generation: AgentBindingGeneration(generation),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 5_000,
        },
    };

    service
        .apply_surface(apply(1, 1, "live-apply-1"))
        .await
        .unwrap();
    service
        .execute(submit_envelope(
            source.clone(),
            "generation one",
            "live-execute-1",
        ))
        .await
        .unwrap();
    assert_eq!(
        host.generations.lock().unwrap().as_slice(),
        &[AgentBindingGeneration(1)]
    );

    let apply_two = apply(2, 2, "live-apply-2");
    store.lose_next_commit_receipt();
    assert_eq!(
        service
            .apply_surface(apply_two.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    let replayed = service.apply_surface(apply_two.clone()).await.unwrap();
    assert_eq!(replayed.applied.revision, AgentSurfaceRevision(2));
    let mut conflicting = apply_two.clone();
    conflicting.callbacks.binding_generation = AgentBindingGeneration(3);
    assert_eq!(
        service.apply_surface(conflicting).await.unwrap_err().code,
        AgentServiceErrorCode::Conflict
    );
    service
        .execute(submit_envelope(
            source.clone(),
            "generation two",
            "live-execute-2",
        ))
        .await
        .unwrap();
    assert_eq!(
        host.generations.lock().unwrap().as_slice(),
        &[AgentBindingGeneration(1), AgentBindingGeneration(2),]
    );

    let reopened = DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider: provider.clone(),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        host.clone(),
        store.clone(),
    );
    assert_eq!(reopened.apply_surface(apply_two).await.unwrap(), replayed);
    reopened
        .execute(submit_envelope(
            source.clone(),
            "generation two reopened",
            "live-execute-3",
        ))
        .await
        .unwrap();
    assert_eq!(
        host.generations.lock().unwrap().as_slice(),
        &[
            AgentBindingGeneration(1),
            AgentBindingGeneration(2),
            AgentBindingGeneration(2),
        ]
    );

    let revoke = RevokeBoundAgentSurface {
        command_id: AgentCommandId::new("live-revoke-command").unwrap(),
        effect_id: AgentEffectIdentity::new("live-revoke-effect").unwrap(),
        idempotency_key: AgentIdempotencyKey::new("live-revoke-idem").unwrap(),
        binding_generation: AgentBindingGeneration(2),
        source: source.clone(),
        expected_revision: AgentSurfaceRevision(2),
    };
    store.lose_next_commit_receipt();
    assert_eq!(
        service
            .revoke_surface(revoke.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    service.revoke_surface(revoke.clone()).await.unwrap();
    let mut conflicting_revoke = revoke;
    conflicting_revoke.binding_generation = AgentBindingGeneration(3);
    assert_eq!(
        service
            .revoke_surface(conflicting_revoke)
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Conflict
    );
    service
        .execute(submit_envelope(
            source.clone(),
            "after revoke",
            "live-execute-after-revoke",
        ))
        .await
        .unwrap();
    assert_eq!(
        host.generations.lock().unwrap().as_slice(),
        &[
            AgentBindingGeneration(1),
            AgentBindingGeneration(2),
            AgentBindingGeneration(2),
        ],
        "revoke must clear the old live callback materializer"
    );
    assert!(
        reopened
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap()
            .applied_surface
            .is_none()
    );
}

fn hook_execution_surface() -> BoundAgentSurface {
    let hook = |id: &str,
                point: AgentHookPoint,
                timing: AgentHookTiming,
                action: AgentHookAction,
                mutation: AgentHookMutationKind| {
        BoundAgentSurfaceContribution {
            key: format!("hook:{id}"),
            required: true,
            route: AgentSurfaceRoute::AgentNativeCallback,
            fidelity: SemanticFidelity::Exact,
            semantics: AgentSurfaceSemanticFacet::Hook(
                agentdash_agent_service_api::AgentHookSemanticFacet {
                    point,
                    timing,
                    blocking: AgentHookBlockingSemantics::Blocking {
                        fidelity: SemanticFidelity::Exact,
                    },
                    mutations: BTreeMap::from([(mutation, SemanticFidelity::Exact)]),
                    effects: BTreeMap::new(),
                },
            ),
            payload: AgentSurfaceContributionPayload::Hook {
                definition_id: AgentHookDefinitionId::new(id).unwrap(),
                point,
                timing,
                actions: BTreeSet::from([AgentHookAction::AllowOrDeny, action]),
                deadline_ms: 2_000,
            },
            payload_digest: AgentPayloadDigest::new(format!("sha256:{id}")).unwrap(),
        }
    };
    BoundAgentSurface {
        revision: AgentSurfaceRevision(1),
        digest: AgentSurfaceDigest::new("hook-execution-surface").unwrap(),
        offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
        contributions: vec![
            BoundAgentSurfaceContribution {
                key: "tool:read".into(),
                required: true,
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::HotUpdate,
                }),
                payload: AgentSurfaceContributionPayload::Tool {
                    name: AgentToolName::new("read").unwrap(),
                    description: "read".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: None,
                },
                payload_digest: AgentPayloadDigest::new("sha256:hook-tool").unwrap(),
            },
            hook(
                "before-tool",
                AgentHookPoint::BeforeTool,
                AgentHookTiming::Before,
                AgentHookAction::RewriteInput,
                AgentHookMutationKind::RewriteInput,
            ),
            hook(
                "after-tool",
                AgentHookPoint::AfterTool,
                AgentHookTiming::After,
                AgentHookAction::RewriteResult,
                AgentHookMutationKind::RewriteResult,
            ),
        ],
    }
}

fn generation_surface(revision: u64) -> BoundAgentSurface {
    BoundAgentSurface {
        revision: AgentSurfaceRevision(revision),
        digest: AgentSurfaceDigest::new(format!("generation-surface-{revision}")).unwrap(),
        offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
        contributions: vec![BoundAgentSurfaceContribution {
            key: "tool:read".into(),
            required: true,
            route: AgentSurfaceRoute::AgentNativeCallback,
            fidelity: SemanticFidelity::Exact,
            semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                delivery: AgentToolDelivery::AgentNativeCallback,
                invocation: SemanticFidelity::Exact,
                update: AgentToolUpdateSemantics::HotUpdate,
            }),
            payload: AgentSurfaceContributionPayload::Tool {
                name: AgentToolName::new("read").unwrap(),
                description: "read".into(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: None,
            },
            payload_digest: AgentPayloadDigest::new(format!("sha256:generation-tool-{revision}"))
                .unwrap(),
        }],
    }
}

#[tokio::test]
async fn exact_hooks_run_once_rewrite_and_do_not_retrigger_on_effect_replay() {
    let store = Arc::new(RecordingCompleteStore::default());
    let provider = Arc::new(HookRoundProvider {
        calls: AtomicUsize::new(0),
        requests: Mutex::new(Vec::new()),
    });
    let host = Arc::new(HookExecutionCallbacks::default());
    let service = DashAgentCompleteService::with_host_callbacks(
        DashExecutionDependencies {
            provider: provider.clone(),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            history_callbacks: Arc::new(NoopDashHistoryCallbacks),
            compactor: Arc::new(FixtureCompactor),
            conversation_namer: Arc::new(NoopDashConversationNamer),
        },
        host.clone(),
        store,
    );
    let source = AgentSourceCoordinate::new("dash-hook-execution").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("hook-create", "hook-effect-create"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    service
        .apply_surface(ApplyBoundAgentSurface {
            command_id: AgentCommandId::new("hook-apply").unwrap(),
            effect_id: AgentEffectIdentity::new("hook-effect-apply").unwrap(),
            idempotency_key: AgentIdempotencyKey::new("hook-idem-apply").unwrap(),
            source: source.clone(),
            bound_surface: hook_execution_surface(),
            callbacks: AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new("hook-route").unwrap(),
                binding_generation: AgentBindingGeneration(11),
                delivery: AgentSurfaceRoute::AgentNativeCallback,
                default_deadline_ms: 5_000,
            },
        })
        .await
        .unwrap();
    let request = submit_envelope(source, "hook-input", "hook-effect-input");
    let first = service.execute(request.clone()).await.unwrap();
    let replay = service.execute(request).await.unwrap();
    assert_eq!(first, replay);
    assert_eq!(host.before.load(Ordering::SeqCst), 1);
    assert_eq!(host.after.load(Ordering::SeqCst), 1);
    assert_eq!(host.tools.load(Ordering::SeqCst), 1);
    assert_eq!(
        host.tool_arguments.lock().unwrap().as_slice(),
        &[serde_json::json!({"rewritten": true})]
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let requests = provider.requests.lock().unwrap();
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|message| message.content == "rewritten-result")
    );
    drop(requests);
    let snapshot = service
        .read(AgentReadQuery {
            source: AgentSourceCoordinate::new("dash-hook-execution").unwrap(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(snapshot.conversation_history.iter().any(|record| matches!(
        &record.presentation.envelope.event,
        BackboneEvent::ItemCompleted(notification)
            if matches!(
                &notification.item,
                agentdash_agent_protocol::AgentDashThreadItem::Codex(
                    codex::ThreadItem::DynamicToolCall { tool, success, .. }
                ) if tool == "read" && *success == Some(Some(true))
            )
    )));
    assert!(snapshot.conversation_history.iter().any(|record| matches!(
        &record.presentation.envelope.event,
        BackboneEvent::ItemCompleted(notification)
            if matches!(
                &notification.item,
                agentdash_agent_protocol::AgentDashThreadItem::Codex(
                    codex::ThreadItem::AgentMessage { text, .. }
                ) if text == "hooked answer"
            )
    )));
}

#[tokio::test]
async fn shared_durable_store_reopens_source_fork_tail_initial_context_and_effects() {
    let store = Arc::new(RecordingCompleteStore::default());
    let first = service_with_store(store.clone());
    let parent = AgentSourceCoordinate::new("dash-durable-parent").unwrap();
    let package = initial_package();
    let created = first
        .create(CreateAgentCommand {
            meta: meta("durable-create", "durable-effect-create"),
            requested_source: Some(parent.clone()),
            initial_context: Some(package.clone()),
        })
        .await
        .unwrap();
    let submitted = first
        .execute(submit_envelope(
            parent.clone(),
            "durable-input",
            "durable-effect-input",
        ))
        .await
        .unwrap();

    let second = service_with_store(store.clone());
    let reopened = second
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(reopened.revision, submitted.snapshot_revision.unwrap());
    assert_eq!(
        reopened.initial_context.unwrap().package_digest,
        package.digest
    );
    let tail = second
        .changes(AgentChangesQuery {
            source: parent.clone(),
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    assert!(!tail.changes.is_empty());
    let create_inspection = second
        .inspect(AgentEffectIdentity::new("durable-effect-create").unwrap())
        .await
        .unwrap();
    assert!(create_inspection.validate());
    assert!(matches!(
        create_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Create { receipt }
        } if receipt.command_id == created.command_id
            && receipt.source == parent
            && receipt.initial_context == created.initial_context
            && receipt.snapshot_revision == created.snapshot_revision
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));
    assert!(matches!(
        second
            .inspect(AgentEffectIdentity::new("durable-effect-input").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { .. }
        }
    ));

    let child = AgentSourceCoordinate::new("dash-durable-child").unwrap();
    second
        .fork(ForkAgentCommand {
            meta: meta("durable-fork", "durable-effect-fork"),
            source: parent.clone(),
            requested_child_source: Some(child.clone()),
            cutoff: AgentForkPoint::Head,
        })
        .await
        .unwrap();

    let third = service_with_store(store);
    let parent_before = third
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    third
        .execute(submit_envelope(
            child.clone(),
            "durable-child-input",
            "durable-child-effect",
        ))
        .await
        .unwrap();
    let parent_after = third
        .read(AgentReadQuery {
            source: parent,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(parent_before.revision, parent_after.revision);
    assert!(
        third
            .read(AgentReadQuery {
                source: child,
                at_revision: None,
            })
            .await
            .unwrap()
            .revision
            > parent_after.revision
    );
    assert!(matches!(
        third
            .inspect(AgentEffectIdentity::new("durable-effect-fork").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt }
        } if receipt.parent_source.as_str() == "dash-durable-parent"
            && receipt.child_source.as_str() == "dash-durable-child"
            && receipt.cutoff == AgentForkPoint::Head
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));
    assert!(matches!(
        third
            .inspect(AgentEffectIdentity::new("durable-unknown").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::NotApplied
    ));
}

#[tokio::test]
async fn atomic_commits_recover_lost_receipts_without_duplicate_source_fork_or_surface_mutation() {
    let store = Arc::new(RecordingCompleteStore::default());
    let parent = AgentSourceCoordinate::new("dash-atomic-parent").unwrap();
    let create = CreateAgentCommand {
        meta: meta("atomic-create", "atomic-effect-create"),
        requested_source: Some(parent.clone()),
        initial_context: None,
    };

    store.lose_next_commit_receipt();
    assert_eq!(
        service_with_store(store.clone())
            .create(create.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    let reopened = service_with_store(store.clone());
    let create_inspection = reopened
        .inspect(create.meta.effect_id.clone())
        .await
        .unwrap();
    assert!(create_inspection.validate());
    assert!(matches!(
        create_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Create { receipt }
        } if receipt.source == parent
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));
    let created = reopened.create(create.clone()).await.unwrap();
    assert_eq!(
        reopened.create(create.clone()).await.unwrap(),
        created,
        "create replay must not create another source"
    );
    assert_eq!(store.durable.read().await.repositories.len(), 1);
    let mut conflicting_create = create;
    conflicting_create.initial_context = Some(initial_package());
    assert_eq!(
        reopened.create(conflicting_create).await.unwrap_err().code,
        AgentServiceErrorCode::Conflict
    );

    let resume = ResumeAgentCommand {
        meta: meta("atomic-resume", "atomic-effect-resume"),
        source: parent.clone(),
    };
    store.lose_next_commit_receipt();
    assert_eq!(
        reopened.resume(resume.clone()).await.unwrap_err().code,
        AgentServiceErrorCode::Unavailable
    );
    let reopened = service_with_store(store.clone());
    let resume_inspection = reopened
        .inspect(resume.meta.effect_id.clone())
        .await
        .unwrap();
    assert!(resume_inspection.validate());
    assert!(matches!(
        resume_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Resume { receipt }
        } if receipt.source == parent
            && receipt.snapshot_revision == created.snapshot_revision
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded)
    ));
    let resumed = reopened.resume(resume.clone()).await.unwrap();
    assert_eq!(reopened.resume(resume).await.unwrap(), resumed);
    assert_eq!(
        store.durable.read().await.repositories.len(),
        1,
        "resume replay must not create or fork a source"
    );

    let child = AgentSourceCoordinate::new("dash-atomic-child").unwrap();
    let fork = ForkAgentCommand {
        meta: meta("atomic-fork", "atomic-effect-fork"),
        source: parent.clone(),
        requested_child_source: Some(child.clone()),
        cutoff: AgentForkPoint::Head,
    };
    store.lose_next_commit_receipt();
    assert_eq!(
        reopened.fork(fork.clone()).await.unwrap_err().code,
        AgentServiceErrorCode::Unavailable
    );
    let reopened = service_with_store(store.clone());
    let fork_inspection = reopened.inspect(fork.meta.effect_id.clone()).await.unwrap();
    assert!(fork_inspection.validate());
    let inspected_history_digest = match fork_inspection.state {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt },
        } if receipt.parent_source == parent
            && receipt.child_source == child
            && receipt.cutoff == fork.cutoff
            && receipt.terminal == Some(AgentTerminalOutcome::Succeeded) =>
        {
            receipt.child_history_digest
        }
        state => panic!("unexpected fork inspection after lost receipt: {state:?}"),
    };
    assert_eq!(
        reopened
            .read(AgentReadQuery {
                source: child.clone(),
                at_revision: None,
            })
            .await
            .unwrap()
            .source,
        child,
        "inspect-only recovery must expose the already-created child"
    );
    let forked = reopened.fork(fork.clone()).await.unwrap();
    assert_eq!(forked.child_source.as_ref(), Some(&child));
    assert_eq!(
        forked.child_history_digest.as_ref(),
        Some(&inspected_history_digest)
    );
    assert_eq!(reopened.fork(fork.clone()).await.unwrap(), forked);
    assert_eq!(store.durable.read().await.repositories.len(), 2);
    let mut conflicting_fork = fork;
    conflicting_fork.requested_child_source =
        Some(AgentSourceCoordinate::new("dash-atomic-other-child").unwrap());
    assert_eq!(
        reopened.fork(conflicting_fork).await.unwrap_err().code,
        AgentServiceErrorCode::Conflict
    );

    let apply = ApplyBoundAgentSurface {
        command_id: AgentCommandId::new("atomic-apply").unwrap(),
        effect_id: AgentEffectIdentity::new("atomic-effect-apply").unwrap(),
        idempotency_key: AgentIdempotencyKey::new("atomic-idem-apply").unwrap(),
        source: parent.clone(),
        bound_surface: hook_execution_surface(),
        callbacks: AgentHostCallbackBinding {
            route_id: AgentCallbackRouteId::new("atomic-route").unwrap(),
            binding_generation: AgentBindingGeneration(7),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 5_000,
        },
    };
    store.lose_next_commit_receipt();
    assert_eq!(
        reopened
            .apply_surface(apply.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    let reopened = service_with_store(store.clone());
    let applied_snapshot = reopened
        .read(AgentReadQuery {
            source: parent.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(
        applied_snapshot
            .applied_surface
            .as_ref()
            .map(|surface| surface.revision),
        Some(AgentSurfaceRevision(1))
    );
    let apply_inspection = reopened.inspect(apply.effect_id.clone()).await.unwrap();
    assert!(apply_inspection.validate());
    let inspected_apply = match apply_inspection.state {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceApply { receipt },
        } => receipt,
        state => panic!("unexpected surface apply inspection after lost receipt: {state:?}"),
    };
    let applied = reopened.apply_surface(apply.clone()).await.unwrap();
    assert_eq!(applied, inspected_apply);
    assert_eq!(
        reopened.apply_surface(apply.clone()).await.unwrap(),
        applied
    );
    let mut conflicting_apply = apply;
    conflicting_apply.bound_surface.digest =
        AgentSurfaceDigest::new("atomic-conflicting-surface").unwrap();
    assert_eq!(
        reopened
            .apply_surface(conflicting_apply)
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Conflict
    );

    let revoke = RevokeBoundAgentSurface {
        command_id: AgentCommandId::new("atomic-revoke").unwrap(),
        effect_id: AgentEffectIdentity::new("atomic-effect-revoke").unwrap(),
        idempotency_key: AgentIdempotencyKey::new("atomic-idem-revoke").unwrap(),
        binding_generation: AgentBindingGeneration(7),
        source: parent.clone(),
        expected_revision: AgentSurfaceRevision(1),
    };
    store.lose_next_commit_receipt();
    assert_eq!(
        reopened
            .revoke_surface(revoke.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    let reopened = service_with_store(store.clone());
    assert!(
        reopened
            .read(AgentReadQuery {
                source: parent,
                at_revision: None,
            })
            .await
            .unwrap()
            .applied_surface
            .is_none()
    );
    let revoke_inspection = reopened.inspect(revoke.effect_id.clone()).await.unwrap();
    assert!(revoke_inspection.validate());
    let inspected_revoke = match revoke_inspection.state {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceRevoke { receipt },
        } => receipt,
        state => panic!("unexpected surface revoke inspection after lost receipt: {state:?}"),
    };
    let revoked = reopened.revoke_surface(revoke.clone()).await.unwrap();
    assert_eq!(revoked.command_id, inspected_revoke.command_id);
    assert_eq!(revoked.effect_id, inspected_revoke.effect_id);
    assert_eq!(revoked.source, inspected_revoke.source);
    assert_eq!(
        revoked.snapshot_revision,
        inspected_revoke.snapshot_revision
    );
    assert_eq!(
        inspected_revoke.terminal,
        Some(AgentTerminalOutcome::Succeeded)
    );
    assert_eq!(
        reopened.revoke_surface(revoke.clone()).await.unwrap(),
        revoked
    );
    let mut conflicting_revoke = revoke;
    conflicting_revoke.expected_revision = AgentSurfaceRevision(2);
    assert_eq!(
        reopened
            .revoke_surface(conflicting_revoke)
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Conflict
    );
    assert_eq!(store.durable.read().await.repositories.len(), 2);
}

#[tokio::test]
async fn execute_reservation_survives_lost_response_and_reconciles_dash_once_after_reopen() {
    let store = Arc::new(RecordingCompleteStore::default());
    let service = service_with_store(store.clone());
    let source = AgentSourceCoordinate::new("dash-atomic-execute").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("atomic-execute-create", "atomic-execute-create-effect"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    let execute = submit_envelope(
        source.clone(),
        "atomic durable input",
        "atomic-execute-effect",
    );

    store.lose_next_commit_receipt();
    assert_eq!(
        service.execute(execute.clone()).await.unwrap_err().code,
        AgentServiceErrorCode::Unavailable
    );
    assert!(matches!(
        service
            .inspect(execute.meta.effect_id.clone())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Accepted { .. }
    ));

    let reopened = service_with_store(store.clone());
    let applied = reopened.execute(execute.clone()).await.unwrap();
    assert!(matches!(
        applied.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));
    assert_eq!(reopened.execute(execute.clone()).await.unwrap(), applied);
    let mut conflicting_execute = execute;
    conflicting_execute.command = AgentCommand::Close;
    assert_eq!(
        reopened
            .execute(conflicting_execute)
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Conflict
    );
    assert!(matches!(
        reopened
            .inspect(AgentEffectIdentity::new("atomic-execute-effect").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { .. }
        }
    ));

    let crash_gap_execute = submit_envelope(
        source.clone(),
        "atomic crash gap input",
        "atomic-crash-gap-effect",
    );
    store.fail_next_terminal_commit();
    assert_eq!(
        reopened
            .execute(crash_gap_execute.clone())
            .await
            .unwrap_err()
            .code,
        AgentServiceErrorCode::Unavailable
    );
    assert!(matches!(
        reopened
            .inspect(crash_gap_execute.meta.effect_id.clone())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Accepted { .. }
    ));
    let recovered = service_with_store(store);
    let recovered_receipt = recovered.execute(crash_gap_execute.clone()).await.unwrap();
    assert_eq!(
        recovered.execute(crash_gap_execute).await.unwrap(),
        recovered_receipt
    );
    assert!(matches!(
        recovered
            .inspect(AgentEffectIdentity::new("atomic-crash-gap-effect").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { .. }
        }
    ));
    let snapshot = recovered
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.conversation().completed_turns().count(), 2);
}

#[tokio::test]
async fn manual_compaction_is_exposed_once_in_canonical_history_and_changes() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-compaction").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-compaction", "effect-create-compaction"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    let receipt = service
        .execute(AgentCommandEnvelope {
            meta: meta("compact-1", "effect-compact-1"),
            source: source.clone(),
            command: AgentCommand::RequestCompaction,
        })
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );

    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    let completed = snapshot
        .conversation()
        .completed_items()
        .find(|completed| completed.item.id() == "compact-1")
        .expect("completed compaction item");
    assert!(matches!(
        completed.item.as_codex(),
        Some(codex::ThreadItem::ContextCompaction { id }) if id == "compact-1"
    ));

    let changes = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(changes.changes.len(), 3);
    let presentation = changes
        .changes
        .iter()
        .find_map(|change| match &change.payload {
            agentdash_agent_service_api::AgentChangePayload::SourceObservation {
                state,
                presentation,
            } if state.is_none()
                && presentation.iter().any(|record| {
                    matches!(
                        &record.presentation.envelope.event,
                        BackboneEvent::ItemStarted(started) if started.turn_id == "compact-1"
                    )
                }) =>
            {
                Some(presentation)
            }
            _ => None,
        })
        .expect("compaction start must be one canonical presentation observation");
    assert_eq!(presentation.len(), 1);
    assert!(matches!(
        &presentation[0].presentation.envelope.event,
        BackboneEvent::ItemStarted(started)
            if started.thread_id == "dash-compaction"
                && started.turn_id == "compact-1"
                && matches!(
                    started.item.as_codex(),
                    Some(codex::ThreadItem::ContextCompaction { id })
                        if id == "compact-1"
                )
    ));
}

async fn create_source(service: &DashAgentCompleteService, source: &str) -> AgentSourceCoordinate {
    let source = AgentSourceCoordinate::new(source).unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta(
                &format!("create-{source}"),
                &format!("effect-create-{source}"),
            ),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    source
}

fn submit_envelope(
    source: AgentSourceCoordinate,
    command: &str,
    effect: &str,
) -> AgentCommandEnvelope {
    AgentCommandEnvelope {
        meta: meta(command, effect),
        source,
        command: AgentCommand::SubmitInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: "question".into(),
                }],
            },
        },
    }
}

#[tokio::test]
async fn dash_complete_agent_streams_source_scoped_live_deltas_without_persisting_a_tail() {
    let service = service();
    let source = create_source(&service, "dash-live-events").await;
    let mut live_events = service.live_events(source.clone()).await.unwrap();

    let receipt = service
        .execute(submit_envelope(source.clone(), "live-input", "live-effect"))
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );

    let accepted_input = tokio::time::timeout(Duration::from_secs(1), live_events.next())
        .await
        .expect("accepted user input should be published immediately")
        .expect("live event stream should remain available")
        .expect("live event stream should remain open");
    assert_eq!(accepted_input.source, source);
    assert_eq!(
        accepted_input.record.presentation.durability,
        PresentationDurability::Durable
    );
    assert!(matches!(
        accepted_input.record.presentation.envelope.event,
        BackboneEvent::UserInputSubmitted(_)
    ));

    let turn_started = tokio::time::timeout(Duration::from_secs(1), live_events.next())
        .await
        .expect("durable turn start should follow accepted input")
        .expect("live event stream should remain available")
        .expect("live event stream should remain open");
    assert_eq!(turn_started.source, source);
    assert_eq!(
        turn_started.record.presentation.durability,
        PresentationDurability::Durable
    );
    assert!(matches!(
        turn_started.record.presentation.envelope.event,
        BackboneEvent::TurnStarted(_)
    ));

    let mut previous_sequence = turn_started.sequence.0;
    let text_delta = loop {
        let event = tokio::time::timeout(Duration::from_secs(1), live_events.next())
            .await
            .expect("live event should arrive")
            .expect("live event stream should remain available")
            .expect("live event stream should remain open");
        assert_eq!(event.source, source);
        assert!(event.sequence.0 > previous_sequence);
        previous_sequence = event.sequence.0;
        if let BackboneEvent::AgentMessageDelta(delta) = event.record.presentation.envelope.event {
            break delta.delta;
        }
    };
    assert_eq!(text_delta, "fixture answer");

    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(snapshot.conversation_history.iter().any(|record| matches!(
        &record.presentation.envelope.event,
        BackboneEvent::TurnCompleted(_)
    )));
}

#[tokio::test]
async fn provider_failed_and_lost_are_terminal_and_inspectable() {
    for (name, error, expected, expected_code, expected_retryable) in [
        (
            "failed",
            DashCoreError::Provider {
                code: "rate_limit".into(),
                message: "retry later".into(),
                retryable: true,
            },
            AgentTerminalOutcome::Failed,
            "rate_limit",
            true,
        ),
        (
            "lost",
            DashCoreError::ProviderStreamDisconnected,
            AgentTerminalOutcome::Lost,
            "provider_stream_disconnected",
            false,
        ),
    ] {
        let service = service_with_provider(Arc::new(ErrorProvider { error }));
        let source = create_source(&service, &format!("dash-{name}")).await;
        let effect = format!("effect-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        assert!(matches!(
            service
                .inspect(AgentEffectIdentity::new(effect).unwrap())
                .await
                .unwrap()
                .state,
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Command { receipt }
            } if receipt.terminal == Some(expected)
        ));
        let snapshot = service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap();
        let failure = snapshot
            .conversation()
            .completed_turn(None)
            .expect("terminal turn")
            .error
            .as_ref()
            .and_then(Option::as_ref)
            .expect("terminal Agent snapshot must retain the Dash failure");
        assert!(
            failure
                .additional_details
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|details| details.contains(expected_code)
                    && details.contains(&format!("retryable={expected_retryable}")))
        );
        assert!(failure.message.contains(if name == "failed" {
            "retry later"
        } else {
            "stream disconnected"
        }));
    }
}

#[tokio::test]
async fn resume_preserves_state_old_tail_digest_and_effect_owner_is_exact() {
    let service = service();
    let first = create_source(&service, "dash-resume-first").await;
    let second = create_source(&service, "dash-resume-second").await;
    service
        .execute(submit_envelope(
            first.clone(),
            "input-first",
            "effect-shared",
        ))
        .await
        .unwrap();
    let before = service
        .read(AgentReadQuery {
            source: first.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    let resumed = service
        .resume(ResumeAgentCommand {
            meta: meta("resume-first", "effect-resume-first"),
            source: first.clone(),
        })
        .await
        .unwrap();
    assert_eq!(resumed.snapshot_revision, Some(before.revision));

    let old_tail = service
        .changes(AgentChangesQuery {
            source: first.clone(),
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    service
        .execute(submit_envelope(
            first.clone(),
            "input-second",
            "effect-second",
        ))
        .await
        .unwrap();
    let expanded = service
        .changes(AgentChangesQuery {
            source: first.clone(),
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    for old in &old_tail.changes {
        let replayed = expanded
            .changes
            .iter()
            .find(|change| change.cursor == old.cursor)
            .unwrap();
        assert_eq!(replayed.source_revision, old.source_revision);
    }
    assert_ne!(
        expanded.changes.first().unwrap().source_revision,
        expanded.changes.last().unwrap().source_revision
    );

    let conflict = service
        .execute(submit_envelope(
            second,
            "different-command",
            "effect-shared",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        conflict.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Conflict
    );
}

#[tokio::test]
async fn steer_and_interrupt_orchestrate_the_active_turn() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let service = Arc::new(service_with_provider(Arc::new(SteerProvider {
        started: started.clone(),
        release,
    })));
    let source = create_source(&service, "dash-steer").await;
    let submit_service = service.clone();
    let submit_source = source.clone();
    let submit = tokio::spawn(async move {
        submit_service
            .execute(submit_envelope(
                submit_source,
                "input-steer",
                "effect-input-steer",
            ))
            .await
    });
    started.notified().await;
    let steer = service
        .execute(AgentCommandEnvelope {
            meta: meta("steer", "effect-steer"),
            source: source.clone(),
            command: AgentCommand::Steer {
                expected_turn_id: agentdash_agent_service_api::AgentTurnId::new("turn:input-steer")
                    .unwrap(),
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "new direction".into(),
                    }],
                },
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        steer.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));
    assert!(matches!(
        submit.await.unwrap().unwrap().state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));

    let started = Arc::new(Notify::new());
    let service = Arc::new(service_with_provider(Arc::new(BlockingProvider {
        started: started.clone(),
    })));
    let source = create_source(&service, "dash-interrupt").await;
    let submit_service = service.clone();
    let submit_source = source.clone();
    let submit = tokio::spawn(async move {
        submit_service
            .execute(submit_envelope(
                submit_source,
                "input-interrupt",
                "effect-input-interrupt",
            ))
            .await
    });
    started.notified().await;
    service
        .execute(AgentCommandEnvelope {
            meta: meta("interrupt", "effect-interrupt"),
            source: source.clone(),
            command: AgentCommand::Interrupt {
                expected_turn_id: agentdash_agent_service_api::AgentTurnId::new(
                    "turn:input-interrupt",
                )
                .unwrap(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        submit.await.unwrap().unwrap().state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Interrupted
        }
    ));
    assert!(
        service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap()
            .active_turn_id()
            .is_none()
    );
}

#[tokio::test]
async fn resolve_interaction_completes_the_suspended_turn() {
    let service = service_with_provider(Arc::new(InteractionProvider));
    let source = create_source(&service, "dash-interaction").await;
    let submit = service
        .execute(submit_envelope(
            source.clone(),
            "input-interaction",
            "effect-input-interaction",
        ))
        .await
        .unwrap();
    assert_eq!(submit.state, AgentReceiptState::Accepted);
    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.interactions.len(), 1);
    service
        .execute(AgentCommandEnvelope {
            meta: meta("resolve", "effect-resolve"),
            source: source.clone(),
            command: AgentCommand::ResolveInteraction {
                interaction_id: snapshot.interactions[0].id.clone(),
                response: agentdash_agent_service_api::AgentInteractionResponse::Approved,
            },
        })
        .await
        .unwrap();
    let resolved = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(
        resolved.interactions[0].status,
        agentdash_agent_service_api::AgentInteractionStatus::Resolved
    );
    assert!(resolved.interactions[0].resolution.is_some());
    assert!(resolved.active_turn_id().is_none());
}

#[tokio::test]
async fn automatic_overflow_runs_a_b_c_through_the_dash_worker() {
    let service = service_with_provider(Arc::new(OverflowProvider {
        calls: AtomicUsize::new(0),
    }));
    let source = create_source(&service, "dash-auto-compaction").await;
    let receipt = service
        .execute(submit_envelope(
            source.clone(),
            "input-auto",
            "effect-input-auto",
        ))
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(
        snapshot
            .conversation()
            .completed_items()
            .any(|completed| completed.item.id() == "input-auto:B")
    );
    assert!(
        snapshot
            .conversation()
            .completed_turns()
            .any(|turn| turn.id == "turn:input-auto:C")
    );
}

#[tokio::test]
async fn automatic_compaction_b_failure_and_lost_settle_original_and_block_c() {
    for (name, lost, expected) in [
        ("failed", false, AgentTerminalOutcome::Failed),
        ("lost", true, AgentTerminalOutcome::Lost),
    ] {
        let service = service_with(
            Arc::new(OverflowProvider {
                calls: AtomicUsize::new(0),
            }),
            Arc::new(FailingCompactor { lost }),
        );
        let source = create_source(&service, &format!("dash-auto-b-{name}")).await;
        let effect = format!("effect-auto-b-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-auto-b-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        let snapshot = service
            .read(AgentReadQuery {
                source: source.clone(),
                at_revision: None,
            })
            .await
            .unwrap();
        assert!(snapshot.active_turn_id().is_none());
        assert!(
            !snapshot
                .conversation()
                .completed_turns()
                .any(|turn| turn.id.ends_with(":C"))
        );
        assert!(matches!(
            service
                .inspect(AgentEffectIdentity::new(effect).unwrap())
                .await
                .unwrap()
                .state,
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Command { receipt }
            } if receipt.terminal == Some(expected)
        ));
    }
}

#[tokio::test]
async fn automatic_continuation_c_failure_and_lost_settle_original_and_clear_active() {
    for (name, error, expected) in [
        (
            "failed",
            DashCoreError::Provider {
                code: "continuation_failed".into(),
                message: "continuation failed".into(),
                retryable: true,
            },
            AgentTerminalOutcome::Failed,
        ),
        (
            "lost",
            DashCoreError::ProviderStreamDisconnected,
            AgentTerminalOutcome::Lost,
        ),
    ] {
        let service = service_with_provider(Arc::new(OverflowThenErrorProvider {
            calls: AtomicUsize::new(0),
            error,
        }));
        let source = create_source(&service, &format!("dash-auto-c-{name}")).await;
        let effect = format!("effect-auto-c-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-auto-c-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        let snapshot = service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap();
        assert!(snapshot.active_turn_id().is_none());
        assert!(
            snapshot
                .conversation()
                .completed_turns()
                .any(|turn| turn.id.ends_with(":C") && turn.status == codex::TurnStatus::Failed)
        );
    }
}

#[tokio::test]
async fn close_is_a_terminal_lifecycle_command() {
    let service = service();
    let source = create_source(&service, "dash-close").await;
    let receipt = service
        .execute(AgentCommandEnvelope {
            meta: meta("close", "effect-close"),
            source: source.clone(),
            command: AgentCommand::Close,
        })
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Closed
        }
    );
    assert_eq!(
        service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap()
            .lifecycle,
        agentdash_agent_service_api::AgentLifecycleStatus::Closed
    );
}
