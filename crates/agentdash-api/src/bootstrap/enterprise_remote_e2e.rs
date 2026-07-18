use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_core::{
    AgentMessage, BridgeRequest, BridgeResponse, ContentPart, LlmBridge, StopReason, StreamChunk,
    TokenUsage, ToolCallInfo,
};
use agentdash_agent_runtime::{RuntimeRepository, RuntimeTransientEvents, RuntimeWorkKind};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::*;
use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart as ToolContentPart, DynAgentTool,
    ToolUpdateCallback,
};
use agentdash_application_agentrun::agent_run::{
    AgentFrameHookRuntime, AgentRunCommandGuard, AgentRunRuntime, EnqueueRuntimeMailboxMessage,
    GuardedAgentRunCommand, ManagedAgentRunRuntime, RuntimeAgentRunMailbox,
    RuntimeMailboxSubmitOutcome,
};
use agentdash_application_ports::agent_run_runtime::*;
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionDecision, AgentRunAdmissionRequest, AgentRunEffectiveCapabilityError,
    AgentRunEffectiveCapabilityPort, AgentRunEffectiveCapabilityRequest,
    AgentRunEffectiveCapabilityView,
};
use agentdash_domain::agent_run_mailbox::MailboxSourceIdentity;
use agentdash_infrastructure::{
    PostgresAgentRuntimeHostRepository, postgres_runtime::PostgresRuntime,
};
use agentdash_integration_api::*;
use agentdash_integration_native_agent::{
    NativeAgentRuntimeIntegration, NativeBridgeResolveError, NativeBridgeResolver,
    NativePresentationMetadata, ResolvedNativeBridge, native_runtime_profile,
};
use agentdash_integration_remote_runtime::{
    RuntimeWireHostPortRouter, remote_runtime_contribution,
};
use agentdash_local::{HostRuntimeDriverEndpointResolver, RuntimeWireCommandHandler};
use agentdash_relay::{CapabilitiesPayload, RelayMessage, RuntimeRelayTransportDescriptor};
use agentdash_spi::{AgentFrameHookSnapshot, NoopExecutionHookProvider};
use agentdash_test_support::workflow::MemoryAgentRunMailboxRepository;
use async_trait::async_trait;
use chrono::Utc;
use futures::{Stream, stream};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_runtime::{
    AgentRunRuntimeSurfaceSource, AgentRunRuntimeSurfaceSourceError, AgentRuntimeCallbacks,
    AgentRuntimeCompositionInput, AppliedNativeAgentRunSurface, PreparedAgentRunRuntime,
    build_agent_runtime_composition,
};
use super::agent_runtime_surface::{
    CompiledAgentRunToolBinding, CompiledAgentRunToolBindingRecovery, CompiledAgentRunToolRegistry,
    PendingCompiledAgentRunToolBinding, PostgresAgentRunToolBrokerResolver,
};
use crate::relay::registry::{BackendRegistry, ConnectedBackend};
use crate::relay::{CloudRemoteRuntimeInventory, CloudRuntimeWirePlacementResolver};

const BACKEND_ID: &str = "enterprise-desktop";
const TRANSPORT_ID: &str = "enterprise-runtime-wire";

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid fixture id")
}

struct EnterpriseBridge {
    calls: AtomicUsize,
    requests: tokio::sync::Mutex<Vec<Vec<AgentMessage>>>,
    block_next: std::sync::atomic::AtomicBool,
    blocked: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
}

#[async_trait]
impl LlmBridge for EnterpriseBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>> {
        self.requests.lock().await.push(request.messages);
        if self.block_next.swap(false, Ordering::SeqCst) {
            self.blocked.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("Enterprise bridge release remains open")
                .forget();
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let message = if call.is_multiple_of(2) {
            AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: ["first", "second"]
                    .into_iter()
                    .map(|ordinal| ToolCallInfo {
                        id: format!("enterprise-tool-{call}-{ordinal}"),
                        call_id: None,
                        name: "enterprise_echo".to_string(),
                        arguments: json!({
                            "value": "through-runtime-wire",
                            "ordinal": ordinal,
                        }),
                    })
                    .collect(),
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        } else {
            AgentMessage::Assistant {
                content: vec![
                    ContentPart::reasoning(
                        "enterprise remote reasoning",
                        Some(format!("enterprise-reasoning-{call}")),
                        None,
                    ),
                    ContentPart::text("enterprise remote completed"),
                ],
                tool_calls: Vec::new(),
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        };
        let mut chunks = Vec::new();
        if !call.is_multiple_of(2) {
            chunks.push(StreamChunk::ReasoningDelta {
                id: Some(format!("enterprise-reasoning-{call}")),
                text: "enterprise remote reasoning".to_string(),
                signature: None,
            });
            chunks.push(StreamChunk::TextDelta(
                "enterprise remote completed".to_string(),
            ));
        }
        chunks.push(StreamChunk::Done(BridgeResponse {
            raw_content: match &message {
                AgentMessage::Assistant { content, .. } => content.clone(),
                _ => Vec::new(),
            },
            message,
            usage: TokenUsage::default(),
        }));
        Box::pin(stream::iter(chunks))
    }
}

struct EnterpriseBridgeResolver(Arc<EnterpriseBridge>);

#[async_trait]
impl NativeBridgeResolver for EnterpriseBridgeResolver {
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
            reason: "fixture has no credentials".to_string(),
        })
    }
}

struct EnterpriseEchoTool(Arc<AtomicUsize>);

#[async_trait]
impl AgentTool for EnterpriseEchoTool {
    fn name(&self) -> &str {
        "enterprise_echo"
    }

    fn description(&self) -> &str {
        "Enterprise reverse RuntimeWire tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type":"object"})
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::Dynamic {
            namespace: Some("enterprise_test".to_string()),
        })
    }
    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_enterprise_remote_e2e_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let is_error = args.get("ordinal").and_then(serde_json::Value::as_str) == Some("second");
        Ok(AgentToolResult {
            content: vec![ToolContentPart::text(if is_error {
                "enterprise echo business error"
            } else {
                "enterprise echo completed"
            })],
            is_error,
            details: Some(json!({"echoed": args})),
        })
    }
}

struct RecordingToolCallback {
    inner: Arc<dyn AgentRuntimeToolCallback>,
    calls: Arc<AtomicUsize>,
    last_error: Arc<tokio::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentRuntimeToolCallback for RecordingToolCallback {
    async fn invoke(
        &self,
        request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = self.inner.invoke(request).await;
        if let Err(error) = &outcome {
            *self.last_error.lock().await = Some(error.to_string());
        }
        outcome
    }
}

struct EnterpriseCapabilityPort;

#[async_trait]
impl AgentRunEffectiveCapabilityPort for EnterpriseCapabilityPort {
    async fn effective_capability(
        &self,
        _request: AgentRunEffectiveCapabilityRequest,
    ) -> Result<AgentRunEffectiveCapabilityView, AgentRunEffectiveCapabilityError> {
        Err(AgentRunEffectiveCapabilityError::Projection {
            message: "Enterprise E2E only exercises strict tool admission".to_string(),
        })
    }

    async fn admit_tool(
        &self,
        request: AgentRunAdmissionRequest,
    ) -> Result<AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError> {
        assert_eq!(request.capability_key, "enterprise");
        assert_eq!(request.tool_name, "enterprise_echo");
        Ok(AgentRunAdmissionDecision::allow())
    }
}

#[derive(Default)]
struct RecordingHookCallback(AtomicUsize);

struct EnterpriseManagedCompactionEngine;

#[async_trait]
impl agentdash_infrastructure::agent_runtime_workers::ManagedCompactionPreparationEngine
    for EnterpriseManagedCompactionEngine
{
    async fn compact(
        &self,
        thread: &agentdash_agent_runtime::RuntimeThreadState,
        surface: &agentdash_integration_api::MaterializedDriverSurface,
        _instance: &agentdash_agent_runtime_host::AgentServiceInstance,
        _input: &agentdash_infrastructure::agent_runtime_workers::ManagedCompactionInput,
        work: &agentdash_agent_runtime::ContextPreparationWorkItem,
    ) -> Result<
        agentdash_infrastructure::agent_runtime_workers::ManagedCompactionOutput,
        agentdash_infrastructure::agent_runtime_workers::RuntimeDurableWorkerError,
    > {
        let summary = "enterprise remote compacted summary".to_string();
        let mut blocks = surface.context.blocks.clone();
        blocks.push(
            agentdash_agent_runtime_contract::ContextBlock::CompactionSummary {
                summary: summary.clone(),
            },
        );
        Ok(
            agentdash_infrastructure::agent_runtime_workers::ManagedCompactionOutput {
                blocks,
                source_item_ids: thread.item_order.clone(),
                presentation: agentdash_agent_runtime::CompactionPresentationFacts {
                    summary,
                    tokens_before: 42,
                    messages_compacted: u32::try_from(thread.item_order.len()).unwrap_or(u32::MAX),
                    compaction_id: Some(work.compaction_id.to_string()),
                    projection_version: None,
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("manual".to_string()),
                    phase: Some("standalone_compact_turn".to_string()),
                    source_start_event_seq: None,
                    source_end_event_seq: None,
                    first_kept_event_seq: None,
                    compacted_until_ref: None,
                    timestamp_ms: Some(1_710_000_000_000),
                },
            },
        )
    }
}

#[async_trait]
impl AgentRuntimeHookCallback for RecordingHookCallback {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        assert_eq!(request.point, HookPoint::BeforeProviderRequest);
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(DriverHookDecision::Continue {
            payload: request.payload,
        })
    }
}

fn enterprise_contribution() -> (
    AgentRuntimeDriverContribution,
    AgentRuntimeTrustManifest,
    Arc<EnterpriseBridge>,
) {
    let bridge = Arc::new(EnterpriseBridge {
        calls: AtomicUsize::new(0),
        requests: tokio::sync::Mutex::new(Vec::new()),
        block_next: std::sync::atomic::AtomicBool::new(false),
        blocked: tokio::sync::Semaphore::new(0),
        release: tokio::sync::Semaphore::new(0),
    });
    let integration =
        NativeAgentRuntimeIntegration::new(Arc::new(EnterpriseBridgeResolver(bridge.clone())));
    let mut contribution = integration
        .agent_runtime_drivers()
        .pop()
        .expect("Native production driver contribution");
    contribution.definition.provenance = AgentServiceProvenance {
        definition_id: AgentServiceDefinitionId::new("enterprise.reference-agent")
            .expect("definition id"),
        publisher_integration: "enterprise.internal".to_string(),
        service_version: "1.0.0".to_string(),
        build_digest: AgentServiceBuildDigest::new("sha256:enterprise-reference-build")
            .expect("build digest"),
    };
    let manifest = AgentRuntimeTrustManifest {
        provenance: contribution.definition.provenance.clone(),
        suite_revision: "enterprise-runtime-conformance-v1".to_string(),
        driver_build_digest: contribution.definition.provenance.build_digest.to_string(),
        protocol_revision: 1,
        verified_profile: contribution.definition.service_profile_upper_bound.clone(),
    };
    (contribution, manifest, bridge)
}

fn trusted_manifest(manifest: &AgentRuntimeTrustManifest) -> TrustedDriverManifest {
    TrustedDriverManifest {
        provenance: manifest.provenance.clone(),
        suite_revision: manifest.suite_revision.clone(),
        driver_build_digest: manifest.driver_build_digest.clone(),
        protocol_revision: manifest.protocol_revision,
        verified_profile_digest: profile_digest(&manifest.verified_profile)
            .expect("verified profile digest"),
    }
}

fn remote_proxy_definition(source: &AgentServiceDefinition) -> AgentServiceDefinition {
    let schema = json!({
        "type":"object",
        "properties": {
            "sourceServiceInstanceId":{"type":"string","minLength":1},
            "sourceDriverGeneration":{"type":"integer","minimum":1},
            "sourceHostIncarnationId":{"type":"string","minLength":1}
        },
        "required":["sourceServiceInstanceId","sourceDriverGeneration","sourceHostIncarnationId"],
        "additionalProperties":false
    });
    let mut definition = source.clone();
    definition.factory_key =
        AgentRuntimeFactoryKey::new("enterprise.remote-runtime-proxy").expect("proxy factory key");
    definition.config_schema_digest =
        AgentServiceSchemaDigest::new(agent_service_schema_digest(&schema)).expect("schema digest");
    definition.config_schema = schema;
    definition.credential_slots.clear();
    definition
}

fn materialized_surface(thread_id: RuntimeThreadId) -> MaterializedDriverSurface {
    MaterializedDriverSurface {
        runtime_thread_id: thread_id,
        revision: SurfaceRevision(1),
        digest: id("sha256:enterprise-surface"),
        authorization_identity: None,
        context: DriverContextSurface {
            recipe: ContextRecipe {
                revision: ContextRecipeRevision(1),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(1),
                    tool_set_revision: ToolSetRevision(1),
                },
                source_item_ids: Vec::new(),
            },
            instructions: vec![DriverInstructionSet {
                channel: InstructionChannel::System,
                entries: vec!["Enterprise remote Agent".to_string()],
            }],
            blocks: Vec::new(),
            digest: id("sha256:enterprise-context"),
            fidelity: ContextFidelity::PlatformExact,
        },
        tools: DriverToolSurface {
            revision: ToolSetRevision(1),
            digest: "sha256:enterprise-tools".to_string(),
            tools: vec![DriverToolDefinition {
                name: "enterprise_echo".to_string(),
                description: "Enterprise reverse RuntimeWire tool".to_string(),
                parameters_schema: json!({"type":"object"}),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection:
                    agentdash_agent_runtime_contract::ToolProtocolProjection::Dynamic {
                        namespace: Some("enterprise_test".to_string()),
                    },
                presentation_emitter:
                    agentdash_agent_runtime_contract::ToolPresentationEmitter::VendorStream,
                parity_fixture_id: "main_tool_enterprise_remote_e2e_dynamic_lifecycle".to_string(),
            }],
        },
        hooks: DriverHookSurface {
            revision: HookPlanRevision(1),
            digest: id("sha256:enterprise-hooks"),
            artifact_digest: Some("sha256:enterprise-hook-artifact".to_string()),
            configuration_boundary: ConfigurationBoundary::Binding,
            bindings: vec![DriverHookBinding {
                definition_id: id("enterprise-before-provider"),
                point: HookPoint::BeforeProviderRequest,
                actions: vec![HookAction::Observe],
                strength: SemanticStrength::ExactSynchronous,
                failure_policy: HookFailurePolicy::FailClosed,
                required: true,
                site: HookExecutionSite::AgentCoreCallback,
            }],
        },
        workspace: DriverWorkspaceSurface {
            digest: "workspace-enterprise-remote".to_string(),
            capabilities: Vec::new(),
            roots: vec!["workspace://enterprise".to_string()],
        },
    }
}

fn compiled_business_surface(
    surface: &MaterializedDriverSurface,
) -> agentdash_agent_runtime::CompiledBusinessAgentSurface {
    let source = agentdash_agent_runtime::SurfaceSourceRef {
        layer: "enterprise_fixture".to_string(),
        key: "enterprise-remote-frame".to_string(),
    };
    let driver_tool = &surface.tools.tools[0];
    let tool = agentdash_agent_runtime::ToolContribution {
        meta: agentdash_agent_runtime::ContributionMeta {
            key: "tool:enterprise_echo".to_string(),
            source: source.clone(),
            priority: 0,
            requirement: agentdash_agent_runtime::ContributionRequirement::Required,
        },
        runtime_name: driver_tool.name.clone(),
        description: driver_tool.description.clone(),
        parameters_schema: driver_tool.parameters_schema.clone(),
        capability_key: "enterprise_echo".to_string(),
        tool_path: "enterprise::echo".to_string(),
        allowed_channels: driver_tool.channels.iter().copied().collect(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: driver_tool.protocol_projection.clone(),
        presentation_emitter: ToolPresentationEmitter::VendorStream,
        parity_fixture_id: driver_tool.parity_fixture_id.clone(),
    };
    let binding = &surface.hooks.bindings[0];
    let hook = agentdash_agent_runtime::HookDefinition {
        meta: agentdash_agent_runtime::ContributionMeta {
            key: format!("hook:{}", binding.definition_id),
            source: source.clone(),
            priority: 0,
            requirement: agentdash_agent_runtime::ContributionRequirement::Required,
        },
        definition_id: binding.definition_id.clone(),
        point: binding.point,
        actions: binding.actions.iter().copied().collect(),
        minimum_strength: binding.strength,
        failure_policy: binding.failure_policy,
        matcher: agentdash_agent_runtime::HookMatcher::Any,
        handler: agentdash_agent_runtime::HookHandler::Builtin {
            key: binding.definition_id.as_str().to_string(),
        },
    };
    agentdash_agent_runtime::AgentSurfaceCompiler
        .compile_business_facts(agentdash_agent_runtime::BusinessAgentSurfaceFacts {
            revision: surface.revision,
            context_recipe: surface.context.recipe.clone(),
            tool_set_revision: surface.tools.revision,
            hook_plan_revision: surface.hooks.revision,
            workspace: agentdash_agent_runtime::WorkspaceRequirement {
                capabilities: BTreeSet::new(),
                minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
                requirement: agentdash_agent_runtime::ContributionRequirement::Required,
            },
            source,
            transition_phase_node: Some("fixture".to_string()),
            instructions: vec!["Enterprise remote Agent".to_string()],
            tools: vec![tool],
            hooks: vec![hook],
            bootstrap_context: Default::default(),
            normalized_context_surface: Default::default(),
            projection_identity: agentdash_agent_runtime::ContextProjectionIdentity {
                operation_id: "enterprise-fixture-compile".to_string(),
                source_frame_id: "enterprise-remote-frame".to_string(),
                source_frame_revision: surface.revision.0,
                recorded_at_ms: 1,
            },
        })
        .expect("compile enterprise fixture business surface")
}

struct EnterpriseSurfaceSource {
    definition: AgentServiceDefinition,
    manifest: AgentRuntimeTrustManifest,
    tool_registry: Arc<CompiledAgentRunToolRegistry>,
    tool_calls: Arc<AtomicUsize>,
}

fn enterprise_tool_catalog(
    surface: &MaterializedDriverSurface,
) -> agentdash_agent_runtime::ToolCatalogRevision {
    agentdash_agent_runtime::ToolCatalogRevision {
        revision: surface.tools.revision,
        digest: surface.tools.digest.clone(),
        tools: vec![agentdash_agent_runtime::ToolContribution {
            meta: agentdash_agent_runtime::ContributionMeta {
                key: "enterprise_echo".to_string(),
                source: agentdash_agent_runtime::SurfaceSourceRef {
                    layer: "enterprise_e2e".to_string(),
                    key: "enterprise_echo".to_string(),
                },
                priority: 0,
                requirement: agentdash_agent_runtime::ContributionRequirement::Required,
            },
            runtime_name: "enterprise_echo".to_string(),
            description: "Enterprise reverse RuntimeWire tool".to_string(),
            parameters_schema: json!({"type":"object"}),
            capability_key: "enterprise".to_string(),
            tool_path: "enterprise::echo".to_string(),
            allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: ToolProtocolProjection::Dynamic {
                namespace: Some("enterprise_test".to_string()),
            },
            presentation_emitter: ToolPresentationEmitter::VendorStream,
            parity_fixture_id: "main_tool_enterprise_remote_e2e_dynamic_lifecycle".to_string(),
        }],
        mcp_servers: Vec::new(),
    }
}

struct EnterpriseCompiledBindingRecovery {
    registry: Arc<CompiledAgentRunToolRegistry>,
    repository: Arc<
        agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository,
    >,
    tool_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl CompiledAgentRunToolBindingRecovery for EnterpriseCompiledBindingRecovery {
    async fn recover(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
        let binding = self
            .repository
            .load_by_runtime_binding(binding_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "recovered Enterprise binding is missing".to_string(),
            })?;
        let surface = self
            .repository
            .load_bound_surface(binding_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "recovered Enterprise surface is missing".to_string(),
            })?;
        let frame_id = Uuid::nil();
        let hook_runtime: agentdash_spi::SharedHookRuntime = Arc::new(AgentFrameHookRuntime::new(
            binding.target.run_id,
            binding.target.agent_id,
            frame_id,
            1,
            binding.thread_id.to_string(),
            Arc::new(NoopExecutionHookProvider),
            AgentFrameHookSnapshot::default(),
        ));
        let applied = AppliedNativeAgentRunSurface {
            runtime_thread_id: binding.thread_id,
            binding_id: binding.binding_id,
            generation: binding.driver_generation,
            source_thread_id: binding.source_thread_id,
            surface_revision: surface.revision,
            surface_digest: surface.digest.clone(),
            tool_set_revision: surface.tools.revision,
            hook_plan_revision: surface.hooks.revision,
            hook_plan_digest: surface.hooks.digest.clone(),
            terminal_hook_effect_binding: binding.surface.terminal_hook_effect_binding,
        };
        self.registry
            .put(CompiledAgentRunToolBinding::from_test_tools(
                applied,
                binding.target.run_id,
                binding.target.agent_id,
                frame_id,
                hook_runtime,
                enterprise_tool_catalog(&surface),
                vec![Arc::new(EnterpriseEchoTool(self.tool_calls.clone())) as DynAgentTool],
            ))
            .await
    }
}

#[async_trait]
impl AgentRunRuntimeSurfaceSource for EnterpriseSurfaceSource {
    async fn prepare(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError> {
        let surface = materialized_surface(thread_id.clone());
        let business_surface = compiled_business_surface(&surface);
        let applied_coordinates = AppliedNativeAgentRunSurface {
            runtime_thread_id: thread_id.clone(),
            binding_id: binding_id.clone(),
            generation: RuntimeDriverGeneration(1),
            source_thread_id: DriverThreadId::new("enterprise-source-thread").unwrap(),
            surface_revision: surface.revision,
            surface_digest: surface.digest.clone(),
            tool_set_revision: surface.tools.revision,
            hook_plan_revision: surface.hooks.revision,
            hook_plan_digest: surface.hooks.digest.clone(),
            terminal_hook_effect_binding: None,
        };
        let hook_runtime: agentdash_spi::SharedHookRuntime = Arc::new(AgentFrameHookRuntime::new(
            request.target.run_id,
            request.target.agent_id,
            Uuid::nil(),
            1,
            thread_id.to_string(),
            Arc::new(NoopExecutionHookProvider),
            AgentFrameHookSnapshot::default(),
        ));
        let catalog = enterprise_tool_catalog(&surface);
        let publication = Arc::new(PendingCompiledAgentRunToolBinding::from_test_tools(
            self.tool_registry.clone(),
            &applied_coordinates,
            request.target.run_id,
            request.target.agent_id,
            Uuid::nil(),
            hook_runtime,
            catalog,
            vec![Arc::new(EnterpriseEchoTool(self.tool_calls.clone())) as DynAgentTool],
        ));
        let profile = native_runtime_profile();
        Ok(PreparedAgentRunRuntime {
            source_frame_id: "enterprise-remote-frame".to_string(),
            service_instance_id: id("enterprise-fallback-unused"),
            definition_id: self.definition.provenance.definition_id.clone(),
            service_config: json!({}),
            placement: AgentRuntimePlacement::InProcess,
            business_surface,
            hook_plan: RuntimeHookPlanBinding {
                thread_id: thread_id.clone(),
                plan: BoundRuntimeHookPlan {
                    revision: surface.hooks.revision,
                    digest: surface.hooks.digest.clone(),
                    entries: vec![BoundRuntimeHookEntry {
                        definition_id: id("enterprise-before-provider"),
                        point: HookPoint::BeforeProviderRequest,
                        actions: BTreeSet::from([HookAction::Observe]),
                        delivered_strength: SemanticStrength::ExactSynchronous,
                        failure_policy: HookFailurePolicy::FailClosed,
                        required: true,
                        site: HookExecutionSite::AgentCoreCallback,
                    }],
                },
            },
            publication,
            terminal_hook_effect_binding: None,
            bound_surface: BoundAgentSurfaceReference {
                revision: surface.revision,
                digest: surface.digest.clone(),
                tool_set_revision: surface.tools.revision,
                tool_set_digest: surface.tools.digest.clone(),
                hook_plan_revision: Some(surface.hooks.revision),
                hook_plan_digest: Some(surface.hooks.digest.clone()),
                hook_artifact_digest: surface.hooks.artifact_digest.clone(),
                hook_configuration_boundary: surface.hooks.configuration_boundary,
                required_hooks: vec![HookRequirement {
                    point: HookPoint::BeforeProviderRequest,
                    actions: BTreeSet::from([HookAction::Observe]),
                    minimum_strength: SemanticStrength::ExactSynchronous,
                    failure_policy: HookFailurePolicy::FailClosed,
                    required: true,
                }],
            },
            surface,
            transport_profile: profile.clone(),
            host_policy_profile: profile,
            conformance: ConformanceEvidence {
                suite_revision: self.manifest.suite_revision.clone(),
                driver_build_digest: self.manifest.driver_build_digest.clone(),
                verified_profile_digest: profile_digest(&self.manifest.verified_profile)
                    .expect("profile digest"),
                verified_at: Utc::now(),
            },
            allow_instance_creation: false,
            context_delivery_target:
                agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                    connector_id: "codex".to_string(),
                    executor: "CODEX".to_string(),
                },
        })
    }
}

async fn migrated_pool(name: &str) -> (PgPool, PostgresRuntime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/enterprise-remote-e2e")
        .join(format!("{name}-{}", Uuid::new_v4().simple()));
    let runtime = PostgresRuntime::resolve_embedded_at_data_root(name, 8, root)
        .await
        .expect("embedded PostgreSQL");
    agentdash_infrastructure::migration::run_postgres_migrations(&runtime.pool)
        .await
        .expect("migrations");
    (runtime.pool.clone(), runtime)
}

async fn seed_agent_run_target(pool: &PgPool, target: &AgentRunRuntimeTarget) {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO lifecycle_runs \
         (id,project_id,topology,status,created_at,updated_at,last_activity_at) \
         VALUES ($1,$2,$3,$4,$5,$5,$5)",
    )
    .bind(target.run_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind("single_agent")
    .bind("running")
    .bind(now)
    .execute(pool)
    .await
    .expect("seed lifecycle run");
    sqlx::query(
        "INSERT INTO lifecycle_agents \
         (id,run_id,project_id,source,status) \
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(target.agent_id.to_string())
    .bind(target.run_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind("project_agent")
    .bind("active")
    .execute(pool)
    .await
    .expect("seed lifecycle agent");
}

async fn local_host(
    pool: PgPool,
    contribution: AgentRuntimeDriverContribution,
    manifest: &AgentRuntimeTrustManifest,
) -> (
    Arc<RuntimeWireCommandHandler>,
    AgentServiceDefinitionRegistry,
    Arc<IntegrationDriverHost>,
) {
    let definition = contribution.definition.clone();
    let trusted_registry = AgentServiceDefinitionRegistry::collect(vec![contribution.clone()])
        .expect("local definition registry");
    let repository = Arc::new(PostgresAgentRuntimeHostRepository::new(pool));
    let host_port_router = Arc::new(RuntimeWireHostPortRouter::default());
    let host = Arc::new(IntegrationDriverHost::new(
        AgentServiceDefinitionRegistry::collect(vec![contribution]).expect("local registry"),
        repository.clone(),
        host_port_router.host_ports(Arc::new(NoCredentials)),
        Arc::new(TrustedDriverConformanceVerifier::new(
            TrustedDriverManifestRegistry::collect(vec![trusted_manifest(manifest)])
                .expect("local trust registry"),
        )),
        BACKEND_ID,
    ));
    let instance = host
        .put_instance(PutAgentServiceInstance {
            id: id("enterprise-local-instance"),
            definition_id: definition.provenance.definition_id,
            config: json!({"provider":"enterprise","model":"reference","credential_scope":{"kind":"platform"}}),
            credentials: BTreeMap::new(),
            placement: AgentRuntimePlacement::LocalProcess {
                host_id: BACKEND_ID.to_string(),
            },
            desired_state: ServiceInstanceDesiredState::Active,
            expected_revision: None,
        })
        .await
        .expect("put local enterprise instance");
    let profile = native_runtime_profile();
    host.activate(ActivateAgentServiceInstance {
        instance_id: instance.id,
        expected_revision: instance.revision,
        transport_profile: profile.clone(),
        transport_profile_digest: profile_digest(&profile).expect("transport digest"),
        host_policy_profile: profile.clone(),
        host_policy_digest: profile_digest(&profile).expect("policy digest"),
        conformance: ConformanceEvidence {
            suite_revision: manifest.suite_revision.clone(),
            driver_build_digest: manifest.driver_build_digest.clone(),
            verified_profile_digest: profile_digest(&manifest.verified_profile)
                .expect("verified profile"),
            verified_at: Utc::now(),
        },
    })
    .await
    .expect("activate local enterprise instance");
    let transport_id = AgentRuntimePlacementId::new(TRANSPORT_ID).expect("transport id");
    let handler = Arc::new(RuntimeWireCommandHandler::new_with_host_port_router(
        Arc::new(HostRuntimeDriverEndpointResolver::new(
            host.clone(),
            BACKEND_ID,
            agentdash_agent_runtime_contract::HostIncarnationId::new("enterprise-e2e-incarnation")
                .expect("host incarnation id"),
            transport_id,
        )),
        RuntimeRelayTransportDescriptor {
            supported_protocol_revisions: vec![
                agentdash_agent_runtime_wire::RUNTIME_WIRE_PROTOCOL_REVISION,
            ],
            profile: profile.clone(),
            profile_digest: profile_digest(&profile).expect("descriptor profile"),
            max_in_flight_frames: 64,
        },
        host_port_router,
    ));
    (handler, trusted_registry, host)
}

async fn route_local_message(
    handler: &RuntimeWireCommandHandler,
    registry: &BackendRegistry,
    message: RelayMessage,
) {
    let responses = match message {
        RelayMessage::RuntimeWireOpen { id, payload } => vec![handler.open(id, payload).await],
        RelayMessage::RuntimeWireFrame { id, payload } => handler.frame(id, *payload).await,
        RelayMessage::RuntimeWireAck { id, payload } => {
            handler.acknowledge(id, payload).await.into_iter().collect()
        }
        _ => Vec::new(),
    };
    for response in responses {
        registry
            .handle_runtime_wire_message(BACKEND_ID, &response)
            .await;
    }
}

#[tokio::test]
async fn enterprise_remote_mailbox_reaches_local_host_and_canonical_snapshot() {
    let (local_pool, _local_postgres) = migrated_pool("enterprise-remote-local").await;
    let (cloud_pool, _cloud_postgres) = migrated_pool("enterprise-remote-cloud").await;
    let (enterprise, manifest, bridge) = enterprise_contribution();
    let source_definition = enterprise.definition.clone();
    let (handler, _trusted_source_registry, local_runtime_host) =
        local_host(local_pool, enterprise, &manifest).await;
    let advertisements = handler.advertised_offers().await.expect("local offers");
    assert_eq!(advertisements.len(), 1);

    let registry = BackendRegistry::new();
    let (cloud_sender, mut cloud_to_local) = mpsc::unbounded_channel();
    let (local_sender, mut local_to_cloud) = mpsc::unbounded_channel();
    handler.attach_outbound(local_sender);
    registry
        .try_register(ConnectedBackend {
            backend_id: BACKEND_ID.to_string(),
            name: "Enterprise Desktop".to_string(),
            version: "1.0.0".to_string(),
            capabilities: CapabilitiesPayload::default(),
            sender: cloud_sender.clone(),
            connected_at: Utc::now(),
        })
        .await
        .expect("register local backend");
    let local_runtime_frames = Arc::new(AtomicUsize::new(0));
    let local_driver_events = Arc::new(AtomicUsize::new(0));
    let cloud_runtime_acks = Arc::new(AtomicUsize::new(0));
    let relay_registry = registry.clone();
    let relay_handler = handler.clone();
    let relay_local_runtime_frames = local_runtime_frames.clone();
    let relay_local_driver_events = local_driver_events.clone();
    let relay_cloud_runtime_acks = cloud_runtime_acks.clone();
    let relay = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(message) = cloud_to_local.recv() => {
                    if matches!(message, RelayMessage::RuntimeWireAck { .. }) {
                        relay_cloud_runtime_acks.fetch_add(1, Ordering::SeqCst);
                    }
                    route_local_message(&relay_handler, &relay_registry, message).await;
                }
                Some(message) = local_to_cloud.recv() => {
                    if matches!(message, RelayMessage::RuntimeWireFrame { .. }) {
                        relay_local_runtime_frames.fetch_add(1, Ordering::SeqCst);
                    }
                    if matches!(&message, RelayMessage::RuntimeWireFrame { payload, .. }
                        if matches!(&payload.envelope.frame, agentdash_agent_runtime_wire::RuntimeWireFrame::Notification(notification)
                            if matches!(notification.as_ref(), agentdash_agent_runtime_wire::RuntimeWireNotification::DriverEvent(_)))) {
                        relay_local_driver_events.fetch_add(1, Ordering::SeqCst);
                    }
                    relay_registry.handle_runtime_wire_message(BACKEND_ID, &message).await;
                }
                else => break,
            }
        }
    });

    let placement_resolver = Arc::new(CloudRuntimeWirePlacementResolver::new(registry.clone(), 64));
    let proxy_definition = remote_proxy_definition(&source_definition);
    let tool_registry = Arc::new(CompiledAgentRunToolRegistry::default());
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::new(AtomicUsize::new(0));
    let callback_last_error = Arc::new(tokio::sync::Mutex::new(None));
    let hooks = Arc::new(RecordingHookCallback::default());
    let callback_pool = cloud_pool.clone();
    let callback_registry = tool_registry.clone();
    let callback_calls_for_factory = callback_calls.clone();
    let callback_last_error_for_factory = callback_last_error.clone();
    let hooks_for_factory = hooks.clone();
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let presentation_thread_id = PresentationThreadId::new("enterprise-presentation-thread")
        .expect("presentation thread id");
    seed_agent_run_target(&cloud_pool, &target).await;
    let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
        pool: cloud_pool.clone(),
        contributions: vec![remote_runtime_contribution(
            proxy_definition.clone(),
            placement_resolver,
        )],
        trusted_manifests: vec![TrustedDriverManifest {
            provenance: proxy_definition.provenance.clone(),
            ..trusted_manifest(&manifest)
        }],
        surface_source: Arc::new(EnterpriseSurfaceSource {
            definition: proxy_definition,
            manifest: manifest.clone(),
            tool_registry: tool_registry.clone(),
            tool_calls: tool_calls.clone(),
        }),
        credential_broker: Arc::new(NoCredentials),
        callback_factory: Arc::new(move |runtime| {
            let tool_broker_resolver = Arc::new(PostgresAgentRunToolBrokerResolver::new(
                callback_pool.clone(),
                runtime,
                callback_registry.clone(),
                Arc::new(EnterpriseCapabilityPort),
            ));
            AgentRuntimeCallbacks {
                tools: Arc::new(RecordingToolCallback {
                    inner: Arc::new(
                        super::agent_runtime::PlatformAgentRuntimeToolCallback::new(
                            tool_broker_resolver,
                        ),
                    ),
                    calls: callback_calls_for_factory.clone(),
                    last_error: callback_last_error_for_factory.clone(),
                }),
                hooks: hooks_for_factory.clone(),
            }
        }),
        application_presentation_projector: Arc::new(
            agentdash_application_agentrun::agent_run::AgentRunRuntimeApplicationPresentationProjector,
        ),
        committed_presentation_observer: Arc::new(
            agentdash_agent_runtime::NoopRuntimeCommittedPresentationObserver,
        ),
        managed_compaction: Some(Arc::new(EnterpriseManagedCompactionEngine)),
        node_id: "enterprise-cloud-host".to_string(),
    })
    .expect("cloud production composition");
    tool_registry
        .bind_recovery(Arc::new(EnterpriseCompiledBindingRecovery {
            registry: tool_registry.clone(),
            repository: Arc::new(
                agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository::new(
                    cloud_pool.clone(),
                ),
            ),
            tool_calls: tool_calls.clone(),
        }))
        .expect("configure Enterprise compiled binding recovery");
    let inventory = CloudRemoteRuntimeInventory::new(composition.host.clone());
    inventory.mark_online(BACKEND_ID).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        inventory.sync(BACKEND_ID, &advertisements),
    )
    .await
    .expect("remote inventory sync timeout")
    .expect("register remote proxy offer");

    let runtime: Arc<dyn AgentRunRuntime> = Arc::new(ManagedAgentRunRuntime::new(
        composition.gateway.clone(),
        composition.bindings.clone(),
        composition.provisioner.clone(),
        composition.presentation_plans.clone(),
        tool_registry.clone(),
    ));
    let mailbox_repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let mailbox = RuntimeAgentRunMailbox::new(
        mailbox_repository.clone(),
        runtime.clone(),
        Arc::new(
            agentdash_test_support::workflow::MemoryAgentRunMessageSubmissionStore::new(
                mailbox_repository,
            ),
        ),
    );
    let initial_started_at_seconds = Utc::now().timestamp();
    let submitted = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        mailbox.submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: presentation_thread_id.clone(),
            presentation: agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(
                    "run through enterprise remote",
                ),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source:
                    agentdash_application_agentrun::agent_run::LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "enterprise-first-message".to_string(),
            input: vec![RuntimeInput::text(
                "run through enterprise remote".to_string(),
            )],
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
            identity: None,
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery_intent: None,
            executor_config: None,
            backend_selection: None,
        }),
    )
    .await
    .expect("mailbox submit timeout")
    .expect("mailbox submit");
    let (receipt, mailbox_message_id, initial_presentation) = match &submitted {
        RuntimeMailboxSubmitOutcome::Dispatched {
            receipt, message, ..
        } => (
            receipt,
            message.id,
            serde_json::from_value::<
                agentdash_application_agentrun::agent_run::AgentRunPresentationDraft,
            >(
                message
                    .launch_planning_input
                    .as_ref()
                    .expect("mailbox command payload")["presentation"]
                    .clone(),
            )
            .expect("persisted mailbox presentation draft"),
        ),
        RuntimeMailboxSubmitOutcome::Queued { .. } => panic!("idle mailbox must dispatch"),
    };
    let initial_operation_id = receipt.operation_id.clone();
    assert!(!receipt.duplicate);
    let pre_dispatch_binding = runtime
        .inspect(target.clone())
        .await
        .expect("inspect prepared enterprise binding")
        .binding
        .expect("enterprise binding exists before outbox dispatch");
    let mut presentation_live = composition
        .runtime_repository
        .subscribe_presentation(&pre_dispatch_binding.thread_id)
        .await;
    let mut runtime_events = runtime
        .read_events(
            agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                target: target.clone(),
                after: None,
                include_transient: true,
                transient_after: None,
                stream_generation: None,
            },
        )
        .await
        .expect("subscribe to canonical runtime events");
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            composition.outbox_worker.run_once(8),
        )
        .await
        .expect("outbox timeout")
        .expect("outbox"),
        1
    );

    let terminal_event = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let envelope = runtime_events
                .next()
                .await
                .expect("canonical runtime event stream remains open")
                .expect("canonical runtime event");
            if matches!(envelope.event, RuntimeEvent::TurnTerminal { .. }) {
                break envelope;
            }
        }
    })
    .await;
    let view = match terminal_event {
        Ok(_) => runtime
            .inspect(target.clone())
            .await
            .expect("terminal inspect"),
        Err(_) => {
            let diagnostic = runtime
                .inspect(target.clone())
                .await
                .expect("inspect timeout diagnostic");
            let mut stream = runtime
                .read_events(
                    agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                        target: target.clone(),
                        after: None,
                        include_transient: true,
                        transient_after: None,
                        stream_generation: None,
                    },
                )
                .await
                .expect("read timeout diagnostic events");
            let mut events = Vec::new();
            while let Ok(Some(event)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), stream.next()).await
            {
                events.push(event.expect("timeout diagnostic event"));
            }
            panic!(
                "enterprise remote terminal snapshot timeout: bridge_calls={}, tool_calls={}, hook_calls={}, local_runtime_frames={}, local_driver_events={}, cloud_runtime_acks={}, relay_finished={}; {diagnostic:#?}; events={events:#?}",
                bridge.calls.load(Ordering::SeqCst),
                tool_calls.load(Ordering::SeqCst),
                hooks.0.load(Ordering::SeqCst),
                local_runtime_frames.load(Ordering::SeqCst),
                local_driver_events.load(Ordering::SeqCst),
                cloud_runtime_acks.load(Ordering::SeqCst),
                relay.is_finished()
            );
        }
    };
    let binding = view.binding.expect("remote binding");
    let snapshot = view.snapshot.expect("canonical snapshot");
    let terminal_records = composition
        .runtime_repository
        .journal_records_after(&binding.thread_id, None)
        .await
        .expect("read production terminal presentation")
        .records;
    let mut live_presentation_records = Vec::new();
    loop {
        match presentation_live.try_recv() {
            Ok(record) => live_presentation_records.push(record),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                panic!("production Session live presentation lagged by {skipped} records")
            }
        }
    }
    let transient_presentation_records = live_presentation_records
        .into_iter()
        .filter(|record| {
            record.as_presentation().is_some_and(|event| {
                event.durability
                    == agentdash_agent_runtime_contract::PresentationDurability::Ephemeral
            })
        })
        .collect::<Vec<_>>();
    let journal_session_id =
        agentdash_application_agentrun::agent_run::agent_run_journal_session_id(
            target.run_id,
            target.agent_id,
        );
    let session_contracts = terminal_records
        .iter()
        .filter(|record| record.as_presentation().is_some())
        .cloned()
        .enumerate()
        .map(|(index, record)| {
            crate::routes::lifecycle_agents::journal_event_to_contract(
                agentdash_application_agentrun::agent_run::AgentRunJournalEvent {
                    journal_seq: index as u64 + 1,
                    segment_role:
                        agentdash_application_agentrun::agent_run::AgentRunJournalSegmentRole::CurrentDelivery,
                    source_runtime_thread_id: binding.thread_id.clone(),
                    source_event_seq: record.carrier().sequence,
                    record,
                },
                &journal_session_id,
            )
            .expect("project production Session API wrapper")
        })
        .collect::<Vec<_>>();
    let transient_session_contracts = transient_presentation_records
        .iter()
        .filter(|record| record.as_presentation().is_some())
        .cloned()
        .enumerate()
        .map(|(index, record)| {
            crate::routes::lifecycle_agents::journal_event_to_contract(
                agentdash_application_agentrun::agent_run::AgentRunJournalEvent {
                    journal_seq: terminal_records.len() as u64 + index as u64 + 1,
                    segment_role:
                        agentdash_application_agentrun::agent_run::AgentRunJournalSegmentRole::CurrentDelivery,
                    source_runtime_thread_id: binding.thread_id.clone(),
                    source_event_seq: None,
                    record,
                },
                &journal_session_id,
            )
            .expect("project production ephemeral Session API wrapper")
        })
        .collect::<Vec<_>>();
    assert!(
        !session_contracts.is_empty(),
        "production Remote→Native journal must reach the Session API wrapper"
    );
    for event in &session_contracts {
        let protected = serde_json::to_value(&event.notification.event)
            .expect("serialize protected Session event");
        assert_eq!(
            protected["type"], event.session_update_type,
            "the outer wrapper discriminant must be derived without rewriting the protected body"
        );
        assert_eq!(event.session_id, journal_session_id);
        assert_eq!(event.notification.session_id, journal_session_id);
        assert_eq!(event.turn_id, event.notification.trace.turn_id);
        if let Some(tool_call_id) = &event.tool_call_id {
            assert_eq!(
                protected["payload"]["item"]["id"],
                tool_call_id.as_str(),
                "Session wrapper tool identity must equal the protected item identity"
            );
        }
    }
    for event in &transient_session_contracts {
        let protected = serde_json::to_value(&event.notification.event)
            .expect("serialize ephemeral protected Session event");
        assert_eq!(protected["type"], event.session_update_type);
        assert_eq!(event.session_id, journal_session_id);
        assert_eq!(event.notification.session_id, journal_session_id);
        assert_eq!(event.turn_id, event.notification.trace.turn_id);
    }
    let protected_bodies = session_contracts
        .iter()
        .map(|event| {
            serde_json::to_value(&event.notification.event)
                .expect("serialize production protected Session body")
        })
        .collect::<Vec<_>>();
    let protected_types = protected_bodies
        .iter()
        .map(|event| {
            event["type"]
                .as_str()
                .expect("protected Session event type")
        })
        .collect::<Vec<_>>();
    let transient_protected_types = transient_session_contracts
        .iter()
        .map(|event| event.session_update_type.as_str())
        .collect::<Vec<_>>();
    assert!(
        protected_types.contains(&"user_input_submitted"),
        "production Session API must preserve the submitted user item: {protected_types:?}"
    );
    let reasoning_position = transient_protected_types
        .iter()
        .position(|actual| *actual == "reasoning_text_delta")
        .unwrap_or_else(|| {
            panic!("missing ephemeral reasoning_text_delta: {transient_protected_types:?}")
        });
    let agent_message_position = transient_protected_types
        .iter()
        .position(|actual| *actual == "agent_message_delta")
        .unwrap_or_else(|| {
            panic!("missing ephemeral agent_message_delta: {transient_protected_types:?}")
        });
    assert!(
        reasoning_position < agent_message_position,
        "production live Session API must preserve reasoning→final assistant order: {transient_protected_types:?}"
    );
    assert!(
        protected_bodies.iter().any(|body| {
            body["type"] == "item_completed" && body["payload"]["item"]["type"] == "reasoning"
        }),
        "reasoning must also have a durable terminal item for cold replay"
    );
    let mut session_tool_lifecycle = BTreeMap::<String, Vec<&str>>::new();
    let mut observed_business_error = false;
    for body in &protected_bodies {
        let phase = match body["type"].as_str() {
            Some("item_started") => "started",
            Some("item_completed") => "completed",
            _ => continue,
        };
        let item = &body["payload"]["item"];
        if item["type"] != "dynamicToolCall" {
            continue;
        }
        let item_id = item["id"]
            .as_str()
            .expect("production Session tool item id")
            .to_string();
        session_tool_lifecycle
            .entry(item_id)
            .or_default()
            .push(phase);
        if phase == "started" {
            assert!(
                item.get("success").is_some_and(serde_json::Value::is_null),
                "main-equivalent tool start keeps explicit success:null: {item}"
            );
            assert!(
                item.get("contentItems")
                    .is_some_and(serde_json::Value::is_null),
                "main-equivalent tool start keeps explicit contentItems:null: {item}"
            );
        } else if item["success"] == false {
            observed_business_error = true;
        }
    }
    assert_eq!(
        session_tool_lifecycle.len(),
        2,
        "the production Session wrapper must expose one card identity per logical tool"
    );
    assert!(
        session_tool_lifecycle
            .values()
            .all(|phases| phases.as_slice() == ["started", "completed"]),
        "tool start/result must merge by one protected item id: {session_tool_lifecycle:?}"
    );
    assert!(
        observed_business_error,
        "a business-error tool result must remain visible without aborting provider continuation"
    );
    let terminal_presentation = terminal_records.iter().any(|record| {
        let Some(event) = record.as_presentation() else {
            return false;
        };
        matches!(
            &event.event,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
            ) if key == "turn_terminal"
        )
    });
    assert!(
        terminal_presentation,
        "production composition must inject the non-empty AgentRun terminal projector"
    );
    if snapshot.status != RuntimeThreadStatus::Active {
        let mut stream = runtime
            .read_events(
                agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                    target: target.clone(),
                    after: None,
                    include_transient: true,
                    transient_after: None,
                    stream_generation: None,
                },
            )
            .await
            .expect("read terminal diagnostic events");
        let mut events = Vec::new();
        while let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), stream.next()).await
        {
            events.push(event.expect("terminal diagnostic event"));
        }
        panic!("unexpected terminal snapshot before compaction: {snapshot:#?}; events={events:#?}");
    }
    assert_eq!(
        snapshot.status,
        RuntimeThreadStatus::Active,
        "unexpected terminal snapshot before compaction: {snapshot:#?}"
    );
    assert!(snapshot.active_turn_id.is_none());
    assert!(snapshot.transcript.iter().any(|item| {
        let agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed) =
            &item.terminal_event.event
        else {
            return false;
        };
        RuntimeItemContent::new(completed.item.clone()).agent_message_text()
            == Some("enterprise remote completed")
    }));
    let tool_lifecycle = terminal_records
        .iter()
        .filter_map(|record| record.as_presentation())
        .filter_map(|event| match &event.event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => notification
                .item
                .tool_call_id()
                .map(|id| ("started", id.to_string())),
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification) => notification
                .item
                .tool_call_id()
                .map(|id| ("completed", id.to_string())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let tool_lifecycle_by_item = tool_lifecycle.iter().fold(
        BTreeMap::<String, Vec<&str>>::new(),
        |mut lifecycle, (phase, item_id)| {
            lifecycle.entry(item_id.clone()).or_default().push(*phase);
            lifecycle
        },
    );
    assert_eq!(
        tool_lifecycle_by_item.len(),
        2,
        "one provider response with two tool calls must create two logical cards: {tool_lifecycle:?}"
    );
    assert!(
        tool_lifecycle_by_item
            .values()
            .all(|phases| phases.as_slice() == ["started", "completed"]),
        "each logical tool must complete the card created by its own start: {tool_lifecycle:?}"
    );
    assert_eq!(
        terminal_records
            .iter()
            .filter_map(|record| record.as_presentation())
            .filter(|event| matches!(
                &event.event,
                agentdash_agent_protocol::BackboneEvent::UserInputSubmitted(_)
            ))
            .count(),
        1,
        "one submitted user message must remain one user presentation event"
    );
    let host_binding = composition
        .host
        .binding(&binding.binding_id)
        .await
        .expect("Host binding");
    let remote_instance = composition
        .host
        .service_instance(&host_binding.service_instance_id)
        .await
        .expect("remote instance read")
        .expect("remote instance");
    assert!(matches!(
        remote_instance.placement,
        AgentRuntimePlacement::Remote { ref host_id, .. } if host_id == BACKEND_ID
    ));
    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        2,
        "reverse RuntimeWire callbacks={}, last_error={:?}",
        callback_calls.load(Ordering::SeqCst),
        callback_last_error.lock().await.as_deref(),
    );
    assert!(hooks.0.load(Ordering::SeqCst) >= 1);

    let context_revision_before = snapshot.context_revision;
    let compaction = runtime
        .compact_context(GuardedAgentRunCommand {
            target: target.clone(),
            client_command_id: "enterprise-compact-context".to_string(),
            guard: AgentRunCommandGuard {
                thread_id: snapshot.thread_id.clone(),
                expected_revision: snapshot.revision,
                expected_active_turn_id: None,
            },
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
        })
        .await
        .expect("accept remote context compaction");
    assert!(!compaction.duplicate);
    assert_eq!(
        composition
            .durable_workers
            .run_once(RuntimeWorkKind::ContextPreparation, 8)
            .await
            .expect("prepare remote context compaction"),
        1
    );
    assert_eq!(
        composition
            .durable_workers
            .run_once(RuntimeWorkKind::ContextActivationDispatch, 8)
            .await
            .expect("dispatch remote context activation"),
        1
    );
    let compacted = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextActivationRecovery, 8)
                .await
                .expect("recover remote context activation");
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect compacted runtime");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.context_revision > context_revision_before
                    && snapshot.active_checkpoint_id.is_some()
            }) {
                break view.snapshot.expect("compacted snapshot");
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("remote context activation timeout");
    assert!(compacted.context_revision > context_revision_before);

    {
        let requests = bridge.requests.lock().await;
        assert_eq!(
            requests.len(),
            2,
            "one tool turn must make one provider call before and one after the callback"
        );
        assert!(
            requests[1]
                .iter()
                .filter(|message| matches!(message, AgentMessage::ToolResult { .. }))
                .count()
                == 2,
            "both tool results must be returned to Agent Core before the final provider call"
        );
        assert_eq!(
            requests[1]
                .iter()
                .filter(|message| matches!(
                    message,
                    AgentMessage::ToolResult { is_error: true, .. }
                ))
                .count(),
            1,
            "a tool business error must be returned to the provider without aborting the turn"
        );
    }

    let duplicate = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target: target.clone(),
                presentation_thread_id: presentation_thread_id.clone(),
                presentation: initial_presentation,
                client_command_id: format!("mailbox-{mailbox_message_id}"),
                input: vec![RuntimeInput::text(
                    "run through enterprise remote".to_string(),
                )],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("duplicate command replay");
    assert!(duplicate.duplicate);
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("duplicate outbox"),
        0
    );

    let follow_up_started_at_seconds = Utc::now().timestamp();
    let follow_up = mailbox
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: presentation_thread_id.clone(),
            presentation: agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(
                    "continue after compaction",
                ),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source:
                    agentdash_application_agentrun::agent_run::LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "enterprise-follow-up-after-compaction".to_string(),
            input: vec![RuntimeInput::text("continue after compaction".to_string())],
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
            identity: None,
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery_intent: None,
            executor_config: None,
            backend_selection: None,
        })
        .await
        .expect("submit follow-up after compaction");
    let follow_up_receipt = match follow_up {
        RuntimeMailboxSubmitOutcome::Dispatched { receipt, .. } => receipt,
        RuntimeMailboxSubmitOutcome::Queued { .. } => {
            panic!("idle compacted runtime must dispatch its follow-up")
        }
    };
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("dispatch follow-up after compaction"),
        1
    );
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect follow-up runtime");
            if bridge.calls.load(Ordering::SeqCst) == 4
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.active_turn_id.is_none())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("follow-up tool turn must continue to final assistant");
    assert_eq!(tool_calls.load(Ordering::SeqCst), 4);
    let follow_up_messages = {
        let requests = bridge.requests.lock().await;
        format!(
            "{:?}",
            requests.last().expect("follow-up final provider request")
        )
    };
    assert!(
        follow_up_messages.contains("enterprise remote compacted summary")
            && follow_up_messages.contains("continue after compaction"),
        "compaction context and the next user message must both reach the provider: {follow_up_messages}"
    );

    for operation_id in [
        initial_operation_id.clone(),
        follow_up_receipt.operation_id.clone(),
    ] {
        let operation = composition
            .runtime_repository
            .find_operation(&operation_id)
            .await
            .expect("read completed production operation")
            .expect("production operation remains durable");
        assert_eq!(
            operation.terminal,
            Some(RuntimeOperationTerminal::Succeeded),
            "a later turn must not rewrite an already completed operation"
        );
        let (attempt_count, dispatched): (i32, bool) = sqlx::query_as(
            "SELECT attempt_count,dispatched_at IS NOT NULL FROM agent_runtime_outbox WHERE operation_id=$1",
        )
        .bind(operation_id.as_str())
        .fetch_one(&cloud_pool)
        .await
        .expect("read production Runtime outbox delivery");
        assert_eq!(
            attempt_count, 1,
            "accepted prompt side effects are not replayed"
        );
        assert!(
            dispatched,
            "accepted prompt outbox remains durably acknowledged"
        );
    }

    let before_recovery = runtime
        .inspect(target.clone())
        .await
        .expect("inspect before cold binding recovery");
    let before_recovery_binding = before_recovery.binding.expect("binding before recovery");
    registry.unregister(BACKEND_ID).await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect idle disconnect");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.status == RuntimeThreadStatus::Lost && snapshot.active_turn_id.is_none()
            }) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle RuntimeWire disconnect must make the binding recoverable");
    inventory
        .withdraw(BACKEND_ID)
        .await
        .expect("withdraw disconnected Runtime inventory");
    let local_instance_id: RuntimeServiceInstanceId = id("enterprise-local-instance");
    let local_instance = local_runtime_host
        .service_instance(&local_instance_id)
        .await
        .expect("read local service instance before replacement")
        .expect("local service instance before replacement");
    local_runtime_host
        .deactivate(&local_instance_id, local_instance.revision)
        .await
        .expect("deactivate disconnected local service generation");
    let inactive_local_instance = local_runtime_host
        .service_instance(&local_instance_id)
        .await
        .expect("read inactive local service instance")
        .expect("inactive local service instance");
    let reenabled_local_instance = local_runtime_host
        .put_instance(PutAgentServiceInstance {
            id: inactive_local_instance.id.clone(),
            definition_id: inactive_local_instance.definition_id.clone(),
            config: inactive_local_instance.config.clone(),
            credentials: inactive_local_instance.credentials.clone(),
            placement: inactive_local_instance.placement.clone(),
            desired_state: ServiceInstanceDesiredState::Active,
            expected_revision: Some(inactive_local_instance.revision),
        })
        .await
        .expect("re-enable local service before replacement activation");
    let recovery_profile = native_runtime_profile();
    local_runtime_host
        .activate(ActivateAgentServiceInstance {
            instance_id: local_instance_id,
            expected_revision: reenabled_local_instance.revision,
            transport_profile: recovery_profile.clone(),
            transport_profile_digest: profile_digest(&recovery_profile)
                .expect("recovery transport digest"),
            host_policy_profile: recovery_profile.clone(),
            host_policy_digest: profile_digest(&recovery_profile).expect("recovery policy digest"),
            conformance: ConformanceEvidence {
                suite_revision: manifest.suite_revision.clone(),
                driver_build_digest: manifest.driver_build_digest.clone(),
                verified_profile_digest: profile_digest(&manifest.verified_profile)
                    .expect("recovery verified profile"),
                verified_at: Utc::now(),
            },
        })
        .await
        .expect("activate replacement local service generation");
    let recovery_advertisements = handler
        .advertised_offers()
        .await
        .expect("replacement local offers");
    assert_eq!(recovery_advertisements.len(), 1);
    assert!(
        recovery_advertisements[0].driver_generation > advertisements[0].driver_generation,
        "Remote recovery requires a genuinely newer advertised driver generation"
    );
    registry
        .try_register(ConnectedBackend {
            backend_id: BACKEND_ID.to_string(),
            name: "Enterprise Desktop Rebound".to_string(),
            version: "1.0.1".to_string(),
            capabilities: CapabilitiesPayload::default(),
            sender: cloud_sender.clone(),
            connected_at: Utc::now(),
        })
        .await
        .expect("re-register backend before cold binding recovery");
    inventory.mark_online(BACKEND_ID).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        inventory.sync(BACKEND_ID, &recovery_advertisements),
    )
    .await
    .expect("reconnected Runtime inventory sync timeout")
    .expect("reconnected Runtime inventory sync");

    let recovery_started_at_seconds = Utc::now().timestamp();
    let recovery = mailbox
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: presentation_thread_id.clone(),
            presentation: agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(
                    "continue after binding recovery",
                ),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source:
                    agentdash_application_agentrun::agent_run::LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "enterprise-follow-up-after-binding-recovery".to_string(),
            input: vec![RuntimeInput::text(
                "continue after binding recovery".to_string(),
            )],
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
            identity: None,
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery_intent: None,
            executor_config: None,
            backend_selection: None,
        })
        .await
        .expect("submit follow-up through a recovered binding");
    let recovery_receipt = match recovery {
        RuntimeMailboxSubmitOutcome::Dispatched { receipt, .. } => receipt,
        RuntimeMailboxSubmitOutcome::Queued { .. } => {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    if let Some((_, receipt, steered)) = mailbox
                        .recover_and_drain_once(&target)
                        .await
                        .expect("drain queued recovery delivery")
                    {
                        assert!(!steered);
                        break receipt;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("queued recovery delivery must drain after Runtime inventory reconnect")
        }
    };
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("dispatch follow-up through recovered binding"),
        1
    );
    let recovered_view = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect recovered runtime");
            if bridge.calls.load(Ordering::SeqCst) == 6
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.active_turn_id.is_none())
            {
                break view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cold binding recovery must restore history and finish the tool loop");
    let recovered_binding = recovered_view.binding.expect("recovered binding");
    assert_ne!(
        recovered_binding.binding_id,
        before_recovery_binding.binding_id
    );
    assert_eq!(
        recovered_binding.binding_epoch.0,
        before_recovery_binding.binding_epoch.0 + 1
    );
    let (recovery_request_diagnostic, prompt_counts) = {
        let requests = bridge.requests.lock().await;
        let prompts = [
            (0, "run through enterprise remote"),
            (2, "continue after compaction"),
            (4, "continue after binding recovery"),
        ];
        let counts = prompts.map(|(request_index, prompt)| {
            requests[request_index]
                .iter()
                .filter_map(|message| match message {
                    AgentMessage::User { content, .. } => Some(content),
                    _ => None,
                })
                .flatten()
                .filter(|part| part.extract_text() == Some(prompt))
                .count()
        });
        (format!("{requests:#?}"), counts)
    };
    assert_eq!(
        prompt_counts,
        [1, 1, 1],
        "each accepted prompt must reach its provider request exactly once: {recovery_request_diagnostic}"
    );
    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        6,
        "recovered tool loop skipped a provider tool phase; bridge_calls={}, requests={recovery_request_diagnostic}",
        bridge.calls.load(Ordering::SeqCst),
    );
    let recovery_messages = {
        let requests = bridge.requests.lock().await;
        format!(
            "{:?}",
            requests.last().expect("recovered final provider request")
        )
    };
    assert!(
        recovery_messages.contains("enterprise remote compacted summary")
            && recovery_messages.contains("continue after compaction")
            && recovery_messages.contains("continue after binding recovery"),
        "cold binding recovery must replay the compacted base and durable tail exactly once: {recovery_messages}"
    );
    let recovered_records = composition
        .runtime_repository
        .journal_records_after(&recovered_binding.thread_id, None)
        .await
        .expect("read journal after cold binding recovery")
        .records;
    let recovered_tool_lifecycle = recovered_records
        .iter()
        .filter_map(|record| record.as_presentation())
        .filter_map(|event| match &event.event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => notification
                .item
                .tool_call_id()
                .map(|id| ("started", id.to_string())),
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification) => notification
                .item
                .tool_call_id()
                .map(|id| ("completed", id.to_string())),
            _ => None,
        })
        .fold(
            BTreeMap::<String, Vec<&str>>::new(),
            |mut lifecycle, (phase, item_id)| {
                lifecycle.entry(item_id).or_default().push(phase);
                lifecycle
            },
        );
    assert_eq!(
        recovered_tool_lifecycle.len(),
        6,
        "three two-tool turns must retain six distinct presentation identities after rebind: {recovered_tool_lifecycle:?}"
    );
    assert!(
        recovered_tool_lifecycle
            .values()
            .all(|phases| phases.as_slice() == ["started", "completed"]),
        "cold rebind must preserve one complete card per logical tool: {recovered_tool_lifecycle:?}"
    );
    let recovered_operation = composition
        .runtime_repository
        .find_operation(&recovery_receipt.operation_id)
        .await
        .expect("read recovered operation")
        .expect("recovered operation remains durable");
    assert_eq!(
        recovered_operation.terminal,
        Some(RuntimeOperationTerminal::Succeeded)
    );
    let recovered_attempt_count: i32 =
        sqlx::query_scalar("SELECT attempt_count FROM agent_runtime_outbox WHERE operation_id=$1")
            .bind(recovery_receipt.operation_id.as_str())
            .fetch_one(&cloud_pool)
            .await
            .expect("read recovered operation outbox");
    assert_eq!(recovered_attempt_count, 1);

    let active_disconnect_binding_id = recovered_binding.binding_id.clone();
    bridge.block_next.store(true, Ordering::SeqCst);
    let disconnect_started_at_seconds = Utc::now().timestamp();
    let active = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target: target.clone(),
                presentation_thread_id: presentation_thread_id.clone(),
                presentation:
                    agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                        content: agentdash_agent_protocol::text_user_input_blocks(
                            "block until RuntimeWire disconnect",
                        ),
                        source: agentdash_agent_protocol::UserInputSource::core_composer(),
                        launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::HttpPrompt,
                        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                    },
                client_command_id: "enterprise-disconnect-active-turn".to_string(),
                input: vec![RuntimeInput::text("block until RuntimeWire disconnect".to_string())],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("accept active disconnect turn");
    assert!(!active.duplicate);
    let active_dispatch = {
        let outbox = composition.outbox_worker.clone();
        tokio::spawn(async move { outbox.run_once(8).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), bridge.blocked.acquire())
        .await
        .expect("blocked provider entry timeout")
        .expect("bridge enters blocked provider call")
        .forget();

    registry.unregister(BACKEND_ID).await;
    let disconnected_dispatch =
        tokio::time::timeout(std::time::Duration::from_secs(10), active_dispatch)
            .await
            .expect("disconnected dispatch worker timeout")
            .expect("disconnected dispatch worker join")
            .expect("terminalized disconnected dispatch is durably acknowledged");
    assert_eq!(disconnected_dispatch, 1);
    let lost = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = runtime.inspect(target.clone()).await.expect("inspect lost");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.status == RuntimeThreadStatus::Lost && snapshot.active_turn_id.is_none()
            }) {
                break view.snapshot.expect("lost snapshot");
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnect must converge active turn to Lost");
    assert_eq!(lost.status, RuntimeThreadStatus::Lost);
    let binding_lost = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let mut lost_events = runtime
                .read_events(
                    agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                        target: target.clone(),
                        after: None,
                        include_transient: false,
                        transient_after: None,
                        stream_generation: None,
                    },
                )
                .await
                .expect("read lost events");
            let mut count = 0;
            while let Some(event) = lost_events.next().await {
                if matches!(
                    event.expect("lost event").event,
                    RuntimeEvent::BindingLost { binding_id, .. }
                        if binding_id == active_disconnect_binding_id
                ) {
                    count += 1;
                }
            }
            if count > 0 {
                break count;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("BindingLost journal convergence timeout");
    assert_eq!(
        binding_lost, 1,
        "the active recovered binding has exactly one BindingLost"
    );
    for operation_id in [
        initial_operation_id,
        follow_up_receipt.operation_id,
        recovery_receipt.operation_id,
    ] {
        let operation = composition
            .runtime_repository
            .find_operation(&operation_id)
            .await
            .expect("read historical operation after later BindingLost")
            .expect("historical operation remains durable after later BindingLost");
        assert_eq!(
            operation.terminal,
            Some(RuntimeOperationTerminal::Succeeded),
            "BindingLost may terminalize only the active operation"
        );
    }
    let active_operation = composition
        .runtime_repository
        .find_operation(&active.operation_id)
        .await
        .expect("read active disconnect operation")
        .expect("active disconnect operation remains durable");
    assert!(matches!(
        active_operation.terminal,
        Some(RuntimeOperationTerminal::Lost { .. })
    ));
    let lost_records = composition
        .runtime_repository
        .journal_records_after(&recovered_binding.thread_id, None)
        .await
        .expect("read Session journal after active binding loss")
        .records;
    let lost_session_bodies = lost_records
        .into_iter()
        .filter(|record| record.as_presentation().is_some())
        .enumerate()
        .map(|(index, record)| {
            crate::routes::lifecycle_agents::journal_event_to_contract(
                agentdash_application_agentrun::agent_run::AgentRunJournalEvent {
                    journal_seq: index as u64 + 1,
                    segment_role:
                        agentdash_application_agentrun::agent_run::AgentRunJournalSegmentRole::CurrentDelivery,
                    source_runtime_thread_id: recovered_binding.thread_id.clone(),
                    source_event_seq: record.carrier().sequence,
                    record,
                },
                &journal_session_id,
            )
            .expect("project failed turn through Session API wrapper")
        })
        .map(|event| {
            serde_json::to_value(event.notification.event)
                .expect("serialize failed Session protected body")
        })
        .collect::<Vec<_>>();
    assert!(
        lost_session_bodies.iter().any(|body| {
            body["type"] == "platform"
                && body["payload"]["kind"] == "session_meta_update"
                && body["payload"]["data"]["key"] == "turn_terminal"
                && body["payload"]["data"]["value"]["terminal_type"] == "turn_lost"
        }),
        "active disconnect must expose the main-equivalent lost terminal through the Session API"
    );
    assert!(
        lost_session_bodies.iter().any(|body| {
            body["type"] == "platform" && body["payload"]["kind"] == "session_rewound"
        }),
        "failed active turn must expose the main-equivalent rewind through the Session API"
    );
    assert_eq!(
        lost_session_bodies
            .iter()
            .filter(|body| body["type"] == "user_input_submitted")
            .count(),
        4,
        "all user submissions, including follow-up and failed turn, remain exact-once Session facts"
    );
    registry.unregister(BACKEND_ID).await;

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        registry.try_register(ConnectedBackend {
            backend_id: BACKEND_ID.to_string(),
            name: "Enterprise Desktop Reconnected".to_string(),
            version: "1.0.1".to_string(),
            capabilities: CapabilitiesPayload::default(),
            sender: cloud_sender,
            connected_at: Utc::now(),
        }),
    )
    .await
    .expect("Runtime Wire reopen timeout")
    .expect("re-register local backend and reopen Runtime Wire");
    bridge.release.add_permits(1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let after_late = runtime
        .inspect(target.clone())
        .await
        .expect("inspect after late old event")
        .snapshot
        .expect("snapshot after late old event");
    assert_eq!(after_late.status, RuntimeThreadStatus::Lost);
    assert!(after_late.active_turn_id.is_none());

    let duplicate_lost = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target,
                presentation_thread_id,
                presentation:
                    agentdash_application_agentrun::agent_run::AgentRunPresentationDraft {
                        content: agentdash_agent_protocol::text_user_input_blocks(
                            "block until RuntimeWire disconnect",
                        ),
                        source: agentdash_agent_protocol::UserInputSource::core_composer(),
                        launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::HttpPrompt,
                        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                    },
                client_command_id: "enterprise-disconnect-active-turn".to_string(),
                input: vec![RuntimeInput::text("block until RuntimeWire disconnect".to_string())],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("duplicate lost command remains replayable");
    assert!(duplicate_lost.duplicate);
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("lost duplicate outbox"),
        0
    );

    relay.abort();
}
