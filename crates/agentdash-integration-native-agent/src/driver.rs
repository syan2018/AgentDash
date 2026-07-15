use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    str::FromStr,
    sync::Arc,
};

use agentdash_agent::{
    Agent, AgentConfig, AgentError, AgentEvent, AssistantStreamEvent, LlmBridge,
    ToolResultRefContext,
};
use agentdash_agent_protocol::{TranscriptProjectionEvent, project_transcript};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_types::DynAgentTool;
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_integration_api::*;
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::{
    context::{NativeBindingContext, NativeToolCallContext},
    hook::{NativeHookDelegate, supported_hook},
    mapping::{context_blocks_to_messages, inputs_to_message, message_content},
    presentation::{
        ChunkEmitState, NativeSessionItemIdentity, NativeToolPresentationRoute,
        StreamMapperEventState, StreamMapperRuntimeContext, ToolCallEmitState,
        convert_event_to_envelopes_with_runtime_context, run_error_terminal_diagnostic,
    },
    tool::NativeRuntimeTool,
};

const PROTOCOL_REVISION: u32 = 1;
const FACTORY_KEY: &str = "agentdash.native_agent";
const DEFINITION_ID: &str = "agentdash.native_agent";
const CONFORMANCE_SUITE: &str = "agentdash-native-runtime-conformance-v1";
/// Main Pi production stream-usage contract. This is presentation metadata, not the automatic
/// compaction policy (whose default reserve is owned by the compaction layer).
pub const NATIVE_STREAM_USAGE_RESERVE_TOKENS: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NativeBridgeResolveError {
    #[error("native provider configuration is invalid: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("native provider is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
}

/// Native service instance 选择 Provider credential 的持久、非密钥坐标。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeCredentialScope {
    Platform,
    User { user_id: String },
}

/// Native service instance 的 schema-validated 配置。API key/OAuth token 永远不进入该对象。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAgentServiceConfig {
    pub provider: String,
    pub model: String,
    pub credential_scope: NativeCredentialScope,
}

impl NativeAgentServiceConfig {
    pub fn from_instance(
        instance: &ActivatedAgentServiceInstance,
    ) -> Result<Self, NativeBridgeResolveError> {
        let mut config: Self =
            serde_json::from_value(instance.config.clone()).map_err(|error| {
                NativeBridgeResolveError::InvalidConfiguration {
                    reason: format!(
                        "service instance config does not match Native schema: {error}"
                    ),
                }
            })?;
        config.provider = config.provider.trim().to_string();
        config.model = config.model.trim().to_string();
        if config.provider.is_empty() || config.model.is_empty() {
            return Err(NativeBridgeResolveError::InvalidConfiguration {
                reason: "provider and model must be non-empty".to_string(),
            });
        }
        if let NativeCredentialScope::User { user_id } = &mut config.credential_scope {
            *user_id = user_id.trim().to_string();
            if user_id.is_empty() {
                return Err(NativeBridgeResolveError::InvalidConfiguration {
                    reason: "user credential scope requires a non-empty user_id".to_string(),
                });
            }
        }
        Ok(config)
    }
}

#[async_trait]
pub trait NativeBridgeResolver: Send + Sync {
    async fn resolve(
        &self,
        instance: &ActivatedAgentServiceInstance,
        host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError>;
}

/// Provider-bound metadata required to render Native Agent events faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePresentationMetadata {
    pub model_context_window: u64,
    pub reserve_tokens: u64,
}

/// One atomic provider/model resolution. Both fields describe the same selected model and
/// compaction policy, so presentation never has to infer metadata after bridge construction.
pub struct ResolvedNativeBridge {
    pub bridge: Arc<dyn LlmBridge>,
    pub presentation: NativePresentationMetadata,
}

pub struct NativeAgentDriverFactory {
    key: AgentRuntimeFactoryKey,
    resolver: Arc<dyn NativeBridgeResolver>,
}

/// Explicit composition object for first-party or enterprise Native Agent Core services.
///
/// The resolver is supplied by the host after provider/configuration repositories exist; this
/// prevents a global registry or placeholder provider from leaking into the Integration model.
pub struct NativeAgentRuntimeIntegration {
    resolver: Arc<dyn NativeBridgeResolver>,
}

impl NativeAgentRuntimeIntegration {
    pub fn new(resolver: Arc<dyn NativeBridgeResolver>) -> Self {
        Self { resolver }
    }
}

impl AgentDashIntegration for NativeAgentRuntimeIntegration {
    fn name(&self) -> &str {
        "builtin.native_agent_runtime"
    }

    fn agent_runtime_drivers(&self) -> Vec<AgentRuntimeDriverContribution> {
        vec![native_agent_contribution(self.resolver.clone())]
    }

    fn agent_runtime_trust_manifests(&self) -> Vec<AgentRuntimeTrustManifest> {
        vec![native_runtime_trust_manifest()]
    }
}

impl NativeAgentDriverFactory {
    pub fn new(resolver: Arc<dyn NativeBridgeResolver>) -> Self {
        Self {
            key: AgentRuntimeFactoryKey::new(FACTORY_KEY).expect("static native factory key"),
            resolver,
        }
    }
}

#[async_trait]
impl AgentRuntimeDriverFactory for NativeAgentDriverFactory {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey {
        &self.key
    }

    async fn create(
        &self,
        instance: ActivatedAgentServiceInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError> {
        let resolved =
            self.resolver
                .resolve(&instance, &host)
                .await
                .map_err(|error| match error {
                    NativeBridgeResolveError::InvalidConfiguration { reason } => {
                        DriverFactoryError::InvalidConfiguration { reason }
                    }
                    NativeBridgeResolveError::Unavailable { reason, retryable } => {
                        DriverFactoryError::Unavailable { reason, retryable }
                    }
                })?;
        Ok(Arc::new(NativeAgentDriver::new(instance, resolved, host)))
    }
}

pub fn native_agent_contribution(
    resolver: Arc<dyn NativeBridgeResolver>,
) -> AgentRuntimeDriverContribution {
    let profile = native_runtime_profile();
    let config_schema = json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "minLength": 1 },
            "model": { "type": "string", "minLength": 1 },
            "credential_scope": {
                "oneOf": [
                    {
                        "type": "object",
                        "properties": { "kind": { "const": "platform" } },
                        "required": ["kind"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {
                            "kind": { "const": "user" },
                            "user_id": { "type": "string", "minLength": 1 }
                        },
                        "required": ["kind", "user_id"],
                        "additionalProperties": false
                    }
                ]
            }
        },
        "required": ["provider", "model", "credential_scope"],
        "additionalProperties": false
    });
    let schema_digest = digest_json(&config_schema);
    let conversation_projection = native_conversation_projection();
    AgentRuntimeDriverContribution {
        conversation_projection,
        definition: AgentServiceDefinition {
            provenance: AgentServiceProvenance {
                definition_id: AgentServiceDefinitionId::new(DEFINITION_ID)
                    .expect("static native definition id"),
                publisher_integration: "agentdash.first_party".to_string(),
                service_version: env!("CARGO_PKG_VERSION").to_string(),
                build_digest: AgentServiceBuildDigest::new(format!(
                    "sha256:{}",
                    digest_bytes(env!("CARGO_PKG_VERSION").as_bytes())
                ))
                .expect("native build digest"),
            },
            factory_key: AgentRuntimeFactoryKey::new(FACTORY_KEY)
                .expect("static native factory key"),
            supported_protocol_revisions: vec![PROTOCOL_REVISION],
            config_schema,
            config_schema_digest: AgentServiceSchemaDigest::new(format!("sha256:{schema_digest}"))
                .expect("native schema digest"),
            credential_slots: Vec::new(),
            service_profile_upper_bound: profile,
        },
        factory: Arc::new(NativeAgentDriverFactory::new(resolver)),
    }
}

fn native_conversation_projection() -> agentdash_integration_api::DriverConversationProjectionProfile
{
    let mut profile =
        agentdash_integration_api::DriverConversationProjectionProfile::full_fidelity(1);
    profile
        .item_families
        .remove(&agentdash_integration_api::DriverConversationItemFamily::Plan);
    profile
}

pub fn native_runtime_trust_manifest() -> AgentRuntimeTrustManifest {
    // The manifest intentionally derives only from immutable first-party build metadata and the
    // conformance-tested profile, never from a live driver or service instance advertisement.
    let provenance = AgentServiceProvenance {
        definition_id: AgentServiceDefinitionId::new(DEFINITION_ID)
            .expect("static native definition id"),
        publisher_integration: "agentdash.first_party".to_string(),
        service_version: env!("CARGO_PKG_VERSION").to_string(),
        build_digest: AgentServiceBuildDigest::new(format!(
            "sha256:{}",
            digest_bytes(env!("CARGO_PKG_VERSION").as_bytes())
        ))
        .expect("native build digest"),
    };
    AgentRuntimeTrustManifest {
        driver_build_digest: provenance.build_digest.to_string(),
        provenance,
        suite_revision: CONFORMANCE_SUITE.to_string(),
        protocol_revision: PROTOCOL_REVISION,
        verified_profile: native_runtime_profile(),
    }
}

pub fn native_runtime_profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: BTreeSet::from([InputModality::Text, InputModality::Image]),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::from([
                InstructionChannel::System,
                InstructionChannel::Developer,
                InstructionChannel::AdditionalContext,
            ]),
            configuration_boundary: ConfigurationBoundary::TurnStart,
        },
        tools: ToolProfile {
            channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::HotReplace,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::from([
                WorkspaceCapability::Read,
                WorkspaceCapability::Write,
                WorkspaceCapability::Search,
                WorkspaceCapability::MultipleRoots,
                WorkspaceCapability::VirtualFileSystem,
            ]),
            mechanism: DeliveryMechanism::HostAdaptedExact,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::from([RuntimeInteractionKind::PermissionApproval]),
            durable_correlation: true,
        },
        lifecycle: BTreeSet::from([
            LifecycleCapability::ThreadStart,
            LifecycleCapability::ThreadResume,
            LifecycleCapability::ThreadFork,
            LifecycleCapability::ThreadRead,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
            LifecycleCapability::ToolSetReplace,
        ]),
        hooks: HookProfile {
            points: native_hook_capabilities(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: BTreeSet::from([
                ContextCapability::Read,
                ContextCapability::Import,
                ContextCapability::PrepareCompaction,
                ContextCapability::ActivateCheckpoint,
            ]),
            fidelity: ContextFidelity::PlatformExact,
            activation_idempotent: true,
        },
        telemetry_config: BTreeSet::from([
            TelemetryCapability::Usage,
            TelemetryCapability::Reasoning,
            TelemetryCapability::Deltas,
        ]),
    }
}

pub struct NativeAgentDriver {
    service_instance_id: RuntimeServiceInstanceId,
    generation: RuntimeDriverGeneration,
    profile: RuntimeProfile,
    bridge: Arc<dyn LlmBridge>,
    presentation_metadata: NativePresentationMetadata,
    host: RuntimeDriverHostPorts,
    bindings: RwLock<BTreeMap<RuntimeBindingId, Arc<NativeBinding>>>,
    dispatch_receipts: Mutex<BTreeMap<DriverRequestId, DriverDispatchReceipt>>,
    request_locks: Mutex<BTreeMap<DriverRequestId, Arc<Mutex<()>>>>,
}

struct NativeBinding {
    driver_binding_id: DriverBindingId,
    source_thread_id: DriverThreadId,
    intent: DriverBindIntent,
    surface: RwLock<MaterializedDriverSurface>,
    thread: RwLock<Option<Arc<NativeThread>>>,
    applied_candidates:
        RwLock<BTreeMap<ContextCandidateId, (ContextDigest, DriverContextRevision)>>,
    applied_checkpoints: RwLock<BTreeMap<ContextCheckpointId, ContextDigest>>,
    projected_items: RwLock<Vec<DriverProjectedItem>>,
    context_digest: RwLock<Option<String>>,
}

struct NativeThread {
    agent: Mutex<Agent>,
    presentation_context: StreamMapperRuntimeContext,
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    active_presentation_turn: Arc<RwLock<Option<PresentationTurnId>>>,
    context_revision: RwLock<ContextRevision>,
    tool_item_identities: Arc<RwLock<BTreeMap<(DriverTurnId, DriverItemId), RuntimeItemId>>>,
}

impl NativeAgentDriver {
    fn new(
        instance: ActivatedAgentServiceInstance,
        resolved: ResolvedNativeBridge,
        host: RuntimeDriverHostPorts,
    ) -> Self {
        Self {
            service_instance_id: instance.instance_id,
            generation: instance.generation,
            profile: native_runtime_profile(),
            bridge: resolved.bridge,
            presentation_metadata: resolved.presentation,
            host,
            bindings: RwLock::new(BTreeMap::new()),
            dispatch_receipts: Mutex::new(BTreeMap::new()),
            request_locks: Mutex::new(BTreeMap::new()),
        }
    }

    async fn binding(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Arc<NativeBinding>, DriverError> {
        self.bindings
            .read()
            .await
            .get(binding_id)
            .cloned()
            .ok_or_else(|| DriverError::Lost {
                reason: format!("native binding {binding_id} does not exist"),
                retryable: true,
            })
    }

    async fn ensure_thread(
        &self,
        binding_id: &RuntimeBindingId,
        binding: &Arc<NativeBinding>,
        exclude_operation_id: Option<&RuntimeOperationId>,
    ) -> Result<Arc<NativeThread>, DriverError> {
        if let Some(thread) = binding.thread.read().await.clone() {
            return Ok(thread);
        }
        let surface = binding.surface.read().await.clone();
        let active_turn = Arc::new(RwLock::new(None));
        let active_runtime_turn = Arc::new(RwLock::new(None));
        let active_presentation_turn = Arc::new(RwLock::new(None));
        let tool_item_identities = Arc::new(RwLock::new(BTreeMap::new()));
        let presentation_identity = NativeSessionItemIdentity::new();
        let presentation_routes = Arc::new(std::sync::RwLock::new(tool_presentation_routes(
            &surface.tools,
        )));
        presentation_identity.bind_presentation_routes(presentation_routes.clone());
        let transcript = self
            .host
            .context
            .load_transcript(DriverTranscriptRequest {
                binding_id: binding_id.clone(),
                generation: self.generation,
                runtime_thread_id: surface.runtime_thread_id.clone(),
            })
            .await
            .map_err(context_error)?;
        let replay_after = transcript.active_compaction_source_end;
        let projected_transcript =
            project_driver_transcript(&transcript, exclude_operation_id, replay_after)?;
        let identity_projection = if replay_after.is_some() {
            project_driver_transcript(
                &transcript,
                exclude_operation_id,
                Some(EventSequence(
                    transcript.earliest_available.0.saturating_sub(1),
                )),
            )?
        } else {
            projected_transcript.clone()
        };
        let transcript_messages = projected_transcript
            .entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>();
        hydrate_presentation_identity(
            presentation_identity.as_ref(),
            &transcript,
            &identity_projection,
        );
        let mut agent = Agent::new(
            self.bridge.clone(),
            AgentConfig {
                system_prompt: surface_system_prompt(&surface),
                ..Default::default()
            },
        );
        let binding_context = NativeBindingContext {
            binding_id: binding_id.clone(),
            generation: self.generation,
            source_thread_id: binding.source_thread_id.clone(),
            runtime_thread_id: surface.runtime_thread_id.clone(),
            authorization_identity: surface.authorization_identity.clone(),
        };
        agent.set_runtime_delegates(NativeHookDelegate::delegates(
            binding_context.clone(),
            active_turn.clone(),
            active_runtime_turn.clone(),
            surface.hooks.clone(),
            self.host.hooks.clone(),
        ));
        agent.set_tools(native_tools(
            &surface,
            binding_context,
            NativeToolCallContext {
                active_turn: active_turn.clone(),
                active_runtime_turn: active_runtime_turn.clone(),
                active_presentation_turn: active_presentation_turn.clone(),
                tool_set_revision: surface.tools.revision,
                item_identities: tool_item_identities.clone(),
                presentation_identity: presentation_identity.clone(),
            },
            self.host.tools.clone(),
        ));
        let has_active_compaction = surface
            .context
            .blocks
            .iter()
            .any(|block| matches!(block, ContextBlock::CompactionSummary { .. }));
        if has_active_compaction != transcript.active_compaction_source_end.is_some() {
            return Err(DriverError::ProtocolViolation {
                reason: "native materialized context and durable transcript disagree about the active compaction boundary"
                    .to_string(),
                critical: true,
            });
        }
        if let Some(source_end) = transcript.active_compaction_source_end {
            let messages = context_blocks_to_messages(&surface.context.blocks)?;
            let mut message_refs = vec![None; messages.len()];
            let mut messages = messages;
            debug_assert_eq!(Some(source_end), replay_after);
            messages.extend(
                projected_transcript
                    .entries
                    .iter()
                    .map(|entry| entry.message.clone()),
            );
            message_refs.extend(
                projected_transcript
                    .entries
                    .iter()
                    .map(|entry| Some(entry.message_ref.clone())),
            );
            agent
                .replace_messages_with_refs(messages, message_refs)
                .await;
        } else if projected_transcript.is_empty() {
            let messages = context_blocks_to_messages(&surface.context.blocks)?;
            let message_refs = vec![None; messages.len()];
            agent
                .replace_messages_with_refs(messages, message_refs)
                .await;
        } else {
            let message_refs = projected_transcript
                .entries
                .iter()
                .map(|entry| Some(entry.message_ref.clone()))
                .collect();
            agent
                .replace_messages_with_refs(transcript_messages, message_refs)
                .await;
        }
        for block in &surface.context.blocks {
            if let ContextBlock::RuntimeItem { content } = block {
                presentation_identity.observe_tool_result_item_id(content.item().id());
            }
        }
        let thread = Arc::new(NativeThread {
            agent: Mutex::new(agent),
            presentation_context: StreamMapperRuntimeContext {
                model_context_window: Some(self.presentation_metadata.model_context_window),
                reserve_tokens: self.presentation_metadata.reserve_tokens,
                session_identity: presentation_identity,
                fixed_event_timestamp_ms: None,
                tool_presentation_routes: presentation_routes,
            },
            active_turn,
            active_runtime_turn,
            active_presentation_turn,
            context_revision: RwLock::new(ContextRevision(0)),
            tool_item_identities,
        });
        let mut slot = binding.thread.write().await;
        Ok(slot.get_or_insert_with(|| thread.clone()).clone())
    }

    async fn start_turn(
        &self,
        envelope: &DriverCommandEnvelope,
        binding: Arc<NativeBinding>,
        input: Vec<RuntimeInput>,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<(), DriverError> {
        let thread = self
            .ensure_thread(&envelope.binding_id, &binding, Some(&envelope.operation_id))
            .await?;
        let source_turn_id: DriverTurnId =
            parsed_id(format!("native-turn-{}", envelope.request_id))?;
        let runtime_turn_id =
            envelope
                .runtime_turn_id
                .clone()
                .ok_or_else(|| DriverError::ProtocolViolation {
                    reason: "native turn command is missing the Managed Runtime turn identity"
                        .to_string(),
                    critical: true,
                })?;
        let presentation_turn_id = envelope.presentation_turn_id.clone().ok_or_else(|| {
            DriverError::ProtocolViolation {
                reason: "native turn command is missing the Session presentation turn identity"
                    .to_string(),
                critical: true,
            }
        })?;
        *thread.active_turn.write().await = Some(source_turn_id.clone());
        *thread.active_runtime_turn.write().await = Some(runtime_turn_id.clone());
        *thread.active_presentation_turn.write().await = Some(presentation_turn_id.clone());
        let (mut events, handle) = {
            let mut agent = thread.agent.lock().await;
            agent.set_tool_result_ref_context(Some(ToolResultRefContext {
                session_id: envelope.presentation_thread_id.to_string(),
                raw_turn_id: presentation_turn_id.to_string(),
                address_provider: thread.presentation_context.session_identity.clone(),
                cache_writer: None,
            }));
            match agent.prompt(inputs_to_message(input)?) {
                Ok(started) => started,
                Err(error) => {
                    agent.set_tool_result_ref_context(None);
                    *thread.active_turn.write().await = None;
                    *thread.active_runtime_turn.write().await = None;
                    *thread.active_presentation_turn.write().await = None;
                    return Err(DriverError::Rejected {
                        reason: error.to_string(),
                    });
                }
            }
        };
        // `Agent::prompt` returning is the authoritative delivery-acceptance boundary. Business
        // execution continues on the event pump; dispatch returns immediately so outbox ack,
        // steer and interrupt are not serialized behind the whole provider/tool loop.
        let accepted_receipt = DriverDispatchReceipt {
            request_id: envelope.request_id.clone(),
            duplicate: false,
            applied_tool_set: None,
            applied_surface: None,
        };
        self.dispatch_receipts
            .lock()
            .await
            .insert(accepted_receipt.request_id.clone(), accepted_receipt);
        let mut mapper = NativeEventMapper::new_with_presentation_turn(
            envelope.presentation_thread_id.to_string(),
            runtime_turn_id,
            source_turn_id,
            presentation_turn_id,
            thread.presentation_context.clone(),
        );
        let envelope = envelope.clone();
        tokio::spawn(async move {
            let mut pending_terminal = None;
            let stream_result = async {
                while let Some(event) = events.next().await {
                    let terminal = matches!(event, AgentEvent::AgentEnd { .. });
                    for mapped in mapper.map(event)? {
                        for fact in &mapped.facts {
                            if let RuntimeJournalFact::Internal(RuntimeEvent::ItemTerminal {
                                turn_id,
                                terminal: RuntimeItemTerminal::Completed { final_content },
                                ..
                            }) = fact
                                && let Some(source_item_id) = &mapped.source_item_id
                            {
                                binding
                                    .projected_items
                                    .write()
                                    .await
                                    .push(DriverProjectedItem {
                                        source_turn_id: mapped
                                            .source_turn_id
                                            .clone()
                                            .unwrap_or_else(|| {
                                                parsed_id(turn_id.to_string())
                                                    .expect("mapped turn id")
                                            }),
                                        source_item_id: source_item_id.clone(),
                                        content: final_content.clone(),
                                    });
                            }
                        }
                        if mapped_is_turn_terminal(&mapped) {
                            if pending_terminal.replace(mapped).is_some() {
                                return Err(DriverError::ProtocolViolation {
                                    reason: "native Agent Core emitted duplicate turn terminals"
                                        .to_string(),
                                    critical: true,
                                });
                            }
                        } else {
                            emit_driver_event(
                                &envelope,
                                &binding.source_thread_id,
                                mapped,
                                sink.clone(),
                            )
                            .await?;
                        }
                    }
                    if terminal {
                        break;
                    }
                }
                Ok::<(), DriverError>(())
            }
            .await;
            let mut result = if let Err(error) = stream_result {
                thread.agent.lock().await.abort();
                let _ = handle.await;
                Err(error)
            } else {
                let terminal_emitted = mapper.turn_terminal;
                match handle.await {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(AgentError::Cancelled)) if !terminal_emitted => {
                        async {
                            for mapped in mapper.cancelled() {
                                if mapped_is_turn_terminal(&mapped) {
                                    if pending_terminal.replace(mapped).is_some() {
                                        return Err(DriverError::ProtocolViolation {
                                            reason:
                                                "native Agent Core emitted duplicate turn terminals"
                                                    .to_string(),
                                            critical: true,
                                        });
                                    }
                                } else {
                                    emit_driver_event(
                                        &envelope,
                                        &binding.source_thread_id,
                                        mapped,
                                        sink.clone(),
                                    )
                                    .await?;
                                }
                            }
                            Ok(())
                        }
                        .await
                    }
                    Ok(Err(_)) | Err(_) if terminal_emitted => Ok(()),
                    Ok(Err(error)) => Err(DriverError::Rejected {
                        reason: error.to_string(),
                    }),
                    Err(error) => Err(DriverError::Lost {
                        reason: format!("native Agent Core task join failed: {error}"),
                        retryable: false,
                    }),
                }
            };
            *thread.active_turn.write().await = None;
            *thread.active_runtime_turn.write().await = None;
            *thread.active_presentation_turn.write().await = None;
            thread.agent.lock().await.set_tool_result_ref_context(None);
            // Runtime may admit the next turn as soon as it observes this
            // terminal. Publish it only after Agent Core and every Native
            // active fence have become idle.
            if result.is_ok()
                && let Some(terminal) = pending_terminal
            {
                result =
                    emit_driver_event(&envelope, &binding.source_thread_id, terminal, sink.clone())
                        .await;
            }
            if let Err(error) = result {
                if matches!(error, DriverError::Terminalized { .. }) {
                    diag!(
                        Warn,
                        Subsystem::AgentRun,
                        operation_id = %envelope.operation_id,
                        error = %error,
                        "Managed Runtime已终结Native accepted turn，停止event pump"
                    );
                    return;
                }
                let reason = format!("native accepted turn event pump failed: {error}");
                diag!(
                    Error,
                    Subsystem::AgentRun,
                    operation_id = %envelope.operation_id,
                    error = %error,
                    "Native accepted turn event pump失败，终结Runtime binding"
                );
                if let Err(terminal_error) = emit_driver_event(
                    &envelope,
                    &binding.source_thread_id,
                    RuntimeEvent::BindingLost {
                        binding_id: envelope.binding_id.clone(),
                        reason,
                    },
                    sink,
                )
                .await
                {
                    diag!(
                        Error,
                        Subsystem::AgentRun,
                        operation_id = %envelope.operation_id,
                        error = %terminal_error,
                        "Native accepted turn终态写入失败"
                    );
                }
            }
        });
        Ok(())
    }
}

#[async_trait]
impl AgentRuntimeDriver for NativeAgentDriver {
    async fn describe(
        &self,
        request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError> {
        if request.service_instance_id != self.service_instance_id {
            return Err(DriverError::Rejected {
                reason: "describe targeted a different native service instance".to_string(),
            });
        }
        Ok(RuntimeDescriptor {
            protocol_revision: PROTOCOL_REVISION,
            service_instance_id: self.service_instance_id.clone(),
            profile: self.profile.clone(),
            profile_digest: profile_digest(&self.profile)?,
        })
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        if request.service_instance_id != self.service_instance_id {
            return Err(DriverError::Rejected {
                reason: "bind targeted a different native service instance".to_string(),
            });
        }
        if let Some(existing) = self.bindings.read().await.get(&request.binding_id).cloned() {
            let surface = existing.surface.read().await;
            if surface.revision != request.surface_revision
                || surface.digest != request.surface_digest
                || existing.intent != request.intent
            {
                return Err(DriverError::ProtocolViolation {
                    reason: "native binding id was reused with a different surface".to_string(),
                    critical: true,
                });
            }
            return Ok(binding_receipt(&existing, &surface));
        }
        let surface = self
            .host
            .surfaces
            .materialize(DriverSurfaceRequest {
                binding_id: request.binding_id.clone(),
                surface_revision: request.surface_revision,
                surface_digest: request.surface_digest.clone(),
            })
            .await
            .map_err(surface_error)?;
        if surface.revision != request.surface_revision || surface.digest != request.surface_digest
        {
            return Err(DriverError::ProtocolViolation {
                reason: "surface broker returned a different native materialization".to_string(),
                critical: true,
            });
        }
        let source_thread_id = match &request.intent {
            DriverBindIntent::Start => parsed_id(format!("native-thread-{}", request.binding_id))?,
            DriverBindIntent::Resume { source_thread_id } => source_thread_id.clone(),
            DriverBindIntent::Fork { .. } => {
                parsed_id(format!("native-thread-{}", request.binding_id))?
            }
        };
        let initial_context_digest = Some(surface.context.digest.to_string());
        let binding = Arc::new(NativeBinding {
            driver_binding_id: parsed_id(format!("native-binding-{}", request.binding_id))?,
            source_thread_id,
            intent: request.intent,
            surface: RwLock::new(surface),
            thread: RwLock::new(None),
            applied_candidates: RwLock::new(BTreeMap::new()),
            applied_checkpoints: RwLock::new(BTreeMap::new()),
            projected_items: RwLock::new(Vec::new()),
            context_digest: RwLock::new(initial_context_digest),
        });
        let receipt = {
            let applied = binding.surface.read().await;
            binding_receipt(&binding, &applied)
        };
        self.bindings
            .write()
            .await
            .insert(request.binding_id, binding);
        Ok(receipt)
    }

    async fn dispatch(
        &self,
        envelope: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        if envelope.generation != self.generation {
            return Err(DriverError::StaleGeneration);
        }
        let binding = self.binding(&envelope.binding_id).await?;
        if envelope.source_thread_id != binding.source_thread_id {
            return Err(DriverError::ProtocolViolation {
                reason: "native command source thread does not match its binding".to_string(),
                critical: true,
            });
        }
        if matches!(
            &envelope.command,
            RuntimeCommand::InteractionRespond {
                response: InteractionResponse::UserInput { .. }
                    | InteractionResponse::DynamicToolResult { .. }
                    | InteractionResponse::McpElicitation { .. },
                ..
            }
        ) {
            return Err(DriverError::Unsupported {
                reason: "native interaction response kind is not declared".to_string(),
            });
        }
        if command_inputs(&envelope.command).is_some_and(|inputs| {
            inputs.iter().any(|input| {
                matches!(
                    input,
                    RuntimeInput::FileReference { .. } | RuntimeInput::Structured { .. }
                )
            })
        }) {
            return Err(DriverError::Unsupported {
                reason: "native Agent Core accepts text and image input only".to_string(),
            });
        }
        let request_lock = {
            let mut locks = self.request_locks.lock().await;
            locks
                .entry(envelope.request_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _request_guard = request_lock.lock().await;
        if let Some(existing) = self
            .dispatch_receipts
            .lock()
            .await
            .get(&envelope.request_id)
        {
            let mut duplicate = existing.clone();
            duplicate.duplicate = true;
            return Ok(duplicate);
        }
        let mut applied_tool_set = None;
        match envelope.command.clone() {
            RuntimeCommand::ThreadStart { input, surface, .. } => {
                if binding.surface.read().await.digest != surface.surface_digest {
                    return Err(DriverError::Rejected {
                        reason: "thread start surface digest is stale".to_string(),
                    });
                }
                self.ensure_thread(&envelope.binding_id, &binding, Some(&envelope.operation_id))
                    .await?;
                emit_driver_event(
                    &envelope,
                    &binding.source_thread_id,
                    RuntimeEvent::ThreadStatusChanged {
                        status: RuntimeThreadStatus::Active,
                    },
                    sink.clone(),
                )
                .await?;
                self.start_turn(&envelope, binding, input, sink.clone())
                    .await?;
            }
            RuntimeCommand::TurnStart { input, .. } => {
                self.start_turn(&envelope, binding, input, sink.clone())
                    .await?;
            }
            RuntimeCommand::TurnSteer {
                expected_turn_id,
                input,
                ..
            } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, Some(&envelope.operation_id))
                    .await?;
                validate_active_turn(&thread, &expected_turn_id).await?;
                thread
                    .agent
                    .lock()
                    .await
                    .steer(inputs_to_message(input)?)
                    .await;
            }
            RuntimeCommand::TurnInterrupt {
                expected_turn_id, ..
            } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                validate_active_turn(&thread, &expected_turn_id).await?;
                thread.agent.lock().await.abort();
            }
            RuntimeCommand::ThreadResume { .. } => {
                self.ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                emit_driver_event(
                    &envelope,
                    &binding.source_thread_id,
                    RuntimeEvent::ThreadStatusChanged {
                        status: RuntimeThreadStatus::Active,
                    },
                    sink,
                )
                .await?;
            }
            RuntimeCommand::ThreadRebind { .. } => {
                return Err(DriverError::Unsupported {
                    reason: "ThreadRebind is a Managed Runtime transition and cannot be dispatched to a driver".to_string(),
                });
            }
            RuntimeCommand::ThreadFork { checkpoint_id, .. } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                if let Some(checkpoint_id) = checkpoint_id {
                    let requested_checkpoint_id = checkpoint_id.clone();
                    let activation = self
                        .host
                        .context
                        .load_checkpoint(DriverContextCheckpointRequest {
                            binding_id: envelope.binding_id.clone(),
                            generation: envelope.generation,
                            checkpoint_id,
                        })
                        .await
                        .map_err(context_error)?;
                    if activation.checkpoint_id != requested_checkpoint_id {
                        return Err(DriverError::ProtocolViolation {
                            reason: "context broker returned a different fork checkpoint"
                                .to_string(),
                            critical: true,
                        });
                    }
                    if let Some(digest) = binding
                        .applied_checkpoints
                        .read()
                        .await
                        .get(&activation.checkpoint_id)
                        && digest != &activation.materialized.digest
                    {
                        return Err(DriverError::ProtocolViolation {
                            reason: "native fork checkpoint was reused with conflicting content"
                                .to_string(),
                            critical: true,
                        });
                    }
                    thread
                        .agent
                        .lock()
                        .await
                        .replace_messages(context_blocks_to_messages(
                            &activation.materialized.blocks,
                        )?)
                        .await;
                    *thread.context_revision.write().await = activation.context_revision;
                    let activated_digest = activation.materialized.digest.to_string();
                    binding
                        .applied_checkpoints
                        .write()
                        .await
                        .insert(activation.checkpoint_id, activation.materialized.digest);
                    *binding.context_digest.write().await = Some(activated_digest);
                }
            }
            RuntimeCommand::ThreadSettingsUpdate { instructions, .. } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                thread
                    .agent
                    .lock()
                    .await
                    .set_system_prompt(instructions.join("\n\n"));
            }
            RuntimeCommand::ToolSetReplace {
                expected_current_tool_set_revision,
                target_tool_set_revision,
                tool_set_digest,
                ..
            } => {
                if binding.surface.read().await.tools.revision != expected_current_tool_set_revision
                {
                    return Err(DriverError::Rejected {
                        reason: "native tool set changed before hot replacement".to_string(),
                    });
                }
                let tools = self
                    .host
                    .surfaces
                    .materialize_tool_set(
                        envelope.binding_id.clone(),
                        target_tool_set_revision,
                        &tool_set_digest,
                    )
                    .await
                    .map_err(surface_error)?;
                if tools.revision != target_tool_set_revision || tools.digest != tool_set_digest {
                    return Err(DriverError::ProtocolViolation {
                        reason: "tool broker returned a different hot-replace revision".to_string(),
                        critical: true,
                    });
                }
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                let updated_surface = {
                    let mut surface = binding.surface.write().await;
                    surface.tools = tools;
                    surface.clone()
                };
                thread.agent.lock().await.set_tools(native_tools(
                    &updated_surface,
                    NativeBindingContext {
                        binding_id: envelope.binding_id.clone(),
                        generation: envelope.generation,
                        source_thread_id: binding.source_thread_id.clone(),
                        runtime_thread_id: updated_surface.runtime_thread_id.clone(),
                        authorization_identity: updated_surface.authorization_identity.clone(),
                    },
                    NativeToolCallContext {
                        active_turn: thread.active_turn.clone(),
                        active_runtime_turn: thread.active_runtime_turn.clone(),
                        active_presentation_turn: thread.active_presentation_turn.clone(),
                        tool_set_revision: updated_surface.tools.revision,
                        item_identities: thread.tool_item_identities.clone(),
                        presentation_identity: thread.presentation_context.session_identity.clone(),
                    },
                    self.host.tools.clone(),
                ));
                *thread
                    .presentation_context
                    .tool_presentation_routes
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    tool_presentation_routes(&updated_surface.tools);
                applied_tool_set = Some(DriverToolSetApplyReceipt {
                    revision: target_tool_set_revision,
                    digest: tool_set_digest,
                });
            }
            RuntimeCommand::SurfaceAdopt { .. } => {
                return Err(DriverError::Unsupported {
                    reason: "Native full Runtime surface adoption is not connected".to_string(),
                });
            }
            RuntimeCommand::ContextCompact {
                compaction_id,
                expected_context_revision,
                ..
            } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                if *thread.context_revision.read().await != expected_context_revision {
                    return Err(DriverError::Rejected {
                        reason: "native context revision changed before activation".to_string(),
                    });
                }
                let activation = self
                    .host
                    .context
                    .compaction_activation(DriverCompactionActivationRequest {
                        binding_id: envelope.binding_id.clone(),
                        generation: envelope.generation,
                        compaction_id,
                    })
                    .await
                    .map_err(context_error)?;
                let driver_revision = parsed_id(format!(
                    "native-context-revision-{}",
                    activation.context_revision.0
                ))?;
                if let Some((digest, revision)) = binding
                    .applied_candidates
                    .read()
                    .await
                    .get(&activation.candidate_id)
                    && (digest != &activation.materialized.digest || revision != &driver_revision)
                {
                    return Err(DriverError::ProtocolViolation {
                        reason: "native compaction candidate was reused with conflicting content"
                            .to_string(),
                        critical: true,
                    });
                }
                if let Some(digest) = binding
                    .applied_checkpoints
                    .read()
                    .await
                    .get(&activation.checkpoint_id)
                    && digest != &activation.materialized.digest
                {
                    return Err(DriverError::ProtocolViolation {
                        reason: "native checkpoint was reused with a conflicting digest"
                            .to_string(),
                        critical: true,
                    });
                }
                thread
                    .agent
                    .lock()
                    .await
                    .replace_messages(context_blocks_to_messages(&activation.materialized.blocks)?)
                    .await;
                *thread.context_revision.write().await = activation.context_revision;
                binding.applied_candidates.write().await.insert(
                    activation.candidate_id,
                    (activation.materialized.digest.clone(), driver_revision),
                );
                let activated_digest = activation.materialized.digest.to_string();
                binding
                    .applied_checkpoints
                    .write()
                    .await
                    .insert(activation.checkpoint_id, activation.materialized.digest);
                *binding.context_digest.write().await = Some(activated_digest);
            }
            RuntimeCommand::InteractionRespond {
                interaction_id,
                response,
                ..
            } => {
                let thread = self
                    .ensure_thread(&envelope.binding_id, &binding, None)
                    .await?;
                match response {
                    InteractionResponse::Approved => {
                        thread
                            .agent
                            .lock()
                            .await
                            .approve_tool_call(interaction_id.as_str())
                            .await
                    }
                    InteractionResponse::Denied { reason } => {
                        thread
                            .agent
                            .lock()
                            .await
                            .reject_tool_call(interaction_id.as_str(), reason)
                            .await
                    }
                    InteractionResponse::UserInput { .. }
                    | InteractionResponse::DynamicToolResult { .. }
                    | InteractionResponse::McpElicitation { .. } => unreachable!(
                        "unsupported interaction responses are rejected before acceptance"
                    ),
                }
                .map_err(|error| DriverError::Rejected {
                    reason: error.to_string(),
                })?;
            }
        }
        let receipt = DriverDispatchReceipt {
            request_id: envelope.request_id,
            duplicate: false,
            applied_tool_set,
            applied_surface: None,
        };
        self.dispatch_receipts
            .lock()
            .await
            .insert(receipt.request_id.clone(), receipt.clone());
        Ok(receipt)
    }

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
        match query {
            DriverInspectionQuery::Binding { driver_binding_id } => Ok(DriverInspection::Binding {
                active: self
                    .bindings
                    .read()
                    .await
                    .values()
                    .any(|binding| binding.driver_binding_id == driver_binding_id),
            }),
            DriverInspectionQuery::CompactionActivation { candidate_id } => {
                for binding in self.bindings.read().await.values() {
                    if let Some((digest, revision)) =
                        binding.applied_candidates.read().await.get(&candidate_id)
                    {
                        return Ok(DriverInspection::CompactionActivation {
                            applied: true,
                            digest: Some(digest.to_string()),
                            driver_context_revision: Some(revision.clone()),
                        });
                    }
                }
                Ok(DriverInspection::CompactionActivation {
                    applied: false,
                    digest: None,
                    driver_context_revision: None,
                })
            }
            DriverInspectionQuery::Checkpoint { checkpoint_id } => {
                for binding in self.bindings.read().await.values() {
                    if let Some(digest) =
                        binding.applied_checkpoints.read().await.get(&checkpoint_id)
                    {
                        return Ok(DriverInspection::Checkpoint {
                            available: true,
                            digest: Some(digest.to_string()),
                        });
                    }
                }
                Ok(DriverInspection::Checkpoint {
                    available: false,
                    digest: None,
                })
            }
            DriverInspectionQuery::ThreadProjection { source_thread_id } => {
                let binding = self
                    .bindings
                    .read()
                    .await
                    .values()
                    .find(|binding| binding.source_thread_id == source_thread_id)
                    .cloned();
                let Some(binding) = binding else {
                    return Err(DriverError::Rejected {
                        reason: "native source thread does not exist".to_string(),
                    });
                };
                let items = binding.projected_items.read().await.clone();
                Ok(DriverInspection::ThreadProjection {
                    source_thread_id,
                    items,
                    fidelity: ContextFidelity::EventProjected,
                })
            }
            DriverInspectionQuery::ContextRead { source_thread_id } => {
                let binding = self
                    .bindings
                    .read()
                    .await
                    .values()
                    .find(|binding| binding.source_thread_id == source_thread_id)
                    .cloned();
                let Some(binding) = binding else {
                    return Err(DriverError::Rejected {
                        reason: "native source thread does not exist".to_string(),
                    });
                };
                let digest = binding.context_digest.read().await.clone();
                Ok(DriverInspection::ContextRead {
                    source_thread_id,
                    fidelity: ContextFidelity::PlatformExact,
                    digest,
                })
            }
        }
    }
}

fn binding_receipt(binding: &NativeBinding, surface: &MaterializedDriverSurface) -> DriverBinding {
    DriverBinding {
        driver_binding_id: binding.driver_binding_id.clone(),
        source_thread_id: binding.source_thread_id.clone(),
        applied_surface_revision: surface.revision,
        applied_surface_digest: surface.digest.clone(),
        applied_tool_set_revision: surface.tools.revision,
        applied_tool_set_digest: surface.tools.digest.clone(),
        applied_hook_plan_revision: Some(surface.hooks.revision),
        applied_hook_plan_digest: Some(surface.hooks.digest.clone()),
        applied_hooks: surface
            .hooks
            .bindings
            .iter()
            .map(|binding| DriverHookApplyStatus {
                point: binding.point,
                acknowledged: supported_hook(binding),
                artifact_digest: supported_hook(binding)
                    .then(|| surface.hooks.artifact_digest.clone())
                    .flatten(),
            })
            .collect(),
    }
}

fn tool_presentation_routes(
    surface: &DriverToolSurface,
) -> HashMap<String, NativeToolPresentationRoute> {
    surface
        .tools
        .iter()
        .map(|tool| {
            (
                tool.name.clone(),
                NativeToolPresentationRoute {
                    projection: tool.protocol_projection.clone(),
                    emitter: tool.presentation_emitter,
                },
            )
        })
        .collect()
}

fn command_inputs(command: &RuntimeCommand) -> Option<&[RuntimeInput]> {
    match command {
        RuntimeCommand::ThreadStart { input, .. } | RuntimeCommand::TurnStart { input, .. } => {
            Some(input)
        }
        _ => None,
    }
}

pub(crate) fn native_hook_capabilities() -> Vec<HookPointCapability> {
    let exact = |point, actions, failure_policies| HookPointCapability {
        point,
        actions,
        strength: SemanticStrength::ExactSynchronous,
        mechanism: DeliveryMechanism::HostAdaptedExact,
        failure_policies,
        acknowledged: true,
    };
    let fail_closed = || BTreeSet::from([HookFailurePolicy::FailClosed]);
    vec![
        exact(
            HookPoint::BeforeProviderRequest,
            BTreeSet::from([HookAction::Observe]),
            fail_closed(),
        ),
        exact(
            HookPoint::BeforeTool,
            BTreeSet::from([
                HookAction::Observe,
                HookAction::Block,
                HookAction::RewriteInput,
                HookAction::RequestApproval,
            ]),
            fail_closed(),
        ),
        exact(
            HookPoint::AfterTool,
            BTreeSet::from([
                HookAction::Observe,
                HookAction::RewriteResult,
                HookAction::EmitEffect,
            ]),
            BTreeSet::from([
                HookFailurePolicy::FailClosed,
                HookFailurePolicy::FailOpenWithDiagnostic,
            ]),
        ),
        exact(
            HookPoint::AfterTurn,
            BTreeSet::from([HookAction::Observe, HookAction::ContinueTurn]),
            fail_closed(),
        ),
        exact(
            HookPoint::BeforeStop,
            BTreeSet::from([HookAction::Observe, HookAction::ContinueTurn]),
            fail_closed(),
        ),
    ]
}

fn native_tools(
    surface: &MaterializedDriverSurface,
    binding: NativeBindingContext,
    call: NativeToolCallContext,
    callback: Arc<dyn AgentRuntimeToolCallback>,
) -> Vec<DynAgentTool> {
    surface
        .tools
        .tools
        .iter()
        .filter(|tool| tool.channels.contains(&ToolChannel::DirectCallback))
        .cloned()
        .map(|tool| {
            Arc::new(NativeRuntimeTool::new(
                tool,
                binding.clone(),
                call.clone(),
                callback.clone(),
            )) as DynAgentTool
        })
        .collect()
}

async fn validate_active_turn(
    thread: &NativeThread,
    expected_turn_id: &RuntimeTurnId,
) -> Result<(), DriverError> {
    if thread.active_runtime_turn.read().await.as_ref() == Some(expected_turn_id) {
        Ok(())
    } else {
        Err(DriverError::Rejected {
            reason: "native expected turn does not match the active turn".to_string(),
        })
    }
}

fn surface_system_prompt(surface: &MaterializedDriverSurface) -> String {
    surface
        .context
        .instructions
        .iter()
        .flat_map(|set| set.entries.iter())
        .chain(
            surface
                .context
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContextBlock::Instruction { text } => Some(text),
                    _ => None,
                }),
        )
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n")
}

struct NativeEventMapper {
    session_id: String,
    runtime_turn_id: RuntimeTurnId,
    source_turn_id: DriverTurnId,
    presentation_turn_id: PresentationTurnId,
    next_item: u64,
    current_item: Option<(RuntimeItemId, DriverItemId)>,
    turn_started: bool,
    turn_terminal: bool,
    presentation_entry_index: u32,
    chunk_emit_states: HashMap<String, ChunkEmitState>,
    tool_call_states: HashMap<String, ToolCallEmitState>,
    presentation_context: StreamMapperRuntimeContext,
}

fn assistant_has_canonical_content(message: &agentdash_agent::AgentMessage) -> bool {
    matches!(message, agentdash_agent::AgentMessage::Assistant { content, .. } if content.iter().any(|part| match part {
        agentdash_agent::ContentPart::Text { text }
        | agentdash_agent::ContentPart::Reasoning { text, .. } => !text.is_empty(),
        agentdash_agent::ContentPart::Image { .. } => false,
    }))
}

impl NativeEventMapper {
    #[cfg(test)]
    fn new(
        session_id: String,
        runtime_turn_id: RuntimeTurnId,
        source_turn_id: DriverTurnId,
        presentation_context: StreamMapperRuntimeContext,
    ) -> Self {
        let presentation_turn_id = PresentationTurnId::new(runtime_turn_id.to_string())
            .expect("validated Native test presentation turn identity");
        Self::new_with_presentation_turn(
            session_id,
            runtime_turn_id,
            source_turn_id,
            presentation_turn_id,
            presentation_context,
        )
    }

    fn new_with_presentation_turn(
        session_id: String,
        runtime_turn_id: RuntimeTurnId,
        source_turn_id: DriverTurnId,
        presentation_turn_id: PresentationTurnId,
        presentation_context: StreamMapperRuntimeContext,
    ) -> Self {
        Self {
            session_id,
            runtime_turn_id,
            source_turn_id,
            presentation_turn_id,
            next_item: 0,
            current_item: None,
            turn_started: false,
            turn_terminal: false,
            presentation_entry_index: 0,
            chunk_emit_states: HashMap::new(),
            tool_call_states: HashMap::new(),
            presentation_context,
        }
    }

    fn map(&mut self, event: AgentEvent) -> Result<Vec<MappedEvent>, DriverError> {
        let presentation_source_item_id = match &event {
            AgentEvent::MessageUpdate {
                event:
                    AssistantStreamEvent::ToolCallStart { tool_call_id, .. }
                    | AssistantStreamEvent::ToolCallDelta { tool_call_id, .. },
                ..
            }
            | AgentEvent::ToolExecutionStart { tool_call_id, .. }
            | AgentEvent::ToolExecutionUpdate { tool_call_id, .. }
            | AgentEvent::ToolExecutionPendingApproval { tool_call_id, .. }
            | AgentEvent::ToolExecutionApprovalResolved { tool_call_id, .. }
            | AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                Some(parsed_id(tool_call_id.clone())?)
            }
            AgentEvent::AgentStart
            | AgentEvent::AgentEnd { .. }
            | AgentEvent::TurnStart
            | AgentEvent::TurnEnd { .. }
            | AgentEvent::MessageStart { .. }
            | AgentEvent::MessageUpdate { .. }
            | AgentEvent::MessageEnd { .. }
            | AgentEvent::ContextCompactionStarted { .. }
            | AgentEvent::ContextCompactionNoop { .. }
            | AgentEvent::ContextCompacted { .. }
            | AgentEvent::ContextCompactionFailed { .. }
            | AgentEvent::ProviderAttemptStatus { .. }
            | AgentEvent::RunError { .. } => None,
        };
        let presentation_source_request_id = match &event {
            AgentEvent::ToolExecutionPendingApproval { tool_call_id, .. }
            | AgentEvent::ToolExecutionApprovalResolved { tool_call_id, .. } => {
                Some(tool_call_id.clone())
            }
            _ => None,
        };
        let source = agentdash_agent_protocol::SourceInfo {
            connector_id: FACTORY_KEY.to_string(),
            connector_type: "pi_agent".to_string(),
            executor_id: None,
        };
        let presentation = match convert_event_to_envelopes_with_runtime_context(
            &event,
            &self.session_id,
            &source,
            self.presentation_turn_id.as_str(),
            StreamMapperEventState {
                entry_index: &mut self.presentation_entry_index,
                chunk_emit_states: &mut self.chunk_emit_states,
                tool_call_states: &mut self.tool_call_states,
            },
            self.presentation_context.clone(),
        ) {
            Ok(presentation) => presentation,
            Err(
                error @ crate::presentation::NativePresentationError::InvalidToolProjection {
                    family: "presentation_route",
                    ..
                },
            ) => {
                return Err(DriverError::ProtocolViolation {
                    reason: error.to_string(),
                    critical: true,
                });
            }
            Err(error) => {
                diag!(
                    Warn,
                    Subsystem::AgentRun,
                    runtime_turn_id = %self.runtime_turn_id,
                    source_turn_id = %self.source_turn_id,
                    error = %error,
                    "Native presentation projection失败，canonical Agent loop继续"
                );
                Vec::new()
            }
        };
        let mut mapped = Vec::new();
        match event {
            AgentEvent::AgentStart | AgentEvent::TurnStart if !self.turn_started => {
                self.turn_started = true;
                mapped.push(self.event(RuntimeEvent::TurnStarted {
                    turn_id: self.runtime_turn_id.clone(),
                    presentation_turn_id: self.presentation_turn_id.clone(),
                }));
            }
            AgentEvent::AgentStart | AgentEvent::TurnStart => {}
            AgentEvent::MessageStart { message } => {
                if !matches!(&message, agentdash_agent::AgentMessage::Assistant { .. })
                    || !assistant_has_canonical_content(&message)
                {
                    self.current_item = None;
                    return Ok(mapped);
                }
                let item = self.next_item()?;
                self.current_item = Some(item.clone());
                mapped.push(self.item_event(
                    &item,
                    RuntimeEvent::ItemStarted {
                        turn_id: self.runtime_turn_id.clone(),
                        item_id: item.0.clone(),
                        initial_content: RuntimeItemContent::agent_message(
                            item.0.as_str(),
                            String::new(),
                        ),
                    },
                ));
            }
            AgentEvent::MessageUpdate { .. } => {}
            AgentEvent::MessageEnd { message } => {
                if let agentdash_agent::AgentMessage::Assistant {
                    usage: Some(usage), ..
                } = &message
                    && usage
                        .input
                        .saturating_add(usage.cache_read_input)
                        .saturating_add(usage.cache_creation_input)
                        .saturating_add(usage.output)
                        > 0
                {
                    mapped.push(
                        self.event(RuntimeEvent::TokenUsageUpdated {
                            turn_id: self.runtime_turn_id.clone(),
                            usage: agentdash_agent_runtime_contract::RuntimeTokenUsage {
                                input_tokens: usage.input,
                                cached_input_tokens: usage
                                    .cache_read_input
                                    .saturating_add(usage.cache_creation_input),
                                output_tokens: usage.output,
                                reasoning_output_tokens: 0,
                                total_tokens: usage
                                    .input
                                    .saturating_add(usage.cache_read_input)
                                    .saturating_add(usage.cache_creation_input)
                                    .saturating_add(usage.output),
                            },
                        }),
                    );
                }
                if !matches!(&message, agentdash_agent::AgentMessage::Assistant { .. })
                    || !assistant_has_canonical_content(&message)
                {
                    self.current_item = None;
                } else {
                    let item = match self.current_item.take() {
                        Some(item) => item,
                        None => {
                            let item = self.next_item()?;
                            mapped.push(self.item_event(
                                &item,
                                RuntimeEvent::ItemStarted {
                                    turn_id: self.runtime_turn_id.clone(),
                                    item_id: item.0.clone(),
                                    initial_content: RuntimeItemContent::agent_message(
                                        item.0.as_str(),
                                        String::new(),
                                    ),
                                },
                            ));
                            item
                        }
                    };
                    mapped.push(self.item_event(
                        &item,
                        RuntimeEvent::ItemTerminal {
                            turn_id: self.runtime_turn_id.clone(),
                            item_id: item.0.clone(),
                            terminal: RuntimeItemTerminal::Completed {
                                final_content: message_content(&message, item.0.as_str())?,
                            },
                        },
                    ));
                }
            }
            AgentEvent::ToolExecutionStart { .. } => {}
            AgentEvent::ToolExecutionEnd { .. } => {}
            AgentEvent::TurnEnd { .. } => {}
            AgentEvent::AgentEnd { .. } if !self.turn_terminal => {
                self.turn_terminal = true;
                mapped.push(self.event(RuntimeEvent::TurnTerminal {
                    turn_id: self.runtime_turn_id.clone(),
                    terminal: RuntimeTurnTerminal::Completed,
                    message: None,
                    diagnostic: None,
                }));
            }
            AgentEvent::RunError { error } if !self.turn_terminal => {
                self.turn_terminal = true;
                mapped.push(
                    self.event(RuntimeEvent::ConversationError {
                        turn_id: Some(self.runtime_turn_id.clone()),
                        error: agentdash_agent_runtime_contract::RuntimeConversationError {
                            code: error.code.clone(),
                            message: error.message.clone(),
                            retryable: error.retryable,
                            details: Some(
                                agentdash_agent_runtime_contract::RuntimeConversationErrorDetails {
                                    error_type: Some(format!("{:?}", error.kind)),
                                    http_status: error.http_status,
                                    request_id: None,
                                    metadata: [
                                        (
                                            "provider".to_string(),
                                            error.provider.clone().unwrap_or_default(),
                                        ),
                                        (
                                            "model".to_string(),
                                            error.model.clone().unwrap_or_default(),
                                        ),
                                    ]
                                    .into(),
                                },
                            ),
                        },
                    }),
                );
                mapped.push(self.event(RuntimeEvent::TurnTerminal {
                    turn_id: self.runtime_turn_id.clone(),
                    terminal: RuntimeTurnTerminal::Failed,
                    message: Some(error.to_string()),
                    diagnostic: Some(run_error_terminal_diagnostic(&error)),
                }));
            }
            AgentEvent::RunError { .. } | AgentEvent::AgentEnd { .. } => {}
            AgentEvent::ContextCompactionStarted { .. }
            | AgentEvent::ContextCompactionNoop { .. }
            | AgentEvent::ContextCompacted { .. }
            | AgentEvent::ContextCompactionFailed { .. } => {}
            AgentEvent::ToolExecutionPendingApproval {
                tool_call_id,
                reason,
                ..
            } => {
                let runtime_item_id: RuntimeItemId = parsed_id(tool_call_id.clone())?;
                let source_item_id: DriverItemId = parsed_id(tool_call_id.clone())?;
                mapped.push(self.item_event(
                    &(runtime_item_id.clone(), source_item_id),
                    RuntimeEvent::InteractionRequested {
                        turn_id: self.runtime_turn_id.clone(),
                        item_id: Some(runtime_item_id.clone()),
                        interaction_id: parsed_id(tool_call_id)?,
                        request: agentdash_agent_runtime_contract::RuntimeInteractionRequest::temporary_permission_approval(
                            "native-thread", self.runtime_turn_id.as_str(),
                            runtime_item_id.as_str(), reason,
                        ),
                    },
                ));
            }
            AgentEvent::ToolExecutionApprovalResolved {
                tool_call_id,
                approved,
                reason,
                ..
            } => {
                let runtime_item_id: RuntimeItemId = parsed_id(tool_call_id.clone())?;
                let source_item_id: DriverItemId = parsed_id(tool_call_id.clone())?;
                mapped.push(self.item_event(
                    &(runtime_item_id.clone(), source_item_id.clone()),
                    RuntimeEvent::InteractionTerminal {
                        turn_id: self.runtime_turn_id.clone(),
                        interaction_id: parsed_id(tool_call_id)?,
                        terminal: RuntimeInteractionTerminal::Resolved,
                    },
                ));
                if !approved {
                    mapped.push(self.item_event(
                        &(runtime_item_id.clone(), source_item_id),
                        RuntimeEvent::ItemTerminal {
                            turn_id: self.runtime_turn_id.clone(),
                            item_id: runtime_item_id,
                            terminal: RuntimeItemTerminal::Failed { message: reason },
                        },
                    ));
                }
            }
            AgentEvent::ProviderAttemptStatus { .. } => {}
            AgentEvent::ToolExecutionUpdate { .. } => {}
        }
        let source_turn_id = mapped
            .iter()
            .find_map(|mapped| mapped.source_turn_id.clone())
            .or_else(|| Some(self.source_turn_id.clone()));
        let source_item_id = mapped
            .iter()
            .rev()
            .find_map(|mapped| mapped.source_item_id.clone())
            .or(presentation_source_item_id);
        let facts = mapped
            .into_iter()
            .flat_map(|mapped| mapped.facts)
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(usize::from(!facts.is_empty()) + presentation.len());
        if !facts.is_empty() {
            output.push(MappedEvent {
                source_turn_id: source_turn_id.clone(),
                source_item_id: source_item_id.clone(),
                source_request_id: presentation_source_request_id.clone(),
                source_entry_index: None,
                facts,
            });
        }
        output.extend(presentation.into_iter().map(|envelope| {
            let source_entry_index = envelope.trace.entry_index;
            let durability = presentation_durability(&envelope.event);
            MappedEvent {
                source_turn_id: source_turn_id.clone(),
                source_item_id: source_item_id.clone(),
                source_request_id: presentation_source_request_id.clone(),
                source_entry_index,
                facts: vec![RuntimeJournalFact::Presentation(
                    ImmutablePresentationEvent::new(durability, envelope.event),
                )],
            }
        }));
        Ok(output)
    }

    fn next_item(&mut self) -> Result<(RuntimeItemId, DriverItemId), DriverError> {
        self.next_item += 1;
        let value = format!("{}-item-{}", self.source_turn_id, self.next_item);
        Ok((parsed_id(value.clone())?, parsed_id(value)?))
    }

    fn cancelled(&mut self) -> Vec<MappedEvent> {
        let mut events = Vec::new();
        if let Some(item) = self.current_item.take() {
            events.push(self.item_event(
                &item,
                RuntimeEvent::ItemTerminal {
                    turn_id: self.runtime_turn_id.clone(),
                    item_id: item.0.clone(),
                    terminal: RuntimeItemTerminal::Cancelled { message: None },
                },
            ));
        }
        if !self.turn_terminal {
            self.turn_terminal = true;
            events.push(self.event(RuntimeEvent::TurnTerminal {
                turn_id: self.runtime_turn_id.clone(),
                terminal: RuntimeTurnTerminal::Interrupted,
                message: None,
                diagnostic: None,
            }));
        }
        events
    }

    fn event(&self, event: RuntimeEvent) -> MappedEvent {
        MappedEvent {
            source_turn_id: Some(self.source_turn_id.clone()),
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(event)],
        }
    }

    fn item_event(&self, item: &(RuntimeItemId, DriverItemId), event: RuntimeEvent) -> MappedEvent {
        MappedEvent {
            source_turn_id: Some(self.source_turn_id.clone()),
            source_item_id: Some(item.1.clone()),
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(event)],
        }
    }
}

fn presentation_durability(
    event: &agentdash_agent_protocol::BackboneEvent,
) -> PresentationDurability {
    use agentdash_agent_protocol::{BackboneEvent, PlatformEvent};
    if matches!(
        event,
        BackboneEvent::AgentMessageDelta(_)
            | BackboneEvent::ReasoningTextDelta(_)
            | BackboneEvent::ReasoningSummaryDelta(_)
            | BackboneEvent::CommandOutputDelta(_)
            | BackboneEvent::FileChangeDelta(_)
            | BackboneEvent::McpToolCallProgress(_)
            | BackboneEvent::ItemUpdated(_)
            | BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(_))
    ) {
        PresentationDurability::Ephemeral
    } else {
        PresentationDurability::Durable
    }
}

struct MappedEvent {
    source_turn_id: Option<DriverTurnId>,
    source_item_id: Option<DriverItemId>,
    source_request_id: Option<String>,
    source_entry_index: Option<u32>,
    facts: Vec<RuntimeJournalFact>,
}

fn mapped_is_turn_terminal(mapped: &MappedEvent) -> bool {
    mapped.facts.iter().any(|fact| {
        matches!(
            fact,
            RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
        )
    })
}

async fn emit_driver_event(
    command: &DriverCommandEnvelope,
    source_thread_id: &DriverThreadId,
    mapped: impl Into<MappedEvent>,
    sink: Arc<dyn DriverEventSink>,
) -> Result<(), DriverError> {
    let mapped = mapped.into();
    sink.emit(DriverEventEnvelope {
        binding_id: command.binding_id.clone(),
        generation: command.generation,
        operation_id: Some(command.operation_id.clone()),
        source_thread_id: source_thread_id.clone(),
        source_turn_id: mapped.source_turn_id,
        source_item_id: mapped.source_item_id,
        source_request_id: mapped.source_request_id,
        source_entry_index: mapped.source_entry_index,
        facts: mapped.facts,
    })
    .await
}

impl From<RuntimeEvent> for MappedEvent {
    fn from(event: RuntimeEvent) -> Self {
        Self {
            source_turn_id: None,
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(event)],
        }
    }
}

fn profile_digest(profile: &RuntimeProfile) -> Result<ProfileDigest, DriverError> {
    let value = serde_json::to_value(profile).map_err(|error| DriverError::ProtocolViolation {
        reason: format!("native profile serialization failed: {error}"),
        critical: true,
    })?;
    ProfileDigest::new(format!("sha256:{}", digest_bytes(&canonical_json(&value)))).map_err(
        |error| DriverError::ProtocolViolation {
            reason: error.to_string(),
            critical: true,
        },
    )
}

fn digest_json(value: &serde_json::Value) -> String {
    digest_bytes(&canonical_json(value))
}

fn canonical_json(value: &serde_json::Value) -> Vec<u8> {
    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                let mut canonical = serde_json::Map::new();
                for (key, value) in entries {
                    canonical.insert(key.clone(), canonicalize(value));
                }
                serde_json::Value::Object(canonical)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(canonicalize).collect())
            }
            other => other.clone(),
        }
    }
    serde_json::to_vec(&canonicalize(value)).expect("JSON value serializes")
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn parsed_id<T>(value: impl Into<String>) -> Result<T, DriverError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let value = value.into();
    value
        .parse()
        .map_err(|error| DriverError::ProtocolViolation {
            reason: format!("invalid native runtime identity `{value}`: {error}"),
            critical: true,
        })
}

fn project_driver_transcript(
    transcript: &DriverTranscript,
    exclude_operation_id: Option<&RuntimeOperationId>,
    after_sequence: Option<EventSequence>,
) -> Result<agentdash_agent::ProjectedTranscript, DriverError> {
    let required_after = after_sequence.unwrap_or(EventSequence(0));
    if transcript.latest_available.0 > required_after.0
        && transcript.earliest_available.0 > required_after.0.saturating_add(1)
    {
        return Err(DriverError::ProtocolViolation {
            reason: format!(
                "native durable transcript is incomplete after sequence {}: earliest_available={}, latest_available={}",
                required_after.0, transcript.earliest_available.0, transcript.latest_available.0
            ),
            critical: true,
        });
    }

    let mut last_sequence = 0;
    let mut events = Vec::new();
    for record in &transcript.records {
        let carrier = record.carrier();
        let sequence = carrier
            .sequence
            .as_ref()
            .map(|sequence| sequence.0)
            .ok_or_else(|| DriverError::ProtocolViolation {
                reason: "native durable transcript contains a record without sequence".to_string(),
                critical: true,
            })?;
        if sequence <= last_sequence || sequence > transcript.latest_available.0 {
            return Err(DriverError::ProtocolViolation {
                reason: format!(
                    "native durable transcript sequence {sequence} is outside its ordered retention window"
                ),
                critical: true,
            });
        }
        last_sequence = sequence;
        if after_sequence.is_some_and(|after| sequence <= after.0) {
            continue;
        }
        if exclude_operation_id.is_some_and(|excluded| {
            carrier
                .operation_id
                .as_ref()
                .is_some_and(|operation_id| operation_id == excluded)
        }) {
            continue;
        }
        let Some(presentation) = record.as_presentation() else {
            continue;
        };
        if presentation.durability != PresentationDurability::Durable {
            continue;
        }
        events.push(TranscriptProjectionEvent {
            event_seq: sequence,
            turn_id: carrier
                .coordinate
                .presentation_turn_id
                .as_ref()
                .map(|turn_id| turn_id.as_str()),
            entry_index: carrier.coordinate.source_entry_index,
            event: &presentation.event,
        });
    }
    Ok(project_transcript(events))
}

fn hydrate_presentation_identity(
    identity: &NativeSessionItemIdentity,
    transcript: &DriverTranscript,
    projected: &agentdash_agent::ProjectedTranscript,
) {
    for item_id in &transcript.completed_presentation_item_ids {
        identity.observe_tool_result_item_id(item_id);
    }
    identity.observe_messages(
        &projected
            .entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>(),
    );
}

fn surface_error(error: DriverSurfaceError) -> DriverError {
    match error {
        DriverSurfaceError::Unavailable { reason, retryable } => {
            DriverError::Unavailable { reason, retryable }
        }
        DriverSurfaceError::Stale => DriverError::Rejected {
            reason: "native surface materialization is stale".to_string(),
        },
        DriverSurfaceError::InvalidMaterialization { reason } => DriverError::ProtocolViolation {
            reason,
            critical: true,
        },
    }
}

fn context_error(error: DriverContextError) -> DriverError {
    match error {
        DriverContextError::Unavailable { reason, retryable } => {
            DriverError::Unavailable { reason, retryable }
        }
        DriverContextError::Stale => DriverError::Rejected {
            reason: "native context materialization is stale".to_string(),
        },
        DriverContextError::NotFound => DriverError::Rejected {
            reason: "native context checkpoint does not exist".to_string(),
        },
        DriverContextError::InvalidMaterialization { reason } => DriverError::ProtocolViolation {
            reason,
            critical: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::{AgentMessage, ToolResultAddressProvider};
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{BackboneEvent, ItemCompletedNotification};
    use agentdash_agent_runtime_test_support::session_parity::{
        NormalizedPresentationEvent, PresentationDurability as StrictDurability,
        compare_ordered_presentation_events,
    };

    fn presentation_context(
        session_identity: Arc<NativeSessionItemIdentity>,
    ) -> StreamMapperRuntimeContext {
        presentation_context_with_reserve(session_identity, NATIVE_STREAM_USAGE_RESERVE_TOKENS)
    }

    fn presentation_context_with_reserve(
        session_identity: Arc<NativeSessionItemIdentity>,
        reserve_tokens: u64,
    ) -> StreamMapperRuntimeContext {
        let tool_presentation_routes = Arc::new(std::sync::RwLock::new(HashMap::from([
            (
                "read".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::Dynamic { namespace: None },
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            ),
            (
                "shell_exec".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::Command,
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            ),
            (
                "fs_apply_patch".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::FileChange,
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            ),
        ])));
        session_identity.bind_presentation_routes(tool_presentation_routes.clone());
        StreamMapperRuntimeContext {
            model_context_window: Some(200_000),
            reserve_tokens,
            session_identity,
            fixed_event_timestamp_ms: Some(1_783_684_800_000),
            tool_presentation_routes,
        }
    }

    fn internal_events(mapped: &[MappedEvent]) -> Vec<&RuntimeEvent> {
        mapped
            .iter()
            .flat_map(|mapped| mapped.facts.iter())
            .filter_map(|fact| match fact {
                RuntimeJournalFact::Internal(event) => Some(event),
                RuntimeJournalFact::Presentation(_) => None,
            })
            .collect()
    }

    fn presentation_events(mapped: &[MappedEvent]) -> Vec<&ImmutablePresentationEvent> {
        mapped
            .iter()
            .flat_map(|mapped| mapped.facts.iter())
            .filter_map(RuntimeJournalFact::as_presentation)
            .collect()
    }

    #[test]
    fn native_provider_iterations_map_to_one_canonical_turn_lifecycle() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").expect("runtime turn"),
            parsed_id("source-turn").expect("source turn"),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let started = mapper.map(AgentEvent::AgentStart).expect("agent start");
        assert!(matches!(
            internal_events(&started).as_slice(),
            [RuntimeEvent::TurnStarted {
                turn_id,
                presentation_turn_id,
            }] if turn_id.as_str() == "runtime-turn"
                && presentation_turn_id.as_str() == "runtime-turn"
        ));
        assert!(
            mapper
                .map(AgentEvent::TurnStart)
                .expect("first provider iteration")
                .is_empty()
        );
        assert!(
            mapper
                .map(AgentEvent::TurnEnd {
                    message: agentdash_agent::AgentMessage::assistant("tool call iteration"),
                    tool_results: Vec::new(),
                })
                .expect("intermediate provider iteration")
                .is_empty()
        );
        assert!(
            mapper
                .map(AgentEvent::TurnStart)
                .expect("second provider iteration")
                .is_empty()
        );
        let terminal = mapper
            .map(AgentEvent::AgentEnd {
                messages: Vec::new(),
            })
            .expect("agent terminal");
        assert!(matches!(
            internal_events(&terminal).as_slice(),
            [RuntimeEvent::TurnTerminal {
                terminal: RuntimeTurnTerminal::Completed,
                ..
            }]
        ));
        assert!(
            mapper
                .map(AgentEvent::AgentEnd {
                    messages: Vec::new(),
                })
                .expect("duplicate agent terminal")
                .is_empty()
        );
    }

    #[test]
    fn native_mapper_preserves_explicit_runtime_source_and_presentation_turns() {
        let mut mapper = NativeEventMapper::new_with_presentation_turn(
            "presentation-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            parsed_id("presentation-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );

        let started = mapper.map(AgentEvent::AgentStart).expect("turn start");
        assert!(internal_events(&started).iter().any(|event| matches!(
            event,
            RuntimeEvent::TurnStarted {
                turn_id,
                presentation_turn_id,
            } if turn_id.as_str() == "runtime-turn"
                && presentation_turn_id.as_str() == "presentation-turn"
        )));

        let text = mapper
            .map(AgentEvent::MessageUpdate {
                message: agentdash_agent::AgentMessage::assistant("hello"),
                event: AssistantStreamEvent::TextDelta {
                    content_index: 0,
                    text: "hello".to_string(),
                },
            })
            .expect("presentation text");
        let body =
            serde_json::to_value(&presentation_events(&text)[0].event).expect("presentation body");
        assert_eq!(body["payload"]["threadId"], "presentation-thread");
        assert_eq!(body["payload"]["turnId"], "presentation-turn");
    }

    #[test]
    fn native_provider_status_is_only_projected_as_ephemeral_presentation() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let provider = mapper
            .map(AgentEvent::ProviderAttemptStatus {
                status: agentdash_agent::ProviderAttemptStatus {
                    phase: agentdash_agent::ProviderAttemptPhase::RetryScheduled,
                    attempt: 2,
                    max_attempts: 3,
                    will_retry: true,
                    delay_ms: Some(10),
                    reason_code: Some("rate_limit".into()),
                    message: Some("retry".into()),
                    provider: Some("provider".into()),
                    model: Some("model".into()),
                },
            })
            .unwrap();
        assert!(internal_events(&provider).is_empty());
        let presentation = presentation_events(&provider);
        assert_eq!(presentation.len(), 1);
        assert_eq!(
            presentation[0].durability,
            PresentationDurability::Ephemeral
        );
        assert!(matches!(
            &presentation[0].event,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::ProviderAttemptStatus(status)
            ) if status.will_retry
        ));
    }

    #[test]
    fn native_tool_updates_preserve_presentation_without_internal_summary() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let update=mapper.map(AgentEvent::ToolExecutionUpdate { tool_call_id:"tool-item".into(), tool_name:"read".into(), args:serde_json::json!({"path":"main://README.md"}), partial_result:serde_json::json!({"content":[{"type":"text","text":"progress"}],"is_error":false,"details":null}) }).unwrap();
        assert!(internal_events(&update).is_empty());
        assert!(!presentation_events(&update).is_empty());
    }

    #[test]
    fn native_message_start_has_no_phantom_presentation_and_terminals_are_independent() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let partial = agentdash_agent::AgentMessage::assistant("");
        let started = mapper
            .map(AgentEvent::MessageStart {
                message: partial.clone(),
            })
            .expect("message start");
        assert!(
            started
                .iter()
                .flat_map(|mapped| mapped.facts.iter())
                .all(|fact| !matches!(fact, RuntimeJournalFact::Presentation(_)))
        );

        let text = mapper
            .map(AgentEvent::MessageUpdate {
                message: partial.clone(),
                event: AssistantStreamEvent::TextDelta {
                    content_index: 0,
                    text: "answer".to_string(),
                },
            })
            .expect("text delta");
        let reasoning = mapper
            .map(AgentEvent::MessageUpdate {
                message: partial,
                event: AssistantStreamEvent::ThinkingDelta {
                    content_index: 1,
                    id: Some("reasoning-1".to_string()),
                    text: "thought".to_string(),
                },
            })
            .expect("reasoning delta");
        let completed = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::Assistant {
                    content: vec![
                        agentdash_agent::ContentPart::text("answer"),
                        agentdash_agent::ContentPart::reasoning("thought", None, None),
                    ],
                    tool_calls: Vec::new(),
                    stop_reason: None,
                    error_message: None,
                    usage: Some(agentdash_agent::TokenUsage {
                        input: 10,
                        cache_read_input: 2,
                        cache_creation_input: 3,
                        output: 4,
                    }),
                    timestamp: Some(1),
                },
            })
            .expect("message terminal");

        let presentation = text
            .iter()
            .chain(reasoning.iter())
            .chain(completed.iter())
            .flat_map(|mapped| mapped.facts.iter())
            .filter_map(RuntimeJournalFact::as_presentation)
            .collect::<Vec<_>>();
        assert_eq!(presentation.len(), 5);
        assert!(matches!(
            &presentation[0].event,
            agentdash_agent_protocol::BackboneEvent::AgentMessageDelta(_)
        ));
        assert!(matches!(
            &presentation[1].event,
            agentdash_agent_protocol::BackboneEvent::ReasoningTextDelta(_)
        ));
        assert!(matches!(
            &presentation[2].event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification)
                if matches!(&notification.item, agentdash_agent_protocol::AgentDashThreadItem::Codex(agentdash_agent_protocol::CodexThreadItem::AgentMessage { .. }))
        ));
        assert!(matches!(
            &presentation[3].event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification)
                if matches!(&notification.item, agentdash_agent_protocol::AgentDashThreadItem::Codex(agentdash_agent_protocol::CodexThreadItem::Reasoning { .. }))
        ));
        let agentdash_agent_protocol::BackboneEvent::TokenUsageUpdated(usage) =
            &presentation[4].event
        else {
            panic!("usage must remain the final presentation event");
        };
        assert_eq!(usage.token_usage.model_context_window, Some(200_000));
        assert_eq!(
            usage.token_usage.context.model_context_window,
            Some(200_000)
        );
        assert_eq!(
            usage.token_usage.context.effective_context_window,
            Some(200_000)
        );
        assert_eq!(
            usage.token_usage.context.reserve_tokens,
            i64::try_from(NATIVE_STREAM_USAGE_RESERVE_TOKENS).unwrap()
        );
        assert_eq!(
            presentation[0].durability,
            PresentationDurability::Ephemeral
        );
        assert_eq!(
            presentation[1].durability,
            PresentationDurability::Ephemeral
        );
        assert!(
            presentation[2..]
                .iter()
                .all(|event| event.durability == PresentationDurability::Durable)
        );
    }

    #[test]
    fn native_application_owned_and_empty_messages_do_not_create_items_or_revisions() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let messages = [
            agentdash_agent::AgentMessage::user("user input"),
            agentdash_agent::AgentMessage::tool_result("tool-1", "tool output", false),
            agentdash_agent::AgentMessage::assistant(""),
        ];

        for message in messages {
            let started = mapper
                .map(AgentEvent::MessageStart {
                    message: message.clone(),
                })
                .expect("non-canonical message start");
            let completed = mapper
                .map(AgentEvent::MessageEnd { message })
                .expect("non-canonical message end");
            assert!(internal_events(&started).is_empty());
            assert!(internal_events(&completed).is_empty());
            assert!(presentation_events(&started).is_empty());
            assert!(presentation_events(&completed).is_empty());
        }

        let assistant = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::assistant("canonical answer"),
            })
            .expect("canonical assistant message after application-owned messages");
        assert!(matches!(
            internal_events(&assistant).as_slice(),
            [RuntimeEvent::ItemStarted { item_id: started, .. }, RuntimeEvent::ItemTerminal { item_id: completed, .. }]
                if started == completed && started.as_str() == "source-turn-item-1"
        ));
    }

    #[test]
    fn native_tool_only_message_has_no_phantom_canonical_message_item() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let started = mapper
            .map(AgentEvent::MessageStart {
                message: agentdash_agent::AgentMessage::assistant(""),
            })
            .expect("tool-only message start");
        assert!(internal_events(&started).is_empty());

        let completed = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::Assistant {
                    content: Vec::new(),
                    tool_calls: vec![agentdash_agent::ToolCallInfo {
                        id: "tool-1".to_string(),
                        call_id: None,
                        name: "read".to_string(),
                        arguments: serde_json::json!({"path":"main://README.md"}),
                    }],
                    stop_reason: Some(agentdash_agent::StopReason::ToolUse),
                    error_message: None,
                    usage: Some(agentdash_agent::TokenUsage::default()),
                    timestamp: Some(1),
                },
            })
            .expect("tool-only message terminal");
        assert!(internal_events(&completed).is_empty());
        assert!(presentation_events(&completed).iter().any(|event| matches!(
            event.event,
            agentdash_agent_protocol::BackboneEvent::ItemStarted(_)
        )));
    }

    #[test]
    fn native_usage_preserves_an_explicit_nonzero_stream_reserve() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context_with_reserve(NativeSessionItemIdentity::new(), 8_192),
        );
        mapper
            .map(AgentEvent::MessageStart {
                message: agentdash_agent::AgentMessage::assistant(""),
            })
            .unwrap();
        let completed = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::Assistant {
                    content: vec![agentdash_agent::ContentPart::text("answer")],
                    tool_calls: Vec::new(),
                    stop_reason: None,
                    error_message: None,
                    usage: Some(agentdash_agent::TokenUsage {
                        input: 10,
                        cache_read_input: 2,
                        cache_creation_input: 3,
                        output: 4,
                    }),
                    timestamp: Some(1),
                },
            })
            .unwrap();
        let usage = presentation_events(&completed)
            .into_iter()
            .find_map(|event| match &event.event {
                agentdash_agent_protocol::BackboneEvent::TokenUsageUpdated(usage) => Some(usage),
                _ => None,
            })
            .expect("usage presentation");
        assert_eq!(usage.token_usage.context.reserve_tokens, 8_192);
    }

    #[test]
    fn native_vendor_tool_stream_is_the_single_complete_presentation_emitter() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let arguments = serde_json::json!({"path":"main://README.md"});
        let started = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                args: arguments.clone(),
            })
            .expect("tool start");
        let updated = mapper
            .map(AgentEvent::ToolExecutionUpdate {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                args: arguments.clone(),
                partial_result: serde_json::json!({
                    "content": [{"type":"text","text":"partial"}],
                    "content_items": [{"type":"inputText","text":"partial"}],
                    "is_error": false,
                    "details": null
                }),
            })
            .expect("tool update");
        let completed = mapper
            .map(AgentEvent::ToolExecutionEnd {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                result: serde_json::json!({
                    "content": [{"type":"text","text":"complete"}],
                    "is_error": false,
                    "details": null
                }),
                is_error: false,
            })
            .expect("tool terminal");

        let started_presentation = presentation_events(&started);
        let updated_presentation = presentation_events(&updated);
        let completed_presentation = presentation_events(&completed);
        assert_eq!(started_presentation.len(), 1);
        assert_eq!(updated_presentation.len(), 1);
        assert_eq!(completed_presentation.len(), 1);
        let item_id = match &started_presentation[0].event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => {
                notification.item.id().to_string()
            }
            other => panic!("unexpected tool start: {other:?}"),
        };
        assert!(matches!(
            &updated_presentation[0].event,
            agentdash_agent_protocol::BackboneEvent::ItemUpdated(notification)
                if notification.item.id() == item_id
        ));
        assert!(matches!(
            &completed_presentation[0].event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification)
                if notification.item.id() == item_id
        ));
        assert_eq!(
            updated_presentation[0].durability,
            PresentationDurability::Ephemeral
        );
        assert_eq!(
            completed_presentation[0].durability,
            PresentationDurability::Durable
        );
    }

    #[test]
    fn native_three_tool_round_trip_emits_one_card_per_call_and_no_generic_runtime_items() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let calls = [
            (
                "tool-read",
                "read",
                serde_json::json!({"path":"main://README.md"}),
                serde_json::json!({
                    "content": [{"type":"text","text":"read complete"}],
                    "is_error": false,
                    "details": null
                }),
                "turn_001:tool_001",
            ),
            (
                "tool-patch",
                "fs_apply_patch",
                serde_json::json!({
                    "patch":"*** Begin Patch\n*** Add File: main://new.txt\n+hello\n*** End Patch"
                }),
                serde_json::json!({
                    "content": [{"type":"text","text":"patch complete"}],
                    "is_error": false,
                    "details": null
                }),
                "turn_001:tool_002",
            ),
            (
                "tool-shell",
                "shell_exec",
                serde_json::json!({"command":"echo ok"}),
                serde_json::json!({
                    "content": [{"type":"text","text":"ok"}],
                    "is_error": false,
                    "details": {"exit_code": 0}
                }),
                "turn_001:cmd_001",
            ),
        ];

        let announced = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::Assistant {
                    content: Vec::new(),
                    tool_calls: calls
                        .iter()
                        .map(|(tool_call_id, tool_name, arguments, _, _)| {
                            agentdash_agent::ToolCallInfo {
                                id: (*tool_call_id).to_string(),
                                call_id: Some((*tool_call_id).to_string()),
                                name: (*tool_name).to_string(),
                                arguments: arguments.clone(),
                            }
                        })
                        .collect(),
                    stop_reason: Some(agentdash_agent::StopReason::ToolUse),
                    error_message: None,
                    usage: Some(agentdash_agent::TokenUsage::default()),
                    timestamp: Some(1),
                },
            })
            .expect("tool-only provider message announces every call once");
        assert!(internal_events(&announced).is_empty());
        let announced = presentation_events(&announced)
            .into_iter()
            .filter(|event| matches!(event.event, BackboneEvent::ItemStarted(_)))
            .collect::<Vec<_>>();
        assert_eq!(announced.len(), calls.len());
        for (event, (_, _, _, _, expected_item_id)) in announced.iter().zip(calls.iter()) {
            assert!(matches!(
                &event.event,
                BackboneEvent::ItemStarted(notification)
                    if notification.item.id() == *expected_item_id
            ));
        }

        for (tool_call_id, tool_name, arguments, result, expected_item_id) in calls {
            let started = mapper
                .map(AgentEvent::ToolExecutionStart {
                    tool_call_id: tool_call_id.to_string(),
                    tool_name: tool_name.to_string(),
                    args: arguments,
                })
                .expect("execution start reuses the provider-announced card");
            let completed = mapper
                .map(AgentEvent::ToolExecutionEnd {
                    tool_call_id: tool_call_id.to_string(),
                    tool_name: tool_name.to_string(),
                    result,
                    is_error: false,
                })
                .expect("tool completion");

            assert!(internal_events(&started).is_empty());
            assert!(internal_events(&completed).is_empty());
            assert!(presentation_events(&started).is_empty());
            assert!(matches!(
                presentation_events(&completed).as_slice(),
                [event] if matches!(
                    &event.event,
                    BackboneEvent::ItemCompleted(notification)
                        if notification.item.id() == expected_item_id
                )
            ));
        }
    }

    #[test]
    fn native_tool_call_and_execution_start_emit_one_started_item() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let arguments = serde_json::json!({"path":"main://README.md"});
        let message = agentdash_agent::AgentMessage::Assistant {
            content: Vec::new(),
            tool_calls: vec![agentdash_agent::ToolCallInfo {
                id: "tool-1".to_string(),
                call_id: None,
                name: "read".to_string(),
                arguments: arguments.clone(),
            }],
            stop_reason: None,
            error_message: None,
            usage: None,
            timestamp: None,
        };

        let streamed_start = mapper
            .map(AgentEvent::MessageUpdate {
                message,
                event: AssistantStreamEvent::ToolCallStart {
                    content_index: 0,
                    tool_call_id: "tool-1".to_string(),
                    name: "read".to_string(),
                },
            })
            .expect("streamed tool start");
        let execution_start = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                args: arguments,
            })
            .expect("execution tool start");

        assert_eq!(presentation_events(&streamed_start).len(), 1);
        assert!(presentation_events(&execution_start).is_empty());
    }

    #[test]
    fn native_partial_fs_glob_arguments_fall_back_without_aborting() {
        let context = presentation_context(NativeSessionItemIdentity::new());
        context.tool_presentation_routes.write().unwrap().insert(
            "fs_glob".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::FsGlob,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        );
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            context,
        );

        let started = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "glob-1".to_string(),
                tool_name: "fs_glob".to_string(),
                args: serde_json::json!({}),
            })
            .expect("partial fs_glob start remains non-critical");
        let body = serde_json::to_value(&presentation_events(&started)[0].event)
            .expect("typed fs_glob body");
        assert_eq!(body["payload"]["item"]["type"], "fsGlob");
    }

    #[test]
    fn native_tool_broker_route_suppresses_vendor_presentation() {
        let context = presentation_context(NativeSessionItemIdentity::new());
        context.tool_presentation_routes.write().unwrap().insert(
            "read".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::Dynamic { namespace: None },
                emitter: ToolPresentationEmitter::ToolBroker,
            },
        );
        let routes = context.tool_presentation_routes.clone();
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            context,
        );

        let mapped = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                args: serde_json::json!({"path":"main://README.md"}),
            })
            .expect("broker-owned tool start");
        assert!(presentation_events(&mapped).is_empty());
        routes.write().unwrap().insert(
            "read".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::Dynamic { namespace: None },
                emitter: ToolPresentationEmitter::VendorStream,
            },
        );
        let completed = mapper
            .map(AgentEvent::ToolExecutionEnd {
                tool_call_id: "tool-1".to_string(),
                tool_name: "read".to_string(),
                result: serde_json::json!({
                    "content": [{"type":"text","text":"complete"}],
                    "is_error": false,
                    "details": null
                }),
                is_error: false,
            })
            .expect("broker-owned tool completion remains pinned after hot replace");
        assert!(presentation_events(&completed).is_empty());
    }

    #[test]
    fn native_tool_family_comes_only_from_the_owner_projector() {
        let context = presentation_context(NativeSessionItemIdentity::new());
        context.tool_presentation_routes.write().unwrap().insert(
            "renamed_command".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::Command,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        );
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            context,
        );

        let started = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "command-1".to_string(),
                tool_name: "renamed_command".to_string(),
                args: serde_json::json!({"command":"echo owner"}),
            })
            .expect("owner command projector");
        let body = serde_json::to_value(&presentation_events(&started)[0].event)
            .expect("presentation JSON");
        assert_eq!(body["payload"]["item"]["type"], "shellExec");
        assert_eq!(body["payload"]["item"]["id"], "turn_001:cmd_001");
    }

    #[test]
    fn native_missing_owner_route_is_a_typed_protocol_violation() {
        let context = presentation_context(NativeSessionItemIdentity::new());
        context.tool_presentation_routes.write().unwrap().clear();
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            context,
        );

        let error = match mapper.map(AgentEvent::ToolExecutionStart {
            tool_call_id: "unknown-1".to_string(),
            tool_name: "unknown_tool".to_string(),
            args: serde_json::json!({}),
        }) {
            Ok(_) => panic!("missing owner route must not silently change protocol variant"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("owner-declared presentation route")
        );
    }

    #[test]
    fn native_dynamic_and_incomplete_file_change_keep_a_displayable_lifecycle() {
        let context = presentation_context(NativeSessionItemIdentity::new());
        {
            let mut routes = context.tool_presentation_routes.write().unwrap();
            routes.insert(
                "explicit_dynamic".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::Dynamic {
                        namespace: Some("owner.namespace".to_string()),
                    },
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            );
            routes.insert(
                "renamed_patch".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::FileChange,
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            );
        }
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            context,
        );

        let dynamic = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "dynamic-1".to_string(),
                tool_name: "explicit_dynamic".to_string(),
                args: serde_json::json!({"value":1}),
            })
            .expect("explicit dynamic projector");
        let body = serde_json::to_value(&presentation_events(&dynamic)[0].event)
            .expect("presentation JSON");
        assert_eq!(body["payload"]["item"]["type"], "dynamicToolCall");
        assert_eq!(body["payload"]["item"]["namespace"], "owner.namespace");

        let partial_patch = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "patch-1".to_string(),
                tool_name: "renamed_patch".to_string(),
                args: serde_json::json!({"patch":"not an apply-patch document"}),
            })
            .expect("partial file change must not abort the Agent loop");
        let body = serde_json::to_value(&presentation_events(&partial_patch)[0].event)
            .expect("partial patch presentation JSON");
        assert_eq!(body["payload"]["item"]["type"], "dynamicToolCall");
    }

    fn durable_transcript_record(
        sequence: u64,
        operation_id: &str,
        turn_id: &str,
        entry_index: u32,
        event: BackboneEvent,
    ) -> RuntimeJournalRecord {
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: parsed_id("runtime-thread-transcript").unwrap(),
                recorded_at_ms: sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: Some(parsed_id(operation_id).unwrap()),
                append_idempotency_key: None,
                binding_id: Some(parsed_id("binding-transcript").unwrap()),
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: Some(parsed_id(turn_id).unwrap()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("native-source-thread".to_string()),
                    source_turn_id: Some(turn_id.to_string()),
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
        .expect("durable transcript fixture")
    }

    #[test]
    fn native_durable_transcript_excludes_current_operation_and_hydrates_readable_watermarks() {
        let old_user = BackboneEvent::UserInputSubmitted(
            agentdash_agent_protocol::UserInputSubmittedNotification::new(
                "presentation-thread",
                "presentation-turn-old",
                "user-old",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "old prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            ),
        );
        let old_tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::DynamicToolCall {
                id: "turn_005:tool_009".to_string(),
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
            "presentation-thread",
            "presentation-turn-old",
        ));
        let current_user = BackboneEvent::UserInputSubmitted(
            agentdash_agent_protocol::UserInputSubmittedNotification::new(
                "presentation-thread",
                "presentation-turn-current",
                "user-current",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "current prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            ),
        );
        let transcript = DriverTranscript {
            earliest_available: EventSequence(1),
            latest_available: EventSequence(3),
            active_compaction_source_end: None,
            completed_presentation_item_ids: vec!["turn_005:tool_009".to_string()],
            records: vec![
                durable_transcript_record(1, "operation-old", "presentation-turn-old", 0, old_user),
                durable_transcript_record(2, "operation-old", "presentation-turn-old", 1, old_tool),
                durable_transcript_record(
                    3,
                    "operation-current",
                    "presentation-turn-current",
                    0,
                    current_user,
                ),
            ],
        };
        let current_operation: RuntimeOperationId = parsed_id("operation-current").unwrap();
        let projected = project_driver_transcript(&transcript, Some(&current_operation), None)
            .expect("project durable transcript");
        assert_eq!(projected.entries.len(), 3);
        assert!(matches!(
            &projected.entries[0].message,
            AgentMessage::User { content, .. }
                if content[0].extract_text() == Some("old prompt")
        ));
        assert!(matches!(
            &projected.entries[1].message,
            AgentMessage::Assistant { tool_calls, .. }
                if tool_calls[0].id == "turn_005:tool_009"
        ));
        assert!(matches!(
            &projected.entries[2].message,
            AgentMessage::ToolResult { tool_call_id, .. }
                if tool_call_id == "turn_005:tool_009"
        ));
        assert!(projected.entries.iter().all(|entry| {
            entry
                .message
                .first_text()
                .is_none_or(|text| text != "current prompt")
        }));

        let identity = NativeSessionItemIdentity::new();
        let messages = projected
            .entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>();
        identity.observe_messages(&messages);
        let next = identity.tool_result_ref("raw-turn-new", "raw-call-new", "fs_glob");
        assert_eq!(next.item_id, "turn_006:tool_010");
    }

    #[test]
    fn native_rejects_a_retention_gapped_durable_transcript() {
        let transcript = DriverTranscript {
            earliest_available: EventSequence(8),
            latest_available: EventSequence(12),
            active_compaction_source_end: None,
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        };
        assert!(matches!(
            project_driver_transcript(&transcript, None, None),
            Err(DriverError::ProtocolViolation { critical: true, .. })
        ));
    }

    #[test]
    fn native_compacted_replay_accepts_a_pruned_prefix_when_the_tail_is_contiguous() {
        let tail_user = BackboneEvent::UserInputSubmitted(
            agentdash_agent_protocol::UserInputSubmittedNotification::new(
                "presentation-thread",
                "presentation-turn-tail",
                "user-tail",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "retained tail".to_string(),
                    text_elements: Vec::new(),
                }],
            ),
        );
        let transcript = DriverTranscript {
            earliest_available: EventSequence(9),
            latest_available: EventSequence(9),
            active_compaction_source_end: Some(EventSequence(8)),
            completed_presentation_item_ids: vec!["turn_005:tool_009".to_string()],
            records: vec![durable_transcript_record(
                9,
                "operation-tail",
                "presentation-turn-tail",
                0,
                tail_user,
            )],
        };
        let projected = project_driver_transcript(&transcript, None, Some(EventSequence(8)))
            .expect("compacted base makes the pruned prefix unnecessary");
        assert_eq!(projected.entries.len(), 1);
        assert_eq!(
            projected.entries[0].message.first_text(),
            Some("retained tail")
        );
        let identity = NativeSessionItemIdentity::new();
        hydrate_presentation_identity(identity.as_ref(), &transcript, &projected);
        assert_eq!(
            identity
                .tool_result_ref("raw-turn-after-rebind", "raw-call-after-rebind", "fs_glob")
                .item_id,
            "turn_006:tool_010"
        );
    }

    #[test]
    fn native_compacted_replay_rejects_a_gap_between_the_base_and_retained_tail() {
        let transcript = DriverTranscript {
            earliest_available: EventSequence(10),
            latest_available: EventSequence(12),
            active_compaction_source_end: Some(EventSequence(8)),
            completed_presentation_item_ids: Vec::new(),
            records: Vec::new(),
        };
        assert!(matches!(
            project_driver_transcript(&transcript, None, Some(EventSequence(8))),
            Err(DriverError::ProtocolViolation { critical: true, .. })
        ));
    }

    #[test]
    fn native_tool_presentation_identity_is_session_scoped_across_turns() {
        let identity = NativeSessionItemIdentity::new();
        let mut first_turn = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn-1").unwrap(),
            parsed_id("source-turn-1").unwrap(),
            presentation_context(identity.clone()),
        );
        let mut second_turn = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn-2").unwrap(),
            parsed_id("source-turn-2").unwrap(),
            presentation_context(identity.clone()),
        );

        let start = |mapper: &mut NativeEventMapper, tool_call_id: &str| {
            mapper
                .map(AgentEvent::ToolExecutionStart {
                    tool_call_id: tool_call_id.to_string(),
                    tool_name: "read".to_string(),
                    args: serde_json::json!({"path":"main://README.md"}),
                })
                .expect("tool start")
        };
        let first = start(&mut first_turn, "tool-1");
        let second = start(&mut first_turn, "tool-2");
        let truncated_ref = identity.tool_result_ref("runtime-turn-1", "tool-2", "read");
        let truncated_completed = first_turn
            .map(AgentEvent::ToolExecutionEnd {
                tool_call_id: "tool-2".to_string(),
                tool_name: "read".to_string(),
                result: serde_json::json!({
                    "content": [{"type":"text","text":"bounded preview"}],
                    "is_error": false,
                    "details": {
                        "truncation": {"original_bytes": 200000},
                        "readable_ref": {
                            "item_id": truncated_ref.item_id,
                            "lifecycle_path": truncated_ref.lifecycle_path
                        }
                    }
                }),
                is_error: false,
            })
            .expect("truncated tool result");
        let third = start(&mut first_turn, "tool-3");
        let later_turn = start(&mut second_turn, "tool-4");
        let item_id = |mapped: &[MappedEvent]| match &presentation_events(mapped)[0].event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => {
                notification.item.id().to_string()
            }
            other => panic!("unexpected tool start: {other:?}"),
        };

        assert_eq!(item_id(&first), "turn_001:tool_001");
        assert_eq!(item_id(&second), "turn_001:tool_002");
        assert!(matches!(
            &presentation_events(&truncated_completed)[0].event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification)
                if notification.item.id() == item_id(&second)
        ));
        assert_eq!(item_id(&third), "turn_001:tool_003");
        assert_eq!(item_id(&later_turn), "turn_002:tool_004");
    }

    #[test]
    fn owner_declared_command_alias_reuses_the_start_id_for_truncated_completion() {
        let identity = NativeSessionItemIdentity::new();
        let context = presentation_context(identity.clone());
        context.tool_presentation_routes.write().unwrap().insert(
            "owner_command_alias".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::Command,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        );
        let routes = context.tool_presentation_routes.clone();
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn-command").unwrap(),
            parsed_id("source-turn-command").unwrap(),
            context,
        );
        let started = mapper
            .map(AgentEvent::ToolExecutionStart {
                tool_call_id: "raw-command-call".to_string(),
                tool_name: "owner_command_alias".to_string(),
                args: serde_json::json!({"command":"echo main"}),
            })
            .expect("command start");
        routes.write().unwrap().insert(
            "owner_command_alias".to_string(),
            NativeToolPresentationRoute {
                projection: ToolProtocolProjection::Dynamic { namespace: None },
                emitter: ToolPresentationEmitter::VendorStream,
            },
        );
        let readable_ref = identity.tool_result_ref(
            "runtime-turn-command",
            "raw-command-call",
            "owner_command_alias",
        );
        assert_eq!(readable_ref.item_id, "turn_001:cmd_001");
        let completed = mapper
            .map(AgentEvent::ToolExecutionEnd {
                tool_call_id: "raw-command-call".to_string(),
                tool_name: "owner_command_alias".to_string(),
                result: serde_json::json!({
                    "content": [{"type":"text","text":"bounded output"}],
                    "is_error": false,
                    "details": {
                        "exit_code": 0,
                        "truncation": {"original_bytes": 200000},
                        "readable_ref": {
                            "item_id": readable_ref.item_id,
                            "lifecycle_path": readable_ref.lifecycle_path
                        }
                    }
                }),
                is_error: false,
            })
            .expect("command completion keeps the start-time route after hot replace");
        let item_id = |mapped: &[MappedEvent]| match &presentation_events(mapped)[0].event {
            BackboneEvent::ItemStarted(notification) => notification.item.id().to_string(),
            BackboneEvent::ItemCompleted(notification) => notification.item.id().to_string(),
            event => panic!("unexpected command lifecycle event: {event:?}"),
        };
        assert_eq!(item_id(&started), "turn_001:cmd_001");
        assert_eq!(item_id(&completed), "turn_001:cmd_001");
    }

    #[test]
    fn native_provider_error_and_approval_keep_main_presentation_families() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let phases = [
            agentdash_agent::ProviderAttemptPhase::Connecting,
            agentdash_agent::ProviderAttemptPhase::ConnectedWaitingFirstDelta,
            agentdash_agent::ProviderAttemptPhase::Streaming,
            agentdash_agent::ProviderAttemptPhase::RetryScheduled,
            agentdash_agent::ProviderAttemptPhase::Retrying,
            agentdash_agent::ProviderAttemptPhase::Failed,
            agentdash_agent::ProviderAttemptPhase::Succeeded,
        ];
        for phase in phases {
            let mapped = mapper
                .map(AgentEvent::ProviderAttemptStatus {
                    status: agentdash_agent::ProviderAttemptStatus {
                        phase,
                        attempt: 1,
                        max_attempts: 2,
                        will_retry: matches!(
                            phase,
                            agentdash_agent::ProviderAttemptPhase::RetryScheduled
                                | agentdash_agent::ProviderAttemptPhase::Retrying
                        ),
                        delay_ms: None,
                        reason_code: None,
                        message: None,
                        provider: Some("provider".to_string()),
                        model: Some("model".to_string()),
                    },
                })
                .expect("provider phase");
            let events = presentation_events(&mapped);
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].durability, PresentationDurability::Ephemeral);
            assert!(matches!(
                &events[0].event,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::ProviderAttemptStatus(_)
                )
            ));
        }

        let requested = mapper
            .map(AgentEvent::ToolExecutionPendingApproval {
                tool_call_id: "approval-1".to_string(),
                tool_name: "shell_exec".to_string(),
                args: serde_json::json!({"command":"echo ok"}),
                reason: "permission required".to_string(),
                details: Some(serde_json::json!({"scope":"workspace"})),
            })
            .expect("approval requested");
        let resolved = mapper
            .map(AgentEvent::ToolExecutionApprovalResolved {
                tool_call_id: "approval-1".to_string(),
                tool_name: "shell_exec".to_string(),
                args: serde_json::json!({"command":"echo ok"}),
                approved: true,
                reason: Some("approved".to_string()),
            })
            .expect("approval resolved");
        for mapped in [&requested, &resolved] {
            let events = presentation_events(mapped);
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0].event,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { .. }
                )
            ));
        }

        let failure = mapper
            .map(AgentEvent::RunError {
                error: agentdash_agent::AgentRunError::new(
                    agentdash_agent::AgentRunErrorKind::Provider,
                    "provider failed",
                ),
            })
            .expect("run error");
        let events = presentation_events(&failure);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0].event,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::RuntimeTerminalDiagnostic(_)
            )
        ));
        assert!(matches!(
            &events[1].event,
            agentdash_agent_protocol::BackboneEvent::Error(_)
        ));
        assert!(
            events
                .iter()
                .all(|event| event.durability == PresentationDurability::Durable)
        );
    }

    #[test]
    fn native_w5_scenarios_match_main_oracle_golden_strictly() {
        fn snapshot(events: Vec<AgentEvent>) -> serde_json::Value {
            let mut mapper = NativeEventMapper::new(
                "native-thread".to_string(),
                parsed_id("runtime-turn").unwrap(),
                parsed_id("source-turn").unwrap(),
                presentation_context(NativeSessionItemIdentity::new()),
            );
            let records = events
                .into_iter()
                .flat_map(|event| mapper.map(event).unwrap())
                .flat_map(|mapped| {
                    let source_entry_index = mapped.source_entry_index;
                    mapped.facts.into_iter().filter_map(move |fact| match fact {
                        RuntimeJournalFact::Presentation(event) => Some(serde_json::json!({
                            "source_entry_index": source_entry_index,
                            "durability": match event.durability {
                                PresentationDurability::Durable => "durable",
                                PresentationDurability::Ephemeral => "ephemeral",
                            },
                            "event": event.event,
                        })),
                        RuntimeJournalFact::Internal(_) => None,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::Value::Array(records)
        }

        let partial = agentdash_agent::AgentMessage::assistant("");
        let usage_message = agentdash_agent::AgentMessage::Assistant {
            content: vec![agentdash_agent::ContentPart::text("answer")],
            tool_calls: Vec::new(),
            stop_reason: None,
            error_message: None,
            usage: Some(agentdash_agent::TokenUsage {
                input: 10,
                cache_read_input: 2,
                cache_creation_input: 3,
                output: 4,
            }),
            timestamp: Some(1),
        };
        let mut scenarios = serde_json::Map::new();
        scenarios.insert(
            "assistant_message_delta_terminal".into(),
            snapshot(vec![
                AgentEvent::MessageStart {
                    message: partial.clone(),
                },
                AgentEvent::MessageUpdate {
                    message: partial.clone(),
                    event: AssistantStreamEvent::TextDelta {
                        content_index: 0,
                        text: "answer".into(),
                    },
                },
                AgentEvent::MessageEnd {
                    message: agentdash_agent::AgentMessage::assistant("answer"),
                },
            ]),
        );
        scenarios.insert(
            "reasoning_text_summary_terminal".into(),
            snapshot(vec![
                AgentEvent::MessageStart {
                    message: partial.clone(),
                },
                AgentEvent::MessageUpdate {
                    message: partial.clone(),
                    event: AssistantStreamEvent::ThinkingDelta {
                        content_index: 0,
                        id: Some("reasoning-1".into()),
                        text: "thought".into(),
                    },
                },
                AgentEvent::MessageEnd {
                    message: agentdash_agent::AgentMessage::Assistant {
                        content: vec![agentdash_agent::ContentPart::reasoning(
                            "thought", None, None,
                        )],
                        tool_calls: Vec::new(),
                        stop_reason: None,
                        error_message: None,
                        usage: None,
                        timestamp: Some(1),
                    },
                },
            ]),
        );
        let args = serde_json::json!({"path":"main://README.md"});
        scenarios.insert(
            "item_started_updated_completed".into(),
            snapshot(vec![
                AgentEvent::ToolExecutionStart {
                    tool_call_id: "tool-1".into(),
                    tool_name: "read".into(),
                    args: args.clone(),
                },
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id: "tool-1".into(),
                    tool_name: "read".into(),
                    args: args.clone(),
                    partial_result: serde_json::json!({
                        "content": [{"type":"text","text":"partial"}],
                        "content_items": [{"type":"inputText","text":"partial"}],
                        "is_error": false,
                        "details": null
                    }),
                },
                AgentEvent::ToolExecutionEnd {
                    tool_call_id: "tool-1".into(),
                    tool_name: "read".into(),
                    result: serde_json::json!({
                        "content": [{"type":"text","text":"complete"}],
                        "is_error": false,
                        "details": null
                    }),
                    is_error: false,
                },
            ]),
        );
        scenarios.insert(
            "usage_context".into(),
            snapshot(vec![
                AgentEvent::MessageStart {
                    message: partial.clone(),
                },
                AgentEvent::MessageEnd {
                    message: usage_message,
                },
            ]),
        );
        scenarios.insert(
            "provider_phases_error".into(),
            snapshot(
                [
                    agentdash_agent::ProviderAttemptPhase::Connecting,
                    agentdash_agent::ProviderAttemptPhase::ConnectedWaitingFirstDelta,
                    agentdash_agent::ProviderAttemptPhase::Streaming,
                    agentdash_agent::ProviderAttemptPhase::RetryScheduled,
                    agentdash_agent::ProviderAttemptPhase::Retrying,
                    agentdash_agent::ProviderAttemptPhase::Failed,
                    agentdash_agent::ProviderAttemptPhase::Succeeded,
                ]
                .into_iter()
                .map(|phase| AgentEvent::ProviderAttemptStatus {
                    status: agentdash_agent::ProviderAttemptStatus {
                        phase,
                        attempt: 1,
                        max_attempts: 2,
                        will_retry: matches!(
                            phase,
                            agentdash_agent::ProviderAttemptPhase::RetryScheduled
                                | agentdash_agent::ProviderAttemptPhase::Retrying
                        ),
                        delay_ms: Some(250),
                        reason_code: Some("rate_limit".into()),
                        message: Some("provider phase".into()),
                        provider: Some("provider".into()),
                        model: Some("model".into()),
                    },
                })
                .chain([AgentEvent::RunError {
                    error: agentdash_agent::AgentRunError::new(
                        agentdash_agent::AgentRunErrorKind::Provider,
                        "provider failed",
                    ),
                }])
                .collect(),
            ),
        );
        scenarios.insert(
            "thread_status_title_compaction".into(),
            snapshot(vec![AgentEvent::ContextCompactionFailed {
                item_id: "compaction-1".into(),
                error: "compaction failed".into(),
                metadata: None,
            }]),
        );
        scenarios.insert(
            "interactions_all_connectors".into(),
            snapshot(vec![
                AgentEvent::ToolExecutionPendingApproval {
                    tool_call_id: "approval-1".into(),
                    tool_name: "shell_exec".into(),
                    args: serde_json::json!({"command":"echo ok"}),
                    reason: "permission required".into(),
                    details: Some(serde_json::json!({"scope":"workspace"})),
                },
                AgentEvent::ToolExecutionApprovalResolved {
                    tool_call_id: "approval-1".into(),
                    tool_name: "shell_exec".into(),
                    args: serde_json::json!({"command":"echo ok"}),
                    approved: true,
                    reason: Some("approved".into()),
                },
            ]),
        );
        fn normalize(records: &serde_json::Value) -> Vec<NormalizedPresentationEvent> {
            records
                .as_array()
                .expect("scenario records")
                .iter()
                .map(|record| NormalizedPresentationEvent {
                    durability: match record["durability"].as_str().unwrap() {
                        "durable" => StrictDurability::Durable,
                        "ephemeral" => StrictDurability::Ephemeral,
                        other => panic!("unknown durability {other}"),
                    },
                    event: record["event"].clone(),
                })
                .collect()
        }

        fn source_entry_indices(records: &serde_json::Value) -> Vec<Option<u32>> {
            records
                .as_array()
                .expect("scenario records")
                .iter()
                .map(|record| {
                    record["source_entry_index"]
                        .as_u64()
                        .map(|value| value as u32)
                })
                .collect()
        }

        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/main-oracle-presentation.json"))
                .expect("parse fixed Main oracle golden");
        assert_eq!(
            golden["oracle_commit"],
            "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
        );
        assert_eq!(
            golden["source_sha256"],
            "d2e1cea154e40e8f66aa8e5ec36ef0cd57ebee78332f157a22c639a4db4bbb05"
        );
        assert_eq!(
            golden["oracle_test_source_sha256"],
            "43eb493aaf08cf749ba857ce97f6fc4c55367203eb7f9c0e9792613032f5e94d"
        );
        let expected = golden["scenarios"].as_object().unwrap();
        let expected_source_entry_index = golden["source_entry_index"].as_u64().unwrap() as u32;
        assert_eq!(expected.len(), 7);
        assert_eq!(scenarios.len(), 7);
        for (scenario, main_records) in expected {
            let current_records = scenarios
                .get(scenario)
                .unwrap_or_else(|| panic!("missing current scenario {scenario}"));
            compare_ordered_presentation_events(
                &normalize(main_records),
                &normalize(current_records),
            )
            .unwrap_or_else(|error| panic!("strict parity failed for {scenario}: {error:?}"));
            assert_eq!(
                vec![Some(expected_source_entry_index); main_records.as_array().unwrap().len()],
                source_entry_indices(current_records),
                "source entry coordinates drifted for {scenario}"
            );
        }
    }

    #[test]
    fn native_presentation_carriers_keep_each_main_source_entry() {
        let mut mapper = NativeEventMapper::new(
            "native-thread".to_string(),
            parsed_id("runtime-turn").unwrap(),
            parsed_id("source-turn").unwrap(),
            presentation_context(NativeSessionItemIdentity::new()),
        );
        let first = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::assistant("first"),
            })
            .unwrap();
        let second = mapper
            .map(AgentEvent::MessageEnd {
                message: agentdash_agent::AgentMessage::assistant("second"),
            })
            .unwrap();
        let presentation_indices = |mapped: &[MappedEvent]| {
            mapped
                .iter()
                .filter(|mapped| {
                    matches!(
                        mapped.facts.as_slice(),
                        [RuntimeJournalFact::Presentation(_)]
                    )
                })
                .map(|mapped| mapped.source_entry_index)
                .collect::<Vec<_>>()
        };
        assert_eq!(presentation_indices(&first), vec![Some(0), Some(0)]);
        assert_eq!(presentation_indices(&second), vec![Some(1), Some(1)]);
        assert!(first.iter().chain(&second).all(|mapped| {
            mapped.source_entry_index.is_none()
                || matches!(
                    mapped.facts.as_slice(),
                    [RuntimeJournalFact::Presentation(_)]
                )
        }));
    }

    #[test]
    fn native_projection_profile_does_not_claim_plan_events_absent_from_agent_core() {
        let profile = native_conversation_projection();
        assert!(
            !profile
                .item_families
                .contains(&agentdash_integration_api::DriverConversationItemFamily::Plan)
        );
        profile
            .validate_required_families()
            .expect("native required families");
    }
}
