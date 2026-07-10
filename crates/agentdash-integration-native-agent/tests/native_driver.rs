use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use agentdash_agent::{BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, TokenUsage};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_test_support::RuntimeTraceValidator;
use agentdash_agent_types::{AgentMessage, ContentPart};
use agentdash_integration_api::*;
use agentdash_integration_native_agent::{
    NativeBridgeResolveError, NativeBridgeResolver, native_agent_contribution,
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

struct Resolver;

#[async_trait]
impl NativeBridgeResolver for Resolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn LlmBridge>, NativeBridgeResolveError> {
        Ok(Arc::new(EchoBridge))
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
}

#[async_trait]
impl AgentRuntimeContextBroker for Contexts {
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
        revision: SurfaceRevision(7),
        digest: id("sha256:native-surface"),
        context: DriverContextSurface {
            recipe: recipe(ToolSetRevision(3)),
            instructions: vec![DriverInstructionSet {
                channel: InstructionChannel::System,
                entries: vec!["system".to_string()],
            }],
            blocks: vec![ContextBlock::Input {
                input: vec![RuntimeInput::Text {
                    text: "restored".to_string(),
                }],
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
            }],
        },
        workspace: DriverWorkspaceSurface {
            capabilities: Vec::new(),
            roots: Vec::new(),
        },
    }
}

async fn driver_fixture() -> (Arc<dyn AgentRuntimeDriver>, DriverBinding) {
    let contribution = native_agent_contribution(Arc::new(Resolver));
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
                config: json!({"provider": "test", "model": "echo"}),
                credentials: BTreeMap::new(),
                placement: AgentRuntimePlacement::InProcess,
            },
            RuntimeDriverHostPorts {
                credentials: Arc::new(NoCredentials),
                surfaces: Arc::new(Surfaces {
                    initial: fixture_surface(),
                    replacement: DriverToolSurface {
                        revision: ToolSetRevision(4),
                        digest: "sha256:tools-4".to_string(),
                        tools: Vec::new(),
                    },
                }),
                context: Arc::new(Contexts { activation }),
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
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                command: RuntimeCommand::ThreadStart {
                    input: vec![RuntimeInput::Text {
                        text: "hello".to_string(),
                    }],
                    surface_digest: id("sha256:native-surface"),
                },
            },
            sink.clone(),
        )
        .await
        .expect("native turn");

    let events = sink.0.lock().await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event, RuntimeEvent::TurnStarted { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event, RuntimeEvent::ItemDelta { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        event.event,
        RuntimeEvent::TurnTerminal {
            terminal: RuntimeTurnTerminal::Completed,
            ..
        }
    )));
    let mut validator = RuntimeTraceValidator::default();
    for (index, event) in events.iter().enumerate() {
        validator
            .observe(&RuntimeEventEnvelope {
                thread_id: id("runtime-thread-1"),
                sequence: Some(EventSequence(index as u64 + 1)),
                revision: RuntimeRevision(index as u64 + 1),
                event: event.event.clone(),
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
async fn native_compaction_activation_is_exact_and_idempotently_inspectable() {
    let (driver, binding) = driver_fixture().await;
    driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-compact-1"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
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
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
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
                binding_id: id("binding-resume"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: source_thread_id.clone(),
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
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(4),
        source_thread_id: binding.source_thread_id,
        command: RuntimeCommand::ToolSetReplace {
            thread_id: id("thread-1"),
            expected_tool_set_revision: ToolSetRevision(4),
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
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id.clone(),
                command: RuntimeCommand::ThreadStart {
                    input: vec![RuntimeInput::Text {
                        text: "hello".to_string(),
                    }],
                    surface_digest: id("sha256:native-surface"),
                },
            },
            Arc::new(FailingSink),
        )
        .await
        .expect_err("authoritative event delivery failure must fail dispatch");

    let error = driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-interrupt-stale"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
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

    let sink = Arc::new(Sink::default());
    let error = driver
        .dispatch(
            DriverCommandEnvelope {
                request_id: id("request-structured-input"),
                binding_id: id("binding-1"),
                generation: RuntimeDriverGeneration(4),
                source_thread_id: binding.source_thread_id,
                command: RuntimeCommand::ThreadStart {
                    input: vec![RuntimeInput::Structured {
                        schema: "example".to_string(),
                        value: json!({"value": 1}),
                    }],
                    surface_digest: id("sha256:native-surface"),
                },
            },
            sink.clone(),
        )
        .await
        .expect_err("prompt flattening is not a native structured-input guarantee");
    assert!(matches!(error, DriverError::Unsupported { .. }));
    assert!(sink.0.lock().await.is_empty());
}
