use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_protocol::{
    AgentCapabilityChannel, AgentCapabilityCompanionAgent, AgentCapabilityDiagnostic,
    AgentCapabilityManifest, AgentCapabilityMcpServer, AgentCapabilityMemorySource,
    AgentCapabilityMount, AgentCapabilitySkill, AgentCapabilityVfs, AgentCapabilityWorkspaceModule,
    AgentSurfaceInstructionPresentation,
};
use agentdash_agent_runtime::{
    ManagedRuntimeDispatchContext, ManagedRuntimeLifecyclePort, PlatformToolBroker,
    RuntimeToolDefinition, map_initial_context_package,
};
use agentdash_agent_runtime_contract::{ManagedRuntimeInitialContextPackage, RuntimeThreadId};
use agentdash_agent_runtime_host::{
    CompleteAgentBindingTarget, CompleteAgentHost, CompleteAgentHostError,
    CompleteAgentRuntimeTargetProvisioningRequest, CompleteAgentRuntimeTargetRecoveryRequest,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentEffectIdentity, AgentForkPoint, AgentHookAction,
    AgentHookBlockingSemantics, AgentHookDefinitionId, AgentHookEffectKind, AgentHookMutationKind,
    AgentHookPoint, AgentHookSemanticFacet, AgentHookTiming, AgentIdempotencyKey,
    AgentPayloadDigest, AgentSurfaceContributionPayload, AgentSurfaceDigest,
    AgentSurfaceRequirement, AgentSurfaceRevision, AgentSurfaceRoute, AgentSurfaceSemanticFacet,
    AgentSurfaceSnapshot, AgentToolDelivery, AgentToolSemanticFacet, AgentToolUpdateSemantics,
    SemanticFidelity,
};
use agentdash_application_agentrun::agent_run::frame::{
    AgentContextSourceSnapshot, runtime_backend_anchor_from_vfs,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunCompleteAgentAssociation, AgentRunProductAgentCreateEvidence,
    AgentRunProductAgentForkEvidence, AgentRunProductRuntimeBinding,
    AgentRunProductRuntimeProvisioningError, AgentRunProductRuntimeProvisioningEvidence,
    AgentRunProductRuntimeProvisioningPort, AgentRunProductRuntimeProvisioningRequest,
    AgentRunProductRuntimeSurfaceRebindEvidence, AgentRunProductRuntimeSurfaceRebindPort,
    AgentRunProductRuntimeSurfaceRebindRequest, ProductAgentSurfaceFacts,
    ProductExecutionProfileRef,
};
use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlan, AgentFrameHookRequirement, HookAction, HookExecutionSite, HookPoint,
    SemanticStrength,
};
use agentdash_domain::{common::Vfs, workflow::AgentFrameRepository};
use agentdash_platform_spi::{
    AgentConfig, CapabilityState, RelayMcpCallContext, RuntimeMcpServer, RuntimeVfsAccessPolicy,
    ToolCluster,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::mcp::{
    RuntimeDynamicToolCatalog, RuntimeMcpToolCatalogError, RuntimeMcpToolCatalogRequest,
};

const DEFAULT_CALLBACK_DEADLINE_MS: u64 = 30_000;

#[derive(Default)]
pub struct CompleteAgentServiceSelectionCatalog {
    profiles: RwLock<BTreeMap<(String, String), CompleteAgentBindingTarget>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCompleteAgentSelection {
    pub target: CompleteAgentBindingTarget,
    pub verified_product_profile_digest: String,
}

#[async_trait]
pub trait CompleteAgentServiceSelector: Send + Sync {
    async fn select(
        &self,
        profile: &ProductExecutionProfileRef,
    ) -> Result<VerifiedCompleteAgentSelection, AgentRunProductRuntimeProvisioningError>;
}

impl CompleteAgentServiceSelectionCatalog {
    pub async fn register(
        &self,
        profile: &ProductExecutionProfileRef,
        target: CompleteAgentBindingTarget,
    ) -> Result<(), AgentRunProductRuntimeProvisioningError> {
        if !profile.validate() {
            return Err(invalid(
                "execution profile must have a valid immutable digest before registration",
            ));
        }
        let profile_key = normalize_profile_key(&profile.profile_key)?;
        let coordinate = (profile_key.clone(), profile.profile_digest.clone());
        let mut profiles = self.profiles.write().await;
        if let Some(existing) = profiles.get(&coordinate) {
            if existing == &target {
                return Ok(());
            }
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: format!(
                    "execution profile `{profile_key}` is already bound to another Complete Agent"
                ),
            });
        }
        profiles.insert(coordinate, target);
        Ok(())
    }

    /// Activates the latest independently verified placement for an exact Product profile.
    pub async fn activate(
        &self,
        profile: &ProductExecutionProfileRef,
        target: CompleteAgentBindingTarget,
    ) -> Result<Option<CompleteAgentBindingTarget>, AgentRunProductRuntimeProvisioningError> {
        if !profile.validate() {
            return Err(invalid(
                "execution profile must have a valid immutable digest before activation",
            ));
        }
        let coordinate = (
            normalize_profile_key(&profile.profile_key)?,
            profile.profile_digest.clone(),
        );
        Ok(self.profiles.write().await.insert(coordinate, target))
    }

    pub async fn deactivate(
        &self,
        profile: &ProductExecutionProfileRef,
        expected_target: &CompleteAgentBindingTarget,
    ) -> Result<bool, AgentRunProductRuntimeProvisioningError> {
        let coordinate = (
            normalize_profile_key(&profile.profile_key)?,
            profile.profile_digest.clone(),
        );
        let mut profiles = self.profiles.write().await;
        if profiles.get(&coordinate) != Some(expected_target) {
            return Ok(false);
        }
        profiles.remove(&coordinate);
        Ok(true)
    }

    /// Atomically publishes one placement across all of its exact Product profiles.
    ///
    /// `previous` is removed only when it is still the current value. A profile owned by another
    /// placement is a conflict and leaves the complete catalog unchanged.
    pub async fn switch_placement(
        &self,
        previous: Option<(&[ProductExecutionProfileRef], &CompleteAgentBindingTarget)>,
        next_profiles: &[ProductExecutionProfileRef],
        next_target: &CompleteAgentBindingTarget,
    ) -> Result<(), AgentRunProductRuntimeProvisioningError> {
        let next = next_profiles
            .iter()
            .map(profile_coordinate)
            .collect::<Result<Vec<_>, _>>()?;
        let previous = previous
            .map(|(profiles, instance_id)| {
                profiles
                    .iter()
                    .map(profile_coordinate)
                    .collect::<Result<Vec<_>, _>>()
                    .map(|profiles| (profiles, instance_id))
            })
            .transpose()?;
        let mut selections = self.profiles.write().await;
        for coordinate in &next {
            if let Some(current) = selections.get(coordinate)
                && current != next_target
                && previous
                    .as_ref()
                    .is_none_or(|(_, previous_instance)| current != *previous_instance)
            {
                return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                    reason: format!(
                        "Product execution profile `{}/{}` is active on another placement",
                        coordinate.0, coordinate.1
                    ),
                });
            }
        }
        if let Some((previous_profiles, previous_instance)) = previous {
            for coordinate in previous_profiles {
                if selections.get(&coordinate) == Some(previous_instance) {
                    selections.remove(&coordinate);
                }
            }
        }
        for coordinate in next {
            selections.insert(coordinate, next_target.clone());
        }
        Ok(())
    }
}

#[async_trait]
impl CompleteAgentServiceSelector for CompleteAgentServiceSelectionCatalog {
    async fn select(
        &self,
        profile: &ProductExecutionProfileRef,
    ) -> Result<VerifiedCompleteAgentSelection, AgentRunProductRuntimeProvisioningError> {
        if !profile.validate() {
            return Err(invalid(
                "execution profile digest does not cover its immutable configuration",
            ));
        }
        let profile_key = normalize_profile_key(&profile.profile_key)?;
        let target = self
            .profiles
            .read()
            .await
            .get(&(profile_key.clone(), profile.profile_digest.clone()))
            .cloned()
            .ok_or_else(|| AgentRunProductRuntimeProvisioningError::Incompatible {
                reason: format!(
                    "no verified Complete Agent is registered for execution profile \
                     `{profile_key}` at digest `{}`",
                    profile.profile_digest
                ),
            })?;
        Ok(VerifiedCompleteAgentSelection {
            target,
            verified_product_profile_digest: profile.profile_digest.clone(),
        })
    }
}

pub struct CompleteAgentProductRuntimeProvisioner {
    host: Arc<CompleteAgentHost>,
    selections: Arc<dyn CompleteAgentServiceSelector>,
    broker: Arc<PlatformToolBroker>,
    dynamic_tools: Arc<dyn RuntimeDynamicToolCatalog>,
    frames: Arc<dyn AgentFrameRepository>,
    callback_deadline_ms: u64,
}

impl CompleteAgentProductRuntimeProvisioner {
    pub fn new(
        host: Arc<CompleteAgentHost>,
        selections: Arc<dyn CompleteAgentServiceSelector>,
        broker: Arc<PlatformToolBroker>,
        dynamic_tools: Arc<dyn RuntimeDynamicToolCatalog>,
        frames: Arc<dyn AgentFrameRepository>,
    ) -> Self {
        Self {
            host,
            selections,
            broker,
            dynamic_tools,
            frames,
            callback_deadline_ms: DEFAULT_CALLBACK_DEADLINE_MS,
        }
    }

    pub fn with_callback_deadline_ms(mut self, callback_deadline_ms: u64) -> Self {
        self.callback_deadline_ms = callback_deadline_ms;
        self
    }

    /// Rebuilds the current process route from Product intent and the persisted Agent source.
    ///
    /// Host state is deliberately absent after restart. The immutable AgentFrame is recompiled,
    /// the current live attachment is selected, and the concrete Agent receives a new callback
    /// surface for this Host incarnation.
    pub async fn ensure_product_binding_route(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentBindingGeneration, AgentRunProductRuntimeProvisioningError> {
        if let Ok(generation) = self
            .host
            .runtime_binding_generation(&binding.runtime_thread_id, &binding.agent.source)
            .await
        {
            return Ok(generation);
        }
        let frame = self
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| failed(error.to_string()))?
            .ok_or_else(|| failed("Product binding AgentFrame does not exist"))?;
        if frame.agent_id != binding.target.agent_id
            || u64::try_from(frame.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: "Product binding does not reference the exact immutable AgentFrame"
                    .to_owned(),
            });
        }
        let surface_facts = ProductAgentSurfaceFacts::from_frame(&frame);
        let selection = self.selections.select(&binding.execution_profile).await?;
        let compiled = compile_product_surface(
            &binding.runtime_thread_id,
            &binding.execution_profile,
            &surface_facts,
            self.broker.as_ref(),
            self.dynamic_tools.as_ref(),
        )
        .await?;
        let request = AgentRunProductRuntimeProvisioningRequest {
            target: binding.target.clone(),
            runtime_thread_id: binding.runtime_thread_id.clone(),
            idempotency_key: format!("restore-route:v1:{}", binding.runtime_thread_id),
            frame: binding.launch_frame.clone(),
            execution_profile: binding.execution_profile.clone(),
            surface_facts,
        };
        let request_digest = provisioning_request_digest(&request)?;
        let attachment_id = selection.target.live_attachment_id.clone();
        self.host
            .provision_runtime_target(CompleteAgentRuntimeTargetProvisioningRequest {
                idempotency_key: AgentIdempotencyKey::new(request.idempotency_key)
                    .map_err(|error| invalid(error.to_string()))?,
                request_digest,
                runtime_thread_id: binding.runtime_thread_id.clone(),
                target: selection.target,
                desired_surface: compiled,
                callback_deadline_ms: self.callback_deadline_ms,
            })
            .await
            .map_err(map_host_error)?;
        self.host
            .restore_runtime_source_route(
                &binding.runtime_thread_id,
                binding.agent.source.clone(),
                AgentEffectIdentity::new(format!(
                    "restore-route:v1:{}:{}",
                    binding.runtime_thread_id, attachment_id
                ))
                .map_err(|error| invalid(error.to_string()))?,
                format!("restore-route:{}", attachment_id),
                self.callback_deadline_ms,
            )
            .await
            .map_err(map_host_error)
    }
}

#[async_trait]
impl AgentRunProductRuntimeProvisioningPort for CompleteAgentProductRuntimeProvisioner {
    async fn provision_runtime_target(
        &self,
        request: AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeProvisioningEvidence, AgentRunProductRuntimeProvisioningError>
    {
        request.validate()?;
        let selection = self.selections.select(&request.execution_profile).await?;
        if selection.verified_product_profile_digest != request.execution_profile.profile_digest {
            return Err(incompatible(
                "Complete Agent selection did not verify the requested execution profile digest",
            ));
        }
        let compiled = compile_product_surface(
            &request.runtime_thread_id,
            &request.execution_profile,
            &request.surface_facts,
            self.broker.as_ref(),
            self.dynamic_tools.as_ref(),
        )
        .await?;
        let request_digest = provisioning_request_digest(&request)?;
        self.host
            .provision_runtime_target(CompleteAgentRuntimeTargetProvisioningRequest {
                idempotency_key: AgentIdempotencyKey::new(request.idempotency_key.clone())
                    .map_err(|error| invalid(error.to_string()))?,
                request_digest,
                runtime_thread_id: request.runtime_thread_id.clone(),
                target: selection.target,
                desired_surface: compiled,
                callback_deadline_ms: self.callback_deadline_ms,
            })
            .await
            .map_err(map_host_error)?;
        Ok(AgentRunProductRuntimeProvisioningEvidence {
            target: request.target,
            runtime_thread_id: request.runtime_thread_id,
            idempotency_key: request.idempotency_key,
            frame: request.frame,
            profile_digest: request.execution_profile.profile_digest,
            surface_facts_digest: request.surface_facts.surface_digest,
        })
    }

    async fn create_agent_source(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
        initial_context: Option<ManagedRuntimeInitialContextPackage>,
    ) -> Result<AgentRunProductAgentCreateEvidence, AgentRunProductRuntimeProvisioningError> {
        request.validate()?;
        let identity = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    "agentdash.product-agent-create/v2",
                    &request.target,
                    &request.runtime_thread_id,
                    &request.idempotency_key,
                ))
                .map_err(|error| failed(error.to_string()))?
            )
        );
        let effect_id = AgentEffectIdentity::new(format!("product-create:v2:{identity}"))
            .map_err(|error| invalid(error.to_string()))?;
        let initial_context = initial_context
            .map(map_initial_context_package)
            .transpose()
            .map_err(|error| invalid(error.to_string()))?;
        let outcome = ManagedRuntimeLifecyclePort::create(
            self.host.as_ref(),
            ManagedRuntimeDispatchContext {
                runtime_thread_id: request.runtime_thread_id.clone(),
                effect_id,
                dispatch_owner: "product-agent-create".to_owned(),
                now_ms: current_time_ms(),
                lease_duration_ms: self.callback_deadline_ms,
            },
            initial_context,
        )
        .await
        .map_err(|error| failed(error.to_string()))?;
        let (service_instance_id, source) = self
            .host
            .runtime_source_association(&request.runtime_thread_id)
            .await
            .map_err(map_host_error)?;
        if source != outcome.receipt.source {
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: "Host route and concrete Agent Create receipt identify different sources"
                    .to_owned(),
            });
        }
        Ok(AgentRunProductAgentCreateEvidence {
            association: AgentRunCompleteAgentAssociation {
                service_instance_id,
                source,
            },
            receipt: outcome.receipt,
        })
    }

    async fn fork_agent_source(
        &self,
        parent: &AgentRunProductRuntimeBinding,
        child_runtime_thread_id: &RuntimeThreadId,
        cutoff: AgentForkPoint,
        effect_id: AgentEffectIdentity,
    ) -> Result<AgentRunProductAgentForkEvidence, AgentRunProductRuntimeProvisioningError> {
        self.ensure_product_binding_route(parent).await?;
        let outcome = self
            .host
            .fork_runtime_source(
                &parent.runtime_thread_id,
                &parent.agent.source,
                child_runtime_thread_id.clone(),
                cutoff,
                effect_id,
                "product-agent-fork".to_owned(),
                self.callback_deadline_ms,
            )
            .await
            .map_err(|error| failed(error.to_string()))?;
        let (service_instance_id, source) = self
            .host
            .runtime_source_association(child_runtime_thread_id)
            .await
            .map_err(map_host_error)?;
        if source
            != outcome
                .receipt
                .child_source
                .clone()
                .ok_or_else(|| failed("concrete Agent Fork receipt has no child source"))?
        {
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason:
                    "Host child route and concrete Agent Fork receipt identify different sources"
                        .to_owned(),
            });
        }
        Ok(AgentRunProductAgentForkEvidence {
            association: AgentRunCompleteAgentAssociation {
                service_instance_id,
                source,
            },
            child_history_digest: outcome.child_history_digest,
            receipt: outcome.receipt,
        })
    }

    async fn bind_agent_source(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
        association: &AgentRunCompleteAgentAssociation,
    ) -> Result<(), AgentRunProductRuntimeProvisioningError> {
        request.validate()?;
        self.provision_runtime_target(request.clone()).await?;
        let target = self
            .host
            .runtime_target(&request.runtime_thread_id)
            .await
            .map_err(map_host_error)?;
        if target.target.logical_instance_id != association.service_instance_id {
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: "child Product profile selected a different concrete Agent service"
                    .to_owned(),
            });
        }
        self.host
            .restore_runtime_source_route(
                &request.runtime_thread_id,
                association.source.clone(),
                AgentEffectIdentity::new(format!(
                    "product-bind-fork:v1:{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&(
                            &request.target,
                            &request.runtime_thread_id,
                            association,
                            &request.idempotency_key,
                        ))
                        .map_err(|error| failed(error.to_string()))?
                    )
                ))
                .map_err(|error| invalid(error.to_string()))?,
                "product-bind-fork".to_owned(),
                self.callback_deadline_ms,
            )
            .await
            .map_err(map_host_error)?;
        Ok(())
    }

    async fn created_agent_association(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<AgentRunCompleteAgentAssociation, AgentRunProductRuntimeProvisioningError> {
        let (service_instance_id, source) = self
            .host
            .runtime_source_association(runtime_thread_id)
            .await
            .map_err(map_host_error)?;
        Ok(AgentRunCompleteAgentAssociation {
            service_instance_id,
            source,
        })
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[async_trait]
impl AgentRunProductRuntimeSurfaceRebindPort for CompleteAgentProductRuntimeProvisioner {
    async fn prepare_runtime_surface_rebind(
        &self,
        request: AgentRunProductRuntimeSurfaceRebindRequest,
    ) -> Result<AgentRunProductRuntimeSurfaceRebindEvidence, AgentRunProductRuntimeProvisioningError>
    {
        request.validate()?;
        let current = self
            .host
            .runtime_target(&request.runtime_thread_id)
            .await
            .map_err(map_host_error)?;
        let desired_surface = compile_product_surface(
            &request.runtime_thread_id,
            &request.execution_profile,
            &request.surface_facts,
            self.broker.as_ref(),
            self.dynamic_tools.as_ref(),
        )
        .await?;
        let request_digest = payload_digest(&serde_json::json!({
            "schema": "agentdash.product-runtime-surface-rebind/v1",
            "target": request.target,
            "runtime_thread_id": request.runtime_thread_id,
            "frame": request.frame,
            "execution_profile_digest": request.execution_profile.profile_digest,
            "surface_facts_digest": request.surface_facts.surface_digest,
            "desired_surface": desired_surface,
        }))?;
        let prepared = self
            .host
            .prepare_runtime_surface_rebind(CompleteAgentRuntimeTargetRecoveryRequest {
                idempotency_key: AgentIdempotencyKey::new(request.idempotency_key.clone())
                    .map_err(|error| invalid(error.to_string()))?,
                request_digest,
                runtime_thread_id: request.runtime_thread_id.clone(),
                expected_generation: current.generation,
                target: current.target,
                desired_surface,
                callback_deadline_ms: current.callbacks.default_deadline_ms,
            })
            .await
            .map_err(map_host_error)?;
        let applied = self
            .host
            .apply_prepared_runtime_surface(
                &request.runtime_thread_id,
                AgentEffectIdentity::new(format!(
                    "product-surface:v2:{:x}",
                    Sha256::digest(request.idempotency_key.as_bytes())
                ))
                .map_err(|error| invalid(error.to_string()))?,
                "product-surface-apply".to_owned(),
                self.callback_deadline_ms,
            )
            .await
            .map_err(|error| failed(error.to_string()))?;
        if applied.binding.generation != prepared.recovered_target.generation
            || applied.binding.source
                != self
                    .host
                    .runtime_source_association(&request.runtime_thread_id)
                    .await
                    .map_err(map_host_error)?
                    .1
            || applied.binding.applied_surface.revision.0 != request.frame.revision
        {
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: "concrete Agent applied surface does not match the Product frame"
                    .to_owned(),
            });
        }
        Ok(AgentRunProductRuntimeSurfaceRebindEvidence {
            target: request.target,
            runtime_thread_id: request.runtime_thread_id,
            idempotency_key: request.idempotency_key,
            previous_generation: prepared.previous_target.generation.0,
            prepared_generation: prepared.recovered_target.generation.0,
            frame: request.frame,
            surface_facts_digest: request.surface_facts.surface_digest,
        })
    }
}

async fn compile_product_surface(
    runtime_thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    execution_profile: &ProductExecutionProfileRef,
    facts: &ProductAgentSurfaceFacts,
    broker: &PlatformToolBroker,
    dynamic_tools: &dyn RuntimeDynamicToolCatalog,
) -> Result<AgentSurfaceSnapshot, AgentRunProductRuntimeProvisioningError> {
    let capability_state =
        decode_optional::<CapabilityState>("capability", &facts.capability)?.unwrap_or_default();
    let vfs = decode_optional::<Vfs>("vfs", &facts.vfs)?;
    let mcp_servers =
        decode_optional::<Vec<RuntimeMcpServer>>("mcp", &facts.mcp)?.unwrap_or_default();
    if capability_state.vfs.active.as_ref() != vfs.as_ref() {
        return Err(invalid(
            "AgentFrame capability VFS does not match its same-revision VFS projection",
        ));
    }
    if capability_state.tool.mcp_servers != mcp_servers {
        return Err(invalid(
            "AgentFrame capability MCP state does not match its same-revision MCP projection",
        ));
    }
    let context =
        decode_optional::<AgentContextSourceSnapshot>("context_source", &facts.context_source)?;
    let hook_plan = decode_optional::<AgentFrameHookPlan>("hook_plan", &facts.hook_plan)?;
    let execution_config: AgentConfig =
        serde_json::from_value(execution_profile.configuration.clone()).map_err(|error| {
            invalid(format!(
                "execution profile configuration is invalid: {error}"
            ))
        })?;
    if let Some(plan) = &hook_plan {
        plan.validate()
            .map_err(|error| invalid(error.to_string()))?;
    }

    let relay_context = vfs
        .as_ref()
        .map(|vfs| {
            let backend_anchor =
                runtime_backend_anchor_from_vfs(vfs, Some("complete_agent_surface".to_owned()))
                    .map_err(|error| invalid(error.to_string()))?;
            Ok(RelayMcpCallContext {
                session_id: runtime_thread_id.to_string(),
                turn_id: None,
                tool_call_id: None,
                backend_anchor,
                vfs: Some(vfs.clone()),
                vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs)),
                identity: None,
            })
        })
        .transpose()?;
    let dynamic = dynamic_tools
        .resolve(RuntimeMcpToolCatalogRequest {
            servers: mcp_servers.clone(),
            capability_state: capability_state.clone(),
            relay_context,
        })
        .await
        .map_err(map_dynamic_catalog_error)?;
    let dynamic_definitions = broker
        .bind_runtime_catalog(runtime_thread_id.clone(), dynamic)
        .await
        .map_err(|error| failed(error.to_string()))?;

    let mut requirements = Vec::new();
    if let Some(prompt) = execution_config
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        requirements.push(surface_requirement(
            "instruction:execution-profile:system-prompt".to_owned(),
            true,
            SemanticFidelity::Exact,
            BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
            AgentSurfaceSemanticFacet::Instruction,
            AgentSurfaceContributionPayload::Instruction {
                channel: "system".to_owned(),
                text: prompt.to_owned(),
                presentation: AgentSurfaceInstructionPresentation::SystemGuidelines,
            },
        )?);
    }
    requirements.extend(instruction_requirements(context.as_ref())?);
    requirements.push(capability_manifest_requirement(&capability_state)?);
    requirements.extend(workspace_requirements(vfs.as_ref())?);
    requirements.extend(tool_requirements(
        broker.definitions(),
        &capability_state,
        false,
    )?);
    requirements.extend(tool_requirements(
        dynamic_definitions,
        &capability_state,
        true,
    )?);
    if let Some(plan) = hook_plan.as_ref() {
        for requirement in &plan.requirements {
            if let Some(requirement) = hook_requirement(requirement)? {
                requirements.push(requirement);
            }
        }
    }
    let digest = payload_digest(&serde_json::json!({
        "schema": "agentdash.complete-agent-compiled-surface/v1",
        "product_surface_digest": facts.surface_digest,
        "verified_execution_profile_digest": execution_profile.profile_digest,
        "requirements": requirements,
    }))?;
    Ok(AgentSurfaceSnapshot {
        revision: AgentSurfaceRevision(facts.surface_revision),
        digest: AgentSurfaceDigest::new(digest.as_str().to_owned())
            .map_err(|error| invalid(error.to_string()))?,
        requirements,
    })
}

fn capability_manifest_requirement(
    state: &CapabilityState,
) -> Result<AgentSurfaceRequirement, AgentRunProductRuntimeProvisioningError> {
    let manifest = capability_manifest(state)?;
    surface_requirement(
        "instruction:capability:manifest".to_owned(),
        true,
        SemanticFidelity::Exact,
        BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
        AgentSurfaceSemanticFacet::Instruction,
        AgentSurfaceContributionPayload::Instruction {
            channel: "capabilities".to_owned(),
            text: render_capability_manifest(&manifest),
            presentation: AgentSurfaceInstructionPresentation::CapabilityManifest { manifest },
        },
    )
}

fn capability_manifest(
    state: &CapabilityState,
) -> Result<AgentCapabilityManifest, AgentRunProductRuntimeProvisioningError> {
    let mcp_servers = state
        .tool
        .mcp_servers
        .iter()
        .map(|server| {
            let (status, tool_count, reason_code, message) = match &server.readiness {
                agentdash_platform_spi::RuntimeMcpSourceReadiness::Pending => {
                    ("pending", None, None, None)
                }
                agentdash_platform_spi::RuntimeMcpSourceReadiness::Ready { tool_count } => (
                    "ready",
                    Some(u32::try_from(*tool_count).map_err(|_| {
                        invalid("MCP tool count exceeds the Complete Agent protocol boundary")
                    })?),
                    None,
                    None,
                ),
                agentdash_platform_spi::RuntimeMcpSourceReadiness::Unavailable {
                    reason_code,
                    message,
                } => (
                    "unavailable",
                    None,
                    Some(reason_code.clone()),
                    Some(message.clone()),
                ),
            };
            Ok(AgentCapabilityMcpServer {
                name: server.name.clone(),
                uses_relay: server.uses_relay,
                status: status.to_owned(),
                tool_count,
                reason_code,
                message,
            })
        })
        .collect::<Result<Vec<_>, AgentRunProductRuntimeProvisioningError>>()?;
    let companion_agents = state
        .companion
        .agents
        .iter()
        .map(|agent| AgentCapabilityCompanionAgent {
            agent_key: agent.name.clone(),
            executor: agent.executor.clone(),
            display_name: agent.display_name.clone(),
        })
        .collect();
    let channels = state
        .channel
        .visible_channels
        .iter()
        .map(|channel| {
            Ok(AgentCapabilityChannel {
                channel_ref: format!(
                    "{}:{}",
                    channel.channel_ref.owner.stable_key(),
                    channel.channel_ref.channel_id
                ),
                aliases: channel.aliases.clone(),
                operations: channel
                    .operations
                    .iter()
                    .map(wire_name)
                    .collect::<Result<Vec<_>, _>>()?,
                readiness: wire_name(&channel.readiness)?,
            })
        })
        .collect::<Result<Vec<_>, AgentRunProductRuntimeProvisioningError>>()?;
    let vfs = state
        .vfs
        .active
        .as_ref()
        .map(|vfs| {
            Ok(AgentCapabilityVfs {
                default_mount: vfs.default_mount_id.clone(),
                mounts: vfs
                    .mounts
                    .iter()
                    .map(|mount| {
                        Ok(AgentCapabilityMount {
                            id: mount.id.clone(),
                            display_name: mount.display_name.clone(),
                            root_ref: mount.root_ref.clone(),
                            capabilities: mount
                                .capabilities
                                .iter()
                                .map(wire_name)
                                .collect::<Result<Vec<_>, _>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>, AgentRunProductRuntimeProvisioningError>>()?,
            })
        })
        .transpose()?;
    let skills = state
        .skill
        .skills
        .iter()
        .map(|skill| {
            Ok(AgentCapabilitySkill {
                name: skill.name.clone(),
                capability_key: skill.capability_key.clone(),
                provider_key: skill.provider_key.clone(),
                local_name: skill.local_name.clone(),
                display_name: skill.display_name.clone(),
                description: skill.description.clone(),
                file_path: skill.file_path.clone(),
                base_dir: skill.base_dir.clone(),
                exposure: wire_name(&skill.exposure)?,
                disable_model_invocation: skill.disable_model_invocation,
            })
        })
        .collect::<Result<Vec<_>, AgentRunProductRuntimeProvisioningError>>()?;
    let memory_sources = state
        .memory
        .inventory
        .clusters
        .iter()
        .flat_map(|cluster| cluster.sources.iter())
        .map(|source| {
            Ok(AgentCapabilityMemorySource {
                provider_key: source.provider_key.clone(),
                source_key: source.source_key.clone(),
                display_name: source.display_name.clone(),
                source_uri: source.source_uri.clone(),
                index_uri: source.index_uri.clone(),
                mount_id: source.mount_id.clone(),
                scope: wire_name(&source.scope)?,
                index_status: wire_name(&source.index_status)?,
                trust_level: wire_name(&source.trust_level)?,
                summary: source.summary.clone(),
            })
        })
        .collect::<Result<Vec<_>, AgentRunProductRuntimeProvisioningError>>()?;
    let skill_diagnostics = state
        .skill
        .diagnostics
        .iter()
        .map(|diagnostic| AgentCapabilityDiagnostic {
            provider_key: diagnostic.provider_key.clone(),
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
            source_key: diagnostic.local_name.clone(),
            uri: diagnostic.file_path.clone(),
        })
        .collect();
    let memory_diagnostics = state
        .memory
        .inventory
        .diagnostics
        .iter()
        .map(|diagnostic| AgentCapabilityDiagnostic {
            provider_key: diagnostic.provider_key.clone(),
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
            source_key: diagnostic.source_key.clone(),
            uri: diagnostic.uri.clone(),
        })
        .collect();
    Ok(AgentCapabilityManifest {
        tool_capabilities: state.capability_keys().into_iter().collect(),
        tool_clusters: state
            .tool
            .enabled_clusters
            .iter()
            .map(wire_name)
            .collect::<Result<Vec<_>, _>>()?,
        included_tool_paths: state.included_tool_paths().into_iter().collect(),
        excluded_tool_paths: state.excluded_tool_paths().into_iter().collect(),
        mcp_servers,
        companion_agents,
        channels,
        vfs,
        skills,
        skill_diagnostics,
        memory_sources,
        memory_diagnostics,
        workspace_module: AgentCapabilityWorkspaceModule {
            mode: wire_name(&state.workspace_module.mode)?,
            allowed_module_ids: state.workspace_module.allowed_module_ids.clone(),
        },
    })
}

fn wire_name(
    value: &impl serde::Serialize,
) -> Result<String, AgentRunProductRuntimeProvisioningError> {
    serde_json::to_value(value)
        .map_err(|error| invalid(error.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("capability enum did not serialize to a wire name"))
}

fn render_capability_manifest(manifest: &AgentCapabilityManifest) -> String {
    let mut lines = vec![
        "## Agent Capability Surface".to_owned(),
        "This manifest is the complete capability input accepted for this Agent surface."
            .to_owned(),
    ];
    lines.push(format!(
        "### Tool Capabilities\n{}",
        render_list(&manifest.tool_capabilities)
    ));
    lines.push(format!(
        "### Tool Clusters\n{}",
        render_list(&manifest.tool_clusters)
    ));
    if !manifest.mcp_servers.is_empty() {
        lines.push(format!(
            "### MCP Servers\n{}",
            manifest
                .mcp_servers
                .iter()
                .map(|server| format!("- `{}` ({})", server.name, server.status))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if let Some(vfs) = &manifest.vfs {
        lines.push(format!(
            "### VFS Mounts\n{}",
            vfs.mounts
                .iter()
                .map(|mount| format!(
                    "- `{}`: `{}` [{}]{}",
                    mount.id,
                    mount.root_ref,
                    mount.capabilities.join(", "),
                    (vfs.default_mount.as_deref() == Some(mount.id.as_str()))
                        .then_some(" (default)")
                        .unwrap_or_default()
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    let model_skills = manifest
        .skills
        .iter()
        .filter(|skill| skill.exposure == "default_exposed" && !skill.disable_model_invocation)
        .collect::<Vec<_>>();
    if !model_skills.is_empty() {
        lines.push(format!(
            "### Available Skills\nRead the matching `SKILL.md` before acting.\n{}",
            model_skills
                .iter()
                .map(|skill| format!(
                    "- `{}`: {} (path: `{}`)",
                    skill.display_name.as_deref().unwrap_or(&skill.name),
                    skill.description,
                    skill.file_path
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !manifest.skill_diagnostics.is_empty() {
        lines.push(format!(
            "### Skill Discovery Diagnostics\n{}",
            manifest
                .skill_diagnostics
                .iter()
                .map(|diagnostic| format!(
                    "- `{}` / `{}`: {}",
                    diagnostic.provider_key, diagnostic.code, diagnostic.message
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !manifest.memory_sources.is_empty() {
        lines.push(format!(
            "### Memory Sources\n{}",
            manifest
                .memory_sources
                .iter()
                .map(|source| format!(
                    "- `{}`: `{}` (index: `{}`)",
                    source.display_name, source.source_uri, source.index_uri
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !manifest.companion_agents.is_empty() {
        lines.push(format!(
            "### Companion Agents\n{}",
            manifest
                .companion_agents
                .iter()
                .map(|agent| format!("- `{}` ({})", agent.display_name, agent.executor))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !manifest.channels.is_empty() {
        lines.push(format!(
            "### Channels\n{}",
            manifest
                .channels
                .iter()
                .map(|channel| format!(
                    "- `{}` [{}] ({})",
                    channel.channel_ref,
                    channel.operations.join(", "),
                    channel.readiness
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    lines.push(format!(
        "### Workspace Modules\n- visibility: `{}`{}",
        manifest.workspace_module.mode,
        if manifest.workspace_module.allowed_module_ids.is_empty() {
            String::new()
        } else {
            format!(
                "\n- allowed: {}",
                manifest.workspace_module.allowed_module_ids.join(", ")
            )
        }
    ));
    lines.join("\n\n")
}

fn render_list(values: &[String]) -> String {
    if values.is_empty() {
        "- none".to_owned()
    } else {
        values
            .iter()
            .map(|value| format!("- `{value}`"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn instruction_requirements(
    context: Option<&AgentContextSourceSnapshot>,
) -> Result<Vec<AgentSurfaceRequirement>, AgentRunProductRuntimeProvisioningError> {
    let Some(context) = context else {
        return Ok(Vec::new());
    };
    context
        .fragments
        .iter()
        .filter(|fragment| fragment.runtime_agent_scope)
        .map(|fragment| {
            let payload = AgentSurfaceContributionPayload::Instruction {
                channel: fragment.slot.clone(),
                text: fragment.content.clone(),
                presentation: instruction_presentation(&fragment.slot),
            };
            surface_requirement(
                format!("instruction:{}:{}", fragment.order, fragment.label),
                true,
                SemanticFidelity::Exact,
                BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
                AgentSurfaceSemanticFacet::Instruction,
                payload,
            )
        })
        .collect()
}

fn instruction_presentation(slot: &str) -> AgentSurfaceInstructionPresentation {
    match slot {
        "persona" | "agent_identity" => AgentSurfaceInstructionPresentation::Identity,
        "workspace" | "vfs" | "runtime_policy" => AgentSurfaceInstructionPresentation::Environment,
        "memory" | "memory_context" => AgentSurfaceInstructionPresentation::MemoryContext,
        "constraint" | "constraints" | "instruction" | "instruction_append" => {
            AgentSurfaceInstructionPresentation::SystemGuidelines
        }
        "user_context" => AgentSurfaceInstructionPresentation::UserContext,
        _ => AgentSurfaceInstructionPresentation::AssignmentContext,
    }
}

fn workspace_requirements(
    vfs: Option<&Vfs>,
) -> Result<Vec<AgentSurfaceRequirement>, AgentRunProductRuntimeProvisioningError> {
    let Some(vfs) = vfs else {
        return Ok(Vec::new());
    };
    vfs.mounts
        .iter()
        .map(|mount| {
            surface_requirement(
                format!("workspace:{}", mount.id),
                vfs.default_mount_id.as_deref() == Some(mount.id.as_str()),
                SemanticFidelity::Exact,
                BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
                AgentSurfaceSemanticFacet::Workspace,
                AgentSurfaceContributionPayload::Workspace {
                    requirement: mount.root_ref.clone(),
                },
            )
        })
        .collect()
}

fn tool_requirements(
    definitions: Vec<RuntimeToolDefinition>,
    capability_state: &CapabilityState,
    dynamic_mcp: bool,
) -> Result<Vec<AgentSurfaceRequirement>, AgentRunProductRuntimeProvisioningError> {
    definitions
        .into_iter()
        .filter(|definition| {
            dynamic_mcp || static_tool_enabled(capability_state, definition.name.as_str())
        })
        .map(|definition| {
            let key = format!("tool:{}", definition.name);
            let payload = AgentSurfaceContributionPayload::Tool {
                name: definition.name,
                description: definition.description,
                input_schema: definition.parameters_schema,
                output_schema: None,
                protocol_projector: definition.protocol_projector,
            };
            surface_requirement(
                key,
                false,
                SemanticFidelity::Exact,
                BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::BindingOnly,
                }),
                payload,
            )
        })
        .collect()
}

fn static_tool_enabled(capability_state: &CapabilityState, name: &str) -> bool {
    let cluster = match name {
        "mounts_list" | "fs_read" | "fs_glob" | "fs_grep" => ToolCluster::Read,
        "fs_apply_patch" => ToolCluster::Write,
        "shell_exec" => ToolCluster::Execute,
        "task_read" | "task_write" => ToolCluster::Task,
        "workspace_module_list"
        | "workspace_module_describe"
        | "workspace_module_operate"
        | "workspace_module_invoke"
        | "workspace_module_present" => ToolCluster::WorkspaceModule,
        "companion_request" | "companion_respond" => ToolCluster::Collaboration,
        "complete_lifecycle_node" => ToolCluster::Workflow,
        "wait" => ToolCluster::Collaboration,
        _ => return false,
    };
    capability_state.has(cluster)
}

fn hook_requirement(
    source: &AgentFrameHookRequirement,
) -> Result<Option<AgentSurfaceRequirement>, AgentRunProductRuntimeProvisioningError> {
    let route = match source.site {
        HookExecutionSite::AgentCoreCallback => AgentSurfaceRoute::AgentNativeCallback,
        HookExecutionSite::DriverNative => AgentSurfaceRoute::AgentNativeRegistry,
        HookExecutionSite::ManagedRuntime
        | HookExecutionSite::ToolBroker
        | HookExecutionSite::ObservedEventReaction => return Ok(None),
    };
    let Some((point, timing)) = hook_point(source.requirement.point) else {
        if source.requirement.required {
            return Err(incompatible(format!(
                "required Agent hook point {:?} has no Complete Agent surface semantic",
                source.requirement.point
            )));
        }
        return Ok(None);
    };
    let Some(actions) = hook_actions(&source.requirement.actions) else {
        if source.requirement.required {
            return Err(incompatible(format!(
                "required Agent hook `{}` contains unsupported actions",
                source.definition_id
            )));
        }
        return Ok(None);
    };
    let fidelity = semantic_strength(source.requirement.minimum_strength);
    let blocking = if actions.contains(&AgentHookAction::AllowOrDeny) {
        AgentHookBlockingSemantics::Blocking { fidelity }
    } else {
        AgentHookBlockingSemantics::NonBlocking
    };
    let mutations = [
        (
            AgentHookAction::RewriteInput,
            AgentHookMutationKind::RewriteInput,
        ),
        (
            AgentHookAction::RewriteResult,
            AgentHookMutationKind::RewriteResult,
        ),
        (
            AgentHookAction::AddContext,
            AgentHookMutationKind::AddContext,
        ),
    ]
    .into_iter()
    .filter(|(action, _)| actions.contains(action))
    .map(|(_, mutation)| (mutation, fidelity))
    .collect();
    let effects = actions
        .contains(&AgentHookAction::EmitEffect)
        .then_some((AgentHookEffectKind::EmitEffect, fidelity))
        .into_iter()
        .collect();
    let semantics = AgentHookSemanticFacet {
        point,
        timing,
        blocking,
        mutations,
        effects,
    };
    let payload = AgentSurfaceContributionPayload::Hook {
        definition_id: AgentHookDefinitionId::new(source.definition_id.to_string())
            .map_err(|error| invalid(error.to_string()))?,
        point,
        timing,
        actions,
        deadline_ms: DEFAULT_CALLBACK_DEADLINE_MS,
    };
    surface_requirement(
        format!("hook:{}", source.definition_id),
        source.requirement.required,
        fidelity,
        BTreeSet::from([route]),
        AgentSurfaceSemanticFacet::Hook(semantics),
        payload,
    )
    .map(Some)
}

fn hook_point(point: HookPoint) -> Option<(AgentHookPoint, AgentHookTiming)> {
    Some(match point {
        HookPoint::BeforeTurn => (AgentHookPoint::BeforeTurn, AgentHookTiming::Before),
        HookPoint::AfterTurn => (AgentHookPoint::AfterTurn, AgentHookTiming::After),
        HookPoint::BeforeProviderRequest => (
            AgentHookPoint::BeforeProviderRequest,
            AgentHookTiming::Before,
        ),
        HookPoint::BeforeTool => (AgentHookPoint::BeforeTool, AgentHookTiming::Before),
        HookPoint::AfterTool => (AgentHookPoint::AfterTool, AgentHookTiming::After),
        HookPoint::BeforeContextCompact => {
            (AgentHookPoint::BeforeCompaction, AgentHookTiming::Before)
        }
        HookPoint::AfterContextCompact => (AgentHookPoint::AfterCompaction, AgentHookTiming::After),
        HookPoint::BeforeStop => (AgentHookPoint::BeforeStop, AgentHookTiming::Before),
        HookPoint::AfterItem => (AgentHookPoint::AfterItem, AgentHookTiming::After),
        HookPoint::BeforeThreadStart | HookPoint::AfterThreadStart => return None,
    })
}

fn hook_actions(source: &BTreeSet<HookAction>) -> Option<BTreeSet<AgentHookAction>> {
    let mut actions = BTreeSet::new();
    for action in source {
        actions.insert(match action {
            HookAction::Observe => AgentHookAction::Observe,
            HookAction::AddContext => AgentHookAction::AddContext,
            HookAction::Block => AgentHookAction::AllowOrDeny,
            HookAction::RewriteInput => AgentHookAction::RewriteInput,
            HookAction::RewriteResult => AgentHookAction::RewriteResult,
            HookAction::EmitEffect => AgentHookAction::EmitEffect,
            HookAction::RequestApproval | HookAction::ContinueTurn | HookAction::RefreshSurface => {
                return None;
            }
        });
    }
    Some(actions)
}

fn semantic_strength(strength: SemanticStrength) -> SemanticFidelity {
    match strength {
        SemanticStrength::ObservedOnly => SemanticFidelity::Observed,
        SemanticStrength::BoundaryAdapted => SemanticFidelity::Approximation,
        SemanticStrength::ExactDurableBoundary | SemanticStrength::ExactSynchronous => {
            SemanticFidelity::Exact
        }
    }
}

fn surface_requirement(
    key: String,
    required: bool,
    minimum_fidelity: SemanticFidelity,
    allowed_routes: BTreeSet<AgentSurfaceRoute>,
    semantics: AgentSurfaceSemanticFacet,
    payload: AgentSurfaceContributionPayload,
) -> Result<AgentSurfaceRequirement, AgentRunProductRuntimeProvisioningError> {
    let payload_digest = payload_digest(&payload)?;
    Ok(AgentSurfaceRequirement {
        key,
        required,
        minimum_fidelity,
        allowed_routes,
        semantics,
        payload,
        payload_digest,
    })
}

fn provisioning_request_digest(
    request: &AgentRunProductRuntimeProvisioningRequest,
) -> Result<AgentPayloadDigest, AgentRunProductRuntimeProvisioningError> {
    payload_digest(request)
}

fn payload_digest(
    value: &impl serde::Serialize,
) -> Result<AgentPayloadDigest, AgentRunProductRuntimeProvisioningError> {
    let bytes = serde_json::to_vec(value).map_err(|error| invalid(error.to_string()))?;
    AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(bytes)))
        .map_err(|error| invalid(error.to_string()))
}

fn decode_optional<T: serde::de::DeserializeOwned>(
    field: &str,
    value: &Option<serde_json::Value>,
) -> Result<Option<T>, AgentRunProductRuntimeProvisioningError> {
    value
        .as_ref()
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|error| invalid(format!("{field} facts are invalid: {error}")))
        })
        .transpose()
}

fn normalize_profile_key(value: &str) -> Result<String, AgentRunProductRuntimeProvisioningError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(invalid("execution profile key cannot be empty"));
    }
    Ok(value)
}

fn profile_coordinate(
    profile: &ProductExecutionProfileRef,
) -> Result<(String, String), AgentRunProductRuntimeProvisioningError> {
    if !profile.validate() {
        return Err(invalid(
            "execution profile must have a valid immutable digest",
        ));
    }
    Ok((
        normalize_profile_key(&profile.profile_key)?,
        profile.profile_digest.clone(),
    ))
}

fn map_dynamic_catalog_error(
    error: RuntimeMcpToolCatalogError,
) -> AgentRunProductRuntimeProvisioningError {
    incompatible(error.to_string())
}

fn map_host_error(error: CompleteAgentHostError) -> AgentRunProductRuntimeProvisioningError {
    match error {
        CompleteAgentHostError::ProvisioningConflict => {
            AgentRunProductRuntimeProvisioningError::Conflict {
                reason: error.to_string(),
            }
        }
        CompleteAgentHostError::DispatchRejected { .. }
        | CompleteAgentHostError::UnavailableAttachment { .. } => incompatible(error.to_string()),
        _ => failed(error.to_string()),
    }
}

fn invalid(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::InvalidRequest {
        reason: reason.into(),
    }
}

fn incompatible(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::Incompatible {
        reason: reason.into(),
    }
}

fn failed(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::Failed {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_host::CompleteAgentPlacement;
    use agentdash_agent_service_api::{
        AgentProfileDigest, AgentServiceDefinitionId, AgentServiceInstanceId,
        CompleteAgentLiveAttachmentId,
    };

    use super::*;

    fn binding_target(instance: &str) -> CompleteAgentBindingTarget {
        let profile = AgentProfileDigest::new("sha256:service-profile").expect("profile");
        CompleteAgentBindingTarget {
            logical_instance_id: AgentServiceInstanceId::new(instance).expect("instance"),
            live_attachment_id: CompleteAgentLiveAttachmentId::new(format!(
                "attachment-{instance}"
            ))
            .expect("attachment"),
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            verified_build_digest: AgentPayloadDigest::new("sha256:build").expect("build"),
            verified_profile_digest: profile.clone(),
            offer_profile_digest: profile,
            placement: CompleteAgentPlacement::InProcess {
                host_incarnation_id: "fixture-host".to_owned(),
            },
            remote_binding: None,
        }
    }

    fn product_profile(system_prompt: &str) -> ProductExecutionProfileRef {
        let mut profile = ProductExecutionProfileRef {
            profile_key: "CODEX".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({
                "executor": "CODEX",
                "system_prompt": system_prompt,
            }),
            credential_scope: None,
        };
        profile.refresh_digest();
        profile
    }

    #[tokio::test]
    async fn exact_selection_is_pinned_to_the_verified_product_profile_digest() {
        let catalog = CompleteAgentServiceSelectionCatalog::default();
        let profile = product_profile("one");
        let target = binding_target("codex-fixture");
        catalog.register(&profile, target.clone()).await.unwrap();

        assert_eq!(
            catalog.select(&profile).await.unwrap(),
            VerifiedCompleteAgentSelection {
                target,
                verified_product_profile_digest: profile.profile_digest.clone(),
            }
        );

        let changed = product_profile("two");
        assert!(matches!(
            catalog.select(&changed).await,
            Err(AgentRunProductRuntimeProvisioningError::Incompatible { .. })
        ));
    }

    #[tokio::test]
    async fn placement_switch_is_atomic_across_every_exact_profile() {
        let catalog = CompleteAgentServiceSelectionCatalog::default();
        let previous_profile = product_profile("previous");
        let next_profile = product_profile("next");
        let conflicting_profile = product_profile("conflict");
        let previous_target = binding_target("previous");
        let next_target = binding_target("next");
        let other_target = binding_target("other");
        catalog
            .register(&previous_profile, previous_target.clone())
            .await
            .unwrap();
        catalog
            .register(&conflicting_profile, other_target)
            .await
            .unwrap();

        assert!(matches!(
            catalog
                .switch_placement(
                    Some((std::slice::from_ref(&previous_profile), &previous_target)),
                    &[next_profile.clone(), conflicting_profile],
                    &next_target,
                )
                .await,
            Err(AgentRunProductRuntimeProvisioningError::Conflict { .. })
        ));
        assert_eq!(
            catalog.select(&previous_profile).await.unwrap().target,
            previous_target
        );
        assert!(catalog.select(&next_profile).await.is_err());

        catalog
            .switch_placement(
                Some((std::slice::from_ref(&previous_profile), &previous_target)),
                std::slice::from_ref(&next_profile),
                &next_target,
            )
            .await
            .unwrap();
        assert!(catalog.select(&previous_profile).await.is_err());
        assert_eq!(
            catalog.select(&next_profile).await.unwrap().target,
            next_target
        );
    }

    #[test]
    fn product_sites_are_not_relabelled_as_agent_callbacks() {
        let requirement = AgentFrameHookRequirement {
            definition_id:
                agentdash_application_ports::agent_frame_hook_plan::HookDefinitionId::new(
                    "product-hook",
                )
                .unwrap(),
            requirement:
                agentdash_application_ports::agent_frame_hook_plan::HookRequirement {
                    point: HookPoint::BeforeTool,
                    actions: BTreeSet::from([HookAction::Block]),
                    minimum_strength: SemanticStrength::ExactSynchronous,
                    failure_policy:
                        agentdash_application_ports::agent_frame_hook_plan::HookFailurePolicy::FailClosed,
                    required: true,
                },
            site: HookExecutionSite::ToolBroker,
        };

        assert_eq!(hook_requirement(&requirement).unwrap(), None);
    }

    #[test]
    fn discovered_skill_and_mcp_facts_compile_to_model_visible_instructions() {
        let mut capability = CapabilityState::default();
        capability
            .skill
            .skills
            .push(agentdash_platform_spi::SkillEntry {
                name: "canvas-system".to_owned(),
                capability_key: "workspace/canvas-system".to_owned(),
                provider_key: "workspace".to_owned(),
                local_name: "canvas-system".to_owned(),
                display_name: Some("Canvas".to_owned()),
                description: "Create and edit Canvas modules.".to_owned(),
                file_path: "lifecycle://skills/canvas-system/SKILL.md".to_owned(),
                base_dir: Some("lifecycle://skills/canvas-system".to_owned()),
                exposure: agentdash_platform_spi::SkillContextExposure::DefaultExposed,
                disable_model_invocation: false,
            });
        let servers = vec![RuntimeMcpServer::new(
            "agentdash-workflow-tools".to_owned(),
            agentdash_platform_spi::McpTransportConfig::Http {
                url: "http://localhost/mcp/workflow/test".to_owned(),
                headers: Vec::new(),
            },
            false,
        )];
        capability.tool.mcp_servers = servers;
        let capability = capability_manifest_requirement(&capability).unwrap();
        assert!(matches!(
            &capability.payload,
            AgentSurfaceContributionPayload::Instruction {
                channel,
                text,
                presentation: AgentSurfaceInstructionPresentation::CapabilityManifest { manifest },
            } if channel == "capabilities"
                && text.contains("Canvas")
                && text.contains("lifecycle://skills/canvas-system/SKILL.md")
                && text.contains("agentdash-workflow-tools")
                && manifest.skills.len() == 1
                && manifest.mcp_servers.len() == 1
        ));

        let dynamic = tool_requirements(
            vec![RuntimeToolDefinition {
                name: agentdash_agent_service_api::AgentToolName::new(
                    "mcp_agentdash_workflow_tools_get_lifecycle",
                )
                .unwrap(),
                description: "Get lifecycle".to_owned(),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Mcp {
                    server_key: "agentdash-workflow-tools".to_owned(),
                },
                parameters_schema: serde_json::json!({"type": "object"}),
                permission: agentdash_agent_runtime::RuntimeToolPermission::ProductRead,
                effect: agentdash_agent_runtime::RuntimeToolEffect::ReadOnly,
            }],
            &CapabilityState::default(),
            true,
        )
        .unwrap();
        assert!(matches!(
            &dynamic[0].payload,
            AgentSurfaceContributionPayload::Tool { name, .. }
                if name.as_str() == "mcp_agentdash_workflow_tools_get_lifecycle"
        ));
    }

    #[test]
    fn agent_callback_hook_preserves_blocking_and_rewrite_semantics() {
        let requirement = AgentFrameHookRequirement {
            definition_id:
                agentdash_application_ports::agent_frame_hook_plan::HookDefinitionId::new(
                    "agent-hook",
                )
                .unwrap(),
            requirement:
                agentdash_application_ports::agent_frame_hook_plan::HookRequirement {
                    point: HookPoint::BeforeTool,
                    actions: BTreeSet::from([HookAction::Block, HookAction::RewriteInput]),
                    minimum_strength: SemanticStrength::ExactSynchronous,
                    failure_policy:
                        agentdash_application_ports::agent_frame_hook_plan::HookFailurePolicy::FailClosed,
                    required: true,
                },
            site: HookExecutionSite::AgentCoreCallback,
        };

        let compiled = hook_requirement(&requirement)
            .unwrap()
            .expect("Agent callback contribution");
        assert_eq!(
            compiled.allowed_routes,
            BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback])
        );
        let AgentSurfaceSemanticFacet::Hook(semantics) = compiled.semantics else {
            panic!("hook semantics");
        };
        assert!(semantics.blocking.is_blocking());
        assert_eq!(
            semantics
                .mutations
                .get(&AgentHookMutationKind::RewriteInput),
            Some(&SemanticFidelity::Exact)
        );
    }

    #[test]
    fn interactive_approval_is_not_relabelled_as_one_phase_allow_or_deny() {
        let requirement = AgentFrameHookRequirement {
            definition_id:
                agentdash_application_ports::agent_frame_hook_plan::HookDefinitionId::new(
                    "approval-hook",
                )
                .unwrap(),
            requirement:
                agentdash_application_ports::agent_frame_hook_plan::HookRequirement {
                    point: HookPoint::BeforeTool,
                    actions: BTreeSet::from([HookAction::RequestApproval]),
                    minimum_strength: SemanticStrength::ExactSynchronous,
                    failure_policy:
                        agentdash_application_ports::agent_frame_hook_plan::HookFailurePolicy::FailClosed,
                    required: true,
                },
            site: HookExecutionSite::AgentCoreCallback,
        };

        assert!(matches!(
            hook_requirement(&requirement),
            Err(AgentRunProductRuntimeProvisioningError::Incompatible { .. })
        ));
    }
}
