use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use agentdash_agent::{AgentMessage, ContentPart};
use agentdash_agent::{BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, TokenUsage};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_test_support::RuntimeTraceValidator;
use agentdash_integration_api::*;
use agentdash_integration_native_agent::{
    NativeAgentServiceConfig, NativeBridgeResolveError, NativeBridgeResolver,
    NativeCredentialScope, NativePresentationMetadata, ResolvedNativeBridge,
    native_agent_contribution,
};
use async_trait::async_trait;
use futures::stream;
use serde_json::json;
use tokio::sync::Mutex;

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid test id")
}

fn config_instance(config: serde_json::Value) -> ActivatedAgentServiceInstance {
    let contribution = native_agent_contribution(Arc::new(Resolver));
    ActivatedAgentServiceInstance {
        instance_id: id("native-config-instance"),
        instance_revision: 1,
        generation: RuntimeDriverGeneration(1),
        definition: contribution.definition,
        config,
        credentials: BTreeMap::new(),
        placement: AgentRuntimePlacement::InProcess,
    }
}

#[test]
fn native_service_config_requires_explicit_user_credential_scope() {
    let config = NativeAgentServiceConfig::from_instance(&config_instance(json!({
        "provider": "openai",
        "model": "gpt-5",
        "credential_scope": { "kind": "user", "user_id": "user-1" }
    })))
    .expect("explicit user scope");

    assert_eq!(
        config.credential_scope,
        NativeCredentialScope::User {
            user_id: "user-1".to_string()
        }
    );
}

#[test]
fn native_service_config_rejects_missing_credential_scope() {
    let error = NativeAgentServiceConfig::from_instance(&config_instance(json!({
        "provider": "openai",
        "model": "gpt-5"
    })))
    .expect_err("missing scope must not imply platform fallback");

    assert!(matches!(
        error,
        NativeBridgeResolveError::InvalidConfiguration { .. }
    ));
}

#[test]
fn native_service_config_rejects_blank_user_coordinate() {
    let error = NativeAgentServiceConfig::from_instance(&config_instance(json!({
        "provider": "openai",
        "model": "gpt-5",
        "credential_scope": { "kind": "user", "user_id": "  " }
    })))
    .expect_err("blank user coordinate");

    assert!(matches!(
        error,
        NativeBridgeResolveError::InvalidConfiguration { .. }
    ));
}

struct EchoBridge;

#[async_trait]
impl LlmBridge for EchoBridge {
    async fn stream_complete(
        &self,
        _request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let message = AgentMessage::assistant("native response");
        Box::pin(stream::iter(vec![
            StreamChunk::TextDelta("native ".to_string()),
            StreamChunk::TextDelta("response".to_string()),
            StreamChunk::Done(BridgeResponse {
                message,
                raw_content: vec![ContentPart::text("native response")],
                usage: TokenUsage::default(),
            }),
        ]))
    }
}

#[derive(Clone, Default)]
struct RecordingBridge {
    requests: Arc<Mutex<Vec<BridgeRequest>>>,
}

#[async_trait]
impl LlmBridge for RecordingBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        self.requests.lock().await.push(request);
        let message = AgentMessage::assistant("restored response");
        Box::pin(stream::iter(vec![
            StreamChunk::TextDelta("restored response".to_string()),
            StreamChunk::Done(BridgeResponse {
                message,
                raw_content: vec![ContentPart::text("restored response")],
                usage: TokenUsage::default(),
            }),
        ]))
    }
}

struct RecordingResolver(Arc<RecordingBridge>);

fn is_naming_request(request: &BridgeRequest) -> bool {
    request
        .system_prompt
        .as_deref()
        .is_some_and(|prompt| prompt.contains("会话标题"))
}

#[async_trait]
impl NativeBridgeResolver for RecordingResolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
        Ok(ResolvedNativeBridge {
            bridge: self.0.clone(),
            presentation: NativePresentationMetadata {
                model_context_window: 200_000,
                reserve_tokens: 0,
            },
        })
    }
}

#[derive(Clone)]
struct BlockingBridge {
    started: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl LlmBridge for BlockingBridge {
    async fn stream_complete(
        &self,
        _request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        self.started.notify_waiters();
        Box::pin(stream::pending())
    }
}

struct BlockingResolver(Arc<BlockingBridge>);

#[async_trait]
impl NativeBridgeResolver for BlockingResolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
        Ok(ResolvedNativeBridge {
            bridge: self.0.clone(),
            presentation: NativePresentationMetadata {
                model_context_window: 200_000,
                reserve_tokens: 0,
            },
        })
    }
}

struct Resolver;

#[async_trait]
impl NativeBridgeResolver for Resolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
        Ok(ResolvedNativeBridge {
            bridge: Arc::new(EchoBridge),
            presentation: NativePresentationMetadata {
                model_context_window: 200_000,
                reserve_tokens: 0,
            },
        })
    }
}

struct NoCredentials;

#[async_trait]
impl AgentRuntimeCredentialBroker for NoCredentials {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Err(CredentialResolveError::Unavailable {
            slot: slot.clone(),
            reason: "native test has no credential slots".to_string(),
        })
    }
}

struct Surfaces {
    initial: MaterializedDriverSurface,
    replacement: DriverToolSurface,
}

#[async_trait]
impl AgentRuntimeSurfaceBroker for Surfaces {
    async fn materialize(
        &self,
        _request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        Ok(self.initial.clone())
    }

    async fn materialize_tool_set(
        &self,
        _binding_id: RuntimeBindingId,
        revision: ToolSetRevision,
        digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError> {
        if self.replacement.revision != revision || self.replacement.digest != digest {
            return Err(DriverSurfaceError::Stale);
        }
        Ok(self.replacement.clone())
    }
}

struct Contexts {
    activation: DriverContextActivation,
    transcript: DriverTranscript,
}

#[async_trait]
impl AgentRuntimeContextBroker for Contexts {
    async fn load_transcript(
        &self,
        _request: DriverTranscriptRequest,
    ) -> Result<DriverTranscript, DriverContextError> {
        Ok(self.transcript.clone())
    }

    async fn load_checkpoint(
        &self,
        _request: DriverContextCheckpointRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        Ok(self.activation.clone())
    }

    async fn compaction_activation(
        &self,
        _request: DriverCompactionActivationRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        Ok(self.activation.clone())
    }
}

struct NoTools;

#[async_trait]
impl AgentRuntimeToolCallback for NoTools {
    async fn invoke(
        &self,
        _request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
        Err(DriverToolCallbackError::ProtocolViolation {
            reason: "test model does not call tools".to_string(),
        })
    }
}

struct ContinueHooks;

#[async_trait]
impl AgentRuntimeHookCallback for ContinueHooks {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        Ok(DriverHookDecision::Continue {
            payload: request.payload,
        })
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<DriverEventEnvelope>>);

#[async_trait]
impl DriverEventSink for Sink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        self.0.lock().await.push(event);
        Ok(())
    }
}

struct DispatchOnTerminalSink {
    driver: Arc<dyn AgentRuntimeDriver>,
    next: Mutex<Option<DriverCommandEnvelope>>,
    next_sink: Arc<Sink>,
    outcome:
        Mutex<Option<tokio::sync::oneshot::Sender<Result<DriverDispatchReceipt, DriverError>>>>,
}

#[async_trait]
impl DriverEventSink for DispatchOnTerminalSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        let terminal = event.facts.iter().any(|fact| {
            matches!(
                fact,
                RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
            )
        });
        if terminal && let Some(next) = self.next.lock().await.take() {
            let result = self.driver.dispatch(next, self.next_sink.clone()).await;
            if let Some(outcome) = self.outcome.lock().await.take() {
                let _ = outcome.send(result);
            }
        }
        Ok(())
    }
}

async fn wait_for_turn_terminal(sink: &Sink) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let terminal = sink.0.lock().await.iter().any(|event| {
                event.facts.iter().any(|fact| {
                    matches!(
                        fact,
                        RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
                    )
                })
            });
            if terminal {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Native turn terminal timeout");
}

async fn wait_for_thread_name(sink: &Sink) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let named = sink.0.lock().await.iter().any(|event| {
                event.facts.iter().any(|fact| {
                    matches!(
                        fact,
                        RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                            event: agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(_),
                            ..
                        })
                    )
                })
            });
            if named {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Native thread name timeout");
}

struct FailingSink;

#[async_trait]
impl DriverEventSink for FailingSink {
    async fn emit(&self, _event: DriverEventEnvelope) -> Result<(), DriverError> {
        Err(DriverError::Lost {
            reason: "test sink disconnected".to_string(),
            retryable: true,
        })
    }
}

#[derive(Default)]
struct FailAfterFirstEventSink(std::sync::atomic::AtomicUsize);

#[async_trait]
impl DriverEventSink for FailAfterFirstEventSink {
    async fn emit(&self, _event: DriverEventEnvelope) -> Result<(), DriverError> {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
            return Err(DriverError::Lost {
                reason: "test sink disconnected during the active prompt".to_string(),
                retryable: true,
            });
        }
        Ok(())
    }
}

#[derive(Default)]
struct TerminalizingSink {
    attempts: std::sync::atomic::AtomicUsize,
    events: Mutex<Vec<DriverEventEnvelope>>,
}

#[async_trait]
impl DriverEventSink for TerminalizingSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        self.events.lock().await.push(event);
        if self
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            > 0
        {
            return Err(DriverError::Terminalized {
                reason: "test Managed Runtime terminalized the turn".into(),
            });
        }
        Ok(())
    }
}

fn recipe(tool_set_revision: ToolSetRevision) -> ContextRecipe {
    ContextRecipe {
        revision: ContextRecipeRevision(1),
        provenance: ContextProvenance {
            settings_revision: ThreadSettingsRevision(1),
            tool_set_revision,
        },
        source_item_ids: Vec::new(),
    }
}

fn fixture_surface() -> MaterializedDriverSurface {
    MaterializedDriverSurface {
        runtime_thread_id: id("runtime-thread-1"),
        revision: SurfaceRevision(7),
        digest: id("sha256:native-surface"),
        authorization_identity: None,
        context: DriverContextSurface {
            recipe: recipe(ToolSetRevision(3)),
            instructions: vec![DriverInstructionSet {
                channel: InstructionChannel::System,
                entries: vec!["system".to_string()],
            }],
            blocks: vec![ContextBlock::Input {
                input: vec![RuntimeInput::text("restored".to_string())],
            }],
            digest: id("sha256:context-0"),
            fidelity: ContextFidelity::PlatformExact,
        },
        tools: DriverToolSurface {
            revision: ToolSetRevision(3),
            digest: "sha256:tools-3".to_string(),
            tools: Vec::new(),
        },
        hooks: DriverHookSurface {
            revision: HookPlanRevision(2),
            digest: id("sha256:hooks-2"),
            artifact_digest: Some("sha256:hook-artifact".to_string()),
            configuration_boundary: ConfigurationBoundary::Binding,
            bindings: vec![DriverHookBinding {
                definition_id: id("hook-before-tool"),
                point: HookPoint::BeforeTool,
                actions: vec![HookAction::Observe],
                strength: SemanticStrength::ExactSynchronous,
                failure_policy: HookFailurePolicy::FailClosed,
                required: true,
                site: HookExecutionSite::AgentCoreCallback,
            }],
        },
        workspace: DriverWorkspaceSurface {
            digest: "sha256:workspace-empty".to_string(),
            capabilities: Vec::new(),
            roots: Vec::new(),
        },
    }
}

fn thread_start(presentation_turn_id: &str, input: Vec<RuntimeInput>) -> RuntimeCommand {
    let profile = native_agent_contribution(Arc::new(Resolver))
        .definition
        .service_profile_upper_bound;
    RuntimeCommand::ThreadStart {
        thread_id: id("runtime-thread-1"),
        presentation_thread_id: id("presentation-thread-1"),
        presentation_turn_id: Some(id(presentation_turn_id)),
        binding_id: id("binding-1"),
        driver_generation: RuntimeDriverGeneration(4),
        source_thread_id: id("native-thread-binding-1"),
        profile_digest: id("native-profile"),
        bound_profile: Box::new(profile),
        input,
        surface: Box::new(RuntimeSurfaceDescriptor {
            source_frame_id: "native-fixture-frame-7".to_string(),
            surface_revision: SurfaceRevision(7),
            surface_digest: id("sha256:native-surface"),
            vfs_digest: "sha256:workspace-empty".to_string(),
            context_recipe_revision: ContextRecipeRevision(1),
            context_digest: id("sha256:context-0"),
            settings_revision: ThreadSettingsRevision(1),
            tool_set_revision: ToolSetRevision(3),
            tool_set_digest: "sha256:tools-3".to_string(),
            hook_plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: id("sha256:native-hook-plan"),
                entries: Vec::new(),
            },
            terminal_hook_effect_binding: None,
        }),
        settings_revision: ThreadSettingsRevision(1),
    }
}

async fn driver_fixture() -> (Arc<dyn AgentRuntimeDriver>, DriverBinding) {
    driver_fixture_with(
        Arc::new(Resolver),
        DriverTranscript {
            current_thread_name: None,
            earliest_available: EventSequence(1),
            latest_available: EventSequence(0),
            active_compaction_source_end: None,
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        },
    )
    .await
}

async fn driver_fixture_with(
    resolver: Arc<dyn NativeBridgeResolver>,
    transcript: DriverTranscript,
) -> (Arc<dyn AgentRuntimeDriver>, DriverBinding) {
    driver_fixture_with_surface(resolver, transcript, fixture_surface()).await
}

async fn driver_fixture_with_surface(
    resolver: Arc<dyn NativeBridgeResolver>,
    transcript: DriverTranscript,
    initial_surface: MaterializedDriverSurface,
) -> (Arc<dyn AgentRuntimeDriver>, DriverBinding) {
    let contribution = native_agent_contribution(resolver);
    let instance_id: RuntimeServiceInstanceId = id("native-instance");
    let activation = DriverContextActivation {
        candidate_id: id("candidate-1"),
        checkpoint_id: id("checkpoint-1"),
        context_revision: ContextRevision(1),
        materialized: MaterializedContext {
            recipe: recipe(ToolSetRevision(3)),
            blocks: vec![ContextBlock::CompactionSummary {
                summary: "compacted".to_string(),
            }],
            digest: id("sha256:context-1"),
            fidelity: ContextFidelity::PlatformExact,
        },
    };
    let driver = contribution
        .factory
        .create(
            ActivatedAgentServiceInstance {
                instance_id: instance_id.clone(),
                instance_revision: 1,
                generation: RuntimeDriverGeneration(4),
                definition: contribution.definition,
                config: json!({
                    "provider": "test",
                    "model": "echo",
                    "credential_scope": { "kind": "platform" }
                }),
                credentials: BTreeMap::new(),
                placement: AgentRuntimePlacement::InProcess,
            },
            RuntimeDriverHostPorts {
                credentials: Arc::new(NoCredentials),
                surfaces: Arc::new(Surfaces {
                    initial: initial_surface,
                    replacement: DriverToolSurface {
                        revision: ToolSetRevision(4),
                        digest: "sha256:tools-4".to_string(),
                        tools: Vec::new(),
                    },
                }),
                context: Arc::new(Contexts {
                    activation,
                    transcript,
                }),
                tools: Arc::new(NoTools),
                hooks: Arc::new(ContinueHooks),
            },
        )
        .await
        .expect("create native driver");
    let binding = driver
        .bind(DriverBindRequest {
            binding_id: id("binding-1"),
            service_instance_id: instance_id,
            surface_revision: SurfaceRevision(7),
            surface_digest: id("sha256:native-surface"),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind native driver");
    (driver, binding)
}

fn transcript_record(
    sequence: u64,
    operation_id: &str,
    presentation_turn_id: &str,
    entry_index: u32,
    event: agentdash_agent_protocol::BackboneEvent,
) -> RuntimeJournalRecord {
    RuntimeJournalRecord::new(
        RuntimeCarrierMetadata {
            thread_id: id("runtime-thread-1"),
            recorded_at_ms: sequence,
            sequence: Some(EventSequence(sequence)),
            transient: None,
            revision: RuntimeRevision(sequence),
            operation_id: Some(id(operation_id)),
            append_idempotency_key: None,
            binding_id: Some(id("binding-1")),
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                presentation_turn_id: Some(id(presentation_turn_id)),
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some("native-thread-binding-1".to_string()),
                source_turn_id: Some(presentation_turn_id.to_string()),
                source_item_id: None,
                source_request_id: None,
                source_entry_index: Some(entry_index),
            },
        },
        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            event,
        )),
    )
    .expect("durable transcript record")
}

#[tokio::test]
async fn native_cold_start_replays_durable_tool_history_without_duplicating_current_prompt() {
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, UserInputSource, UserInputSubmissionKind,
        UserInputSubmittedNotification,
    };

    let old_user = BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
        "presentation-thread-1",
        "presentation-turn-old",
        "user-old",
        UserInputSubmissionKind::Prompt,
        UserInputSource::core_composer(),
        vec![codex::UserInput::Text {
            text: "old prompt".to_string(),
            text_elements: Vec::new(),
        }],
    ));
    let old_tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
        codex::ThreadItem::DynamicToolCall {
            id: "turn_007:tool_012".to_string(),
            tool: "fs_glob".to_string(),
            arguments: serde_json::json!({"pattern":"**/*.rs"}),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(Some(vec![
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "src/lib.rs".to_string(),
                },
            ])),
            duration_ms: None,
            namespace: None,
            success: Some(Some(true)),
        },
        "presentation-thread-1",
        "presentation-turn-old",
    ));
    let old_assistant = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
        codex::ThreadItem::AgentMessage {
            id: "message-old".to_string(),
            text: "old answer".to_string(),
            phase: None,
            memory_citation: None,
        },
        "presentation-thread-1",
        "presentation-turn-old",
    ));
    let current_user = BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
        "presentation-thread-1",
        "native-turn-request-restore",
        "user-current",
        UserInputSubmissionKind::Prompt,
        UserInputSource::core_composer(),
        vec![codex::UserInput::Text {
            text: "current prompt".to_string(),
            text_elements: Vec::new(),
        }],
    ));
    let transcript = DriverTranscript {
        current_thread_name: None,
        earliest_available: EventSequence(1),
        latest_available: EventSequence(4),
        active_compaction_source_end: None,
        completed_presentation_item_ids: vec![
            "turn_007:tool_012".to_string(),
            "message-old".to_string(),
        ],
        records: vec![
            transcript_record(1, "operation-old", "presentation-turn-old", 0, old_user),
            transcript_record(2, "operation-old", "presentation-turn-old", 1, old_tool),
            transcript_record(
                3,
                "operation-old",
                "presentation-turn-old",
                2,
                old_assistant,
            ),
            transcript_record(
                4,
                "operation-restore",
                "native-turn-request-restore",
                0,
                current_user,
            ),
        ],
    };
    let bridge = Arc::new(RecordingBridge::default());
    let (driver, binding) =
        driver_fixture_with(Arc::new(RecordingResolver(bridge.clone())), transcript).await;
    let sink = Arc::new(Sink::default());
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-restore"),
                operation_id: id("operation-restore"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: Some(id("turn-request-restore")),
                presentation_turn_id: Some(id("native-turn-request-restore")),
                command: thread_start(
                    "native-turn-request-restore",
                    vec![RuntimeInput::text("current prompt".to_string())],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("accept restored turn");
    wait_for_turn_terminal(sink.as_ref()).await;

    let requests = bridge.requests.lock().await;
    let request = requests.first().expect("provider request");
    let text_messages = request
        .messages
        .iter()
        .filter_map(AgentMessage::first_text)
        .collect::<Vec<_>>();
    assert!(text_messages.contains(&"old prompt"));
    assert!(text_messages.contains(&"old answer"));
    assert_eq!(
        text_messages
            .iter()
            .filter(|text| **text == "current prompt")
            .count(),
        1,
        "current operation input is appended by Agent::prompt exactly once"
    );
    assert!(request.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Assistant { tool_calls, .. }
            if tool_calls.iter().any(|call| call.id == "turn_007:tool_012")
    )));
    assert!(request.messages.iter().any(|message| matches!(
        message,
        AgentMessage::ToolResult { tool_call_id, .. }
            if tool_call_id == "turn_007:tool_012"
    )));
}

#[tokio::test]
async fn native_cold_start_replays_post_admission_tail_once_after_active_compaction_base() {
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, UserInputSource, UserInputSubmissionKind,
        UserInputSubmittedNotification,
    };

    let user_event = |turn: &str, item: &str, text: &str| {
        BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
            "presentation-thread-1",
            turn,
            item,
            UserInputSubmissionKind::Prompt,
            UserInputSource::core_composer(),
            vec![codex::UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
        ))
    };
    let assistant_event = |turn: &str, item: &str, text: &str| {
        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::AgentMessage {
                id: item.to_string(),
                text: text.to_string(),
                phase: None,
                memory_citation: None,
            },
            "presentation-thread-1",
            turn,
        ))
    };
    let post_compaction_tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
        codex::ThreadItem::DynamicToolCall {
            id: "turn_011:tool_021".to_string(),
            tool: "fs_glob".to_string(),
            arguments: serde_json::json!({"pattern":"**/*.toml"}),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(Some(vec![
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "Cargo.toml".to_string(),
                },
            ])),
            duration_ms: None,
            namespace: None,
            success: Some(Some(true)),
        },
        "presentation-thread-1",
        "presentation-turn-tail",
    ));
    let transcript = DriverTranscript {
        current_thread_name: None,
        earliest_available: EventSequence(1),
        latest_available: EventSequence(6),
        active_compaction_source_end: Some(EventSequence(2)),
        completed_presentation_item_ids: vec![
            "turn_011:tool_021".to_string(),
            "assistant-tail".to_string(),
        ],
        records: vec![
            transcript_record(
                1,
                "operation-before-compaction",
                "presentation-turn-old",
                0,
                user_event("presentation-turn-old", "user-old", "discarded prefix"),
            ),
            transcript_record(
                2,
                "operation-before-compaction",
                "presentation-turn-old",
                1,
                assistant_event("presentation-turn-old", "assistant-old", "discarded answer"),
            ),
            transcript_record(
                3,
                "operation-tail",
                "presentation-turn-tail",
                0,
                user_event("presentation-turn-tail", "user-tail", "tail prompt"),
            ),
            transcript_record(
                4,
                "operation-tail",
                "presentation-turn-tail",
                1,
                post_compaction_tool,
            ),
            transcript_record(
                5,
                "operation-tail",
                "presentation-turn-tail",
                2,
                assistant_event("presentation-turn-tail", "assistant-tail", "tail answer"),
            ),
            transcript_record(
                6,
                "operation-after-restore",
                "presentation-turn-current",
                0,
                user_event(
                    "presentation-turn-current",
                    "user-current",
                    "current prompt",
                ),
            ),
        ],
    };
    let mut surface = fixture_surface();
    surface.context.blocks = vec![ContextBlock::CompactionSummary {
        summary: "active compacted base".to_string(),
    }];
    let bridge = Arc::new(RecordingBridge::default());
    let (driver, binding) = driver_fixture_with_surface(
        Arc::new(RecordingResolver(bridge.clone())),
        transcript,
        surface,
    )
    .await;
    let sink = Arc::new(Sink::default());
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-after-restore"),
                operation_id: id("operation-after-restore"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: Some(id("turn-after-restore")),
                presentation_turn_id: Some(id("presentation-turn-current")),
                command: thread_start(
                    "presentation-turn-current",
                    vec![RuntimeInput::text("current prompt".to_string())],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("accept turn after compacted restore");
    wait_for_turn_terminal(sink.as_ref()).await;

    let requests = bridge.requests.lock().await;
    let request = requests.first().expect("provider request");
    let text_messages = request
        .messages
        .iter()
        .filter_map(AgentMessage::first_text)
        .collect::<Vec<_>>();
    assert_eq!(text_messages.first(), Some(&"active compacted base"));
    assert_eq!(
        text_messages
            .iter()
            .filter(|text| **text == "active compacted base")
            .count(),
        1
    );
    assert!(!text_messages.contains(&"discarded prefix"));
    assert!(!text_messages.contains(&"discarded answer"));
    assert_eq!(
        text_messages
            .iter()
            .filter(|text| **text == "tail prompt")
            .count(),
        1
    );
    assert_eq!(
        text_messages
            .iter()
            .filter(|text| **text == "tail answer")
            .count(),
        1
    );
    assert_eq!(
        text_messages
            .iter()
            .filter(|text| **text == "current prompt")
            .count(),
        1
    );
    assert_eq!(
        request
            .messages
            .iter()
            .filter(|message| matches!(
                message,
                AgentMessage::Assistant { tool_calls, .. }
                    if tool_calls.iter().any(|call| call.id == "turn_011:tool_021")
            ))
            .count(),
        1
    );
    assert_eq!(
        request
            .messages
            .iter()
            .filter(|message| matches!(
                message,
                AgentMessage::ToolResult { tool_call_id, .. }
                    if tool_call_id == "turn_011:tool_021"
            ))
            .count(),
        1
    );
}

#[tokio::test]
async fn native_turn_interrupt_emits_one_interrupted_terminal_without_losing_binding() {
    let bridge = Arc::new(BlockingBridge {
        started: Arc::new(tokio::sync::Notify::new()),
    });
    let provider_started = bridge.started.notified();
    tokio::pin!(provider_started);
    let (driver, binding) = driver_fixture_with(
        Arc::new(BlockingResolver(bridge.clone())),
        DriverTranscript {
            current_thread_name: None,
            earliest_available: EventSequence(1),
            latest_available: EventSequence(0),
            active_compaction_source_end: None,
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        },
    )
    .await;
    let sink = Arc::new(Sink::default());
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-interrupt-running"),
                operation_id: id("operation-interrupt-running"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                runtime_turn_id: Some(id("turn-interrupt-running")),
                presentation_turn_id: Some(id("presentation-turn-interrupt-running")),
                command: thread_start(
                    "presentation-turn-interrupt-running",
                    vec![RuntimeInput::text("wait".to_string())],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("accept running turn");
    provider_started.await;

    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-interrupt-command"),
                operation_id: id("operation-interrupt-command"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: None,
                presentation_turn_id: None,
                command: RuntimeCommand::TurnInterrupt {
                    thread_id: id("runtime-thread-1"),
                    expected_turn_id: id("turn-interrupt-running"),
                },
            },
            sink.clone(),
        )
        .await
        .expect("interrupt active turn");
    wait_for_turn_terminal(sink.as_ref()).await;

    let events = sink.0.lock().await;
    let terminal_count = events
        .iter()
        .flat_map(|event| &event.facts)
        .filter(|fact| {
            matches!(
                fact,
                RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                    terminal: RuntimeTurnTerminal::Interrupted,
                    ..
                })
            )
        })
        .count();
    assert_eq!(terminal_count, 1);
    assert!(!events.iter().flat_map(|event| &event.facts).any(|fact| {
        matches!(
            fact,
            RuntimeJournalFact::Internal(RuntimeEvent::BindingLost { .. })
                | RuntimeJournalFact::Internal(RuntimeEvent::ConversationError { .. })
        )
    }));
}

#[tokio::test]
async fn native_terminal_observer_can_dispatch_the_next_turn_without_waiting() {
    let bridge = Arc::new(RecordingBridge::default());
    let (driver, binding) = driver_fixture_with(
        Arc::new(RecordingResolver(bridge.clone())),
        DriverTranscript {
            current_thread_name: None,
            earliest_available: EventSequence(1),
            latest_available: EventSequence(0),
            active_compaction_source_end: None,
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        },
    )
    .await;
    let next_sink = Arc::new(Sink::default());
    let (outcome_tx, outcome_rx) = tokio::sync::oneshot::channel();
    let first_sink = Arc::new(DispatchOnTerminalSink {
        driver: driver.clone(),
        next: Mutex::new(Some(DriverCommandEnvelope {
            request_id: id("request-terminal-next"),
            operation_id: id("operation-terminal-next"),
            presentation_thread_id: id("presentation-thread-1"),
            binding_id: id("binding-1"),
            generation: RuntimeDriverGeneration(4),
            source_thread_id: binding.source_thread_id.clone(),
            runtime_turn_id: Some(id("turn-terminal-next")),
            presentation_turn_id: Some(id("presentation-turn-terminal-next")),
            command: RuntimeCommand::TurnStart {
                thread_id: id("runtime-thread-1"),
                presentation_turn_id: id("presentation-turn-terminal-next"),
                input: vec![RuntimeInput::text("second".to_string())],
            },
        })),
        next_sink: next_sink.clone(),
        outcome: Mutex::new(Some(outcome_tx)),
    });
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-terminal-first"),
                operation_id: id("operation-terminal-first"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: Some(id("turn-terminal-first")),
                presentation_turn_id: Some(id("presentation-turn-terminal-first")),
                command: thread_start(
                    "presentation-turn-terminal-first",
                    vec![RuntimeInput::text("first".to_string())],
                ),
            },
            first_sink,
        )
        .await
        .expect("accept first turn");

    tokio::time::timeout(std::time::Duration::from_secs(1), outcome_rx)
        .await
        .expect("terminal callback must dispatch synchronously")
        .expect("terminal callback outcome sender")
        .expect("Native must be idle before terminal becomes observable");
    wait_for_turn_terminal(next_sink.as_ref()).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if bridge
                .requests
                .lock()
                .await
                .iter()
                .filter(|request| is_naming_request(request))
                .count()
                == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("single background naming request");
    let requests = bridge.requests.lock().await;
    assert_eq!(
        requests
            .iter()
            .filter(|request| !is_naming_request(request))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| is_naming_request(request))
            .count(),
        1
    );
    let naming_request = requests
        .iter()
        .find(|request| is_naming_request(request))
        .expect("naming request");
    assert!(naming_request.tools.is_empty());
    assert_eq!(
        naming_request
            .messages
            .iter()
            .filter_map(AgentMessage::first_text)
            .collect::<Vec<_>>(),
        vec!["first", "restored response"],
        "naming uses only this turn's canonical user input and final assistant text"
    );
}

#[tokio::test]
async fn native_driver_applies_surface_and_emits_complete_turn_trace() {
    let (driver, binding) = driver_fixture().await;
    assert_eq!(binding.applied_tool_set_revision, ToolSetRevision(3));
    assert_eq!(
        binding.applied_hook_plan_revision,
        Some(HookPlanRevision(2))
    );
    assert!(binding.applied_hooks[0].acknowledged);

    let sink = Arc::new(Sink::default());
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-turn-1"),
                operation_id: id("operation-turn-1"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                runtime_turn_id: Some(id("turn-request-turn-1")),
                presentation_turn_id: Some(id("native-turn-request-turn-1")),
                command: thread_start(
                    "native-turn-request-turn-1",
                    vec![RuntimeInput::text("hello".to_string())],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("native turn");

    wait_for_turn_terminal(sink.as_ref()).await;
    wait_for_thread_name(sink.as_ref()).await;
    let events = sink.0.lock().await;
    let terminal_event_index = events
        .iter()
        .position(|event| {
            event.facts.iter().any(|fact| {
                matches!(
                    fact,
                    RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
                )
            })
        })
        .expect("turn terminal envelope");
    let thread_name_event_index = events
        .iter()
        .position(|event| {
            event.facts.iter().any(|fact| {
                matches!(
                    fact,
                    RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                        event: agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(_),
                        ..
                    })
                )
            })
        })
        .expect("thread name envelope");
    assert!(
        terminal_event_index < thread_name_event_index,
        "the main turn terminal must become observable before background naming"
    );
    assert!(events.iter().all(|event| {
        if event.facts.iter().any(|fact| {
            matches!(
                fact,
                RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                    event: agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(_),
                    ..
                })
            )
        }) {
            event.operation_id.is_none()
                && event.source_turn_id.is_none()
                && event.source_item_id.is_none()
                && event.source_request_id.is_none()
                && event.source_entry_index.is_none()
        } else {
            event
                .operation_id
                .as_ref()
                .is_some_and(|operation_id| operation_id.as_str() == "operation-turn-1")
        }
    }));
    let internal = events
        .iter()
        .flat_map(|event| event.facts.iter())
        .filter_map(|fact| match fact {
            RuntimeJournalFact::Internal(event) => Some(event),
            RuntimeJournalFact::Presentation(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(
        internal
            .iter()
            .any(|event| matches!(event, RuntimeEvent::TurnStarted { .. }))
    );
    assert!(
        internal
            .iter()
            .all(|event| !matches!(event, RuntimeEvent::ConversationDelta { .. }))
    );
    assert!(internal.iter().any(|event| matches!(
        event,
        RuntimeEvent::TurnTerminal {
            terminal: RuntimeTurnTerminal::Completed,
            ..
        }
    )));
    assert!(
        events
            .iter()
            .flat_map(|event| event.facts.iter())
            .any(|fact| { matches!(fact, RuntimeJournalFact::Presentation(_)) })
    );
    assert!(
        events
            .iter()
            .flat_map(|event| event.facts.iter())
            .any(|fact| {
                matches!(
                    fact,
                    RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                        durability: PresentationDurability::Ephemeral,
                        event: agentdash_agent_protocol::BackboneEvent::AgentMessageDelta(_),
                    })
                )
            })
    );
    let presentation_thread_ids = events
        .iter()
        .flat_map(|event| event.facts.iter())
        .filter_map(|fact| match fact {
            RuntimeJournalFact::Presentation(presentation) => {
                if matches!(
                    presentation.event,
                    agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(_)
                ) {
                    return None;
                }
                let value = serde_json::to_value(&presentation.event)
                    .expect("serialize native protected presentation body");
                value
                    .pointer("/payload/threadId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            }
            RuntimeJournalFact::Internal(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(!presentation_thread_ids.is_empty());
    assert!(
        presentation_thread_ids
            .iter()
            .all(|thread_id| thread_id == "presentation-thread-1")
    );
    assert!(events.iter().all(|event| {
        let first_presentation = event
            .facts
            .iter()
            .position(|fact| matches!(fact, RuntimeJournalFact::Presentation(_)));
        first_presentation.is_none_or(|index| {
            event.facts[..index]
                .iter()
                .all(|fact| matches!(fact, RuntimeJournalFact::Internal(_)))
        })
    }));
    let mut validator = RuntimeTraceValidator::default();
    for (index, event) in internal.into_iter().enumerate() {
        validator
            .observe(&RuntimeEventEnvelope {
                thread_id: id("runtime-thread-1"),
                occurred_at_ms: 0,
                sequence: Some(EventSequence(index as u64 + 1)),
                transient: None,
                revision: RuntimeRevision(index as u64 + 1),
                event: event.clone(),
            })
            .expect("native event trace remains conformant");
    }
    validator.finish().expect("native trace reaches terminals");
    drop(events);

    let projection = driver
        .inspect(DriverInspectionQuery::ThreadProjection {
            source_thread_id: binding.source_thread_id,
        })
        .await
        .expect("thread projection");
    assert!(matches!(
        projection,
        DriverInspection::ThreadProjection {
            fidelity: ContextFidelity::EventProjected,
            ..
        }
    ));
}

#[tokio::test]
async fn native_cold_bind_with_current_thread_name_does_not_generate_another_name() {
    let bridge = Arc::new(RecordingBridge::default());
    let (driver, binding) = driver_fixture_with(
        Arc::new(RecordingResolver(bridge.clone())),
        DriverTranscript {
            current_thread_name: Some("已有会话名".to_string()),
            earliest_available: EventSequence(1),
            latest_available: EventSequence(0),
            active_compaction_source_end: None,
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        },
    )
    .await;
    let sink = Arc::new(Sink::default());

    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-existing-name"),
                operation_id: id("operation-existing-name"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: Some(id("turn-existing-name")),
                presentation_turn_id: Some(id("presentation-turn-existing-name")),
                command: thread_start(
                    "presentation-turn-existing-name",
                    vec![RuntimeInput::text("hello".to_string())],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("native turn with existing name");

    wait_for_turn_terminal(sink.as_ref()).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert!(
        sink.0
            .lock()
            .await
            .iter()
            .all(|event| event.facts.iter().all(|fact| !matches!(
                fact,
                RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                    event: agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(_),
                    ..
                })
            )))
    );
    assert_eq!(
        bridge.requests.lock().await.len(),
        1,
        "only the main provider request is allowed"
    );
}

#[tokio::test]
async fn native_compaction_activation_is_exact_and_idempotently_inspectable() {
    let (driver, binding) = driver_fixture().await;
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-compact-1"),
                operation_id: id("operation-compact-1"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: None,
                presentation_turn_id: None,
                command: RuntimeCommand::ContextCompact {
                    thread_id: id("thread-1"),
                    compaction_id: id("compaction-1"),
                    trigger: ContextCompactionTrigger::Manual,
                    base_checkpoint_id: None,
                    expected_context_revision: ContextRevision(0),
                },
            },
            Arc::new(Sink::default()),
        )
        .await
        .expect("activate managed compaction");

    assert_eq!(
        driver
            .inspect(DriverInspectionQuery::CompactionActivation {
                candidate_id: id("candidate-1"),
            })
            .await
            .expect("activation inspection"),
        DriverInspection::CompactionActivation {
            applied: true,
            digest: Some("sha256:context-1".to_string()),
            driver_context_revision: Some(id("native-context-revision-1")),
        }
    );
    assert_eq!(
        driver
            .inspect(DriverInspectionQuery::Checkpoint {
                checkpoint_id: id("checkpoint-1"),
            })
            .await
            .expect("checkpoint inspection"),
        DriverInspection::Checkpoint {
            available: true,
            digest: Some("sha256:context-1".to_string()),
        }
    );
}

#[tokio::test]
async fn native_fork_imports_the_requested_checkpoint_and_preserves_its_digest() {
    let (driver, binding) = driver_fixture().await;
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-fork-1"),
                operation_id: id("operation-fork-1"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                runtime_turn_id: None,
                presentation_turn_id: None,
                command: RuntimeCommand::ThreadFork {
                    thread_id: id("thread-1"),
                    checkpoint_id: Some(id("checkpoint-1")),
                },
            },
            Arc::new(Sink::default()),
        )
        .await
        .expect("fork from exact checkpoint");

    assert_eq!(
        driver
            .inspect(DriverInspectionQuery::Checkpoint {
                checkpoint_id: id("checkpoint-1"),
            })
            .await
            .expect("fork checkpoint inspection"),
        DriverInspection::Checkpoint {
            available: true,
            digest: Some("sha256:context-1".to_string()),
        }
    );
    assert_eq!(
        driver
            .inspect(DriverInspectionQuery::ContextRead {
                source_thread_id: binding.source_thread_id.clone(),
            })
            .await
            .expect("fork context inspection"),
        DriverInspection::ContextRead {
            source_thread_id: binding.source_thread_id,
            fidelity: ContextFidelity::PlatformExact,
            digest: Some("sha256:context-1".to_string()),
        }
    );
}

#[tokio::test]
async fn native_resume_reuses_the_source_thread_and_materialized_context_digest() {
    let (driver, _) = driver_fixture().await;
    let source_thread_id: DriverThreadId = id("native-existing-thread");
    let binding = driver
        .bind(DriverBindRequest {
            binding_id: id("binding-resume"),
            service_instance_id: id("native-instance"),
            surface_revision: SurfaceRevision(7),
            surface_digest: id("sha256:native-surface"),
            intent: DriverBindIntent::Resume {
                source_thread_id: source_thread_id.clone(),
            },
        })
        .await
        .expect("bind resumed native thread");
    assert_eq!(binding.source_thread_id, source_thread_id);

    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-resume-1"),
                operation_id: id("operation-resume-1"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-resume"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: source_thread_id.clone(),
                runtime_turn_id: None,
                presentation_turn_id: None,
                command: RuntimeCommand::ThreadResume {
                    thread_id: id("thread-resume"),
                },
            },
            Arc::new(Sink::default()),
        )
        .await
        .expect("resume materialized native thread");

    assert_eq!(
        driver
            .inspect(DriverInspectionQuery::ContextRead {
                source_thread_id: source_thread_id.clone(),
            })
            .await
            .expect("resumed context inspection"),
        DriverInspection::ContextRead {
            source_thread_id,
            fidelity: ContextFidelity::PlatformExact,
            digest: Some("sha256:context-0".to_string()),
        }
    );
}

#[tokio::test]
async fn native_hot_tool_replace_returns_and_replays_exact_apply_receipt() {
    let (driver, binding) = driver_fixture().await;
    let command = DriverCommandEnvelope {
        request_id: id("request-tools-4"),
        operation_id: id("operation-tools-4"),
        presentation_thread_id: id("presentation-thread-1"),
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(4),
        source_thread_id: binding.source_thread_id,
        runtime_turn_id: None,
        presentation_turn_id: None,
        command: RuntimeCommand::ToolSetReplace {
            thread_id: id("thread-1"),
            expected_current_tool_set_revision: ToolSetRevision(3),
            target_tool_set_revision: ToolSetRevision(4),
            tool_set_digest: "sha256:tools-4".to_string(),
        },
    };
    let first = driver
        .dispatch(command.clone(), Arc::new(Sink::default()))
        .await
        .expect("hot replace tools");
    assert_eq!(
        first.applied_tool_set,
        Some(DriverToolSetApplyReceipt {
            revision: ToolSetRevision(4),
            digest: "sha256:tools-4".to_string(),
        })
    );
    assert!(!first.duplicate);

    let duplicate = driver
        .dispatch(command, Arc::new(Sink::default()))
        .await
        .expect("replay hot replace receipt");
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.applied_tool_set, first.applied_tool_set);
}

#[tokio::test]
async fn failed_event_delivery_clears_the_active_turn_fence() {
    let (driver, binding) = driver_fixture().await;
    let failed_request_id: DriverRequestId = id("request-turn-fail");
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: failed_request_id,
                operation_id: id("operation-turn-fail"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                runtime_turn_id: Some(id("turn-request-turn-fail")),
                presentation_turn_id: Some(id("native-turn-request-turn-fail")),
                command: thread_start(
                    "native-turn-request-turn-fail",
                    vec![RuntimeInput::text("hello".to_string())],
                ),
            },
            Arc::new(FailingSink),
        )
        .await
        .expect_err("authoritative event delivery failure must fail dispatch");

    let error = driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-interrupt-stale"),
                operation_id: id("operation-interrupt-stale"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: None,
                presentation_turn_id: None,
                command: RuntimeCommand::TurnInterrupt {
                    thread_id: id("thread-1"),
                    expected_turn_id: id("native-turn-request-turn-fail"),
                },
            },
            Arc::new(Sink::default()),
        )
        .await
        .expect_err("failed turn must not remain active");
    assert!(matches!(error, DriverError::Rejected { .. }));
}

#[tokio::test]
async fn failed_event_delivery_aborts_agent_core_before_the_next_turn() {
    let (driver, binding) = driver_fixture().await;
    let accepted_then_failed = DriverCommandEnvelope {
        request_id: id("request-turn-stream-fail"),
        operation_id: id("operation-turn-stream-fail"),
        presentation_thread_id: id("presentation-thread-1"),
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(4),
        source_thread_id: binding.source_thread_id.clone(),
        runtime_turn_id: Some(id("turn-request-turn-stream-fail")),
        presentation_turn_id: Some(id("native-turn-request-turn-stream-fail")),
        command: thread_start(
            "native-turn-request-turn-stream-fail",
            vec![RuntimeInput::text("first".to_string())],
        ),
    };
    let accepted = driver
        .dispatch(
            accepted_then_failed.clone(),
            Arc::new(FailAfterFirstEventSink::default()),
        )
        .await
        .expect("prompt delivery is accepted before its event pump runs");
    assert!(!accepted.duplicate);

    let replay = driver
        .dispatch(accepted_then_failed, Arc::new(Sink::default()))
        .await
        .expect("an accepted prompt must replay its receipt after a later delivery failure");
    assert!(replay.duplicate);

    let after_failure = DriverCommandEnvelope {
        request_id: id("request-turn-after-stream-fail"),
        operation_id: id("operation-turn-after-stream-fail"),
        presentation_thread_id: id("presentation-thread-1"),
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(4),
        source_thread_id: binding.source_thread_id,
        runtime_turn_id: Some(id("turn-request-turn-after-stream-fail")),
        presentation_turn_id: Some(id("native-turn-request-turn-after-stream-fail")),
        command: RuntimeCommand::TurnStart {
            thread_id: id("runtime-thread-1"),
            presentation_turn_id: id("native-turn-request-turn-after-stream-fail"),
            input: vec![RuntimeInput::text("second".to_string())],
        },
    };
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            match driver
                .dispatch(after_failure.clone(), Arc::new(Sink::default()))
                .await
            {
                Ok(_) => break,
                Err(DriverError::Rejected { .. }) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected next-turn dispatch error: {error}"),
            }
        }
    })
    .await
    .expect("Agent Core must become idle before the next turn");
}

#[tokio::test]
async fn managed_runtime_terminalization_stops_event_pump_without_binding_lost_fallback() {
    let (driver, binding) = driver_fixture().await;
    let sink = Arc::new(TerminalizingSink::default());
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-runtime-terminalized"),
                operation_id: id("operation-runtime-terminalized"),
                presentation_thread_id: id("presentation-thread-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                runtime_turn_id: Some(id("turn-runtime-terminalized")),
                presentation_turn_id: Some(id("presentation-turn-runtime-terminalized")),
                command: thread_start(
                    "presentation-turn-runtime-terminalized",
                    vec![RuntimeInput::text("stop after canonical terminal")],
                ),
            },
            sink.clone(),
        )
        .await
        .expect("prompt delivery is accepted before Managed Runtime terminalizes the stream");

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.attempts.load(std::sync::atomic::Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("event pump must observe terminalized admission");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(sink.events.lock().await.iter().all(|event| {
        event.facts.iter().all(|fact| {
            !matches!(
                fact,
                RuntimeJournalFact::Internal(RuntimeEvent::BindingLost { .. })
            )
        })
    }));
}

#[tokio::test]
async fn native_descriptor_does_not_claim_prompt_flattened_input_modalities() {
    let (driver, binding) = driver_fixture().await;
    let descriptor = driver
        .describe(DriverDescribeRequest {
            service_instance_id: id("native-instance"),
        })
        .await
        .expect("describe native driver");
    assert_eq!(
        descriptor.profile.input.modalities,
        [InputModality::Text, InputModality::Image].into()
    );

    let unsupported = [
        ("blank-text", RuntimeInput::text(" \r\n\t ")),
        (
            "local-image",
            RuntimeInput::user_input(agentdash_agent_protocol::UserInputBlock::LocalImage {
                detail: Some(None),
                path: "C:/workspace/image.png".to_string(),
            }),
        ),
        (
            "skill",
            RuntimeInput::user_input(agentdash_agent_protocol::UserInputBlock::Skill {
                name: "review".to_string(),
                path: "C:/skills/review/SKILL.md".to_string(),
            }),
        ),
        (
            "mention",
            RuntimeInput::user_input(agentdash_agent_protocol::UserInputBlock::Mention {
                name: "main.rs".to_string(),
                path: "C:/workspace/src/main.rs".to_string(),
            }),
        ),
        (
            "structured",
            RuntimeInput::Structured {
                schema: "example".to_string(),
                value: json!({"value": 1}),
            },
        ),
    ];
    for (kind, input) in unsupported {
        let sink = Arc::new(Sink::default());
        let error = driver
            .dispatch(
                DriverCommandEnvelope {
                    request_id: id(&format!("request-{kind}-input")),
                    operation_id: id(&format!("operation-{kind}-input")),
                    presentation_thread_id: id("presentation-thread-1"),
                    binding_id: id("binding-1"),
                    generation: RuntimeDriverGeneration(4),
                    source_thread_id: binding.source_thread_id.clone(),
                    runtime_turn_id: Some(id(&format!("turn-request-{kind}-input"))),
                    presentation_turn_id: Some(id(&format!("native-turn-request-{kind}-input"))),
                    command: thread_start(
                        &format!("native-turn-request-{kind}-input"),
                        vec![input],
                    ),
                },
                sink.clone(),
            )
            .await
            .expect_err("unsupported native input must be rejected before dispatch side effects");
        if kind == "blank-text" {
            assert!(matches!(error, DriverError::Rejected { .. }), "{kind}");
        } else {
            assert!(matches!(error, DriverError::Unsupported { .. }), "{kind}");
        }
        assert!(sink.0.lock().await.is_empty(), "{kind}");
    }
}
