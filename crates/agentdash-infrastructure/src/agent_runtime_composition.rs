use std::{collections::BTreeMap, sync::Arc};

use crate::{
    PostgresAgentRuntimeCompositionRepository, PostgresAgentRuntimeContextBroker,
    PostgresAgentRuntimeHostRepository, PostgresRuntimeRepository,
    agent_runtime_driver_sink::admit_driver_event_to_pump,
};
use agentdash_agent_runtime::{
    ManagedAgentRuntime, PlatformToolBroker, RuntimeRepository, RuntimeTransientEvents,
    RuntimeWorkClaim, RuntimeWorkClaimRequest, RuntimeWorkKind, RuntimeWorkPayload,
    RuntimeWorkQueue, RuntimeWorkerId, ToolBrokerCallStatus, ToolBrokerInvocation,
    ToolBrokerOutcome, ToolCallCoordinates,
};
use agentdash_agent_runtime_contract::{
    AgentRuntimeGateway, BindingEpoch, BoundRuntimeHookPlan, DriverBindIntent,
    DriverCommandEnvelope, DriverError, DriverEventEnvelope, DriverEventSink, DriverRequestId,
    DriverThreadId, HookPlanDigest, HookPlanRevision, IdempotencyKey, OperationMeta, ProfileDigest,
    RuntimeActor, RuntimeBindingId, RuntimeCommand, RuntimeCommandEnvelope,
    RuntimeDriverGeneration, RuntimeEvent, RuntimeHookPlanBinding, RuntimeJournalFact,
    RuntimeOperationId, RuntimeRecoveryIntentId, RuntimeServiceInstanceId, RuntimeSnapshotQuery,
    RuntimeSnapshotResult, RuntimeTerminalHookEffectBinding, RuntimeThreadId, SurfaceDigest,
    SurfaceRevision, ToolSetRevision,
};
use agentdash_agent_runtime_host::{
    ActivateAgentServiceInstance, AgentRuntimeHostError, AgentRuntimeHostRepository,
    AgentServiceDefinitionId, AgentServiceDefinitionRegistry, BindRuntimeRequest,
    BoundAgentSurfaceReference, ConformanceEvidence, IntegrationDriverHost,
    PutAgentServiceInstance, RouteDriverCommand, ServiceInstanceDesiredState,
    TrustedDriverConformanceVerifier, TrustedDriverManifest, TrustedDriverManifestRegistry,
    canonical_json, profile_digest,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeProvisionRequest, AgentRunRuntimeProvisioner, AgentRunRuntimeRecoveryIntent,
    AgentRunRuntimeRecoveryState, AgentRunRuntimeTarget,
};
use agentdash_application_ports::launch::{BackendSelectionInput, BackendSelectionInputMode};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_integration_api::{
    ActivatedAgentServiceInstance, AgentDashIntegration, AgentRuntimeCredentialBroker,
    AgentRuntimeHookCallback, AgentRuntimePlacement, AgentRuntimeToolCallback, DriverHookBinding,
    MaterializedDriverSurface, RuntimeDriverHostPorts,
};
use agentdash_integration_native_agent::{
    NATIVE_STREAM_USAGE_RESERVE_TOKENS, NativeAgentRuntimeIntegration, NativeAgentServiceConfig,
    NativeBridgeResolveError, NativeBridgeResolver, NativeCredentialScope,
    NativePresentationMetadata, ResolvedNativeBridge, native_runtime_profile,
    native_runtime_trust_manifest,
};
use agentdash_llm_provider::{
    ProviderBridgeResolveError, ProviderCredentialScope,
    resolve_effective_bridge_with_model_for_scope,
};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const NATIVE_DEFINITION_ID: &str = "agentdash.native_agent";
const CODEX_DEFINITION_ID: &str = "builtin.codex-app-server";
const NATIVE_CONFORMANCE_SUITE: &str = "agentdash-native-runtime-conformance-v1";

#[async_trait]
pub trait AgentRunPlatformToolBrokerResolver: Send + Sync {
    async fn resolve(
        &self,
        request: &agentdash_integration_api::DriverToolInvocation,
    ) -> Result<PlatformToolBroker, agentdash_integration_api::DriverToolCallbackError>;
}

pub struct PlatformAgentRuntimeToolCallback {
    resolver: Arc<dyn AgentRunPlatformToolBrokerResolver>,
}

impl PlatformAgentRuntimeToolCallback {
    pub fn new(resolver: Arc<dyn AgentRunPlatformToolBrokerResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl AgentRuntimeToolCallback for PlatformAgentRuntimeToolCallback {
    async fn invoke(
        &self,
        request: agentdash_integration_api::DriverToolInvocation,
    ) -> Result<
        agentdash_integration_api::DriverToolOutcome,
        agentdash_integration_api::DriverToolCallbackError,
    > {
        let broker = self.resolver.resolve(&request).await?;
        let outcome = broker
            .invoke(
                agentdash_agent_runtime_contract::ToolChannel::DirectCallback,
                ToolBrokerInvocation {
                    coordinates: ToolCallCoordinates {
                        thread_id: request.thread_id,
                        turn_id: request.turn_id,
                        item_id: request.item_id,
                        presentation_item_id: request.presentation_item_id,
                        source_thread_id: request.source_thread_id,
                        source_turn_id: request.source_turn_id,
                        source_item_id: request.source_item_id,
                        binding_id: request.binding_id,
                        binding_generation: request.generation,
                        tool_set_revision: request.tool_set_revision,
                    },
                    tool_name: request.tool_name,
                    arguments: request.arguments,
                    timeout_ms: request.timeout_ms,
                },
                CancellationToken::new(),
            )
            .await
            .map_err(|error| {
                agentdash_integration_api::DriverToolCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                }
            })?;
        Ok(match outcome {
            ToolBrokerOutcome::Terminal { status, result, .. } => {
                agentdash_integration_api::DriverToolOutcome::Completed {
                    output: result.output,
                    is_error: result.is_error || status != ToolBrokerCallStatus::Completed,
                }
            }
            ToolBrokerOutcome::ApprovalRequired {
                interaction_id,
                reason,
            } => agentdash_integration_api::DriverToolOutcome::InteractionRequired {
                interaction_id,
                reason,
            },
            ToolBrokerOutcome::Denied { reason, .. } => {
                agentdash_integration_api::DriverToolOutcome::Denied { reason }
            }
        })
    }
}

/// 生产 Native Agent bridge resolver。
///
/// Service instance 只保存 provider/model 与明确的 platform/user 凭据作用域；真实 secret
/// 由 Provider repository + codec 在激活 driver 时短暂解析，不进入 instance config、Host
/// descriptor 或日志。User scope 必须带 user_id，因此不会以 `None` 静默回退到全局凭据。
pub struct RepositoryNativeBridgeResolver {
    provider_repository: Arc<dyn LlmProviderRepository>,
    credential_repository: Arc<dyn LlmProviderCredentialRepository>,
    secret_codec: Arc<dyn LlmSecretCodec>,
}

impl RepositoryNativeBridgeResolver {
    pub fn new(
        provider_repository: Arc<dyn LlmProviderRepository>,
        credential_repository: Arc<dyn LlmProviderCredentialRepository>,
        secret_codec: Arc<dyn LlmSecretCodec>,
    ) -> Self {
        Self {
            provider_repository,
            credential_repository,
            secret_codec,
        }
    }
}

#[async_trait]
impl NativeBridgeResolver for RepositoryNativeBridgeResolver {
    async fn resolve(
        &self,
        instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
        let config = NativeAgentServiceConfig::from_instance(instance)?;
        let scope = match config.credential_scope {
            NativeCredentialScope::Platform => ProviderCredentialScope::Platform,
            NativeCredentialScope::User { user_id } => ProviderCredentialScope::User { user_id },
        };

        let resolved = resolve_effective_bridge_with_model_for_scope(
            self.provider_repository.as_ref(),
            Some(self.credential_repository.as_ref()),
            self.secret_codec.as_ref(),
            &scope,
            &config.provider,
            Some(&config.model),
        )
        .await
        .map_err(map_provider_bridge_error)?;
        Ok(ResolvedNativeBridge {
            bridge: resolved.bridge,
            presentation: NativePresentationMetadata {
                model_context_window: resolved.model.context_window,
                reserve_tokens: NATIVE_STREAM_USAGE_RESERVE_TOKENS,
            },
        })
    }
}

fn map_provider_bridge_error(error: ProviderBridgeResolveError) -> NativeBridgeResolveError {
    match error {
        ProviderBridgeResolveError::CatalogUnavailable { reason } => {
            NativeBridgeResolveError::Unavailable {
                reason,
                retryable: true,
            }
        }
        ProviderBridgeResolveError::ProviderUnavailable { reason, .. } => {
            NativeBridgeResolveError::Unavailable {
                reason,
                retryable: false,
            }
        }
        ProviderBridgeResolveError::ProviderNotFound { .. }
        | ProviderBridgeResolveError::InvalidModel { .. } => {
            NativeBridgeResolveError::InvalidConfiguration {
                reason: error.to_string(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedNativeAgentRunSurface {
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub surface_revision: SurfaceRevision,
    pub surface_digest: SurfaceDigest,
    pub tool_set_revision: ToolSetRevision,
    pub hook_plan_revision: HookPlanRevision,
    pub hook_plan_digest: HookPlanDigest,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
}

#[async_trait]
pub trait NativeAgentRunSurfacePublication: Send + Sync {
    async fn reserve(
        &self,
        applied: AppliedNativeAgentRunSurface,
    ) -> Result<
        Box<dyn NativeAgentRunSurfacePublicationReservation>,
        AgentRunRuntimeSurfaceSourceError,
    >;
}

#[async_trait]
pub trait NativeAgentRunSurfacePublicationReservation: Send {
    async fn commit(self: Box<Self>) -> Result<(), AgentRunRuntimeSurfaceSourceError>;
    async fn abort(self: Box<Self>);
}

#[derive(Clone)]
pub struct NativeAgentRunSurfacePlan {
    pub source_frame_id: String,
    pub executor: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub surface: MaterializedDriverSurface,
    pub business_surface: agentdash_agent_runtime::CompiledBusinessAgentSurface,
    pub hook_plan: BoundRuntimeHookPlan,
    pub publication: Arc<dyn NativeAgentRunSurfacePublication>,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunRuntimeSurfaceSourceError {
    #[error("AgentRun runtime surface is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("AgentRun runtime surface is invalid: {reason}")]
    Invalid { reason: String },
}

/// Product/business Surface 的生产入口。
///
/// 实现负责从 AgentRun、AgentFrame、workspace、tool catalog 与 HookPlan 等真实业务事实
/// 编译 immutable surface；provisioner 不构造默认或空 surface。
#[async_trait]
pub trait NativeAgentRunSurfaceCompiler: Send + Sync {
    async fn compile(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError>;
}

#[derive(Clone)]
pub struct PreparedAgentRunRuntime {
    pub source_frame_id: String,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub definition_id: AgentServiceDefinitionId,
    pub service_config: serde_json::Value,
    pub placement: AgentRuntimePlacement,
    pub surface: MaterializedDriverSurface,
    pub business_surface: agentdash_agent_runtime::CompiledBusinessAgentSurface,
    pub hook_plan: RuntimeHookPlanBinding,
    pub publication: Arc<dyn NativeAgentRunSurfacePublication>,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
    pub bound_surface: BoundAgentSurfaceReference,
    pub transport_profile: agentdash_agent_runtime_contract::RuntimeProfile,
    pub host_policy_profile: agentdash_agent_runtime_contract::RuntimeProfile,
    pub conformance: ConformanceEvidence,
    pub allow_instance_creation: bool,
    pub context_delivery_target:
        agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget,
}

#[async_trait]
pub trait AgentRunRuntimeSurfaceSource: Send + Sync {
    async fn prepare(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError>;
}

pub struct NativeAgentRunRuntimeSurfaceSource {
    compiler: Arc<dyn NativeAgentRunSurfaceCompiler>,
    definitions: BTreeMap<String, agentdash_integration_api::AgentServiceDefinition>,
    profile_digest: ProfileDigest,
}

impl NativeAgentRunRuntimeSurfaceSource {
    pub fn new(
        compiler: Arc<dyn NativeAgentRunSurfaceCompiler>,
        native_definition: agentdash_integration_api::AgentServiceDefinition,
        additional_definitions: Vec<agentdash_integration_api::AgentServiceDefinition>,
    ) -> Result<Self, AgentRuntimeCompositionError> {
        let profile_digest = profile_digest(&native_runtime_profile())
            .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
        let mut definitions = additional_definitions
            .into_iter()
            .map(|definition| {
                (
                    definition.provenance.definition_id.as_str().to_string(),
                    definition,
                )
            })
            .collect::<BTreeMap<_, _>>();
        definitions.insert(NATIVE_DEFINITION_ID.to_string(), native_definition);
        Ok(Self {
            compiler,
            definitions,
            profile_digest,
        })
    }
}

#[async_trait]
impl AgentRunRuntimeSurfaceSource for NativeAgentRunRuntimeSurfaceSource {
    async fn prepare(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError> {
        let plan = self
            .compiler
            .compile(request, thread_id, binding_id)
            .await?;
        let definition_id = match plan.executor.trim() {
            "PI_AGENT" => NATIVE_DEFINITION_ID,
            "CODEX" => CODEX_DEFINITION_ID,
            executor => {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: format!("unknown execution profile {executor}"),
                });
            }
        };
        let connector_id = match plan.executor.trim() {
            "PI_AGENT" => "pi-agent",
            "CODEX" => "codex",
            _ => unreachable!("executor was validated above"),
        };
        let context_delivery_target =
            agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                connector_id: connector_id.to_string(),
                executor: plan.executor.clone(),
            };
        let definition = self.definitions.get(definition_id).ok_or_else(|| {
            AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: format!("Runtime definition {definition_id} is not installed"),
                retryable: false,
            }
        })?;
        let mut surface = plan.surface;
        surface.runtime_thread_id = thread_id.clone();
        surface.authorization_identity = request.identity.clone();
        let provider = plan
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let model = plan
            .model
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if definition_id == NATIVE_DEFINITION_ID && (provider.is_none() || model.is_none()) {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "Native provider and model must be non-empty".to_string(),
            });
        }
        let credential_scope = match &request.identity {
            Some(identity) if !identity.user_id.trim().is_empty() => {
                json!({ "kind": "user", "user_id": identity.user_id.trim() })
            }
            Some(_) => {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: "authenticated Native provisioning requires a non-empty user_id"
                        .to_string(),
                });
            }
            None => json!({ "kind": "platform" }),
        };
        let service_config = if definition_id == NATIVE_DEFINITION_ID {
            json!({
                "provider": provider,
                "model": model,
                "credential_scope": credential_scope,
            })
        } else {
            json!({ "executionProfile": plan.executor })
        };
        let service_instance_id = deterministic_service_instance_id(
            &definition.provenance.definition_id,
            &service_config,
        )?;
        let bound_surface = bound_surface_reference(&surface);
        let hook_plan = RuntimeHookPlanBinding {
            thread_id: thread_id.clone(),
            plan: plan.hook_plan,
        };
        let publication = plan.publication;
        let business_surface = plan.business_surface;
        let terminal_hook_effect_binding = plan.terminal_hook_effect_binding;
        let profile = definition.service_profile_upper_bound.clone();
        let definition_profile_digest = profile_digest(&profile).map_err(|error| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: error.to_string(),
            }
        })?;
        Ok(PreparedAgentRunRuntime {
            source_frame_id: plan.source_frame_id,
            service_instance_id,
            definition_id: definition.provenance.definition_id.clone(),
            service_config,
            placement: AgentRuntimePlacement::InProcess,
            surface,
            business_surface,
            hook_plan,
            publication,
            terminal_hook_effect_binding,
            bound_surface,
            transport_profile: profile.clone(),
            host_policy_profile: profile,
            conformance: ConformanceEvidence {
                suite_revision: if definition_id == NATIVE_DEFINITION_ID {
                    NATIVE_CONFORMANCE_SUITE.to_string()
                } else {
                    "activated-offer-required".to_string()
                },
                driver_build_digest: definition.provenance.build_digest.to_string(),
                verified_profile_digest: if definition_id == NATIVE_DEFINITION_ID {
                    self.profile_digest.clone()
                } else {
                    definition_profile_digest
                },
                verified_at: Utc::now(),
            },
            allow_instance_creation: definition_id == NATIVE_DEFINITION_ID,
            context_delivery_target,
        })
    }
}

fn deterministic_service_instance_id(
    definition_id: &AgentServiceDefinitionId,
    config: &serde_json::Value,
) -> Result<RuntimeServiceInstanceId, AgentRunRuntimeSurfaceSourceError> {
    let digest = Sha256::digest(canonical_json(&json!({
        "definition_id": definition_id,
        "config": config,
    })));
    RuntimeServiceInstanceId::new(format!("service-{:x}", digest)).map_err(|error| {
        AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        }
    })
}

fn bound_surface_reference(surface: &MaterializedDriverSurface) -> BoundAgentSurfaceReference {
    BoundAgentSurfaceReference {
        revision: surface.revision,
        digest: surface.digest.clone(),
        tool_set_revision: surface.tools.revision,
        tool_set_digest: surface.tools.digest.clone(),
        hook_plan_revision: Some(surface.hooks.revision),
        hook_plan_digest: Some(surface.hooks.digest.clone()),
        hook_artifact_digest: surface.hooks.artifact_digest.clone(),
        hook_configuration_boundary: surface.hooks.configuration_boundary,
        required_hooks: surface
            .hooks
            .bindings
            .iter()
            .map(hook_requirement)
            .collect(),
    }
}

fn runtime_surface_descriptor(
    source_frame_id: String,
    surface: &MaterializedDriverSurface,
    hook_plan: BoundRuntimeHookPlan,
    terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
) -> agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor {
    agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor {
        source_frame_id,
        surface_revision: surface.revision,
        surface_digest: surface.digest.clone(),
        vfs_digest: surface.workspace.digest.clone(),
        context_recipe_revision: surface.context.recipe.revision,
        context_digest: surface.context.digest.clone(),
        settings_revision: surface.context.recipe.provenance.settings_revision,
        tool_set_revision: surface.tools.revision,
        tool_set_digest: surface.tools.digest.clone(),
        hook_plan,
        terminal_hook_effect_binding,
    }
}

fn hook_requirement(
    binding: &DriverHookBinding,
) -> agentdash_agent_runtime_contract::HookRequirement {
    agentdash_agent_runtime_contract::HookRequirement {
        point: binding.point,
        actions: binding.actions.iter().copied().collect(),
        minimum_strength: binding.strength,
        failure_policy: binding.failure_policy,
        required: binding.required,
    }
}

#[async_trait]
pub trait AgentRunRuntimeSurfaceStore: Send + Sync {
    async fn put_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface: &MaterializedDriverSurface,
        business_surface: &agentdash_agent_runtime::CompiledBusinessAgentSurface,
    ) -> Result<(), AgentRunRuntimeBindingError>;
    async fn load_surface(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<MaterializedDriverSurface>, AgentRunRuntimeBindingError>;
    async fn load_business_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface_revision: SurfaceRevision,
        surface_digest: &SurfaceDigest,
    ) -> Result<agentdash_agent_runtime::CompiledBusinessAgentSurface, AgentRunRuntimeBindingError>;
}

#[async_trait]
impl AgentRunRuntimeSurfaceStore for PostgresAgentRuntimeCompositionRepository {
    async fn put_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface: &MaterializedDriverSurface,
        business_surface: &agentdash_agent_runtime::CompiledBusinessAgentSurface,
    ) -> Result<(), AgentRunRuntimeBindingError> {
        PostgresAgentRuntimeCompositionRepository::put_surface(
            self,
            binding_id,
            surface,
            business_surface,
        )
        .await
        .map_err(|error| AgentRunRuntimeBindingError::Unavailable {
            reason: error.to_string(),
            retryable: true,
        })
    }
    async fn load_surface(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<MaterializedDriverSurface>, AgentRunRuntimeBindingError> {
        self.load_bound_surface(binding_id).await.map_err(|error| {
            AgentRunRuntimeBindingError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            }
        })
    }
    async fn load_business_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface_revision: SurfaceRevision,
        surface_digest: &SurfaceDigest,
    ) -> Result<agentdash_agent_runtime::CompiledBusinessAgentSurface, AgentRunRuntimeBindingError>
    {
        PostgresAgentRuntimeCompositionRepository::load_business_surface(
            self,
            binding_id,
            surface_revision,
            surface_digest,
        )
        .await
        .map_err(|error| AgentRunRuntimeBindingError::Unavailable {
            reason: error.to_string(),
            retryable: true,
        })
    }
}

#[async_trait]
impl agentdash_application_ports::agent_run_runtime::AgentRunRuntimePresentationPlanStore
    for PostgresAgentRuntimeCompositionRepository
{
    async fn load_exact_presentation_plan(
        &self,
        binding_id: &RuntimeBindingId,
        surface_revision: SurfaceRevision,
        surface_digest: &SurfaceDigest,
    ) -> Result<agentdash_agent_runtime::RuntimeSurfacePresentationPlan, AgentRunRuntimeBindingError>
    {
        Ok(
            PostgresAgentRuntimeCompositionRepository::load_business_surface(
                self,
                binding_id,
                surface_revision,
                surface_digest,
            )
            .await
            .map_err(|error| AgentRunRuntimeBindingError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .presentation,
        )
    }
}

pub struct HostAgentRunRuntimeProvisioner {
    host: Arc<IntegrationDriverHost>,
    host_repository: Arc<dyn AgentRuntimeHostRepository>,
    bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    surfaces: Arc<dyn AgentRunRuntimeSurfaceStore>,
    source: Arc<dyn AgentRunRuntimeSurfaceSource>,
    gateway: Arc<dyn AgentRuntimeGateway>,
}

impl HostAgentRunRuntimeProvisioner {
    pub fn new(
        host: Arc<IntegrationDriverHost>,
        host_repository: Arc<dyn AgentRuntimeHostRepository>,
        bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        surfaces: Arc<dyn AgentRunRuntimeSurfaceStore>,
        source: Arc<dyn AgentRunRuntimeSurfaceSource>,
        gateway: Arc<dyn AgentRuntimeGateway>,
    ) -> Self {
        Self {
            host,
            host_repository,
            bindings,
            surfaces,
            source,
            gateway,
        }
    }

    async fn ensure_offer(
        &self,
        prepared: &PreparedAgentRunRuntime,
    ) -> Result<agentdash_agent_runtime_host::RuntimeOffer, AgentRunRuntimeBindingError> {
        let instance = match self
            .host_repository
            .load_instance(&prepared.service_instance_id)
            .await
            .map_err(host_store_error)?
        {
            Some(existing) => {
                if existing.definition_id != prepared.definition_id
                    || existing.config != prepared.service_config
                    || existing.placement != prepared.placement
                    || !existing.credentials.is_empty()
                {
                    return Err(binding_unavailable(
                        format!(
                            "service instance {} was reused with different immutable coordinates",
                            prepared.service_instance_id
                        ),
                        false,
                    ));
                }
                if existing.desired_state == ServiceInstanceDesiredState::Active {
                    existing
                } else {
                    self.host
                        .put_instance(PutAgentServiceInstance {
                            id: existing.id,
                            definition_id: existing.definition_id,
                            config: existing.config,
                            credentials: existing.credentials,
                            placement: existing.placement,
                            desired_state: ServiceInstanceDesiredState::Active,
                            expected_revision: Some(existing.revision),
                        })
                        .await
                        .map_err(host_error)?
                }
            }
            None => self
                .host
                .put_instance(PutAgentServiceInstance {
                    id: prepared.service_instance_id.clone(),
                    definition_id: prepared.definition_id.clone(),
                    config: prepared.service_config.clone(),
                    credentials: BTreeMap::new(),
                    placement: prepared.placement.clone(),
                    desired_state: ServiceInstanceDesiredState::Active,
                    expected_revision: None,
                })
                .await
                .map_err(host_error)?,
        };

        if let Some(offer) = self
            .host_repository
            .list_offers()
            .await
            .map_err(host_store_error)?
            .into_iter()
            .find(|offer| {
                offer.available
                    && offer.service_instance_id == instance.id
                    && offer.instance_revision == instance.revision
                    && offer.conformance.suite_revision == prepared.conformance.suite_revision
                    && offer.conformance.driver_build_digest
                        == prepared.conformance.driver_build_digest
                    && offer_supports_surface(offer, &prepared.surface)
            })
        {
            return Ok(offer);
        }

        let transport_profile_digest = profile_digest(&prepared.transport_profile)
            .map_err(|error| binding_unavailable(error.to_string(), false))?;
        let host_policy_digest = profile_digest(&prepared.host_policy_profile)
            .map_err(|error| binding_unavailable(error.to_string(), false))?;
        let offer = self
            .host
            .activate(ActivateAgentServiceInstance {
                instance_id: instance.id,
                expected_revision: instance.revision,
                transport_profile: prepared.transport_profile.clone(),
                transport_profile_digest,
                host_policy_profile: prepared.host_policy_profile.clone(),
                host_policy_digest,
                conformance: prepared.conformance.clone(),
            })
            .await
            .map_err(host_error)?;
        if !offer_supports_surface(&offer, &prepared.surface) {
            return Err(binding_unavailable(
                "activated Runtime offer does not satisfy the prepared AgentFrame surface"
                    .to_string(),
                false,
            ));
        }
        Ok(offer)
    }

    async fn select_activated_offer(
        &self,
        prepared: &PreparedAgentRunRuntime,
        backend_selection: Option<&BackendSelectionInput>,
    ) -> Result<Option<agentdash_agent_runtime_host::RuntimeOffer>, AgentRunRuntimeBindingError>
    {
        let mut offers = self
            .host
            .offers()
            .await
            .map_err(host_error)?
            .into_iter()
            .filter(|offer| {
                offer.available
                    && offer.provenance.definition_id == prepared.definition_id
                    && offer_supports_surface(offer, &prepared.surface)
                    && offer_matches_backend_selection(offer, backend_selection)
            })
            .collect::<Vec<_>>();
        offers.sort_by(|left, right| {
            placement_priority(&left.placement)
                .cmp(&placement_priority(&right.placement))
                .then_with(|| {
                    left.provenance
                        .definition_id
                        .cmp(&right.provenance.definition_id)
                })
                .then_with(|| left.service_instance_id.cmp(&right.service_instance_id))
        });
        Ok(offers.into_iter().next())
    }
}

fn offer_matches_backend_selection(
    offer: &agentdash_agent_runtime_host::RuntimeOffer,
    selection: Option<&BackendSelectionInput>,
) -> bool {
    placement_matches_backend_selection(&offer.placement, selection)
}

fn placement_matches_backend_selection(
    placement: &AgentRuntimePlacement,
    selection: Option<&BackendSelectionInput>,
) -> bool {
    let Some(selection) = selection else {
        return true;
    };
    match selection.mode {
        BackendSelectionInputMode::AutoIdle | BackendSelectionInputMode::WorkspaceBinding => true,
        BackendSelectionInputMode::Explicit => match (placement, selection.backend_id.as_deref()) {
            (AgentRuntimePlacement::Remote { host_id, .. }, Some(backend_id)) => {
                host_id == backend_id
            }
            _ => false,
        },
    }
}

fn placement_priority(placement: &AgentRuntimePlacement) -> u8 {
    match placement {
        AgentRuntimePlacement::Remote { .. } => 0,
        AgentRuntimePlacement::LocalProcess { .. } => 1,
        AgentRuntimePlacement::InProcess => 2,
    }
}

fn backend_selection_requires_activated_offer(selection: Option<&BackendSelectionInput>) -> bool {
    selection.is_some_and(|selection| selection.mode == BackendSelectionInputMode::Explicit)
}

fn offer_supports_surface(
    offer: &agentdash_agent_runtime_host::RuntimeOffer,
    surface: &MaterializedDriverSurface,
) -> bool {
    let profile = &offer.effective_profile.profile;
    if !profile
        .lifecycle
        .contains(&agentdash_agent_runtime_contract::LifecycleCapability::ThreadStart)
        || !profile
            .lifecycle
            .contains(&agentdash_agent_runtime_contract::LifecycleCapability::TurnStart)
    {
        return false;
    }
    if surface
        .context
        .instructions
        .iter()
        .any(|instruction| !profile.instruction.channels.contains(&instruction.channel))
        || surface.tools.tools.iter().any(|tool| {
            tool.channels
                .iter()
                .any(|channel| !profile.tools.channels.contains(channel))
        })
        || surface
            .workspace
            .capabilities
            .iter()
            .any(|capability| !profile.workspace.capabilities.contains(capability))
    {
        return false;
    }
    surface.hooks.bindings.iter().all(|binding| {
        !binding.required
            || profile
                .hooks
                .satisfies(&agentdash_agent_runtime_contract::HookRequirement {
                    point: binding.point,
                    actions: binding.actions.iter().copied().collect(),
                    minimum_strength: binding.strength,
                    failure_policy: binding.failure_policy,
                    required: true,
                })
    })
}

fn select_recovery_offer(
    offers: Vec<agentdash_agent_runtime_host::RuntimeOffer>,
    old_offer: &agentdash_agent_runtime_host::RuntimeOffer,
    surface: &MaterializedDriverSurface,
) -> Option<agentdash_agent_runtime_host::RuntimeOffer> {
    let mut offers =
        offers
            .into_iter()
            .filter(|offer| {
                offer.available
                    && offer.id != old_offer.id
                    && offer.provenance.definition_id == old_offer.provenance.definition_id
                    && offer.placement == old_offer.placement
                    && offer.effective_profile.profile.lifecycle.contains(
                        &agentdash_agent_runtime_contract::LifecycleCapability::ThreadResume,
                    )
                    && offer_supports_surface(offer, surface)
            })
            .collect::<Vec<_>>();
    offers.sort_by(|a, b| b.generation.cmp(&a.generation));
    offers.into_iter().next()
}

async fn activate_in_process_recovery_offer(
    host: &IntegrationDriverHost,
    host_repository: &dyn AgentRuntimeHostRepository,
    old_offer: &agentdash_agent_runtime_host::RuntimeOffer,
    surface: &MaterializedDriverSurface,
) -> Result<agentdash_agent_runtime_host::RuntimeOffer, AgentRunRuntimeBindingError> {
    if old_offer.placement != AgentRuntimePlacement::InProcess {
        return Err(binding_unavailable(
            "no same-owner Runtime offer can Resume the lost binding".to_string(),
            true,
        ));
    }
    let instance = host_repository
        .load_instance(&old_offer.service_instance_id)
        .await
        .map_err(host_store_error)?
        .ok_or(AgentRunRuntimeBindingError::NotFound)?;
    let profile = old_offer.effective_profile.profile.clone();
    let digest =
        profile_digest(&profile).map_err(|error| binding_unavailable(error.to_string(), false))?;
    let offer = host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: profile.clone(),
            transport_profile_digest: digest.clone(),
            host_policy_profile: profile,
            host_policy_digest: digest,
            conformance: old_offer.conformance.clone(),
        })
        .await
        .map_err(host_error)?;
    if offer.service_instance_id != old_offer.service_instance_id
        || offer.generation <= old_offer.generation
        || offer.provenance.definition_id != old_offer.provenance.definition_id
        || offer.placement != old_offer.placement
        || !offer
            .effective_profile
            .profile
            .lifecycle
            .contains(&agentdash_agent_runtime_contract::LifecycleCapability::ThreadResume)
        || !offer_supports_surface(&offer, surface)
    {
        return Err(binding_unavailable(
            "reactivated InProcess Runtime offer cannot Resume the lost binding".to_string(),
            false,
        ));
    }
    Ok(offer)
}

#[async_trait]
impl AgentRunRuntimeProvisioner for HostAgentRunRuntimeProvisioner {
    async fn provision(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        if let Some(existing) = self.bindings.load(&request.target).await? {
            return Ok(existing);
        }
        let thread_id = runtime_thread_id(&request.target)?;
        let binding_id = runtime_binding_id(&request.target)?;
        let prepared = self
            .source
            .prepare(request, &thread_id, &binding_id)
            .await
            .map_err(surface_source_error)?;
        self.surfaces
            .put_surface(&binding_id, &prepared.surface, &prepared.business_surface)
            .await?;
        let fork_source = if let Some(fork) = request.fork.as_ref() {
            let source = self
                .bindings
                .load(&fork.source_target)
                .await?
                .ok_or(AgentRunRuntimeBindingError::NotFound)?;
            let host_binding = self
                .host_repository
                .load_binding(&source.binding_id)
                .await
                .map_err(host_store_error)?
                .ok_or(AgentRunRuntimeBindingError::NotFound)?;
            let offer = self
                .host_repository
                .load_offer(&host_binding.offer_id)
                .await
                .map_err(host_store_error)?
                .ok_or(AgentRunRuntimeBindingError::NotFound)?;
            Some((source, offer))
        } else {
            None
        };
        let offer = if let Some((_, offer)) = fork_source.as_ref() {
            offer.clone()
        } else {
            match self
                .select_activated_offer(&prepared, request.backend_selection.as_ref())
                .await?
            {
                Some(offer) => offer,
                None if prepared.allow_instance_creation
                    && !backend_selection_requires_activated_offer(
                        request.backend_selection.as_ref(),
                    ) =>
                {
                    self.ensure_offer(&prepared).await?
                }
                None => {
                    return Err(binding_unavailable(
                        "no activated Runtime offer matches the requested execution profile and backend placement".to_string(),
                        true,
                    ));
                }
            }
        };
        let hook_plan = prepared.hook_plan.plan.clone();
        let surface_descriptor = runtime_surface_descriptor(
            prepared.source_frame_id.clone(),
            &prepared.surface,
            hook_plan,
            prepared.terminal_hook_effect_binding.clone(),
        );
        let host_binding = self
            .host
            .bind(BindRuntimeRequest {
                binding_id: binding_id.clone(),
                thread_id: thread_id.clone(),
                offer_id: offer.id,
                bound_surface: prepared.bound_surface,
                intent: match (fork_source.as_ref(), request.fork.as_ref()) {
                    (Some((source, _)), Some(fork)) => DriverBindIntent::Fork {
                        source_thread_id: source.source_thread_id.clone(),
                        through_source_turn_id: fork.through_source_turn_id.clone(),
                    },
                    _ => DriverBindIntent::Start,
                },
            })
            .await
            .map_err(host_error)?;
        let source_thread_id = host_binding.source_thread_id.ok_or_else(|| {
            binding_unavailable(
                "Host binding became active without a source thread coordinate".to_string(),
                false,
            )
        })?;
        let settings_revision = prepared.surface.context.recipe.provenance.settings_revision;
        let binding = AgentRunRuntimeBinding {
            target: request.target.clone(),
            presentation_thread_id: request.presentation_thread_id.clone(),
            thread_id,
            binding_id,
            binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
            driver_generation: host_binding.driver_generation,
            source_thread_id,
            profile_digest: offer.profile_digest,
            profile_provenance: offer.effective_profile.provenance,
            bound_profile: offer.effective_profile.profile,
            surface: surface_descriptor,
            settings_revision,
            context_delivery_target: prepared.context_delivery_target,
        };
        let publication = prepared
            .publication
            .reserve(AppliedNativeAgentRunSurface {
                runtime_thread_id: binding.thread_id.clone(),
                binding_id: binding.binding_id.clone(),
                generation: binding.driver_generation,
                source_thread_id: binding.source_thread_id.clone(),
                surface_revision: binding.surface.surface_revision,
                surface_digest: binding.surface.surface_digest.clone(),
                tool_set_revision: binding.surface.tool_set_revision,
                hook_plan_revision: binding.surface.hook_plan.revision,
                hook_plan_digest: binding.surface.hook_plan.digest.clone(),
                terminal_hook_effect_binding: binding.surface.terminal_hook_effect_binding.clone(),
            })
            .await
            .map_err(surface_source_error)?;
        publication.commit().await.map_err(surface_source_error)?;
        self.bindings.insert(binding).await
    }

    async fn recover(
        &self,
        old: &AgentRunRuntimeBinding,
        runtime_revision: agentdash_agent_runtime_contract::RuntimeRevision,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        if let Some(current) = self.bindings.load(&old.target).await?
            && current.binding_id != old.binding_id
        {
            return Ok(current);
        }
        let old_host = self
            .host_repository
            .load_binding(&old.binding_id)
            .await
            .map_err(host_store_error)?
            .ok_or(AgentRunRuntimeBindingError::NotFound)?;
        let old_offer = self
            .host_repository
            .load_offer(&old_host.offer_id)
            .await
            .map_err(host_store_error)?
            .ok_or(AgentRunRuntimeBindingError::NotFound)?;
        let mut surface = self
            .surfaces
            .load_surface(&old.binding_id)
            .await?
            .ok_or_else(|| {
                binding_unavailable(
                    "old materialized Runtime surface is missing".to_string(),
                    false,
                )
            })?;
        let context = self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Context {
                thread_id: old.thread_id.clone(),
                at_context_revision: None,
            })
            .await
            .map_err(runtime_context_snapshot_error)?;
        let RuntimeSnapshotResult::Context { context } = context else {
            return Err(binding_unavailable(
                "Runtime returned a non-context snapshot for context recovery".to_string(),
                false,
            ));
        };
        match (context.head.as_ref(), context.checkpoint.as_ref()) {
            (None, None) => {}
            (Some(head), Some(checkpoint)) => {
                if checkpoint.checkpoint_id != head.checkpoint_id
                    || checkpoint.revision != head.revision
                    || checkpoint.materialized.digest != head.digest
                    || checkpoint.materialized.recipe.provenance != head.provenance
                    || checkpoint.materialized.fidelity != head.fidelity
                {
                    return Err(binding_unavailable(
                        "active Runtime context checkpoint does not match its head".to_string(),
                        false,
                    ));
                }
                surface.context.recipe = checkpoint.materialized.recipe.clone();
                surface.context.blocks = checkpoint.materialized.blocks.clone();
                surface.context.digest = checkpoint.materialized.digest.clone();
                surface.context.fidelity = checkpoint.materialized.fidelity;
            }
            _ => {
                return Err(binding_unavailable(
                    "active Runtime context head and checkpoint are incomplete".to_string(),
                    false,
                ));
            }
        }
        let business_surface = self
            .surfaces
            .load_business_surface(
                &old.binding_id,
                old.surface.surface_revision,
                &old.surface.surface_digest,
            )
            .await?;
        let offers = self.host.offers().await.map_err(host_error)?;
        let offer = match select_recovery_offer(offers, &old_offer, &surface) {
            Some(offer) => offer,
            None => {
                activate_in_process_recovery_offer(
                    self.host.as_ref(),
                    self.host_repository.as_ref(),
                    &old_offer,
                    &surface,
                )
                .await?
            }
        };
        let epoch = BindingEpoch(old.binding_epoch.0 + 1);
        let proposed =
            RuntimeBindingId::new(format!("{}-epoch-{}", old.binding_id.as_str(), epoch.0))
                .map_err(|e| binding_unavailable(e.to_string(), false))?;
        let intent_id = format!(
            "recovery-{}-{}-{}-{}-{}",
            old.target.run_id, old.target.agent_id, epoch.0, offer.id, offer.generation.0
        );
        let intent = self
            .bindings
            .prepare_recovery(AgentRunRuntimeRecoveryIntent {
                id: intent_id.clone(),
                target: old.target.clone(),
                thread_id: old.thread_id.clone(),
                expected_old_binding_id: old.binding_id.clone(),
                expected_old_generation: old.driver_generation,
                expected_runtime_revision: runtime_revision,
                binding_epoch: epoch,
                proposed_binding_id: proposed,
                selected_offer_id: offer.id.as_str().to_string(),
                source_thread_id: old.source_thread_id.clone(),
                state: AgentRunRuntimeRecoveryState::Prepared,
                failure_reason: None,
            })
            .await?;
        let epoch = intent.binding_epoch;
        diag!(Info, Subsystem::AgentRun,
            recovery_intent_id = %intent.id,
            binding_epoch = intent.binding_epoch.0,
            old_binding_id = %intent.expected_old_binding_id,
            new_binding_id = %intent.proposed_binding_id,
            stage = "prepared",
            result = "ok",
            "AgentRun Runtime recovery advanced"
        );
        let offer_id = agentdash_agent_runtime_host::AgentServiceOfferId::new(
            intent.selected_offer_id.clone(),
        )
        .map_err(|error| binding_unavailable(error.to_string(), false))?;
        let offer = self
            .host_repository
            .load_offer(&offer_id)
            .await
            .map_err(host_store_error)?
            .ok_or_else(|| {
                binding_unavailable("selected recovery offer no longer exists".to_string(), true)
            })?;
        let proposed = intent.proposed_binding_id.clone();
        self.host_repository
            .mark_binding_lost(&old.binding_id, old.driver_generation)
            .await
            .map_err(host_store_error)?;
        self.surfaces
            .put_surface(&proposed, &surface, &business_surface)
            .await?;
        let host_binding = match self
            .host
            .bind(BindRuntimeRequest {
                binding_id: proposed.clone(),
                thread_id: old.thread_id.clone(),
                offer_id: offer.id.clone(),
                bound_surface: bound_surface_reference(&surface),
                intent: DriverBindIntent::Resume {
                    source_thread_id: old.source_thread_id.clone(),
                },
            })
            .await
        {
            Ok(binding) => binding,
            Err(error) => {
                let mapped = host_error(error);
                if matches!(
                    &mapped,
                    AgentRunRuntimeBindingError::Unavailable {
                        retryable: false,
                        ..
                    }
                ) {
                    let reason = mapped.to_string();
                    let _ = self
                        .bindings
                        .advance_recovery(
                            &intent.id,
                            intent.state,
                            AgentRunRuntimeRecoveryState::Failed,
                            Some(reason.clone()),
                        )
                        .await;
                    diag!(Warn, Subsystem::AgentRun,
                        recovery_intent_id = %intent.id,
                        binding_epoch = intent.binding_epoch.0,
                        stage = "host_bind",
                        result = "failed",
                        reason = %reason,
                        "AgentRun Runtime recovery failed"
                    );
                }
                return Err(mapped);
            }
        };
        let source = host_binding.source_thread_id.clone().ok_or_else(|| {
            binding_unavailable(
                "resumed Host binding has no source coordinate".to_string(),
                false,
            )
        })?;
        let intent = self
            .bindings
            .advance_recovery(
                &intent.id,
                AgentRunRuntimeRecoveryState::Prepared,
                AgentRunRuntimeRecoveryState::HostBound,
                None,
            )
            .await?;
        diag!(Info, Subsystem::AgentRun,
            recovery_intent_id = %intent.id,
            binding_epoch = intent.binding_epoch.0,
            stage = "host_bound",
            result = "ok",
            "AgentRun Runtime recovery advanced"
        );
        let surface_descriptor = runtime_surface_descriptor(
            old.surface.source_frame_id.clone(),
            &surface,
            old.surface.hook_plan.clone(),
            old.surface.terminal_hook_effect_binding.clone(),
        );
        let settings_revision = surface.context.recipe.provenance.settings_revision;
        let binding = AgentRunRuntimeBinding {
            target: old.target.clone(),
            presentation_thread_id: old.presentation_thread_id.clone(),
            thread_id: old.thread_id.clone(),
            binding_id: proposed,
            binding_epoch: epoch,
            driver_generation: host_binding.driver_generation,
            source_thread_id: source.clone(),
            profile_digest: offer.profile_digest.clone(),
            profile_provenance: offer.effective_profile.provenance.clone(),
            bound_profile: offer.effective_profile.profile.clone(),
            surface: surface_descriptor,
            settings_revision,
            context_delivery_target: old.context_delivery_target.clone(),
        };
        self.bindings
            .append_lineage(old, binding.clone(), &intent.id)
            .await?;
        let op = format!("agentrun-runtime-rebind-{}", intent.id);
        if let Err(error) = self
            .gateway
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: RuntimeOperationId::new(op.clone())
                        .expect("recovery operation id"),
                    idempotency_key: IdempotencyKey::new(op).expect("recovery idempotency key"),
                    expected_thread_revision: Some(intent.expected_runtime_revision),
                    actor: RuntimeActor::System {
                        component: "agent_run_runtime_recovery".to_string(),
                    },
                },
                command: RuntimeCommand::ThreadRebind {
                    thread_id: old.thread_id.clone(),
                    recovery_intent_id: RuntimeRecoveryIntentId::new(intent.id.clone())
                        .expect("recovery intent id"),
                    binding_epoch: epoch,
                    expected_binding_id: old.binding_id.clone(),
                    expected_driver_generation: old.driver_generation,
                    new_binding_id: binding.binding_id.clone(),
                    new_driver_generation: binding.driver_generation,
                    source_thread_id: source,
                    profile_digest: binding.profile_digest.clone(),
                    bound_profile: Box::new(binding.bound_profile.clone()),
                },
            })
            .await
        {
            let mapped = runtime_rebind_error(error);
            let reason = mapped.to_string();
            if matches!(
                &mapped,
                AgentRunRuntimeBindingError::Unavailable {
                    retryable: false,
                    ..
                }
            ) {
                finalize_nonretryable_recovery_failure(
                    || async {
                        self.host_repository
                            .mark_binding_lost(&binding.binding_id, binding.driver_generation)
                            .await
                            .map_err(host_store_error)
                    },
                    || async {
                        self.bindings
                            .advance_recovery(
                                &intent.id,
                                AgentRunRuntimeRecoveryState::HostBound,
                                AgentRunRuntimeRecoveryState::Failed,
                                Some(reason.clone()),
                            )
                            .await
                            .map(|_| ())
                    },
                )
                .await?;
                diag!(Warn, Subsystem::AgentRun,
                    recovery_intent_id = %intent.id,
                    binding_epoch = intent.binding_epoch.0,
                    stage = "runtime_rebind",
                    result = "failed",
                    reason = %reason,
                    "AgentRun Runtime recovery failed"
                );
            } else {
                diag!(Warn, Subsystem::AgentRun,
                    recovery_intent_id = %intent.id,
                    binding_epoch = intent.binding_epoch.0,
                    stage = "runtime_rebind",
                    result = "retryable",
                    reason = %reason,
                    "AgentRun Runtime recovery remains pending"
                );
            }
            return Err(mapped);
        }
        self.bindings
            .advance_recovery(
                &intent.id,
                AgentRunRuntimeRecoveryState::HostBound,
                AgentRunRuntimeRecoveryState::Committed,
                None,
            )
            .await?;
        diag!(Info, Subsystem::AgentRun,
            recovery_intent_id = %intent.id,
            binding_epoch = intent.binding_epoch.0,
            stage = "committed",
            result = "ok",
            "AgentRun Runtime recovery advanced"
        );
        Ok(binding)
    }
}

fn runtime_context_snapshot_error(
    error: agentdash_agent_runtime_contract::RuntimeSnapshotError,
) -> AgentRunRuntimeBindingError {
    use agentdash_agent_runtime_contract::RuntimeSnapshotError;
    let retryable = matches!(
        error,
        RuntimeSnapshotError::Unavailable { .. }
            | RuntimeSnapshotError::RevisionUnavailable { .. }
            | RuntimeSnapshotError::ContextRevisionUnavailable { .. }
    );
    binding_unavailable(error.to_string(), retryable)
}

fn runtime_rebind_error(
    error: agentdash_agent_runtime_contract::RuntimeExecuteError,
) -> AgentRunRuntimeBindingError {
    use agentdash_agent_runtime_contract::RuntimeExecuteError;
    let retryable = match &error {
        RuntimeExecuteError::RevisionConflict { .. } => true,
        RuntimeExecuteError::Unavailable { retryable, .. }
        | RuntimeExecuteError::Persistence { retryable, .. } => *retryable,
        RuntimeExecuteError::Unsupported { .. }
        | RuntimeExecuteError::OperationConflict { .. }
        | RuntimeExecuteError::ContextCompactionInProgress { .. }
        | RuntimeExecuteError::InvalidCommand { .. }
        | RuntimeExecuteError::Incompatible { .. } => false,
    };
    binding_unavailable(error.to_string(), retryable)
}

async fn finalize_nonretryable_recovery_failure<Mark, MarkFuture, Advance, AdvanceFuture>(
    mark_host_lost: Mark,
    advance_intent_failed: Advance,
) -> Result<(), AgentRunRuntimeBindingError>
where
    Mark: FnOnce() -> MarkFuture,
    MarkFuture: std::future::Future<Output = Result<(), AgentRunRuntimeBindingError>>,
    Advance: FnOnce() -> AdvanceFuture,
    AdvanceFuture: std::future::Future<Output = Result<(), AgentRunRuntimeBindingError>>,
{
    mark_host_lost().await?;
    advance_intent_failed().await
}

fn runtime_thread_id(
    target: &AgentRunRuntimeTarget,
) -> Result<RuntimeThreadId, AgentRunRuntimeBindingError> {
    RuntimeThreadId::new(format!("thread-{}-{}", target.run_id, target.agent_id))
        .map_err(|error| binding_unavailable(error.to_string(), false))
}

fn runtime_binding_id(
    target: &AgentRunRuntimeTarget,
) -> Result<RuntimeBindingId, AgentRunRuntimeBindingError> {
    RuntimeBindingId::new(format!("binding-{}-{}", target.run_id, target.agent_id))
        .map_err(|error| binding_unavailable(error.to_string(), false))
}

fn surface_source_error(error: AgentRunRuntimeSurfaceSourceError) -> AgentRunRuntimeBindingError {
    match error {
        AgentRunRuntimeSurfaceSourceError::Unavailable { reason, retryable } => {
            binding_unavailable(reason, retryable)
        }
        AgentRunRuntimeSurfaceSourceError::Invalid { reason } => binding_unavailable(reason, false),
    }
}

fn host_store_error(
    error: agentdash_agent_runtime_host::HostStoreError,
) -> AgentRunRuntimeBindingError {
    binding_unavailable(error.to_string(), true)
}

fn host_error(
    error: agentdash_agent_runtime_host::AgentRuntimeHostError,
) -> AgentRunRuntimeBindingError {
    let retryable = matches!(
        error,
        agentdash_agent_runtime_host::AgentRuntimeHostError::Store(_)
            | agentdash_agent_runtime_host::AgentRuntimeHostError::OfferUnavailable { .. }
            | agentdash_agent_runtime_host::AgentRuntimeHostError::Factory { .. }
    );
    binding_unavailable(error.to_string(), retryable)
}

fn binding_unavailable(reason: String, retryable: bool) -> AgentRunRuntimeBindingError {
    AgentRunRuntimeBindingError::Unavailable { reason, retryable }
}

struct ManagedRuntimeDriverEventSink {
    runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
}

#[async_trait]
impl DriverEventSink for ManagedRuntimeDriverEventSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        match self.runtime.ingest_driver_event(event).await {
            Ok(admission) => admit_driver_event_to_pump(admission),
            Err(error) => Err(DriverError::Lost {
                reason: format!("Managed Runtime rejected driver event: {error}"),
                retryable: true,
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeOutboxWorkerError {
    #[error("Runtime outbox store failed: {0}")]
    Store(String),
    #[error("Runtime outbox claim is invalid: {0}")]
    InvalidClaim(String),
    #[error("Runtime outbox Host dispatch failed: {0}")]
    Host(String),
    #[error("Runtime outbox binding was lost: {0}")]
    BindingLost(String),
    #[error("Runtime outbox driver stream was already terminalized: {0}")]
    Terminalized(String),
}

fn classify_outbox_dispatch_error(error: AgentRuntimeHostError) -> RuntimeOutboxWorkerError {
    match error {
        AgentRuntimeHostError::Driver(DriverError::Lost { reason, .. }) => {
            RuntimeOutboxWorkerError::BindingLost(reason)
        }
        AgentRuntimeHostError::Driver(DriverError::Terminalized { reason }) => {
            RuntimeOutboxWorkerError::Terminalized(reason)
        }
        error => RuntimeOutboxWorkerError::Host(error.to_string()),
    }
}

pub struct RuntimeOutboxWorker {
    store: Arc<PostgresRuntimeRepository>,
    runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
    host: Arc<IntegrationDriverHost>,
    worker_id: RuntimeWorkerId,
    #[cfg(test)]
    observed_dispatches: tokio::sync::Mutex<Vec<DriverCommandEnvelope>>,
}

impl RuntimeOutboxWorker {
    pub fn new(
        store: Arc<PostgresRuntimeRepository>,
        runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
        host: Arc<IntegrationDriverHost>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            runtime,
            host,
            worker_id: RuntimeWorkerId(worker_id.into()),
            #[cfg(test)]
            observed_dispatches: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    #[cfg(test)]
    async fn observed_dispatches(&self) -> Vec<DriverCommandEnvelope> {
        self.observed_dispatches.lock().await.clone()
    }

    pub async fn run_once(&self, limit: u32) -> Result<usize, RuntimeOutboxWorkerError> {
        let claims = self
            .store
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::RuntimeOutbox,
                owner: self.worker_id.clone(),
                lease_duration_ms: 5 * 60 * 1_000,
                limit,
            })
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Store(error.to_string()))?;
        let count = claims.len();
        let mut first_error = None;
        for claim in claims {
            if let Err(error) = self.process_claim(claim).await {
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(count)
    }

    async fn process_claim(&self, claim: RuntimeWorkClaim) -> Result<(), RuntimeOutboxWorkerError> {
        if self.claim_is_obsolete(&claim).await? {
            return self
                .store
                .ack(&claim)
                .await
                .map_err(|store| RuntimeOutboxWorkerError::Store(store.to_string()));
        }
        let dispatch_result = self.dispatch_claim(&claim).await;
        self.settle_dispatch_result(&claim, dispatch_result).await
    }

    async fn settle_dispatch_result(
        &self,
        claim: &RuntimeWorkClaim,
        dispatch_result: Result<(), RuntimeOutboxWorkerError>,
    ) -> Result<(), RuntimeOutboxWorkerError> {
        if let Err(error) = dispatch_result {
            if self.claim_is_obsolete(claim).await? {
                return self
                    .store
                    .ack(claim)
                    .await
                    .map_err(|store| RuntimeOutboxWorkerError::Store(store.to_string()));
            }
            return match self.store.release(claim, error.to_string()).await {
                Ok(()) => Err(error),
                Err(store) => Err(RuntimeOutboxWorkerError::Store(store.to_string())),
            };
        }
        if let Err(error) = self.store.ack(claim).await {
            let error = RuntimeOutboxWorkerError::Store(error.to_string());
            let _ = self.store.release(claim, error.to_string()).await;
            return Err(error);
        }
        Ok(())
    }

    async fn claim_is_obsolete(
        &self,
        claim: &RuntimeWorkClaim,
    ) -> Result<bool, RuntimeOutboxWorkerError> {
        let RuntimeWorkPayload::RuntimeOutbox(entry) = &claim.payload else {
            return Ok(false);
        };
        let operation = self
            .store
            .find_operation(&entry.operation_id)
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Store(error.to_string()))?;
        if operation.is_none_or(|operation| operation.terminal.is_some()) {
            return Ok(true);
        }
        let thread = self
            .store
            .load_thread(&entry.thread_id)
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Store(error.to_string()))?;
        let Some(thread) = thread else {
            return Ok(true);
        };
        if matches!(
            thread.status,
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost
                | agentdash_agent_runtime_contract::RuntimeThreadStatus::Closed
        ) {
            return Ok(true);
        }
        Ok(!entry.matches_thread_binding(&thread))
    }

    async fn dispatch_claim(
        &self,
        claim: &RuntimeWorkClaim,
    ) -> Result<(), RuntimeOutboxWorkerError> {
        let RuntimeWorkPayload::RuntimeOutbox(entry) = &claim.payload else {
            return Err(RuntimeOutboxWorkerError::InvalidClaim(
                "claim payload is not RuntimeOutbox".to_string(),
            ));
        };
        let thread = self
            .store
            .load_thread(&entry.thread_id)
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeOutboxWorkerError::InvalidClaim(format!(
                    "Runtime thread {} does not exist",
                    entry.thread_id
                ))
            })?;
        let operation = self
            .store
            .find_operation(&entry.operation_id)
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeOutboxWorkerError::InvalidClaim(
                    "outbox operation does not exist".to_string(),
                )
            })?;
        if operation.terminal.is_some() {
            return Err(RuntimeOutboxWorkerError::InvalidClaim(
                "outbox operation is already terminal".to_string(),
            ));
        }
        if !entry.matches_thread_binding(&thread) {
            return Err(RuntimeOutboxWorkerError::InvalidClaim(
                "outbox binding identity no longer matches the Runtime thread".to_string(),
            ));
        }
        let request_id = DriverRequestId::new(format!("request-{}", entry.operation_id))
            .map_err(|error| RuntimeOutboxWorkerError::InvalidClaim(error.to_string()))?;
        let runtime_turn_id = matches!(
            &entry.command,
            RuntimeCommand::ThreadStart { .. } | RuntimeCommand::TurnStart { .. }
        )
        .then(|| agentdash_agent_runtime::canonical_turn_id(&entry.operation_id));
        let mut lease = self
            .host
            .acquire_driver_lease(&entry.binding_id)
            .await
            .map_err(|error| RuntimeOutboxWorkerError::Host(error.to_string()))?;
        let sink: Arc<dyn DriverEventSink> = Arc::new(ManagedRuntimeDriverEventSink {
            runtime: self.runtime.clone(),
        });
        let driver_envelope = DriverCommandEnvelope {
            request_id,
            operation_id: entry.operation_id.clone(),
            presentation_thread_id: entry.presentation_thread_id.clone(),
            binding_id: entry.binding_id.clone(),
            generation: entry.generation,
            source_thread_id: thread.source_thread_id.clone(),
            runtime_turn_id,
            presentation_turn_id: match &entry.command {
                RuntimeCommand::ThreadStart {
                    presentation_turn_id,
                    ..
                } => presentation_turn_id.clone(),
                RuntimeCommand::TurnStart {
                    presentation_turn_id,
                    ..
                } => Some(presentation_turn_id.clone()),
                _ => thread.active_turn_id.as_ref().and_then(|turn_id| {
                    thread
                        .turns
                        .get(turn_id)
                        .map(|turn| turn.presentation_turn_id.clone())
                }),
            },
            command: entry.command.clone(),
        };
        #[cfg(test)]
        self.observed_dispatches
            .lock()
            .await
            .push(driver_envelope.clone());
        let dispatch = self.host.dispatch(
            RouteDriverCommand {
                envelope: driver_envelope,
                lease_owner: lease.owner.clone(),
                lease_token: lease.token.clone(),
            },
            sink,
        );
        tokio::pin!(dispatch);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        let result: Result<_, RuntimeOutboxWorkerError> = loop {
            tokio::select! {
                result = &mut dispatch => break match result {
                    Err(error) => Err(classify_outbox_dispatch_error(error)),
                    Ok(receipt) => Ok(receipt),
                },
                _ = heartbeat.tick() => {
                    match self.host.renew_driver_lease(&lease).await {
                        Ok(renewed) => lease = renewed,
                        Err(error) => break Err(RuntimeOutboxWorkerError::Host(error.to_string())),
                    }
                }
            }
        };
        let release_result = self.host.release_driver_lease(&lease).await;
        release_result.map_err(|error| RuntimeOutboxWorkerError::Host(error.to_string()))?;
        match result {
            Err(RuntimeOutboxWorkerError::BindingLost(reason)) => {
                self.runtime
                    .ingest_driver_event(DriverEventEnvelope {
                        binding_id: entry.binding_id.clone(),
                        generation: entry.generation,
                        operation_id: Some(entry.operation_id.clone()),
                        source_thread_id: thread.source_thread_id,
                        source_turn_id: None,
                        source_item_id: None,
                        source_request_id: Some(entry.operation_id.as_str().to_string()),
                        source_entry_index: None,
                        facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                            binding_id: entry.binding_id.clone(),
                            reason,
                        })],
                    })
                    .await
                    .map_err(|error| RuntimeOutboxWorkerError::Host(error.to_string()))?;
                Ok(())
            }
            result => {
                result?;
                if operation_completes_at_driver_acceptance(&entry.command) {
                    self.runtime
                        .complete_driver_dispatch_operation(&entry.thread_id, &entry.operation_id)
                        .await
                        .map_err(|error| RuntimeOutboxWorkerError::Host(error.to_string()))?;
                }
                Ok(())
            }
        }
    }

    pub fn spawn(self: Arc<Self>, cancellation: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut poll = tokio::time::interval(std::time::Duration::from_millis(250));
            poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    _ = poll.tick() => {
                        match self.run_once(16).await {
                            Ok(0) => {},
                            Ok(_) => {},
                            Err(error) => diag!(
                                Error,
                                Subsystem::AgentRun,
                                error = error.to_string(),
                                "Runtime outbox worker dispatch failed"
                            ),
                        }
                    }
                }
            }
        })
    }
}

fn operation_completes_at_driver_acceptance(command: &RuntimeCommand) -> bool {
    match command {
        RuntimeCommand::ThreadStart { input, .. } => input.is_empty(),
        RuntimeCommand::ThreadResume { .. }
        | RuntimeCommand::ThreadFork { .. }
        | RuntimeCommand::ThreadSettingsUpdate { .. }
        | RuntimeCommand::TurnSteer { .. }
        | RuntimeCommand::TurnInterrupt { .. }
        | RuntimeCommand::InteractionRespond { .. }
        | RuntimeCommand::ToolSetReplace { .. }
        | RuntimeCommand::SurfaceAdopt { .. } => true,
        RuntimeCommand::ThreadRebind { .. }
        | RuntimeCommand::TurnStart { .. }
        | RuntimeCommand::ContextCompact { .. } => false,
    }
}

#[derive(Debug, Error)]
pub enum AgentRuntimeCompositionError {
    #[error("Agent Runtime composition is invalid: {0}")]
    Invalid(String),
}

pub struct NativeAgentRuntimeCompositionInput {
    pub pool: PgPool,
    pub provider_repository: Arc<dyn LlmProviderRepository>,
    pub provider_credential_repository: Arc<dyn LlmProviderCredentialRepository>,
    pub secret_codec: Arc<dyn LlmSecretCodec>,
    pub surface_compiler: Arc<dyn NativeAgentRunSurfaceCompiler>,
    pub credential_broker: Arc<dyn AgentRuntimeCredentialBroker>,
    pub callback_factory: AgentRuntimeCallbackFactory,
    pub application_presentation_projector:
        Arc<dyn agentdash_agent_runtime_contract::RuntimeApplicationPresentationProjector>,
    pub remote_definitions: Vec<agentdash_integration_api::AgentServiceDefinition>,
    pub remote_trust_manifests: Vec<agentdash_integration_api::AgentRuntimeTrustManifest>,
    pub remote_placements:
        Arc<dyn agentdash_integration_remote_runtime::RuntimeWirePlacementResolver>,
    pub node_id: String,
}

pub struct AgentRuntimeCompositionInput {
    pub pool: PgPool,
    pub contributions: Vec<agentdash_integration_api::AgentRuntimeDriverContribution>,
    pub trusted_manifests: Vec<TrustedDriverManifest>,
    pub surface_source: Arc<dyn AgentRunRuntimeSurfaceSource>,
    pub credential_broker: Arc<dyn AgentRuntimeCredentialBroker>,
    pub callback_factory: AgentRuntimeCallbackFactory,
    pub application_presentation_projector:
        Arc<dyn agentdash_agent_runtime_contract::RuntimeApplicationPresentationProjector>,
    pub managed_compaction:
        Option<Arc<dyn crate::agent_runtime_workers::ManagedCompactionPreparationEngine>>,
    pub node_id: String,
}

pub struct AgentRuntimeCallbacks {
    pub tools: Arc<dyn AgentRuntimeToolCallback>,
    pub hooks: Arc<dyn AgentRuntimeHookCallback>,
}

pub type AgentRuntimeCallbackFactory = Arc<
    dyn Fn(Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>) -> AgentRuntimeCallbacks
        + Send
        + Sync,
>;

pub struct AgentRuntimeComposition {
    pub gateway: Arc<dyn AgentRuntimeGateway>,
    pub host: Arc<IntegrationDriverHost>,
    pub provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    pub bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub outbox_worker: Arc<RuntimeOutboxWorker>,
    pub durable_workers: Arc<crate::agent_runtime_workers::RuntimeDurableWorkers>,
    pub work_queue: Arc<dyn RuntimeWorkQueue>,
    pub presentation_events: Arc<dyn RuntimeTransientEvents>,
    pub runtime_repository: Arc<PostgresRuntimeRepository>,
    pub managed_runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
    pub surfaces: Arc<dyn AgentRunRuntimeSurfaceStore>,
    pub presentation_plans: Arc<
        dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimePresentationPlanStore,
    >,
}

pub type NativeAgentRuntimeComposition = AgentRuntimeComposition;

pub fn build_agent_runtime_composition(
    input: AgentRuntimeCompositionInput,
) -> Result<AgentRuntimeComposition, AgentRuntimeCompositionError> {
    let runtime_repository = Arc::new(PostgresRuntimeRepository::new(input.pool.clone()));
    let host_repository = Arc::new(PostgresAgentRuntimeHostRepository::new(input.pool.clone()));
    let composition_repository = Arc::new(PostgresAgentRuntimeCompositionRepository::new(
        input.pool.clone(),
    ));
    let context_broker = Arc::new(PostgresAgentRuntimeContextBroker::new(
        runtime_repository.clone(),
        composition_repository.clone(),
    ));
    let runtime = Arc::new(
        ManagedAgentRuntime::new(
            runtime_repository.clone(),
            input.application_presentation_projector,
        )
        .with_surface_validator(composition_repository.clone()),
    );
    let callbacks = (input.callback_factory)(runtime.clone());
    let verifier = Arc::new(TrustedDriverConformanceVerifier::new(
        TrustedDriverManifestRegistry::collect(input.trusted_manifests)
            .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?,
    ));
    let registry = AgentServiceDefinitionRegistry::collect(input.contributions)
        .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
    let host_repository_port: Arc<dyn AgentRuntimeHostRepository> = host_repository.clone();
    let host = Arc::new(IntegrationDriverHost::new(
        registry,
        host_repository_port.clone(),
        RuntimeDriverHostPorts {
            credentials: input.credential_broker,
            surfaces: composition_repository.clone(),
            context: context_broker,
            tools: callbacks.tools,
            hooks: callbacks.hooks,
        },
        verifier,
        input.node_id,
    ));
    let bindings: Arc<dyn AgentRunRuntimeBindingRepository> = composition_repository.clone();
    let surface_store: Arc<dyn AgentRunRuntimeSurfaceStore> = composition_repository.clone();
    let presentation_plans: Arc<
        dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimePresentationPlanStore,
    > = composition_repository.clone();
    let gateway: Arc<dyn AgentRuntimeGateway> = runtime.clone();
    let provisioner: Arc<dyn AgentRunRuntimeProvisioner> =
        Arc::new(HostAgentRunRuntimeProvisioner::new(
            host.clone(),
            host_repository_port.clone(),
            bindings.clone(),
            surface_store.clone(),
            input.surface_source,
            gateway.clone(),
        ));
    let work_queue: Arc<dyn RuntimeWorkQueue> = runtime_repository.clone();
    let presentation_events: Arc<dyn RuntimeTransientEvents> = runtime_repository.clone();
    let outbox_worker = Arc::new(RuntimeOutboxWorker::new(
        runtime_repository.clone(),
        runtime.clone(),
        host.clone(),
        "agentdash-api-runtime-outbox",
    ));
    let durable_workers = Arc::new(crate::agent_runtime_workers::RuntimeDurableWorkers::new(
        runtime_repository.clone(),
        runtime.clone(),
        composition_repository,
        host.clone(),
        host_repository_port,
        input.managed_compaction,
        Arc::new(crate::agent_runtime_workers::DiagnosticRuntimeHookEffectDispatcher),
        "agentdash-api-runtime-durable",
    ));
    Ok(AgentRuntimeComposition {
        gateway,
        host,
        provisioner,
        bindings,
        outbox_worker,
        durable_workers,
        work_queue,
        presentation_events,
        runtime_repository: runtime_repository.clone(),
        managed_runtime: runtime,
        surfaces: surface_store,
        presentation_plans,
    })
}

/// 在 PostgreSQL repositories 与 secret codec 已建立后装配 Native Runtime。
///
/// 该顺序确保 Runtime Integration registry 收集到的 factory 已持有真实 resolver；宿主不再
/// 在 repository bootstrap 之前收集一个无法激活的占位 contribution。
pub fn build_native_agent_runtime_composition(
    input: NativeAgentRuntimeCompositionInput,
) -> Result<NativeAgentRuntimeComposition, AgentRuntimeCompositionError> {
    let managed_compaction = Arc::new(
        crate::agent_runtime_workers::NativeManagedCompactionEngine::new(
            input.provider_repository.clone(),
            input.provider_credential_repository.clone(),
            input.secret_codec.clone(),
            crate::agent_runtime_workers::ManagedCompactionPolicy {
                keep_last_n: 20,
                reserve_tokens: 16_384,
            },
        ),
    );
    let resolver: Arc<dyn NativeBridgeResolver> = Arc::new(RepositoryNativeBridgeResolver::new(
        input.provider_repository,
        input.provider_credential_repository,
        input.secret_codec,
    ));
    let integration = NativeAgentRuntimeIntegration::new(resolver);
    let mut contributions = integration.agent_runtime_drivers();
    let contribution = contributions.pop().ok_or_else(|| {
        AgentRuntimeCompositionError::Invalid(
            "Native Integration did not contribute a Runtime driver".to_string(),
        )
    })?;
    if !contributions.is_empty() {
        return Err(AgentRuntimeCompositionError::Invalid(
            "Native Integration contributed more than one Runtime driver".to_string(),
        ));
    }
    let definition = contribution.definition.clone();
    if definition.provenance.definition_id.as_str() != NATIVE_DEFINITION_ID {
        return Err(AgentRuntimeCompositionError::Invalid(format!(
            "unexpected Native definition id {}",
            definition.provenance.definition_id
        )));
    }
    let trust_manifest = native_runtime_trust_manifest();
    if trust_manifest.provenance != definition.provenance
        || trust_manifest.verified_profile != definition.service_profile_upper_bound
    {
        return Err(AgentRuntimeCompositionError::Invalid(
            "Native driver contribution does not match its trusted Integration manifest"
                .to_string(),
        ));
    }
    let verified_profile_digest = profile_digest(&trust_manifest.verified_profile)
        .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
    let manifest = TrustedDriverManifest {
        provenance: trust_manifest.provenance,
        suite_revision: trust_manifest.suite_revision,
        driver_build_digest: trust_manifest.driver_build_digest,
        protocol_revision: trust_manifest.protocol_revision,
        verified_profile_digest,
    };
    let surface_source: Arc<dyn AgentRunRuntimeSurfaceSource> =
        Arc::new(NativeAgentRunRuntimeSurfaceSource::new(
            input.surface_compiler,
            definition,
            input.remote_definitions.clone(),
        )?);
    let mut runtime_contributions = vec![contribution];
    let mut trusted_manifests = vec![manifest];
    let mut remote_manifests = input
        .remote_trust_manifests
        .into_iter()
        .map(|manifest| (manifest.provenance.definition_id.clone(), manifest))
        .collect::<std::collections::BTreeMap<_, _>>();
    for definition in input.remote_definitions {
        let manifest = remote_manifests
            .remove(&definition.provenance.definition_id)
            .ok_or_else(|| {
                AgentRuntimeCompositionError::Invalid(format!(
                    "remote definition {} has no trusted Integration manifest",
                    definition.provenance.definition_id
                ))
            })?;
        if manifest.provenance != definition.provenance
            || manifest.driver_build_digest != definition.provenance.build_digest.as_str()
            || !definition
                .supported_protocol_revisions
                .contains(&manifest.protocol_revision)
            || manifest.verified_profile != definition.service_profile_upper_bound
        {
            return Err(AgentRuntimeCompositionError::Invalid(format!(
                "remote definition {} does not match its trusted Integration manifest",
                definition.provenance.definition_id
            )));
        }
        let verified_profile_digest = profile_digest(&manifest.verified_profile)
            .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
        trusted_manifests.push(TrustedDriverManifest {
            provenance: manifest.provenance,
            suite_revision: manifest.suite_revision,
            driver_build_digest: manifest.driver_build_digest,
            protocol_revision: manifest.protocol_revision,
            verified_profile_digest,
        });
        let mut proxy_definition = definition;
        proxy_definition.factory_key = agentdash_integration_api::AgentRuntimeFactoryKey::new(
            format!("remote-proxy.{}", proxy_definition.provenance.definition_id),
        )
        .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
        proxy_definition.config_schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sourceServiceInstanceId", "sourceDriverGeneration", "sourceHostIncarnationId"],
            "properties": {
                "sourceServiceInstanceId": { "type": "string", "minLength": 1 },
                "sourceDriverGeneration": { "type": "integer", "minimum": 1 },
                "sourceHostIncarnationId": { "type": "string", "minLength": 1 }
            }
        });
        proxy_definition.config_schema_digest =
            agentdash_integration_api::AgentServiceSchemaDigest::new(
                agentdash_agent_runtime_host::schema_digest(&proxy_definition.config_schema),
            )
            .map_err(|error| AgentRuntimeCompositionError::Invalid(error.to_string()))?;
        proxy_definition.credential_slots.clear();
        runtime_contributions.push(
            agentdash_integration_remote_runtime::remote_runtime_contribution(
                proxy_definition,
                input.remote_placements.clone(),
            ),
        );
    }
    if !remote_manifests.is_empty() {
        return Err(AgentRuntimeCompositionError::Invalid(format!(
            "trusted Integration manifests reference {} unavailable remote definitions",
            remote_manifests.len()
        )));
    }
    build_agent_runtime_composition(AgentRuntimeCompositionInput {
        pool: input.pool,
        contributions: runtime_contributions,
        trusted_manifests,
        surface_source,
        credential_broker: input.credential_broker,
        callback_factory: input.callback_factory,
        application_presentation_projector: input.application_presentation_projector,
        managed_compaction: Some(managed_compaction),
        node_id: input.node_id,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::pin::Pin;

    use super::*;
    use agentdash_agent::{
        AgentMessage, BridgeRequest, BridgeResponse, ContentPart, LlmBridge, StreamChunk,
        TokenUsage,
    };
    use agentdash_agent_runtime_contract::*;
    use agentdash_domain::{
        common::error::DomainError,
        llm_provider::{LlmCredentialMode, LlmProvider, LlmProviderUserCredential, WireProtocol},
    };
    use agentdash_integration_api::*;
    use agentdash_integration_codex::{
        CodexAppServerLauncher, codex_runtime_contribution_with_launcher,
        codex_runtime_trust_manifest,
    };
    use agentdash_spi::{AuthIdentity, AuthMode};
    use futures::stream;
    use sqlx::postgres::PgConnectOptions;
    use uuid::Uuid;

    fn snapshot_contains_agent_message(snapshot: &RuntimeSnapshot, expected: &str) -> bool {
        snapshot.transcript.iter().any(|entry| {
            let agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed) =
                &entry.terminal_event.event
            else {
                return false;
            };
            let agentdash_agent_protocol::AgentDashThreadItem::Codex(
                agentdash_agent_protocol::CodexThreadItem::AgentMessage { text, .. },
            ) = &completed.item
            else {
                return false;
            };
            text == expected
        })
    }

    fn fixture_terminal_hook_effect_binding() -> RuntimeTerminalHookEffectBinding {
        RuntimeTerminalHookEffectBinding {
            handler: RuntimeTerminalHookEffectHandlerRef {
                handler_type: RuntimeTerminalHookEffectHandlerType::new("agent_run_post_turn")
                    .expect("terminal handler type"),
                handler_id: RuntimeTerminalHookEffectHandlerId::new("handler-fixture")
                    .expect("terminal handler id"),
                revision: RuntimeTerminalHookEffectHandlerRevision(7),
            },
            supported_effect_kinds: BTreeSet::from([RuntimeHookEffectKind::new(
                "agent_run_control_effect",
            )
            .expect("terminal effect kind")]),
        }
    }

    struct TestTerminalPresentationProjector;

    impl RuntimeApplicationPresentationProjector for TestTerminalPresentationProjector {
        fn project_terminal(
            &self,
            context: RuntimeTerminalPresentationContext,
        ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError>
        {
            let terminal_type = match context.terminal {
                RuntimeTurnTerminal::Completed => "turn_completed",
                RuntimeTurnTerminal::Interrupted => "turn_interrupted",
                RuntimeTurnTerminal::Lost => "turn_lost",
                RuntimeTurnTerminal::Refused
                | RuntimeTurnTerminal::LimitReached
                | RuntimeTurnTerminal::Failed => "turn_failed",
            };
            Ok(vec![RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(context.runtime_turn_id.clone()),
                    presentation_turn_id: Some(context.presentation_turn_id.clone()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(context.presentation_thread_id.to_string()),
                    source_turn_id: Some(context.presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some(format!(
                        "test-turn-terminal:{}:{terminal_type}",
                        context.runtime_turn_id
                    )),
                    source_entry_index: None,
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: "turn_terminal".into(),
                            value: serde_json::json!({
                                "terminal_type": terminal_type,
                                "message": context.message,
                                "diagnostic": context.diagnostic,
                                "started_at_ms": context.started_at_ms,
                                "completed_at_ms": context.completed_at_ms,
                                "duration_ms": context.started_at_ms.map(|started_at_ms| {
                                    context.completed_at_ms.saturating_sub(started_at_ms)
                                }),
                            }),
                        },
                    ),
                ),
            }])
        }
    }

    #[test]
    fn outbox_classifies_missing_driver_binding_as_binding_lost() {
        let error =
            classify_outbox_dispatch_error(AgentRuntimeHostError::Driver(DriverError::Lost {
                reason: "binding disappeared with the ephemeral host".to_string(),
                retryable: true,
            }));

        assert!(matches!(
            error,
            RuntimeOutboxWorkerError::BindingLost(reason)
                if reason == "binding disappeared with the ephemeral host"
        ));
    }

    #[test]
    fn outbox_classifies_runtime_terminalization_without_fabricating_binding_loss() {
        assert!(matches!(
            classify_outbox_dispatch_error(AgentRuntimeHostError::Driver(
                DriverError::Terminalized {
                    reason: "canonical terminal committed".into(),
                }
            )),
            RuntimeOutboxWorkerError::Terminalized(reason)
                if reason == "canonical terminal committed"
        ));
    }

    #[tokio::test]
    async fn active_outbox_terminalized_dispatch_is_released_until_canonical_state_is_terminal() {
        let (pool, _postgres, _serial) = test_database().await;
        let integration = NativeAgentRuntimeIntegration::new(Arc::new(EchoResolver));
        let contribution = integration
            .agent_runtime_drivers()
            .pop()
            .expect("Native contribution");
        let definition = contribution.definition.clone();
        let manifest = native_runtime_trust_manifest();
        let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
            pool: pool.clone(),
            contributions: vec![contribution],
            trusted_manifests: vec![TrustedDriverManifest {
                verified_profile_digest: profile_digest(&manifest.verified_profile)
                    .expect("profile digest"),
                provenance: manifest.provenance,
                suite_revision: manifest.suite_revision,
                driver_build_digest: manifest.driver_build_digest,
                protocol_revision: manifest.protocol_revision,
            }],
            surface_source: Arc::new(
                NativeAgentRunRuntimeSurfaceSource::new(
                    Arc::new(FixtureSurfaceCompiler),
                    definition,
                    Vec::new(),
                )
                .expect("Native surface source"),
            ),
            credential_broker: Arc::new(NoCredentials),
            callback_factory: Arc::new(|_| AgentRuntimeCallbacks {
                tools: Arc::new(NoTools),
                hooks: Arc::new(ContinueHooks),
            }),
            application_presentation_projector: Arc::new(TestTerminalPresentationProjector),
            managed_compaction: None,
            node_id: "terminalized-outbox-test".to_string(),
        })
        .expect("build outbox test Host");
        let suffix = Uuid::new_v4().simple().to_string();
        let binding_id: RuntimeBindingId = parsed(&format!("binding-{suffix}"));
        let source_thread_id: DriverThreadId = parsed(&format!("source-{suffix}"));
        let thread_id: RuntimeThreadId = parsed(&format!("thread-{suffix}"));
        let operation_id: RuntimeOperationId = parsed(&format!("operation-{suffix}"));
        let profile = native_runtime_profile();
        let profile_digest = profile_digest(&profile).expect("Native profile digest");
        sqlx::query(
            "INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) \
             VALUES ($1,1,$2)",
        )
        .bind(binding_id.as_str())
        .bind(profile_digest.as_str())
        .execute(&pool)
        .await
        .expect("seed Runtime binding");
        sqlx::query(
            "INSERT INTO agent_runtime_source_coordinate \
             (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)",
        )
        .bind(binding_id.as_str())
        .bind(source_thread_id.as_str())
        .bind(thread_id.as_str())
        .execute(&pool)
        .await
        .expect("seed Runtime source coordinate");
        let store = composition.runtime_repository.clone();
        let runtime = Arc::new(ManagedAgentRuntime::new(
            store.clone(),
            Arc::new(TestTerminalPresentationProjector),
        ));
        runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: operation_id.clone(),
                    idempotency_key: parsed(&format!("key-{suffix}")),
                    expected_thread_revision: None,
                    actor: RuntimeActor::System {
                        component: "terminalized-outbox-test".to_string(),
                    },
                },
                command: RuntimeCommand::ThreadStart {
                    thread_id: thread_id.clone(),
                    presentation_thread_id: parsed(&format!("presentation-{suffix}")),
                    presentation_turn_id: None,
                    binding_id,
                    driver_generation: RuntimeDriverGeneration(1),
                    source_thread_id,
                    profile_digest,
                    bound_profile: Box::new(profile),
                    input: Vec::new(),
                    surface: Box::new(RuntimeSurfaceDescriptor {
                        source_frame_id: format!("frame-{suffix}"),
                        surface_revision: SurfaceRevision(1),
                        surface_digest: parsed(&format!("surface-{suffix}")),
                        vfs_digest: format!("vfs-{suffix}"),
                        context_recipe_revision: ContextRecipeRevision(1),
                        context_digest: parsed(&format!("context-{suffix}")),
                        settings_revision: ThreadSettingsRevision(0),
                        tool_set_revision: ToolSetRevision(0),
                        tool_set_digest: format!("tools-{suffix}"),
                        hook_plan: BoundRuntimeHookPlan {
                            revision: HookPlanRevision(1),
                            digest: parsed(&format!("hooks-{suffix}")),
                            entries: Vec::new(),
                        },
                        terminal_hook_effect_binding: None,
                    }),
                    settings_revision: ThreadSettingsRevision(0),
                },
            })
            .await
            .expect("accept active outbox operation");
        let worker = RuntimeOutboxWorker::new(
            store.clone(),
            runtime.clone(),
            composition.host.clone(),
            "terminalized-outbox-worker",
        );
        let claim_request = || RuntimeWorkClaimRequest {
            kind: RuntimeWorkKind::RuntimeOutbox,
            owner: RuntimeWorkerId("terminalized-outbox-worker".to_string()),
            lease_duration_ms: 30_000,
            limit: 8,
        };
        let first_claim = store
            .claim(claim_request())
            .await
            .expect("claim active outbox")
            .into_iter()
            .find(|claim| {
                matches!(
                    &claim.identity,
                    agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id)
                        if id == &operation_id
                )
            })
            .expect("active operation claim");
        let fabricated = || {
            RuntimeOutboxWorkerError::Terminalized(
                "fabricated terminalized result while canonical operation is active".to_string(),
            )
        };
        assert!(matches!(
            worker
                .settle_dispatch_result(&first_claim, Err(fabricated()))
                .await,
            Err(RuntimeOutboxWorkerError::Terminalized(_))
        ));
        let released: (bool, bool, Option<String>) = sqlx::query_as(
            "SELECT dispatched_at IS NULL,claim_token IS NULL,last_error \
             FROM agent_runtime_outbox WHERE operation_id=$1",
        )
        .bind(operation_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("load released active outbox");
        assert_eq!((released.0, released.1), (true, true));
        assert!(
            released
                .2
                .as_deref()
                .is_some_and(|error| error.contains("fabricated terminalized result"))
        );

        let second_claim = store
            .claim(claim_request())
            .await
            .expect("reclaim released outbox")
            .into_iter()
            .find(|claim| {
                matches!(
                    &claim.identity,
                    agentdash_agent_runtime::RuntimeWorkIdentity::Operation(id)
                        if id == &operation_id
                )
            })
            .expect("released operation remains claimable");
        runtime
            .complete_driver_dispatch_operation(&thread_id, &operation_id)
            .await
            .expect("terminalize canonical operation during dispatch");
        worker
            .settle_dispatch_result(&second_claim, Err(fabricated()))
            .await
            .expect("canonical terminal re-read acks obsolete claim");
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT dispatched_at IS NOT NULL FROM agent_runtime_outbox WHERE operation_id=$1",
            )
            .bind(operation_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("load acked obsolete outbox")
        );
    }

    struct ProviderRepository(LlmProvider);

    #[async_trait]
    impl LlmProviderRepository for ProviderRepository {
        async fn create(&self, _provider: &LlmProvider) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get_by_id(&self, _id: Uuid) -> Result<Option<LlmProvider>, DomainError> {
            unimplemented!()
        }
        async fn list_all(&self) -> Result<Vec<LlmProvider>, DomainError> {
            Ok(vec![self.0.clone()])
        }
        async fn list_enabled(&self) -> Result<Vec<LlmProvider>, DomainError> {
            unimplemented!()
        }
        async fn update(&self, _provider: &LlmProvider) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn reorder(&self, _ids: &[Uuid]) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    struct NoUserCredentials;

    #[async_trait]
    impl LlmProviderCredentialRepository for NoUserCredentials {
        async fn get_for_user_provider(
            &self,
            _user_id: &str,
            _provider_id: Uuid,
        ) -> Result<Option<LlmProviderUserCredential>, DomainError> {
            Ok(None)
        }
        async fn list_for_user(
            &self,
            _user_id: &str,
        ) -> Result<Vec<LlmProviderUserCredential>, DomainError> {
            Ok(Vec::new())
        }
        async fn upsert_for_user_provider(
            &self,
            _credential: &LlmProviderUserCredential,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn delete_for_user_provider(
            &self,
            _user_id: &str,
            _provider_id: Uuid,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    struct UserCredentials(LlmProviderUserCredential);

    #[async_trait]
    impl LlmProviderCredentialRepository for UserCredentials {
        async fn get_for_user_provider(
            &self,
            user_id: &str,
            provider_id: Uuid,
        ) -> Result<Option<LlmProviderUserCredential>, DomainError> {
            Ok(
                (self.0.user_id == user_id && self.0.provider_id == provider_id)
                    .then(|| self.0.clone()),
            )
        }
        async fn list_for_user(
            &self,
            user_id: &str,
        ) -> Result<Vec<LlmProviderUserCredential>, DomainError> {
            Ok((self.0.user_id == user_id)
                .then(|| self.0.clone())
                .into_iter()
                .collect())
        }
        async fn upsert_for_user_provider(
            &self,
            _credential: &LlmProviderUserCredential,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn delete_for_user_provider(
            &self,
            _user_id: &str,
            _provider_id: Uuid,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    struct PlaintextCodec;

    impl LlmSecretCodec for PlaintextCodec {
        fn encrypt(&self, plaintext: &str) -> Result<String, DomainError> {
            Ok(plaintext.to_string())
        }
        fn decrypt(&self, ciphertext: &str) -> Result<String, DomainError> {
            Ok(ciphertext.to_string())
        }
    }

    struct NoCredentials;

    struct NoRemotePlacements;

    #[async_trait]
    impl agentdash_integration_remote_runtime::RuntimeWirePlacementResolver for NoRemotePlacements {
        async fn resolve(
            &self,
            _request: agentdash_integration_remote_runtime::RuntimeWirePlacementRequest,
        ) -> Result<
            Arc<dyn agentdash_integration_remote_runtime::RuntimeWirePlacement>,
            agentdash_integration_remote_runtime::RemoteRuntimeTransportError,
        > {
            Err(
                agentdash_integration_remote_runtime::RemoteRuntimeTransportError::Unavailable {
                    reason: "fixture has no remote placements".to_string(),
                    retryable: false,
                },
            )
        }
    }

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
                reason: "fixture has no credential slots".to_string(),
            })
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
                reason: "fixture has no tools".to_string(),
            })
        }
    }

    #[derive(Clone)]
    struct CodexSurfaceAdoptionFixture {
        target: RuntimeSurfaceDescriptor,
        presentation_thread_id: PresentationThreadId,
        presentation: agentdash_agent_runtime::RuntimeSurfacePresentationPlan,
    }

    struct CodexSurfaceAdoptingTool {
        runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
        fixture: Arc<tokio::sync::RwLock<Option<CodexSurfaceAdoptionFixture>>>,
    }

    #[async_trait]
    impl AgentRuntimeToolCallback for CodexSurfaceAdoptingTool {
        async fn invoke(
            &self,
            request: DriverToolInvocation,
        ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
            if request.tool_name != "surface_update" {
                return Err(DriverToolCallbackError::ProtocolViolation {
                    reason: format!("unexpected Codex tracer tool `{}`", request.tool_name),
                });
            }
            let fixture = self.fixture.read().await.clone().ok_or_else(|| {
                DriverToolCallbackError::Unavailable {
                    reason: "Codex surface adoption fixture is not prepared".to_string(),
                    retryable: false,
                }
            })?;
            let RuntimeSnapshotResult::Thread { snapshot } = self
                .runtime
                .snapshot(RuntimeSnapshotQuery::Thread {
                    thread_id: request.thread_id.clone(),
                    at_revision: None,
                })
                .await
                .map_err(|error| DriverToolCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                })?
            else {
                return Err(DriverToolCallbackError::ProtocolViolation {
                    reason: "Codex surface tool did not resolve its Runtime thread".to_string(),
                });
            };
            let operation_id = format!(
                "codex-tool-surface-adopt-{}-{}",
                request.turn_id, fixture.target.surface_revision.0
            );
            let presentation = fixture.presentation.adoption_presentation(
                &fixture.presentation_thread_id,
                snapshot.active_presentation_turn_id.as_ref(),
                &operation_id,
            );
            self.runtime
                .execute(RuntimeCommandEnvelope {
                    presentation,
                    meta: OperationMeta {
                        operation_id: RuntimeOperationId::new(operation_id.clone())
                            .expect("surface tool operation id"),
                        idempotency_key: IdempotencyKey::new(operation_id)
                            .expect("surface tool idempotency key"),
                        expected_thread_revision: Some(snapshot.revision),
                        actor: RuntimeActor::System {
                            component: "codex_surface_update_tool".to_string(),
                        },
                    },
                    command: RuntimeCommand::SurfaceAdopt {
                        thread_id: request.thread_id,
                        expected_surface_revision: snapshot.surface.surface_revision,
                        expected_surface_digest: snapshot.surface.surface_digest,
                        target: Box::new(fixture.target),
                    },
                })
                .await
                .map_err(|error| DriverToolCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                })?;
            Ok(DriverToolOutcome::Completed {
                output: json!([{"type":"text","text":"surface updated"}]),
                is_error: false,
            })
        }
    }

    struct ContinueHooks;

    struct NoopSurfacePublication;

    struct NoopSurfacePublicationReservation;

    #[async_trait]
    impl NativeAgentRunSurfacePublication for NoopSurfacePublication {
        async fn reserve(
            &self,
            _applied: AppliedNativeAgentRunSurface,
        ) -> Result<
            Box<dyn NativeAgentRunSurfacePublicationReservation>,
            AgentRunRuntimeSurfaceSourceError,
        > {
            Ok(Box::new(NoopSurfacePublicationReservation))
        }
    }

    #[async_trait]
    impl NativeAgentRunSurfacePublicationReservation for NoopSurfacePublicationReservation {
        async fn commit(self: Box<Self>) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
            Ok(())
        }

        async fn abort(self: Box<Self>) {}
    }

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

    struct EchoBridge;

    #[async_trait]
    impl LlmBridge for EchoBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            Box::pin(stream::iter(vec![
                StreamChunk::TextDelta("native ".to_string()),
                StreamChunk::TextDelta("response".to_string()),
                StreamChunk::Done(BridgeResponse {
                    message: AgentMessage::assistant("native response"),
                    raw_content: vec![ContentPart::text("native response")],
                    usage: TokenUsage::default(),
                }),
            ]))
        }
    }

    struct EchoResolver;

    #[async_trait]
    impl NativeBridgeResolver for EchoResolver {
        async fn resolve(
            &self,
            _instance: &ActivatedAgentServiceInstance,
            _host: &RuntimeDriverHostPorts,
        ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
            Ok(ResolvedNativeBridge {
                bridge: Arc::new(EchoBridge),
                presentation: NativePresentationMetadata {
                    model_context_window: 200_000,
                    reserve_tokens: NATIVE_STREAM_USAGE_RESERVE_TOKENS,
                },
            })
        }
    }

    struct TracerManagedCompactionEngine;

    #[async_trait]
    impl crate::agent_runtime_workers::ManagedCompactionPreparationEngine
        for TracerManagedCompactionEngine
    {
        async fn compact(
            &self,
            thread: &agentdash_agent_runtime::RuntimeThreadState,
            surface: &agentdash_integration_api::MaterializedDriverSurface,
            _instance: &agentdash_agent_runtime_host::AgentServiceInstance,
            _input: &crate::agent_runtime_workers::ManagedCompactionInput,
            work: &agentdash_agent_runtime::ContextPreparationWorkItem,
        ) -> Result<
            crate::agent_runtime_workers::ManagedCompactionOutput,
            crate::agent_runtime_workers::RuntimeDurableWorkerError,
        > {
            let summary = "native tracer compacted summary".to_string();
            let mut blocks = surface.context.blocks.clone();
            blocks.push(
                agentdash_agent_runtime_contract::ContextBlock::CompactionSummary {
                    summary: summary.clone(),
                },
            );
            Ok(crate::agent_runtime_workers::ManagedCompactionOutput {
                blocks,
                source_item_ids: thread.item_order.clone(),
                presentation: agentdash_agent_runtime::CompactionPresentationFacts {
                    summary,
                    tokens_before: 42,
                    messages_compacted: 1,
                    compaction_id: Some(work.compaction_id.to_string()),
                    projection_version: None,
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("auto".to_string()),
                    phase: Some("standalone_compact_turn".to_string()),
                    source_start_event_seq: None,
                    source_end_event_seq: None,
                    first_kept_event_seq: None,
                    compacted_until_ref: None,
                    timestamp_ms: Some(1_710_000_000_000),
                },
            })
        }
    }

    struct CodexTracerLauncher(std::path::PathBuf);

    impl CodexAppServerLauncher for CodexTracerLauncher {
        fn spawn(
            &self,
            cwd: &std::path::Path,
            _hook_endpoint: Option<&str>,
        ) -> Result<tokio::process::Child, String> {
            let mut command = tokio::process::Command::new("node");
            command
                .arg(&self.0)
                .current_dir(cwd)
                .kill_on_drop(true)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            command.spawn().map_err(|error| error.to_string())
        }
    }

    struct CodexTracerSurfaceSource {
        definition: AgentServiceDefinition,
        root: std::path::PathBuf,
    }

    #[async_trait]
    impl AgentRunRuntimeSurfaceSource for CodexTracerSurfaceSource {
        async fn prepare(
            &self,
            request: &AgentRunRuntimeProvisionRequest,
            thread_id: &RuntimeThreadId,
            _binding_id: &RuntimeBindingId,
        ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError> {
            let mut surface = fixture_surface();
            surface.runtime_thread_id = thread_id.clone();
            surface.hooks.bindings.clear();
            surface.hooks.digest = parsed("sha256:codex-tracer-hooks");
            surface.digest = parsed("sha256:codex-tracer-surface");
            surface.workspace.roots = vec![self.root.display().to_string()];
            surface.tools.tools = vec![DriverToolDefinition {
                name: "surface_update".to_string(),
                description: "Update the canonical AgentFrame surface during the active turn"
                    .to_string(),
                parameters_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection: ToolProtocolProjection::Dynamic {
                    namespace: Some("platform".to_string()),
                },
                presentation_emitter: ToolPresentationEmitter::VendorStream,
                parity_fixture_id: "codex-surface-update-tracer".to_string(),
            }];
            let profile = self.definition.service_profile_upper_bound.clone();
            let digest = profile_digest(&profile).map_err(|error| {
                AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: error.to_string(),
                }
            })?;
            let business_surface = fixture_business_surface(&surface);
            Ok(PreparedAgentRunRuntime {
                source_frame_id: "fixture-frame".to_string(),
                service_instance_id: parsed("codex-tracer-service"),
                definition_id: self.definition.provenance.definition_id.clone(),
                service_config: json!({
                    "cwd": self.root,
                    "artifactRoot": self.root.join("artifacts"),
                    "runtimeWorkspaceRoots": [self.root]
                }),
                placement: AgentRuntimePlacement::InProcess,
                bound_surface: bound_surface_reference(&surface),
                hook_plan: RuntimeHookPlanBinding {
                    thread_id: thread_id.clone(),
                    plan: BoundRuntimeHookPlan {
                        revision: HookPlanRevision(1),
                        digest: surface.hooks.digest.clone(),
                        entries: Vec::new(),
                    },
                },
                publication: Arc::new(NoopSurfacePublication),
                terminal_hook_effect_binding: request.terminal_hook_effect_binding.clone(),
                surface,
                business_surface,
                transport_profile: profile.clone(),
                host_policy_profile: profile,
                conformance: ConformanceEvidence {
                    suite_revision: "codex-app-server-runtime-v1".to_string(),
                    driver_build_digest: self.definition.provenance.build_digest.to_string(),
                    verified_profile_digest: digest,
                    verified_at: Utc::now(),
                },
                allow_instance_creation: true,
                context_delivery_target:
                    agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                        connector_id: "codex".to_string(),
                        executor: "CODEX".to_string(),
                    },
            })
        }
    }

    struct FixtureSurfaceCompiler;

    #[async_trait]
    impl NativeAgentRunSurfaceCompiler for FixtureSurfaceCompiler {
        async fn compile(
            &self,
            request: &AgentRunRuntimeProvisionRequest,
            _thread_id: &RuntimeThreadId,
            _binding_id: &RuntimeBindingId,
        ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
            let surface = fixture_surface();
            let business_surface = fixture_business_surface(&surface);
            Ok(NativeAgentRunSurfacePlan {
                source_frame_id: "fixture-frame".to_string(),
                executor: "PI_AGENT".to_string(),
                provider: Some("openai".to_string()),
                model: Some("gpt-test".to_string()),
                surface,
                business_surface,
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: parsed("sha256:production-native-hooks"),
                    entries: vec![
                        BoundRuntimeHookEntry {
                            definition_id: parsed("native-tracer-hook"),
                            point: HookPoint::BeforeTool,
                            actions: BTreeSet::from([HookAction::Observe, HookAction::Block]),
                            delivered_strength: SemanticStrength::ExactSynchronous,
                            failure_policy: HookFailurePolicy::FailClosed,
                            required: true,
                            site: HookExecutionSite::AgentCoreCallback,
                        },
                        BoundRuntimeHookEntry {
                            definition_id: parsed("native-tracer-effect-hook"),
                            point: HookPoint::AfterTool,
                            actions: BTreeSet::from([HookAction::Observe, HookAction::EmitEffect]),
                            delivered_strength: SemanticStrength::ExactSynchronous,
                            failure_policy: HookFailurePolicy::FailClosed,
                            required: true,
                            site: HookExecutionSite::AgentCoreCallback,
                        },
                    ],
                },
                publication: Arc::new(NoopSurfacePublication),
                terminal_hook_effect_binding: request.terminal_hook_effect_binding.clone(),
            })
        }
    }

    fn parsed<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid fixture id")
    }

    #[test]
    fn explicit_backend_selection_matches_only_the_requested_remote_host() {
        let selection = BackendSelectionInput {
            mode: BackendSelectionInputMode::Explicit,
            backend_id: Some("backend-a".to_string()),
        };
        let matching = AgentRuntimePlacement::Remote {
            host_id: "backend-a".to_string(),
            transport_id: agentdash_integration_api::AgentRuntimePlacementId::new("runtime-wire-a")
                .expect("placement id"),
        };
        let other = AgentRuntimePlacement::Remote {
            host_id: "backend-b".to_string(),
            transport_id: agentdash_integration_api::AgentRuntimePlacementId::new("runtime-wire-b")
                .expect("placement id"),
        };

        assert!(placement_matches_backend_selection(
            &matching,
            Some(&selection)
        ));
        assert!(!placement_matches_backend_selection(
            &other,
            Some(&selection)
        ));
        assert!(!placement_matches_backend_selection(
            &AgentRuntimePlacement::InProcess,
            Some(&selection)
        ));
    }

    #[test]
    fn only_explicit_backend_selection_requires_an_activated_remote_offer() {
        assert!(!backend_selection_requires_activated_offer(None));
        assert!(!backend_selection_requires_activated_offer(Some(
            &BackendSelectionInput {
                mode: BackendSelectionInputMode::AutoIdle,
                backend_id: None,
            }
        )));
        assert!(!backend_selection_requires_activated_offer(Some(
            &BackendSelectionInput {
                mode: BackendSelectionInputMode::WorkspaceBinding,
                backend_id: Some("backend-a".to_string()),
            }
        )));
        assert!(backend_selection_requires_activated_offer(Some(
            &BackendSelectionInput {
                mode: BackendSelectionInputMode::Explicit,
                backend_id: Some("backend-a".to_string()),
            }
        )));
    }

    #[test]
    fn codex_admission_accepts_surface_without_driver_owned_hook_requirements() {
        let mut surface = fixture_surface();
        surface.hooks.bindings.clear();
        surface.hooks.configuration_boundary = ConfigurationBoundary::ThreadStart;
        let default_reference = bound_surface_reference(&surface);
        assert!(default_reference.required_hooks.is_empty());

        let codex_profile =
            agentdash_integration_codex::codex_runtime_trust_manifest().verified_profile;
        assert!(
            default_reference
                .required_hooks
                .iter()
                .all(|requirement| codex_profile.hooks.satisfies(requirement))
        );
    }

    fn fixture_surface() -> MaterializedDriverSurface {
        MaterializedDriverSurface {
            runtime_thread_id: parsed("fixture-thread"),
            revision: SurfaceRevision(1),
            digest: parsed("sha256:production-native-surface"),
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
                    entries: vec!["You are the production Native Agent.".to_string()],
                }],
                blocks: vec![ContextBlock::Input {
                    input: vec![RuntimeInput::text("durable initial context".to_string())],
                }],
                digest: parsed("sha256:production-native-context"),
                fidelity: ContextFidelity::PlatformExact,
            },
            tools: DriverToolSurface {
                revision: ToolSetRevision(1),
                digest: "sha256:production-native-tools".to_string(),
                tools: Vec::new(),
            },
            hooks: DriverHookSurface {
                revision: HookPlanRevision(1),
                digest: parsed("sha256:production-native-hooks"),
                artifact_digest: None,
                configuration_boundary: ConfigurationBoundary::Binding,
                bindings: vec![
                    DriverHookBinding {
                        definition_id: parsed("native-tracer-hook"),
                        point: HookPoint::BeforeTool,
                        actions: vec![HookAction::Observe, HookAction::Block],
                        strength: SemanticStrength::ExactSynchronous,
                        failure_policy: HookFailurePolicy::FailClosed,
                        required: true,
                        site:
                            agentdash_agent_runtime_contract::HookExecutionSite::AgentCoreCallback,
                    },
                    DriverHookBinding {
                        definition_id: parsed("native-tracer-effect-hook"),
                        point: HookPoint::AfterTool,
                        actions: vec![HookAction::Observe, HookAction::EmitEffect],
                        strength: SemanticStrength::ExactSynchronous,
                        failure_policy: HookFailurePolicy::FailClosed,
                        required: true,
                        site:
                            agentdash_agent_runtime_contract::HookExecutionSite::AgentCoreCallback,
                    },
                ],
            },
            workspace: DriverWorkspaceSurface {
                digest: "workspace-fixture".to_string(),
                capabilities: Vec::new(),
                roots: vec!["workspace://project".to_string()],
            },
        }
    }

    fn fixture_business_surface(
        surface: &MaterializedDriverSurface,
    ) -> agentdash_agent_runtime::CompiledBusinessAgentSurface {
        let source = agentdash_agent_runtime::SurfaceSourceRef {
            layer: "fixture".to_string(),
            key: "fixture-frame".to_string(),
        };
        let hooks = surface
            .hooks
            .bindings
            .iter()
            .map(|binding| agentdash_agent_runtime::HookDefinition {
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
            })
            .collect();
        agentdash_agent_runtime::AgentSurfaceCompiler
            .compile_business_facts(agentdash_agent_runtime::BusinessAgentSurfaceFacts {
                revision: surface.revision,
                context_recipe: surface.context.recipe.clone(),
                tool_set_revision: surface.tools.revision,
                hook_plan_revision: surface.hooks.revision,
                workspace: agentdash_agent_runtime::WorkspaceRequirement {
                    capabilities: surface.workspace.capabilities.iter().copied().collect(),
                    minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
                    requirement: agentdash_agent_runtime::ContributionRequirement::Required,
                },
                source,
                transition_phase_node: Some("fixture".to_string()),
                instructions: surface
                    .context
                    .instructions
                    .iter()
                    .flat_map(|set| set.entries.clone())
                    .collect(),
                tools: Vec::new(),
                hooks,
                bootstrap_context: Default::default(),
                normalized_context_surface: Default::default(),
                projection_identity: agentdash_agent_runtime::ContextProjectionIdentity {
                    operation_id: "fixture-surface-compile".to_string(),
                    source_frame_id: "fixture-frame".to_string(),
                    source_frame_revision: surface.revision.0,
                    recorded_at_ms: 1,
                },
            })
            .expect("compile fixture business surface")
    }

    async fn test_database() -> (
        PgPool,
        crate::postgres_runtime::PostgresRuntime,
        tokio::sync::OwnedSemaphorePermit,
    ) {
        static SERIAL: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> =
            std::sync::OnceLock::new();
        let permit = SERIAL
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
            .acquire_owned()
            .await
            .expect("native runtime composition test semaphore");
        let data_root = std::env::temp_dir().join("agentdash-tests").join(format!(
            "native-runtime-production-tracer-{}",
            std::process::id()
        ));
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "native-runtime-production-tracer",
            8,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL");
        let database = format!("native_runtime_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database}"))
            .execute(&runtime.pool)
            .await
            .expect("create tracer database");
        let options: PgConnectOptions = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(6)
            .connect_with(options)
            .await
            .expect("connect tracer database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate tracer database");
        (pool, runtime, permit)
    }

    #[tokio::test]
    async fn surface_version_migration_supports_clean_and_existing_schema_upgrade() {
        let (pool, _runtime, _serial) = test_database().await;
        let definition: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='agent_runtime_surface_snapshot'::regclass AND contype='p'",
        )
        .fetch_one(&pool)
        .await
        .expect("read clean surface snapshot primary key");
        assert!(definition.contains("binding_id"));
        assert!(definition.contains("surface_revision"));
        assert!(definition.contains("surface_digest"));

        sqlx::query("ALTER TABLE agent_runtime_surface_snapshot DROP CONSTRAINT agent_runtime_surface_snapshot_pkey, ADD PRIMARY KEY (binding_id)")
            .execute(&pool)
            .await
            .expect("restore pre-0071 surface primary key");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=71")
            .execute(&pool)
            .await
            .expect("rewind latest migration marker");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("upgrade pre-0071 surface schema");
        let upgraded: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='agent_runtime_surface_snapshot'::regclass AND contype='p'",
        )
        .fetch_one(&pool)
        .await
        .expect("read upgraded surface snapshot primary key");
        assert!(upgraded.contains("binding_id"));
        assert!(upgraded.contains("surface_revision"));
        assert!(upgraded.contains("surface_digest"));
        let applied: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM _sqlx_migrations WHERE version=71 AND success",
        )
        .fetch_one(&pool)
        .await
        .expect("read 0071 migration history");
        assert_eq!(applied, 1);
    }

    #[tokio::test]
    async fn production_native_provisions_with_workspace_backend_idempotently() {
        let (pool, _runtime, _serial) = test_database().await;
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain','ready',$3,$3,$3)")
            .bind(run_id.to_string())
            .bind(project_id.to_string())
            .bind(now)
            .execute(&pool)
            .await
            .expect("seed lifecycle run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status) VALUES ($1,$2,$3,'primary','active')")
            .bind(agent_id.to_string())
            .bind(run_id.to_string())
            .bind(project_id.to_string())
            .execute(&pool)
            .await
            .expect("seed lifecycle agent");

        let mut provider = LlmProvider::new("OpenAI", "openai", WireProtocol::OpenaiCompatible);
        provider.credential_mode = LlmCredentialMode::UserRequired;
        provider.default_model = "gpt-test".to_string();
        provider.models = json!(["gpt-test"]);
        let credential = LlmProviderUserCredential {
            id: Uuid::new_v4(),
            provider_id: provider.id,
            user_id: "account-user-1".to_string(),
            api_key_ciphertext: "user-secret".to_string(),
            verification_status: Default::default(),
            verification_message: String::new(),
            verified_at: None,
            created_at: now,
            updated_at: now,
        };
        let composition =
            build_native_agent_runtime_composition(NativeAgentRuntimeCompositionInput {
                pool: pool.clone(),
                provider_repository: Arc::new(ProviderRepository(provider)),
                provider_credential_repository: Arc::new(UserCredentials(credential)),
                secret_codec: Arc::new(PlaintextCodec),
                surface_compiler: Arc::new(FixtureSurfaceCompiler),
                credential_broker: Arc::new(NoCredentials),
                callback_factory: Arc::new(|_| AgentRuntimeCallbacks {
                    tools: Arc::new(NoTools),
                    hooks: Arc::new(ContinueHooks),
                }),
                application_presentation_projector: Arc::new(TestTerminalPresentationProjector),
                remote_definitions: Vec::new(),
                remote_trust_manifests: Vec::new(),
                remote_placements: Arc::new(NoRemotePlacements),
                node_id: "production-tracer-node".to_string(),
            })
            .expect("build production composition");
        assert!(
            composition.host.definitions().iter().any(|definition| {
                definition.provenance.definition_id.as_str() == NATIVE_DEFINITION_ID
            }),
            "the production Host inventory must expose the Native definition added by composition"
        );
        let terminal_hook_effect_binding = fixture_terminal_hook_effect_binding();
        let request = AgentRunRuntimeProvisionRequest {
            target: AgentRunRuntimeTarget { run_id, agent_id },
            presentation_thread_id: agentdash_agent_runtime_contract::PresentationThreadId::new(
                "presentation-production-native",
            )
            .expect("presentation thread id"),
            identity: Some(AuthIdentity {
                auth_mode: AuthMode::Enterprise,
                user_id: "account-user-1".to_string(),
                subject: "directory-subject-1".to_string(),
                display_name: None,
                email: None,
                avatar_url: None,
                groups: Vec::new(),
                is_admin: false,
                provider: Some("enterprise".to_string()),
                extra: serde_json::Value::Null,
            }),
            backend_selection: Some(BackendSelectionInput {
                mode: BackendSelectionInputMode::WorkspaceBinding,
                backend_id: Some("workspace-backend".to_string()),
            }),
            fork: None,
            terminal_hook_effect_binding: Some(terminal_hook_effect_binding.clone()),
        };
        let first = composition
            .provisioner
            .provision(&request)
            .await
            .expect("provision Native binding");
        let replay = composition
            .provisioner
            .provision(&request)
            .await
            .expect("replay Native provisioning");

        assert_eq!(first, replay);
        assert_eq!(first.bound_profile, native_runtime_profile());
        assert_eq!(
            first.surface.terminal_hook_effect_binding,
            Some(terminal_hook_effect_binding.clone())
        );
        let fork_target = AgentRunRuntimeTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain','ready',$3,$3,$3)")
            .bind(fork_target.run_id.to_string())
            .bind(project_id.to_string())
            .bind(now)
            .execute(&pool)
            .await
            .expect("seed fork lifecycle run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status) VALUES ($1,$2,$3,'primary','active')")
            .bind(fork_target.agent_id.to_string())
            .bind(fork_target.run_id.to_string())
            .bind(project_id.to_string())
            .execute(&pool)
            .await
            .expect("seed fork lifecycle agent");
        let forked = composition
            .provisioner
            .provision(&AgentRunRuntimeProvisionRequest {
                target: fork_target.clone(),
                presentation_thread_id:
                    agentdash_agent_runtime_contract::PresentationThreadId::new(
                        "presentation-production-native-fork",
                    )
                    .expect("fork presentation thread id"),
                identity: request.identity.clone(),
                backend_selection: None,
                fork: Some(
                    agentdash_application_ports::agent_run_runtime::AgentRunRuntimeForkSource {
                        source_target: request.target.clone(),
                        through_source_turn_id: None,
                    },
                ),
                terminal_hook_effect_binding: None,
            })
            .await
            .expect("fork Native binding");
        assert_eq!(forked.target, fork_target);
        assert_ne!(forked.source_thread_id, first.source_thread_id);
        let reloaded = composition
            .bindings
            .load(&request.target)
            .await
            .expect("load product binding")
            .expect("persisted product binding");
        assert_eq!(reloaded, first);
        assert_eq!(
            reloaded.surface.terminal_hook_effect_binding,
            Some(terminal_hook_effect_binding)
        );
        let stored_surface: serde_json::Value = sqlx::query_scalar(
            "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1",
        )
        .bind(first.binding_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("load immutable surface");
        assert_eq!(
            stored_surface["context"]["instructions"][0]["entries"][0],
            "You are the production Native Agent."
        );
        let config: serde_json::Value =
            sqlx::query_scalar("SELECT config FROM agent_runtime_service_instance LIMIT 1")
                .fetch_one(&pool)
                .await
                .expect("load service config");
        assert_eq!(config["credential_scope"]["kind"], "user");
        assert_eq!(config["credential_scope"]["user_id"], "account-user-1");

        let host_repository = PostgresAgentRuntimeHostRepository::new(pool.clone());
        let old_host_binding = host_repository
            .load_binding(&first.binding_id)
            .await
            .expect("load original Host binding")
            .expect("original Host binding");
        let old_offer = host_repository
            .load_offer(&old_host_binding.offer_id)
            .await
            .expect("load original Native offer")
            .expect("original Native offer");
        let surface = composition
            .surfaces
            .load_surface(&first.binding_id)
            .await
            .expect("load original Runtime surface")
            .expect("original Runtime surface");
        let recovery_offer = activate_in_process_recovery_offer(
            composition.host.as_ref(),
            &host_repository,
            &old_offer,
            &surface,
        )
        .await
        .expect("reactivate the durable Native owner for recovery");
        assert_eq!(
            recovery_offer.service_instance_id,
            old_offer.service_instance_id
        );
        assert_eq!(
            recovery_offer.generation,
            RuntimeDriverGeneration(old_offer.generation.0 + 1)
        );
        assert!(
            !host_repository
                .load_offer(&old_offer.id)
                .await
                .expect("reload original Native offer")
                .expect("original Native offer row")
                .available,
            "reactivation must fence the previous Native generation"
        );
        assert_eq!(
            select_recovery_offer(
                composition.host.offers().await.expect("recovery offers"),
                &old_offer,
                &surface,
            )
            .expect("new Native generation is selectable")
            .id,
            recovery_offer.id
        );
    }

    #[tokio::test]
    async fn production_native_tracer_dispatches_durable_thread_start_and_replays_client_command() {
        let (pool, _postgres, _serial) = test_database().await;
        let integration = NativeAgentRuntimeIntegration::new(Arc::new(EchoResolver));
        let contribution = integration
            .agent_runtime_drivers()
            .pop()
            .expect("Native contribution");
        let definition = contribution.definition.clone();
        let manifest = native_runtime_trust_manifest();
        let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
            pool: pool.clone(),
            contributions: vec![contribution],
            trusted_manifests: vec![TrustedDriverManifest {
                provenance: manifest.provenance,
                suite_revision: manifest.suite_revision,
                driver_build_digest: manifest.driver_build_digest,
                protocol_revision: manifest.protocol_revision,
                verified_profile_digest: profile_digest(&manifest.verified_profile)
                    .expect("profile digest"),
            }],
            surface_source: Arc::new(
                NativeAgentRunRuntimeSurfaceSource::new(
                    Arc::new(FixtureSurfaceCompiler),
                    definition,
                    Vec::new(),
                )
                .expect("Native surface source"),
            ),
            credential_broker: Arc::new(NoCredentials),
            callback_factory: Arc::new(|_| AgentRuntimeCallbacks {
                tools: Arc::new(NoTools),
                hooks: Arc::new(ContinueHooks),
            }),
            application_presentation_projector: Arc::new(TestTerminalPresentationProjector),
            managed_compaction: Some(Arc::new(TracerManagedCompactionEngine)),
            node_id: "native-production-tracer".to_string(),
        })
        .expect("production composition");
        let target = AgentRunRuntimeTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let project_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain','ready',$3,$3,$3)")
            .bind(target.run_id.to_string())
            .bind(project_id.to_string())
            .bind(now)
            .execute(&pool)
            .await
            .expect("seed lifecycle run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status) VALUES ($1,$2,$3,'primary','active')")
            .bind(target.agent_id.to_string())
            .bind(target.run_id.to_string())
            .bind(project_id.to_string())
            .execute(&pool)
            .await
            .expect("seed lifecycle agent");
        let delivery_presentation_thread_id =
            agentdash_agent_runtime_contract::PresentationThreadId::new(
                "presentation-native-tracer",
            )
            .expect("presentation thread id");
        let binding = composition
            .provisioner
            .provision(&AgentRunRuntimeProvisionRequest {
                target,
                presentation_thread_id: delivery_presentation_thread_id.clone(),
                identity: None,
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: None,
            })
            .await
            .expect("provision Native tracer");
        assert_ne!(
            binding.presentation_thread_id.as_str(),
            binding.thread_id.as_str(),
            "product presentation identity must remain independent from runtime identity",
        );
        let command = RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: parsed("native-tracer-start-operation"),
                idempotency_key: parsed("native-tracer-start-key"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "native-production-tracer".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: binding.thread_id.clone(),
                presentation_thread_id: binding.presentation_thread_id.clone(),
                presentation_turn_id: Some(parsed("presentation-turn-native-tracer")),
                binding_id: binding.binding_id.clone(),
                driver_generation: binding.driver_generation,
                source_thread_id: binding.source_thread_id.clone(),
                profile_digest: binding.profile_digest.clone(),
                bound_profile: Box::new(binding.bound_profile.clone()),
                input: vec![RuntimeInput::text("hello native tracer".to_string())],
                surface: Box::new(binding.surface.clone()),
                settings_revision: binding.settings_revision,
            },
        };
        let first = composition
            .gateway
            .execute(command.clone())
            .await
            .expect("accept ThreadStart");
        let replay = composition
            .gateway
            .execute(command)
            .await
            .expect("replay ThreadStart");
        assert!(!first.duplicate);
        assert!(replay.duplicate);
        assert_eq!(first.operation_id, replay.operation_id);
        let accepted_projection = composition
            .runtime_repository
            .load_thread(&binding.thread_id)
            .await
            .expect("load accepted Runtime projection")
            .expect("accepted Runtime projection");
        assert_eq!(
            accepted_projection.presentation_thread_id,
            delivery_presentation_thread_id
        );
        let persisted_outbox: serde_json::Value =
            sqlx::query_scalar("SELECT payload FROM agent_runtime_outbox WHERE operation_id=$1")
                .bind(first.operation_id.as_str())
                .fetch_one(&pool)
                .await
                .expect("persisted Runtime outbox");
        let persisted_outbox: agentdash_agent_runtime::RuntimeOutboxEntry =
            serde_json::from_value(persisted_outbox).expect("typed Runtime outbox");
        assert_eq!(
            persisted_outbox.presentation_thread_id,
            accepted_projection.presentation_thread_id
        );
        let mut presentation_live = composition
            .presentation_events
            .subscribe_presentation(&binding.thread_id)
            .await;
        let presentation_delta = tokio::spawn(async move {
            let mut observed = Vec::new();
            loop {
                match presentation_live.recv().await {
                    Ok(record) => {
                        let variant = record
                            .as_presentation()
                            .map(|event| {
                                serde_json::to_value(&event.event)
                                    .expect("serialize observed presentation")["type"]
                                    .as_str()
                                    .unwrap_or("<missing-type>")
                                    .to_string()
                            })
                            .unwrap_or_else(|| "<internal>".to_string());
                        observed.push(format!(
                            "{variant}:durability={:?}:transient={}",
                            record.as_presentation().map(|event| event.durability),
                            record.carrier().transient.is_some()
                        ));
                        if record.carrier().transient.is_some()
                            && matches!(
                                record.as_presentation(),
                                Some(event)
                                    if event.durability == PresentationDurability::Ephemeral
                                        && matches!(
                                            &event.event,
                                            agentdash_agent_protocol::BackboneEvent::AgentMessageDelta(_)
                                        )
                            )
                        {
                            return (true, observed);
                        }
                    }
                    Err(error) => {
                        observed.push(format!("recv_error={error:?}"));
                        return (false, observed);
                    }
                }
            }
        });
        assert_eq!(
            composition
                .outbox_worker
                .run_once(8)
                .await
                .expect("dispatch durable outbox"),
            1
        );
        let dispatched = composition.outbox_worker.observed_dispatches().await;
        assert_eq!(dispatched.len(), 1);
        assert_eq!(
            dispatched[0].presentation_thread_id,
            persisted_outbox.presentation_thread_id
        );
        assert_eq!(dispatched[0].operation_id, first.operation_id);
        assert_eq!(
            dispatched[0]
                .presentation_turn_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("presentation-turn-native-tracer")
        );
        let terminal_operation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let operation = composition
                    .runtime_repository
                    .find_operation(&first.operation_id)
                    .await
                    .expect("read terminal Native operation")
                    .expect("Native operation exists");
                if operation.terminal.is_some() {
                    break operation;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Native operation terminal timeout");
        assert_eq!(
            terminal_operation.terminal,
            Some(RuntimeOperationTerminal::Succeeded),
            "a completed Native turn must close its accepted Runtime operation in the same journal commit"
        );
        let RuntimeSnapshotResult::Thread { snapshot } = composition
            .gateway
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: binding.thread_id,
                at_revision: None,
            })
            .await
            .expect("canonical Native snapshot")
        else {
            panic!("expected thread snapshot")
        };
        assert_eq!(snapshot.status, RuntimeThreadStatus::Active);
        assert!(snapshot_contains_agent_message(
            &snapshot,
            "native response"
        ));
        let (saw_delta, observed_presentation) =
            tokio::time::timeout(std::time::Duration::from_secs(1), presentation_delta)
                .await
                .expect("presentation live task timeout")
                .expect("presentation live task");
        assert!(
            saw_delta,
            "Native delta must be live ephemeral presentation; observed={observed_presentation:?}"
        );

        let settings_operation_id: RuntimeOperationId = parsed("native-tracer-settings-operation");
        composition
            .gateway
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: settings_operation_id.clone(),
                    idempotency_key: parsed("native-tracer-settings-key"),
                    expected_thread_revision: Some(snapshot.revision),
                    actor: RuntimeActor::System {
                        component: "native-production-tracer".to_string(),
                    },
                },
                command: RuntimeCommand::ThreadSettingsUpdate {
                    thread_id: snapshot.thread_id.clone(),
                    instructions: vec!["updated system instruction".to_string()],
                },
            })
            .await
            .expect("accept delivery-only settings update");
        assert_eq!(
            composition
                .outbox_worker
                .run_once(8)
                .await
                .expect("dispatch delivery-only settings update"),
            1
        );
        assert_eq!(
            composition
                .runtime_repository
                .find_operation(&settings_operation_id)
                .await
                .expect("read settings operation")
                .expect("settings operation exists")
                .terminal,
            Some(RuntimeOperationTerminal::Succeeded),
            "driver acceptance must close delivery-only operations without a later event"
        );
        let RuntimeSnapshotResult::Thread { snapshot } = composition
            .gateway
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: snapshot.thread_id.clone(),
                at_revision: None,
            })
            .await
            .expect("canonical Native snapshot after settings update")
        else {
            panic!("expected thread snapshot")
        };

        composition
            .gateway
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: parsed("native-tracer-compaction-operation"),
                    idempotency_key: parsed("native-tracer-compaction-key"),
                    expected_thread_revision: Some(snapshot.revision),
                    actor: RuntimeActor::System {
                        component: "native-production-tracer".to_string(),
                    },
                },
                command: RuntimeCommand::ContextCompact {
                    thread_id: snapshot.thread_id.clone(),
                    compaction_id: parsed("native-tracer-compaction"),
                    trigger: ContextCompactionTrigger::Automatic,
                    base_checkpoint_id: None,
                    expected_context_revision: ContextRevision(0),
                },
            })
            .await
            .expect("accept managed compaction");
        let abandoned = composition
            .work_queue
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::ContextPreparation,
                owner: RuntimeWorkerId("crashed-context-worker".to_string()),
                lease_duration_ms: 1,
                limit: 1,
            })
            .await
            .expect("crashed worker claim");
        assert_eq!(abandoned.len(), 1);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextPreparation, 8)
                .await
                .expect("take over expired preparation claim"),
            1
        );
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextActivationDispatch, 8)
                .await
                .expect("dispatch context activation"),
            1
        );
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextActivationRecovery, 8)
                .await
                .expect("recover context activation"),
            1
        );
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextActivationRecovery, 8)
                .await
                .expect("terminal activation is not reclaimed"),
            0
        );

        let effect_run_id: HookRunId = parsed("native-tracer-effect-run");
        composition
            .managed_runtime
            .accept_hook(agentdash_agent_runtime::RuntimeHookInvocation {
                hook_run_id: effect_run_id.clone(),
                thread_id: snapshot.thread_id.clone(),
                definition_id: parsed("native-tracer-effect-hook"),
                point: HookPoint::AfterTool,
                correlation: agentdash_agent_runtime::HookCorrelation {
                    operation_id: None,
                    turn_id: None,
                    item_id: None,
                    interaction_id: None,
                },
                input: json!({"tool_name":"read"}),
            })
            .await
            .expect("accept known effect hook");
        composition
            .managed_runtime
            .start_hook(&effect_run_id)
            .await
            .expect("start known effect hook");
        let known_payload = json!({"code":"native_tracer","message":"recorded"});
        composition
            .managed_runtime
            .complete_hook(
                &effect_run_id,
                agentdash_agent_runtime::HookCompletion {
                    status: agentdash_agent_runtime::HookRunStatus::Completed,
                    decision: HookRunDecision::Continue,
                    message: None,
                },
                vec![agentdash_agent_runtime::HookEffect {
                    effect_id: parsed("native-tracer-known-effect"),
                    hook_run_id: effect_run_id.clone(),
                    thread_id: snapshot.thread_id.clone(),
                    idempotency_key: "native-tracer-known-effect".to_string(),
                    descriptor: agentdash_agent_runtime::HookEffectDescriptor {
                        effect_type: "diagnostic:record".to_string(),
                        schema_version: 1,
                        target_authority: "agentdash_hook_effect_dispatcher".to_string(),
                        retry_limit: 3,
                        payload_digest: agentdash_agent_runtime::hook_effect_payload_digest(
                            &known_payload,
                        ),
                    },
                    payload: known_payload,
                    presentation: None,
                }],
            )
            .await
            .expect("complete known effect hook");
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::HookEffect, 8)
                .await
                .expect("dispatch known hook effect"),
            1
        );
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::HookEffect, 8)
                .await
                .expect("known effect is exactly-once acknowledged"),
            0
        );

        let unknown_run_id: HookRunId = parsed("native-tracer-unknown-effect-run");
        composition
            .managed_runtime
            .accept_hook(agentdash_agent_runtime::RuntimeHookInvocation {
                hook_run_id: unknown_run_id.clone(),
                thread_id: snapshot.thread_id.clone(),
                definition_id: parsed("native-tracer-effect-hook"),
                point: HookPoint::AfterTool,
                correlation: agentdash_agent_runtime::HookCorrelation {
                    operation_id: None,
                    turn_id: None,
                    item_id: None,
                    interaction_id: None,
                },
                input: json!({"tool_name":"read"}),
            })
            .await
            .expect("accept unknown effect hook");
        composition
            .managed_runtime
            .start_hook(&unknown_run_id)
            .await
            .expect("start unknown effect hook");
        let unknown_payload = json!({"note":"must remain pending"});
        composition
            .managed_runtime
            .complete_hook(
                &unknown_run_id,
                agentdash_agent_runtime::HookCompletion {
                    status: agentdash_agent_runtime::HookRunStatus::Completed,
                    decision: HookRunDecision::Continue,
                    message: None,
                },
                vec![agentdash_agent_runtime::HookEffect {
                    effect_id: parsed("native-tracer-unknown-effect"),
                    hook_run_id: unknown_run_id.clone(),
                    thread_id: snapshot.thread_id.clone(),
                    idempotency_key: "native-tracer-unknown-effect".to_string(),
                    descriptor: agentdash_agent_runtime::HookEffectDescriptor {
                        effect_type: "record:note".to_string(),
                        schema_version: 1,
                        target_authority: "unknown_enterprise_authority".to_string(),
                        retry_limit: 3,
                        payload_digest: agentdash_agent_runtime::hook_effect_payload_digest(
                            &unknown_payload,
                        ),
                    },
                    payload: unknown_payload,
                    presentation: None,
                }],
            )
            .await
            .expect("complete unknown effect hook");
        assert!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::HookEffect, 8)
                .await
                .is_err(),
            "unknown authority must release instead of acknowledging"
        );
        assert_eq!(
            composition
                .work_queue
                .claim(RuntimeWorkClaimRequest {
                    kind: RuntimeWorkKind::HookEffect,
                    owner: RuntimeWorkerId("unknown-effect-observer".to_string()),
                    lease_duration_ms: 1_000,
                    limit: 8,
                })
                .await
                .expect("unknown effect remains claimable")
                .len(),
            1
        );
        let RuntimeSnapshotResult::Context { context } = composition
            .gateway
            .snapshot(RuntimeSnapshotQuery::Context {
                thread_id: snapshot.thread_id,
                at_context_revision: None,
            })
            .await
            .expect("compacted context snapshot")
        else {
            panic!("expected context snapshot")
        };
        assert!(context.head.is_some());
        assert_eq!(context.fidelity, ContextFidelity::PlatformExact);
        let records = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT record FROM agent_runtime_event WHERE thread_id=$1 ORDER BY event_sequence",
        )
        .bind(context.thread_id.as_str())
        .fetch_all(&pool)
        .await
        .expect("load compaction journal")
        .into_iter()
        .map(|value| {
            serde_json::from_value::<agentdash_agent_runtime_contract::RuntimeJournalRecord>(value)
                .expect("typed Runtime journal record")
        })
        .collect::<Vec<_>>();
        let compaction_frame = records
            .iter()
            .filter_map(agentdash_agent_runtime_contract::RuntimeJournalRecord::as_presentation)
            .find_map(|event| match &event.event {
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(changed),
                ) if changed.frame.kind
                    == agentdash_agent_protocol::ContextFrameKind::CompactionSummary =>
                {
                    Some(&changed.frame)
                }
                _ => None,
            })
            .expect("durable compaction summary presentation");
        assert_eq!(
            compaction_frame.rendered_text,
            "## Compaction Summary\nmessages_compacted: 1\ntokens_before: 42\ntimestamp_ms: 1710000000000\ncompaction_id: native-tracer-compaction\nstrategy: summary_prefix\ntrigger: auto\nphase: standalone_compact_turn\nsource_end_event_seq: 16\n\n以下是之前对话的压缩摘要，用于延续工作上下文。摘要中的路径、函数名等具体信息可能已过时，请在执行前验证。\n\nnative tracer compacted summary"
        );
        assert!(matches!(
            compaction_frame.sections.as_slice(),
            [agentdash_agent_protocol::ContextFrameSection::CompactionSummary {
                summary,
                tokens_before: 42,
                messages_compacted: 1,
                projection_version: None,
                source_start_event_seq: None,
                source_end_event_seq: Some(16),
                first_kept_event_seq: None,
                ..
            }] if summary == "native tracer compacted summary"
        ));

        let hook_run_id: HookRunId = parsed("native-tracer-crashed-hook");
        assert!(matches!(
            composition
                .managed_runtime
                .accept_hook(agentdash_agent_runtime::RuntimeHookInvocation {
                    hook_run_id: hook_run_id.clone(),
                    thread_id: context.thread_id,
                    definition_id: parsed("native-tracer-hook"),
                    point: HookPoint::BeforeTool,
                    correlation: agentdash_agent_runtime::HookCorrelation {
                        operation_id: None,
                        turn_id: None,
                        item_id: None,
                        interaction_id: None,
                    },
                    input: json!({"tool_name":"read"}),
                })
                .await
                .expect("accept durable hook"),
            agentdash_agent_runtime::HookAdmission::Durable(_)
        ));
        let abandoned_hook = composition
            .work_queue
            .claim(RuntimeWorkClaimRequest {
                kind: RuntimeWorkKind::HookRunRecovery,
                owner: RuntimeWorkerId("crashed-hook-worker".to_string()),
                lease_duration_ms: 1,
                limit: 1,
            })
            .await
            .expect("crashed hook worker claim");
        assert_eq!(abandoned_hook.len(), 1);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::HookRunRecovery, 8)
                .await
                .expect("recover expired hook claim"),
            1
        );
        assert_eq!(
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::HookRunRecovery, 8)
                .await
                .expect("terminal hook is not reclaimed"),
            0
        );
    }

    #[tokio::test]
    async fn production_codex_tracer_maps_app_server_stream_and_resolves_interaction() {
        let (pool, _postgres, _serial) = test_database().await;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/codex-runtime-production-tracer")
            .join(Uuid::new_v4().simple().to_string());
        std::fs::create_dir_all(root.join("artifacts")).expect("create Codex tracer root");
        let script = root.join("app-server.cjs");
        std::fs::write(
            &script,
            r#"const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
const send = value => { console.error('codex-tracer-send', value.method || 'response', value.id || 'notification'); process.stdout.write(JSON.stringify(value) + '\n'); };
const threadId = 'codex-source-thread-1';
const turnId = 'codex-source-turn-1';
const activeTurn = { id: turnId, items: [], itemsView: 'full', status: 'inProgress' };
rl.on('line', line => {
  const message = JSON.parse(line);
  console.error('codex-tracer-recv', message.method || 'response', message.id || 'notification');
  if (message.method === 'initialize') send({ id: message.id, result: { capabilities: {} } });
  else if (message.method === 'thread/start') send({ id: message.id, result: { thread: { id: threadId } } });
  else if (message.method === 'thread/resume') send({ id: message.id, result: { thread: { id: threadId } } });
  else if (message.method === 'turn/start') {
    send({ id: message.id, result: { turn: activeTurn } });
    setTimeout(() => {
      send({ method: 'turn/started', params: { threadId, turn: activeTurn } });
      send({ method: 'item/started', params: { threadId, turnId, item: { id: 'codex-item-1', type: 'agentMessage', text: '', phase: null, memoryCitation: null }, startedAtMs: 1000 } });
      send({ method: 'item/agentMessage/delta', params: { threadId, turnId, itemId: 'codex-item-1', delta: 'codex ' } });
      send({ id: 700, method: 'item/permissions/requestApproval', params: { threadId, turnId, itemId: 'codex-item-1', cwd: process.cwd(), permissions: {}, reason: 'approve tracer access', startedAtMs: 1 } });
    }, 10);
  } else if (message.id === 700 && !message.method) {
    send({ method: 'item/started', params: { threadId, turnId, startedAtMs: 1500, item: { id: 'codex-surface-tool-1', type: 'dynamicToolCall', tool: 'surface_update', status: 'inProgress', arguments: {}, success: null, namespace: 'platform', durationMs: null, contentItems: null } } });
    send({ id: 701, method: 'item/tool/call', params: { threadId, turnId, callId: 'codex-surface-tool-1', namespace: 'platform', tool: 'surface_update', arguments: {} } });
  } else if (message.id === 701 && !message.method) {
    setTimeout(() => {
      send({ method: 'item/completed', params: { threadId, turnId, completedAtMs: 1800, item: { id: 'codex-surface-tool-1', type: 'dynamicToolCall', tool: 'surface_update', status: 'completed', arguments: {}, success: true, namespace: 'platform', durationMs: 300, contentItems: message.result.contentItems } } });
      send({ method: 'item/agentMessage/delta', params: { threadId, turnId, itemId: 'codex-item-1', delta: 'response' } });
      send({ method: 'item/completed', params: { threadId, turnId, item: { id: 'codex-item-1', type: 'agentMessage', text: 'codex response', phase: null, memoryCitation: null }, completedAtMs: 2000 } });
      send({ method: 'turn/completed', params: { threadId, turn: { ...activeTurn, status: 'completed', completedAt: 2, durationMs: 1000 } } });
    }, 500);
  }
});
"#,
        )
        .expect("write controllable app-server");
        let contribution =
            codex_runtime_contribution_with_launcher(Arc::new(CodexTracerLauncher(script)));
        let definition = contribution.definition.clone();
        let manifest = codex_runtime_trust_manifest();
        let surface_adoption = Arc::new(tokio::sync::RwLock::new(None));
        let callback_surface_adoption = surface_adoption.clone();
        let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
            pool: pool.clone(),
            contributions: vec![contribution],
            trusted_manifests: vec![TrustedDriverManifest {
                provenance: manifest.provenance,
                suite_revision: manifest.suite_revision,
                driver_build_digest: manifest.driver_build_digest,
                protocol_revision: manifest.protocol_revision,
                verified_profile_digest: profile_digest(&manifest.verified_profile)
                    .expect("Codex profile digest"),
            }],
            surface_source: Arc::new(CodexTracerSurfaceSource {
                definition,
                root: root.clone(),
            }),
            credential_broker: Arc::new(NoCredentials),
            callback_factory: Arc::new(move |runtime| AgentRuntimeCallbacks {
                tools: Arc::new(CodexSurfaceAdoptingTool {
                    runtime,
                    fixture: callback_surface_adoption.clone(),
                }),
                hooks: Arc::new(ContinueHooks),
            }),
            application_presentation_projector: Arc::new(TestTerminalPresentationProjector),
            managed_compaction: None,
            node_id: "codex-production-tracer".to_string(),
        })
        .expect("Codex production composition");
        let target = AgentRunRuntimeTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let project_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain','ready',$3,$3,$3)")
            .bind(target.run_id.to_string()).bind(project_id.to_string()).bind(now)
            .execute(&pool).await.expect("seed Codex lifecycle run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status) VALUES ($1,$2,$3,'primary','active')")
            .bind(target.agent_id.to_string()).bind(target.run_id.to_string()).bind(project_id.to_string())
            .execute(&pool).await.expect("seed Codex lifecycle agent");
        let binding = composition
            .provisioner
            .provision(&AgentRunRuntimeProvisionRequest {
                target,
                presentation_thread_id:
                    agentdash_agent_runtime_contract::PresentationThreadId::new(
                        "presentation-codex-tracer",
                    )
                    .expect("presentation thread id"),
                identity: None,
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: None,
            })
            .await
            .expect("provision Codex binding");
        assert_ne!(
            binding.presentation_thread_id.as_str(),
            binding.thread_id.as_str(),
            "product presentation identity must remain independent from runtime identity",
        );
        let original_surface = composition
            .surfaces
            .load_surface(&binding.binding_id)
            .await
            .expect("load original Codex surface")
            .expect("original Codex surface exists");
        let original_business_surface = composition
            .surfaces
            .load_business_surface(
                &binding.binding_id,
                original_surface.revision,
                &original_surface.digest,
            )
            .await
            .expect("load original Codex business surface");
        let mut adopted = original_surface.clone();
        adopted.revision = SurfaceRevision(adopted.revision.0 + 1);
        adopted.digest = parsed("sha256:codex-tracer-surface-adopted");
        adopted.workspace.digest = "sha256:codex-tracer-workspace-adopted".to_string();
        adopted.context.recipe.revision =
            ContextRecipeRevision(adopted.context.recipe.revision.0 + 1);
        adopted.context.digest = parsed("sha256:codex-tracer-context-adopted");
        adopted.tools.revision = ToolSetRevision(adopted.tools.revision.0 + 1);
        adopted.tools.digest = "sha256:codex-tracer-tools-adopted".to_string();
        adopted.hooks.revision = HookPlanRevision(adopted.hooks.revision.0 + 1);
        adopted.hooks.digest = parsed("sha256:codex-tracer-hooks-adopted");
        composition
            .surfaces
            .put_surface(&binding.binding_id, &adopted, &original_business_surface)
            .await
            .expect("persist adopted Codex surface version");
        let broker = PostgresAgentRuntimeCompositionRepository::new(pool.clone());
        let retained = agentdash_integration_api::AgentRuntimeSurfaceBroker::materialize(
            &broker,
            DriverSurfaceRequest {
                binding_id: binding.binding_id.clone(),
                surface_revision: original_surface.revision,
                surface_digest: original_surface.digest.clone(),
            },
        )
        .await
        .expect("old surface version remains readable for in-flight outbox");
        assert_eq!(retained, original_surface);
        let target_surface = runtime_surface_descriptor(
            "codex-tracer-frame-adopted".to_string(),
            &adopted,
            BoundRuntimeHookPlan {
                revision: adopted.hooks.revision,
                digest: adopted.hooks.digest.clone(),
                entries: Vec::new(),
            },
            Some(fixture_terminal_hook_effect_binding()),
        );
        let mut surface_presentation = agentdash_agent_runtime::ContextProjector::project(
            &agentdash_agent_runtime::ContextProjectionIdentity {
                operation_id: "codex-tool-surface-context".to_string(),
                source_frame_id: target_surface.source_frame_id.clone(),
                source_frame_revision: target_surface.surface_revision.0,
                recorded_at_ms: 1_720_000_000_000,
            },
            [agentdash_agent_runtime::ContextFrameFacts {
                kind: agentdash_agent_protocol::ContextFrameKind::CapabilityStateDelta,
                source: agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
                phase_node: Some("codex_surface_update".to_string()),
                apply_mode: Some("live".to_string()),
                delivery_status:
                    agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
                message_role: agentdash_agent_protocol::ContextMessageRole::User,
                rendered_text: "surface_update tool is active".to_string(),
                sections: vec![
                    agentdash_agent_protocol::ContextFrameSection::ToolSchemaDelta {
                        added_tools: vec![agentdash_agent_protocol::RuntimeToolSchemaEntry {
                            name: "surface_update".to_string(),
                            description: "Update the active AgentFrame surface".to_string(),
                            parameters_schema: json!({"type":"object","properties":{}}),
                            capability_key: Some("runtime.surface.update".to_string()),
                            source: Some("codex-tracer".to_string()),
                            tool_path: Some("runtime/surface/update".to_string()),
                            context_usage_kind: Some("system_tools".to_string()),
                        }],
                    },
                ],
            }],
        );
        surface_presentation.adoption_frames =
            std::mem::take(&mut surface_presentation.bootstrap_frames);
        surface_presentation.transition_phase_node = Some("codex_surface_update".to_string());
        *surface_adoption.write().await = Some(CodexSurfaceAdoptionFixture {
            target: target_surface.clone(),
            presentation_thread_id: binding.presentation_thread_id.clone(),
            presentation: surface_presentation.clone(),
        });
        let start = RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: parsed("codex-tracer-start-operation"),
                idempotency_key: parsed("codex-tracer-start-key"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "codex-production-tracer".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: binding.thread_id.clone(),
                presentation_thread_id: binding.presentation_thread_id.clone(),
                presentation_turn_id: Some(parsed("presentation-turn-codex-tracer")),
                binding_id: binding.binding_id.clone(),
                driver_generation: binding.driver_generation,
                source_thread_id: binding.source_thread_id.clone(),
                profile_digest: binding.profile_digest.clone(),
                bound_profile: Box::new(binding.bound_profile.clone()),
                input: vec![RuntimeInput::text("hello codex tracer".to_string())],
                surface: Box::new(binding.surface.clone()),
                settings_revision: binding.settings_revision,
            },
        };
        assert!(
            !composition
                .gateway
                .execute(start.clone())
                .await
                .expect("accept Codex ThreadStart")
                .duplicate
        );
        assert!(
            composition
                .gateway
                .execute(start)
                .await
                .expect("replay Codex ThreadStart")
                .duplicate
        );
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                composition.outbox_worker.run_once(8),
            )
            .await
            .expect("Codex ThreadStart dispatch timeout")
            .expect("dispatch Codex ThreadStart"),
            1
        );
        let pending = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let RuntimeSnapshotResult::Thread { snapshot } = composition
                    .gateway
                    .snapshot(RuntimeSnapshotQuery::Thread {
                        thread_id: binding.thread_id.clone(),
                        at_revision: None,
                    })
                    .await
                    .expect("Codex pending snapshot")
                else {
                    panic!("expected Codex thread snapshot")
                };
                if let Some(interaction_id) = snapshot.pending_interactions.first() {
                    break (snapshot.revision, interaction_id.clone());
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        let (revision, interaction_id) = match pending {
            Ok(pending) => pending,
            Err(_) => {
                let snapshot = composition
                    .gateway
                    .snapshot(RuntimeSnapshotQuery::Thread {
                        thread_id: binding.thread_id.clone(),
                        at_revision: None,
                    })
                    .await
                    .expect("Codex diagnostic snapshot");
                let events: Vec<serde_json::Value> = sqlx::query_scalar(
                    "SELECT record FROM agent_runtime_event ORDER BY event_sequence",
                )
                .fetch_all(&pool)
                .await
                .expect("load Codex runtime event diagnostics");
                let quarantine: Vec<serde_json::Value> =
                    sqlx::query_scalar("SELECT record FROM agent_runtime_quarantine ORDER BY id")
                        .fetch_all(&pool)
                        .await
                        .expect("load Codex quarantine diagnostics");
                panic!(
                    "Codex interaction timeout; snapshot={snapshot:?}; events={events:?}; quarantine={quarantine:?}"
                )
            }
        };
        composition
            .gateway
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: parsed("codex-tracer-interaction-operation"),
                    idempotency_key: parsed("codex-tracer-interaction-key"),
                    expected_thread_revision: Some(revision),
                    actor: RuntimeActor::User {
                        subject: "codex-tracer-user".to_string(),
                    },
                },
                command: RuntimeCommand::InteractionRespond {
                    thread_id: binding.thread_id.clone(),
                    interaction_id,
                    response: InteractionResponse::Approved,
                },
            })
            .await
            .expect("accept Codex interaction response");
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                composition.outbox_worker.run_once(8),
            )
            .await
            .expect("Codex interaction dispatch timeout")
            .expect("dispatch Codex interaction response"),
            1
        );
        let active_adopted = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let RuntimeSnapshotResult::Thread { snapshot } = composition
                    .gateway
                    .snapshot(RuntimeSnapshotQuery::Thread {
                        thread_id: binding.thread_id.clone(),
                        at_revision: None,
                    })
                    .await
                    .expect("Codex active adoption snapshot")
                else {
                    panic!("expected Codex thread snapshot")
                };
                if snapshot.active_turn_id.is_some() && snapshot.surface == target_surface {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("dynamic tool must commit the canonical surface during the active turn");
        assert_eq!(active_adopted.surface, target_surface);
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                composition.outbox_worker.run_once(8),
            )
            .await
            .expect("deferred Codex SurfaceAdopt dispatch timeout")
            .expect("dispatch deferred Codex SurfaceAdopt"),
            1
        );
        let final_result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let RuntimeSnapshotResult::Thread { snapshot } = composition
                    .gateway
                    .snapshot(RuntimeSnapshotQuery::Thread {
                        thread_id: binding.thread_id.clone(),
                        at_revision: None,
                    })
                    .await
                    .expect("Codex final snapshot")
                else {
                    panic!("expected Codex thread snapshot")
                };
                if snapshot.active_turn_id.is_none()
                    && snapshot.pending_interactions.is_empty()
                    && snapshot_contains_agent_message(&snapshot, "codex response")
                {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        let final_snapshot = match final_result {
            Ok(snapshot) => snapshot,
            Err(_) => {
                let snapshot = composition
                    .gateway
                    .snapshot(RuntimeSnapshotQuery::Thread {
                        thread_id: binding.thread_id.clone(),
                        at_revision: None,
                    })
                    .await
                    .expect("Codex terminal diagnostic snapshot");
                let records: Vec<serde_json::Value> = sqlx::query_scalar(
                    "SELECT record FROM agent_runtime_event WHERE thread_id=$1 ORDER BY event_sequence",
                )
                .bind(binding.thread_id.as_str())
                .fetch_all(&pool)
                .await
                .expect("Codex diagnostic events");
                panic!(
                    "Codex terminal snapshot timeout; snapshot={snapshot:?}; records={records:?}"
                )
            }
        };
        let serialized = serde_json::to_string(&final_snapshot).expect("snapshot serializes");
        assert!(!serialized.contains("item/agentMessage/delta"));
        assert!(!serialized.contains("turn/completed"));
        let adopted_binding = composition
            .host
            .binding(&binding.binding_id)
            .await
            .expect("load adopted Host binding");
        assert_eq!(adopted_binding.bound_surface.revision, adopted.revision);
        assert_eq!(adopted_binding.bound_surface.digest, adopted.digest);
        let RuntimeSnapshotResult::Thread { snapshot } = composition
            .gateway
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: binding.thread_id,
                at_revision: None,
            })
            .await
            .expect("load adopted Runtime snapshot")
        else {
            panic!("expected adopted Runtime thread snapshot")
        };
        assert_eq!(snapshot.surface, target_surface);
        let records = composition
            .runtime_repository
            .journal_records_after(&snapshot.thread_id, None)
            .await
            .expect("load Codex surface/tool journal")
            .records;
        let context_frames = records
            .iter()
            .filter_map(
                |record| match record.as_presentation().map(|event| &event.event) {
                    Some(agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(changed),
                    )) if changed.frame.id == surface_presentation.adoption_frames[0].id => {
                        Some(changed.frame.id.clone())
                    }
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(
            context_frames.len(),
            1,
            "active surface adoption must expose each compiled ContextFrame exactly once"
        );
        let tool_lifecycle = records
            .iter()
            .filter_map(|record| {
                let event = &record.as_presentation()?.event;
                match event {
                    agentdash_agent_protocol::BackboneEvent::ItemStarted(started)
                        if started.item.id() == "codex-surface-tool-1" =>
                    {
                        Some(("started", started.item.id().to_string()))
                    }
                    agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed)
                        if completed.item.id() == "codex-surface-tool-1" =>
                    {
                        Some(("completed", completed.item.id().to_string()))
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tool_lifecycle,
            vec![
                ("started", "codex-surface-tool-1".to_string()),
                ("completed", "codex-surface-tool-1".to_string()),
            ],
            "Codex tool start/result must converge on one frontend card identity"
        );
    }

    #[tokio::test]
    async fn native_surface_source_uses_authenticated_user_coordinate() {
        let resolver: Arc<dyn NativeBridgeResolver> =
            Arc::new(RepositoryNativeBridgeResolver::new(
                Arc::new(ProviderRepository(LlmProvider::new(
                    "OpenAI",
                    "openai",
                    WireProtocol::OpenaiCompatible,
                ))),
                Arc::new(NoUserCredentials),
                Arc::new(PlaintextCodec),
            ));
        let definition = NativeAgentRuntimeIntegration::new(resolver)
            .agent_runtime_drivers()
            .remove(0)
            .definition;
        let source = NativeAgentRunRuntimeSurfaceSource::new(
            Arc::new(FixtureSurfaceCompiler),
            definition,
            Vec::new(),
        )
        .expect("Native surface source");
        let request = AgentRunRuntimeProvisionRequest {
            target: AgentRunRuntimeTarget {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
            },
            presentation_thread_id: agentdash_agent_runtime_contract::PresentationThreadId::new(
                "presentation-account",
            )
            .expect("presentation thread id"),
            identity: Some(AuthIdentity {
                auth_mode: AuthMode::Enterprise,
                user_id: "account-user-1".to_string(),
                subject: "directory-subject-1".to_string(),
                display_name: None,
                email: None,
                avatar_url: None,
                groups: Vec::new(),
                is_admin: false,
                provider: Some("enterprise".to_string()),
                extra: serde_json::Value::Null,
            }),
            backend_selection: None,
            fork: None,
            terminal_hook_effect_binding: None,
        };
        let prepared = source
            .prepare(
                &request,
                &parsed("thread-account"),
                &parsed("binding-account"),
            )
            .await
            .expect("prepare user-scoped service");

        assert_eq!(
            prepared.service_config["credential_scope"],
            json!({"kind":"user","user_id":"account-user-1"})
        );
        assert_ne!(
            prepared.service_config["credential_scope"]["user_id"],
            request.identity.as_ref().unwrap().subject
        );
    }

    #[test]
    fn runtime_rebind_error_preserves_retryable_intent() {
        let mapped = runtime_rebind_error(
            agentdash_agent_runtime_contract::RuntimeExecuteError::RevisionConflict {
                expected: agentdash_agent_runtime_contract::RuntimeRevision(7),
                actual: agentdash_agent_runtime_contract::RuntimeRevision(8),
            },
        );
        assert!(matches!(
            mapped,
            AgentRunRuntimeBindingError::Unavailable {
                retryable: true,
                ..
            }
        ));

        let mapped = runtime_rebind_error(
            agentdash_agent_runtime_contract::RuntimeExecuteError::Incompatible {
                reason: "resume profile changed".to_string(),
            },
        );
        assert!(matches!(
            mapped,
            AgentRunRuntimeBindingError::Unavailable {
                retryable: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn host_lost_failure_keeps_recovery_intent_host_bound() {
        let advanced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = advanced.clone();
        let result = finalize_nonretryable_recovery_failure(
            || async {
                Err(AgentRunRuntimeBindingError::Unavailable {
                    reason: "host store unavailable".to_string(),
                    retryable: true,
                })
            },
            || async move {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(AgentRunRuntimeBindingError::Unavailable {
                retryable: true,
                ..
            })
        ));
        assert!(!advanced.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn host_lost_success_allows_recovery_intent_to_fail() {
        let advanced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = advanced.clone();
        finalize_nonretryable_recovery_failure(
            || async { Ok(()) },
            || async move {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .expect("Host Lost后可推进intent Failed");
        assert!(advanced.load(std::sync::atomic::Ordering::SeqCst));
    }
}
