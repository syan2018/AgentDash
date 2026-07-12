use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
    sync::Arc,
};

use agentdash_agent::{Agent, AgentConfig, AgentEvent, AssistantStreamEvent, LlmBridge};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_types::DynAgentTool;
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
    mapping::{context_blocks_to_messages, inputs_to_message, message_text},
    tool::{NativeRuntimeTool, NativeToolEventContext},
};

const PROTOCOL_REVISION: u32 = 1;
const FACTORY_KEY: &str = "agentdash.native_agent";
const DEFINITION_ID: &str = "agentdash.native_agent";
const CONFORMANCE_SUITE: &str = "agentdash-native-runtime-conformance-v1";

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
    ) -> Result<Arc<dyn LlmBridge>, NativeBridgeResolveError>;
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
        let bridge =
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
        Ok(Arc::new(NativeAgentDriver::new(instance, bridge, host)))
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
    AgentRuntimeDriverContribution {
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
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    context_revision: RwLock<ContextRevision>,
    tool_events: Arc<RwLock<Option<NativeToolEventContext>>>,
}

impl NativeAgentDriver {
    fn new(
        instance: ActivatedAgentServiceInstance,
        bridge: Arc<dyn LlmBridge>,
        host: RuntimeDriverHostPorts,
    ) -> Self {
        Self {
            service_instance_id: instance.instance_id,
            generation: instance.generation,
            profile: native_runtime_profile(),
            bridge,
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
    ) -> Result<Arc<NativeThread>, DriverError> {
        if let Some(thread) = binding.thread.read().await.clone() {
            return Ok(thread);
        }
        let surface = binding.surface.read().await.clone();
        let active_turn = Arc::new(RwLock::new(None));
        let active_runtime_turn = Arc::new(RwLock::new(None));
        let tool_events = Arc::new(RwLock::new(None));
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
                tool_set_revision: surface.tools.revision,
                events: tool_events.clone(),
            },
            self.host.tools.clone(),
        ));
        agent
            .replace_messages(context_blocks_to_messages(&surface.context.blocks))
            .await;
        let thread = Arc::new(NativeThread {
            agent: Mutex::new(agent),
            active_turn,
            active_runtime_turn,
            context_revision: RwLock::new(ContextRevision(0)),
            tool_events,
        });
        let mut slot = binding.thread.write().await;
        Ok(slot.get_or_insert_with(|| thread.clone()).clone())
    }

    async fn run_turn(
        &self,
        envelope: &DriverCommandEnvelope,
        binding: Arc<NativeBinding>,
        input: Vec<RuntimeInput>,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<(), DriverError> {
        let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
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
        *thread.active_turn.write().await = Some(source_turn_id.clone());
        *thread.active_runtime_turn.write().await = Some(runtime_turn_id.clone());
        *thread.tool_events.write().await = Some(NativeToolEventContext {
            sink: sink.clone(),
            binding_id: envelope.binding_id.clone(),
            generation: envelope.generation,
            source_thread_id: binding.source_thread_id.clone(),
        });
        let result = async {
            let (mut events, handle) = {
                let mut agent = thread.agent.lock().await;
                agent
                    .prompt(inputs_to_message(input))
                    .map_err(|error| DriverError::Rejected {
                        reason: error.to_string(),
                    })?
            };
            let mut mapper = NativeEventMapper::new(runtime_turn_id, source_turn_id);
            while let Some(event) = events.next().await {
                let terminal = matches!(event, AgentEvent::AgentEnd { .. });
                for mapped in mapper.map(event)? {
                    if let RuntimeEvent::ItemTerminal {
                        turn_id,
                        item_id,
                        terminal: RuntimeItemTerminal::Completed { final_content },
                    } = &mapped.event
                        && let Some(source_item_id) = &mapped.source_item_id
                    {
                        binding
                            .projected_items
                            .write()
                            .await
                            .push(DriverProjectedItem {
                                source_turn_id: mapped.source_turn_id.clone().unwrap_or_else(
                                    || parsed_id(turn_id.to_string()).expect("mapped turn id"),
                                ),
                                source_item_id: source_item_id.clone(),
                                content: final_content.clone(),
                            });
                        let _ = item_id;
                    }
                    emit_driver_event(envelope, &binding.source_thread_id, mapped, sink.clone())
                        .await?;
                }
                if terminal {
                    break;
                }
            }
            let terminal_emitted = mapper.turn_terminal;
            match handle.await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(_)) | Err(_) if terminal_emitted => Ok(()),
                Ok(Err(error)) => Err(DriverError::Rejected {
                    reason: error.to_string(),
                }),
                Err(error) => Err(DriverError::Lost {
                    reason: format!("native Agent Core task join failed: {error}"),
                    retryable: false,
                }),
            }
        }
        .await;
        *thread.active_turn.write().await = None;
        *thread.active_runtime_turn.write().await = None;
        *thread.tool_events.write().await = None;
        result
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
            RuntimeCommand::ThreadStart {
                input,
                surface_digest,
                ..
            } => {
                if binding.surface.read().await.digest != surface_digest {
                    return Err(DriverError::Rejected {
                        reason: "thread start surface digest is stale".to_string(),
                    });
                }
                self.ensure_thread(&envelope.binding_id, &binding).await?;
                emit_driver_event(
                    &envelope,
                    &binding.source_thread_id,
                    RuntimeEvent::ThreadStatusChanged {
                        status: RuntimeThreadStatus::Active,
                    },
                    sink.clone(),
                )
                .await?;
                self.run_turn(&envelope, binding, input, sink.clone())
                    .await?;
            }
            RuntimeCommand::TurnStart { input, .. } => {
                self.run_turn(&envelope, binding, input, sink.clone())
                    .await?;
            }
            RuntimeCommand::TurnSteer {
                expected_turn_id,
                input,
                ..
            } => {
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
                validate_active_turn(&thread, &expected_turn_id).await?;
                thread
                    .agent
                    .lock()
                    .await
                    .steer(inputs_to_message(input))
                    .await;
            }
            RuntimeCommand::TurnInterrupt {
                expected_turn_id, ..
            } => {
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
                validate_active_turn(&thread, &expected_turn_id).await?;
                thread.agent.lock().await.abort();
            }
            RuntimeCommand::ThreadResume { .. } => {
                self.ensure_thread(&envelope.binding_id, &binding).await?;
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
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
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
                        ))
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
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
                thread
                    .agent
                    .lock()
                    .await
                    .set_system_prompt(instructions.join("\n\n"));
            }
            RuntimeCommand::ToolSetReplace {
                expected_tool_set_revision,
                tool_set_digest,
                ..
            } => {
                let tools = self
                    .host
                    .surfaces
                    .materialize_tool_set(
                        envelope.binding_id.clone(),
                        expected_tool_set_revision,
                        &tool_set_digest,
                    )
                    .await
                    .map_err(surface_error)?;
                if tools.revision != expected_tool_set_revision || tools.digest != tool_set_digest {
                    return Err(DriverError::ProtocolViolation {
                        reason: "tool broker returned a different hot-replace revision".to_string(),
                        critical: true,
                    });
                }
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
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
                        tool_set_revision: updated_surface.tools.revision,
                        events: thread.tool_events.clone(),
                    },
                    self.host.tools.clone(),
                ));
                applied_tool_set = Some(DriverToolSetApplyReceipt {
                    revision: expected_tool_set_revision,
                    digest: tool_set_digest,
                });
            }
            RuntimeCommand::ContextCompact {
                compaction_id,
                expected_context_revision,
                ..
            } => {
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
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
                    .replace_messages(context_blocks_to_messages(&activation.materialized.blocks))
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
                let thread = self.ensure_thread(&envelope.binding_id, &binding).await?;
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
    runtime_turn_id: RuntimeTurnId,
    source_turn_id: DriverTurnId,
    next_item: u64,
    current_item: Option<(RuntimeItemId, DriverItemId)>,
    turn_started: bool,
    turn_terminal: bool,
}

impl NativeEventMapper {
    fn new(runtime_turn_id: RuntimeTurnId, source_turn_id: DriverTurnId) -> Self {
        Self {
            runtime_turn_id,
            source_turn_id,
            next_item: 0,
            current_item: None,
            turn_started: false,
            turn_terminal: false,
        }
    }

    fn map(&mut self, event: AgentEvent) -> Result<Vec<MappedEvent>, DriverError> {
        let mut mapped = Vec::new();
        match event {
            AgentEvent::AgentStart | AgentEvent::TurnStart if !self.turn_started => {
                self.turn_started = true;
                mapped.push(self.event(RuntimeEvent::TurnStarted {
                    turn_id: self.runtime_turn_id.clone(),
                }));
            }
            AgentEvent::AgentStart | AgentEvent::TurnStart => {}
            AgentEvent::MessageStart { .. } => {
                let item = self.next_item()?;
                self.current_item = Some(item.clone());
                mapped.push(self.item_event(
                    &item,
                    RuntimeEvent::ItemStarted {
                        turn_id: self.runtime_turn_id.clone(),
                        item_id: item.0.clone(),
                        initial_content: RuntimeItemContent::AgentMessage {
                            text: String::new(),
                        },
                    },
                ));
            }
            AgentEvent::MessageUpdate { event, .. } => {
                if let Some(item) = self.current_item.clone()
                    && let Some(delta) = stream_delta(event)
                {
                    mapped.push(self.item_event(
                        &item,
                        RuntimeEvent::ItemDelta {
                            turn_id: self.runtime_turn_id.clone(),
                            item_id: item.0.clone(),
                            delta,
                        },
                    ));
                }
            }
            AgentEvent::MessageEnd { message } => {
                if let Some(item) = self.current_item.take() {
                    mapped.push(self.item_event(
                        &item,
                        RuntimeEvent::ItemTerminal {
                            turn_id: self.runtime_turn_id.clone(),
                            item_id: item.0.clone(),
                            terminal: RuntimeItemTerminal::Completed {
                                final_content: RuntimeItemContent::AgentMessage {
                                    text: message_text(&message),
                                },
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
                }));
            }
            AgentEvent::RunError { error } if !self.turn_terminal => {
                self.turn_terminal = true;
                mapped.push(self.event(RuntimeEvent::TurnTerminal {
                    turn_id: self.runtime_turn_id.clone(),
                    terminal: RuntimeTurnTerminal::Failed,
                    message: Some(error.to_string()),
                }));
            }
            AgentEvent::RunError { .. } | AgentEvent::AgentEnd { .. } => {}
            AgentEvent::ContextCompactionStarted { .. }
            | AgentEvent::ContextCompactionNoop { .. }
            | AgentEvent::ContextCompacted { .. }
            | AgentEvent::ContextCompactionFailed { .. } => {
                return Err(DriverError::ProtocolViolation {
                    reason: "Agent Core attempted runtime-owned compaction lifecycle".to_string(),
                    critical: true,
                });
            }
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
                        item_id: Some(runtime_item_id),
                        interaction_id: parsed_id(tool_call_id)?,
                        interaction_kind: RuntimeInteractionKind::PermissionApproval,
                        prompt: reason,
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
            AgentEvent::ProviderAttemptStatus { .. } | AgentEvent::ToolExecutionUpdate { .. } => {}
        }
        Ok(mapped)
    }

    fn next_item(&mut self) -> Result<(RuntimeItemId, DriverItemId), DriverError> {
        self.next_item += 1;
        let value = format!("{}-item-{}", self.source_turn_id, self.next_item);
        Ok((parsed_id(value.clone())?, parsed_id(value)?))
    }

    fn event(&self, event: RuntimeEvent) -> MappedEvent {
        MappedEvent {
            source_turn_id: Some(self.source_turn_id.clone()),
            source_item_id: None,
            event,
        }
    }

    fn item_event(&self, item: &(RuntimeItemId, DriverItemId), event: RuntimeEvent) -> MappedEvent {
        MappedEvent {
            source_turn_id: Some(self.source_turn_id.clone()),
            source_item_id: Some(item.1.clone()),
            event,
        }
    }
}

struct MappedEvent {
    source_turn_id: Option<DriverTurnId>,
    source_item_id: Option<DriverItemId>,
    event: RuntimeEvent,
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
        source_thread_id: source_thread_id.clone(),
        source_turn_id: mapped.source_turn_id,
        source_item_id: mapped.source_item_id,
        event: mapped.event,
    })
    .await
}

impl From<RuntimeEvent> for MappedEvent {
    fn from(event: RuntimeEvent) -> Self {
        Self {
            source_turn_id: None,
            source_item_id: None,
            event,
        }
    }
}

fn stream_delta(event: AssistantStreamEvent) -> Option<String> {
    match event {
        AssistantStreamEvent::TextDelta { text, .. }
        | AssistantStreamEvent::ThinkingDelta { text, .. } => Some(text),
        _ => None,
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

    #[test]
    fn native_provider_iterations_map_to_one_canonical_turn_lifecycle() {
        let mut mapper = NativeEventMapper::new(
            parsed_id("runtime-turn").expect("runtime turn"),
            parsed_id("source-turn").expect("source turn"),
        );
        let started = mapper.map(AgentEvent::AgentStart).expect("agent start");
        assert!(matches!(
            started.as_slice(),
            [MappedEvent {
                event: RuntimeEvent::TurnStarted { .. },
                ..
            }]
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
            terminal.as_slice(),
            [MappedEvent {
                event: RuntimeEvent::TurnTerminal {
                    terminal: RuntimeTurnTerminal::Completed,
                    ..
                },
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
}
