use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime::{PlatformToolBroker, RuntimeToolDefinition};
use agentdash_agent_runtime_host::{
    CompleteAgentHost, CompleteAgentHostError, CompleteAgentRuntimeTargetProvisioningRequest,
};
use agentdash_agent_service_api::{
    AgentHookAction, AgentHookBlockingSemantics, AgentHookDefinitionId, AgentHookEffectKind,
    AgentHookMutationKind, AgentHookPoint, AgentHookSemanticFacet, AgentHookTiming,
    AgentIdempotencyKey, AgentPayloadDigest, AgentServiceInstanceId,
    AgentSurfaceContributionPayload, AgentSurfaceDigest, AgentSurfaceRequirement,
    AgentSurfaceRevision, AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentSurfaceSnapshot,
    AgentToolDelivery, AgentToolSemanticFacet, AgentToolUpdateSemantics, SemanticFidelity,
};
use agentdash_application_agentrun::agent_run::frame::{
    AgentContextSourceSnapshot, runtime_backend_anchor_from_vfs,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeProvisioningError, AgentRunProductRuntimeProvisioningEvidence,
    AgentRunProductRuntimeProvisioningPort, AgentRunProductRuntimeProvisioningRequest,
    ProductAgentSurfaceFacts, ProductExecutionProfileRef,
};
use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlan, AgentFrameHookRequirement, HookAction, HookExecutionSite, HookPoint,
    SemanticStrength,
};
use agentdash_domain::common::Vfs;
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
    profiles: RwLock<BTreeMap<(String, String), AgentServiceInstanceId>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCompleteAgentSelection {
    pub service_instance_id: AgentServiceInstanceId,
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
        instance_id: AgentServiceInstanceId,
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
            if existing == &instance_id {
                return Ok(());
            }
            return Err(AgentRunProductRuntimeProvisioningError::Conflict {
                reason: format!(
                    "execution profile `{profile_key}` is already bound to another Complete Agent"
                ),
            });
        }
        profiles.insert(coordinate, instance_id);
        Ok(())
    }

    /// Activates the latest independently verified placement for an exact Product profile.
    pub async fn activate(
        &self,
        profile: &ProductExecutionProfileRef,
        instance_id: AgentServiceInstanceId,
    ) -> Result<Option<AgentServiceInstanceId>, AgentRunProductRuntimeProvisioningError> {
        if !profile.validate() {
            return Err(invalid(
                "execution profile must have a valid immutable digest before activation",
            ));
        }
        let coordinate = (
            normalize_profile_key(&profile.profile_key)?,
            profile.profile_digest.clone(),
        );
        Ok(self.profiles.write().await.insert(coordinate, instance_id))
    }

    pub async fn deactivate(
        &self,
        profile: &ProductExecutionProfileRef,
        expected_instance_id: &AgentServiceInstanceId,
    ) -> Result<bool, AgentRunProductRuntimeProvisioningError> {
        let coordinate = (
            normalize_profile_key(&profile.profile_key)?,
            profile.profile_digest.clone(),
        );
        let mut profiles = self.profiles.write().await;
        if profiles.get(&coordinate) != Some(expected_instance_id) {
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
        previous: Option<(&[ProductExecutionProfileRef], &AgentServiceInstanceId)>,
        next_profiles: &[ProductExecutionProfileRef],
        next_instance_id: &AgentServiceInstanceId,
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
                && current != next_instance_id
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
            selections.insert(coordinate, next_instance_id.clone());
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
        let service_instance_id = self
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
            service_instance_id,
            verified_product_profile_digest: profile.profile_digest.clone(),
        })
    }
}

pub struct CompleteAgentProductRuntimeProvisioner {
    host: Arc<CompleteAgentHost>,
    selections: Arc<dyn CompleteAgentServiceSelector>,
    broker: Arc<PlatformToolBroker>,
    dynamic_tools: Arc<dyn RuntimeDynamicToolCatalog>,
    callback_deadline_ms: u64,
}

impl CompleteAgentProductRuntimeProvisioner {
    pub fn new(
        host: Arc<CompleteAgentHost>,
        selections: Arc<dyn CompleteAgentServiceSelector>,
        broker: Arc<PlatformToolBroker>,
        dynamic_tools: Arc<dyn RuntimeDynamicToolCatalog>,
    ) -> Self {
        Self {
            host,
            selections,
            broker,
            dynamic_tools,
            callback_deadline_ms: DEFAULT_CALLBACK_DEADLINE_MS,
        }
    }

    pub fn with_callback_deadline_ms(mut self, callback_deadline_ms: u64) -> Self {
        self.callback_deadline_ms = callback_deadline_ms;
        self
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
                service_instance_id: selection.service_instance_id,
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
            servers: mcp_servers,
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
            },
        )?);
    }
    requirements.extend(instruction_requirements(context.as_ref())?);
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
        "workspace_module_present" => ToolCluster::WorkspaceModule,
        "companion_request" | "companion_respond" => ToolCluster::Collaboration,
        "complete_lifecycle_node" => ToolCluster::Workflow,
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
            HookAction::Block | HookAction::RequestApproval => AgentHookAction::AllowOrDeny,
            HookAction::RewriteInput => AgentHookAction::RewriteInput,
            HookAction::RewriteResult => AgentHookAction::RewriteResult,
            HookAction::EmitEffect => AgentHookAction::EmitEffect,
            HookAction::ContinueTurn | HookAction::RefreshSurface => return None,
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
        | CompleteAgentHostError::UnknownService { .. } => incompatible(error.to_string()),
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
    use super::*;

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
        let instance = AgentServiceInstanceId::new("codex-fixture").unwrap();
        catalog.register(&profile, instance.clone()).await.unwrap();

        assert_eq!(
            catalog.select(&profile).await.unwrap(),
            VerifiedCompleteAgentSelection {
                service_instance_id: instance,
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
        let previous_instance = AgentServiceInstanceId::new("previous").unwrap();
        let next_instance = AgentServiceInstanceId::new("next").unwrap();
        let other_instance = AgentServiceInstanceId::new("other").unwrap();
        catalog
            .register(&previous_profile, previous_instance.clone())
            .await
            .unwrap();
        catalog
            .register(&conflicting_profile, other_instance)
            .await
            .unwrap();

        assert!(matches!(
            catalog
                .switch_placement(
                    Some((&[previous_profile.clone()], &previous_instance)),
                    &[next_profile.clone(), conflicting_profile],
                    &next_instance,
                )
                .await,
            Err(AgentRunProductRuntimeProvisioningError::Conflict { .. })
        ));
        assert_eq!(
            catalog
                .select(&previous_profile)
                .await
                .unwrap()
                .service_instance_id,
            previous_instance
        );
        assert!(catalog.select(&next_profile).await.is_err());

        catalog
            .switch_placement(
                Some((&[previous_profile.clone()], &previous_instance)),
                &[next_profile.clone()],
                &next_instance,
            )
            .await
            .unwrap();
        assert!(catalog.select(&previous_profile).await.is_err());
        assert_eq!(
            catalog
                .select(&next_profile)
                .await
                .unwrap()
                .service_instance_id,
            next_instance
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
}
