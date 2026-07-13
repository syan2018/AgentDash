use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, DriverBindIntent, DriverBindRequest, DriverCommandEnvelope,
    DriverDispatchReceipt, DriverError, DriverEventEnvelope, DriverEventSink, ProfileDigest,
    ProfileProvenance, RuntimeBindingId, RuntimeDriverGeneration, RuntimeEvent, RuntimeJournalFact,
    RuntimeProfile, RuntimeServiceInstanceId, RuntimeThreadId, intersect_profile_layers,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_integration_api::{
    ActivatedAgentServiceInstance, AgentRuntimeCredentialBroker, AgentRuntimeCredentialRef,
    AgentRuntimeCredentialSlot, AgentServiceOfferId, CredentialLease, CredentialResolveError,
    CredentialSlotDefinition, DriverSurfaceRequest, RuntimeDriverHostPorts,
};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use jsonschema::validator_for;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::{
    ActivateAgentServiceInstance, AgentRuntimeHostRepository, AgentServiceDefinitionRegistry,
    AgentServiceInstance, AppliedSurface, BoundAgentSurfaceReference, DefinitionRegistryError,
    DriverConformanceVerifier, DriverLease, HookApplyStatus, HostStoreError,
    PutAgentServiceInstance, RuntimeBinding, RuntimeBindingState, RuntimeDriverCoordinate,
    RuntimeOffer, RuntimeSourceCoordinate, ServiceInstanceDesiredState,
    ServiceInstanceObservedState,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRuntimeHostError {
    #[error(transparent)]
    Registry(#[from] DefinitionRegistryError),
    #[error(transparent)]
    Store(#[from] HostStoreError),
    #[error("Agent service instance configuration is invalid: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("Agent service credential slot is invalid: {reason}")]
    InvalidCredentialBinding { reason: String },
    #[error("Agent service instance is disabled")]
    Disabled,
    #[error("Agent service instance revision is stale: expected {expected}, actual {actual}")]
    StaleInstanceRevision { expected: u64, actual: u64 },
    #[error("Agent runtime offer is unavailable: {reason}")]
    OfferUnavailable { reason: String },
    #[error("Agent runtime driver failed: {0}")]
    Driver(#[from] DriverError),
    #[error("Agent runtime driver factory failed: {reason}")]
    Factory { reason: String },
    #[error("Agent runtime descriptor is invalid: {reason}")]
    InvalidDescriptor { reason: String },
    #[error("Agent runtime conformance verification failed: {reason}")]
    ConformanceRejected { reason: String },
    #[error("Agent runtime binding is not dispatchable: {reason}")]
    DispatchRejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRuntimeRequest {
    pub binding_id: RuntimeBindingId,
    pub thread_id: RuntimeThreadId,
    pub offer_id: AgentServiceOfferId,
    pub bound_surface: BoundAgentSurfaceReference,
    pub intent: DriverBindIntent,
}

#[derive(Debug, Clone)]
pub struct RouteDriverCommand {
    pub envelope: DriverCommandEnvelope,
    pub lease_owner: String,
    pub lease_token: String,
}

#[derive(Clone)]
struct ActiveDriver {
    driver: Arc<dyn AgentRuntimeDriver>,
}

#[derive(Clone)]
struct ScopedCredentialBroker {
    inner: Arc<dyn AgentRuntimeCredentialBroker>,
    grants: BTreeMap<AgentRuntimeCredentialSlot, (AgentRuntimeCredentialRef, String)>,
}

impl ScopedCredentialBroker {
    fn new(
        inner: Arc<dyn AgentRuntimeCredentialBroker>,
        definitions: &[CredentialSlotDefinition],
        bindings: &BTreeMap<AgentRuntimeCredentialSlot, AgentRuntimeCredentialRef>,
    ) -> Self {
        let grants = definitions
            .iter()
            .filter_map(|definition| {
                bindings.get(&definition.slot).map(|reference| {
                    (
                        definition.slot.clone(),
                        (reference.clone(), definition.purpose.clone()),
                    )
                })
            })
            .collect();
        Self { inner, grants }
    }
}

#[async_trait]
impl AgentRuntimeCredentialBroker for ScopedCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        reference: &AgentRuntimeCredentialRef,
        purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        let Some((allowed_reference, allowed_purpose)) = self.grants.get(slot) else {
            return Err(CredentialResolveError::PurposeDenied { slot: slot.clone() });
        };
        if allowed_reference != reference || allowed_purpose != purpose {
            return Err(CredentialResolveError::PurposeDenied { slot: slot.clone() });
        }
        let lease = self.inner.resolve(slot, reference, purpose).await?;
        if lease.slot != *slot || lease.purpose != purpose || lease.secret.is_empty() {
            return Err(CredentialResolveError::Unavailable {
                slot: slot.clone(),
                reason: "credential broker returned mismatched or empty lease material".to_string(),
            });
        }
        Ok(lease)
    }
}

pub struct IntegrationDriverHost {
    registry: AgentServiceDefinitionRegistry,
    repository: Arc<dyn AgentRuntimeHostRepository>,
    ports: RuntimeDriverHostPorts,
    conformance: Arc<dyn DriverConformanceVerifier>,
    node_id: String,
    lease_duration: Duration,
    active_drivers:
        RwLock<BTreeMap<(RuntimeServiceInstanceId, RuntimeDriverGeneration), ActiveDriver>>,
    driver_activation_lock: Mutex<()>,
}

impl IntegrationDriverHost {
    pub fn new(
        registry: AgentServiceDefinitionRegistry,
        repository: Arc<dyn AgentRuntimeHostRepository>,
        ports: RuntimeDriverHostPorts,
        conformance: Arc<dyn DriverConformanceVerifier>,
        node_id: impl Into<String>,
    ) -> Self {
        Self {
            registry,
            repository,
            ports,
            conformance,
            node_id: node_id.into(),
            lease_duration: Duration::from_secs(30),
            active_drivers: RwLock::new(BTreeMap::new()),
            driver_activation_lock: Mutex::new(()),
        }
    }

    pub fn definitions(&self) -> Vec<agentdash_integration_api::AgentServiceDefinition> {
        self.registry.definitions()
    }

    pub fn definition(
        &self,
        definition_id: &agentdash_integration_api::AgentServiceDefinitionId,
    ) -> Result<agentdash_integration_api::AgentServiceDefinition, AgentRuntimeHostError> {
        self.registry
            .definition(definition_id)
            .map_err(AgentRuntimeHostError::from)
    }

    pub async fn service_instance(
        &self,
        instance_id: &RuntimeServiceInstanceId,
    ) -> Result<Option<AgentServiceInstance>, AgentRuntimeHostError> {
        self.repository
            .load_instance(instance_id)
            .await
            .map_err(AgentRuntimeHostError::from)
    }

    pub async fn put_instance(
        &self,
        change: PutAgentServiceInstance,
    ) -> Result<AgentServiceInstance, AgentRuntimeHostError> {
        let definition = self.registry.definition(&change.definition_id)?;
        validate_config(&definition.config_schema, &change.config)?;
        validate_credentials(&definition.credential_slots, &change.credentials)?;
        if let Some(expected_revision) = change.expected_revision
            && let Some(existing) = self.repository.load_instance(&change.id).await?
        {
            if existing.revision != expected_revision {
                return Err(AgentRuntimeHostError::StaleInstanceRevision {
                    expected: expected_revision,
                    actual: existing.revision,
                });
            }
            if existing.definition_id != change.definition_id {
                return Err(AgentRuntimeHostError::InvalidConfiguration {
                    reason: "service instance definition identity cannot be changed in place"
                        .to_string(),
                });
            }
        }
        let instance = AgentServiceInstance {
            id: change.id,
            definition_id: change.definition_id,
            definition_build_digest: definition.provenance.build_digest.to_string(),
            config: change.config,
            credentials: change.credentials,
            placement: change.placement,
            desired_state: change.desired_state,
            observed_state: ServiceInstanceObservedState::Inactive,
            revision: change.expected_revision.unwrap_or_default(),
        };
        let updated = self
            .repository
            .put_instance(instance, change.expected_revision)
            .await
            .map_err(AgentRuntimeHostError::from)?;
        if change.expected_revision.is_some() {
            self.repository.disable_offers(&updated.id).await?;
        }
        Ok(updated)
    }

    pub async fn activate(
        &self,
        request: ActivateAgentServiceInstance,
    ) -> Result<RuntimeOffer, AgentRuntimeHostError> {
        let mut instance = self
            .repository
            .load_instance(&request.instance_id)
            .await?
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_service_instance",
                id: request.instance_id.to_string(),
            })?;
        if instance.revision != request.expected_revision {
            return Err(AgentRuntimeHostError::StaleInstanceRevision {
                expected: request.expected_revision,
                actual: instance.revision,
            });
        }
        if instance.desired_state != ServiceInstanceDesiredState::Active {
            return Err(AgentRuntimeHostError::Disabled);
        }
        let definition = self.registry.definition(&instance.definition_id)?;
        validate_config(&definition.config_schema, &instance.config)?;
        validate_credentials(&definition.credential_slots, &instance.credentials)?;
        if instance.definition_build_digest != definition.provenance.build_digest.as_str() {
            return Err(AgentRuntimeHostError::InvalidConfiguration {
                reason: "instance is pinned to a different definition build digest".to_string(),
            });
        }
        if profile_digest(&request.transport_profile)? != request.transport_profile_digest {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "transport profile digest does not match its profile".to_string(),
            });
        }
        if profile_digest(&request.host_policy_profile)? != request.host_policy_digest {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "Host policy profile digest does not match its profile".to_string(),
            });
        }
        if request.conformance.suite_revision.trim().is_empty()
            || request.conformance.driver_build_digest.trim().is_empty()
        {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "conformance evidence is missing suite or driver build identity"
                    .to_string(),
            });
        }
        let scoped_credentials: Arc<dyn AgentRuntimeCredentialBroker> =
            Arc::new(ScopedCredentialBroker::new(
                self.ports.credentials.clone(),
                &definition.credential_slots,
                &instance.credentials,
            ));
        for slot_definition in &definition.credential_slots {
            if let Some(reference) = instance.credentials.get(&slot_definition.slot) {
                scoped_credentials
                    .resolve(&slot_definition.slot, reference, &slot_definition.purpose)
                    .await
                    .map_err(|error| AgentRuntimeHostError::InvalidCredentialBinding {
                        reason: error.to_string(),
                    })?;
            }
        }
        let generation = self
            .repository
            .next_generation(&instance.id, instance.revision)
            .await?;
        let factory = self.registry.factory(&instance.definition_id)?;
        let driver = factory
            .create(
                ActivatedAgentServiceInstance {
                    instance_id: instance.id.clone(),
                    instance_revision: instance.revision,
                    generation,
                    definition: definition.clone(),
                    config: instance.config.clone(),
                    credentials: instance.credentials.clone(),
                    placement: instance.placement.clone(),
                },
                RuntimeDriverHostPorts {
                    credentials: scoped_credentials,
                    surfaces: self.ports.surfaces.clone(),
                    context: self.ports.context.clone(),
                    tools: self.ports.tools.clone(),
                    hooks: self.ports.hooks.clone(),
                },
            )
            .await
            .map_err(|error| AgentRuntimeHostError::Factory {
                reason: error.to_string(),
            })?;
        let descriptor = driver
            .describe(agentdash_agent_runtime_contract::DriverDescribeRequest {
                service_instance_id: instance.id.clone(),
            })
            .await?;
        if descriptor.service_instance_id != instance.id {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "descriptor service instance does not match activation".to_string(),
            });
        }
        if !definition
            .supported_protocol_revisions
            .contains(&descriptor.protocol_revision)
        {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: format!(
                    "protocol revision {} is outside the definition contract",
                    descriptor.protocol_revision
                ),
            });
        }
        let declared_digest = profile_digest(&descriptor.profile)?;
        if declared_digest != descriptor.profile_digest {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "descriptor profile digest does not match its profile".to_string(),
            });
        }
        if request.conformance.verified_profile_digest != descriptor.profile_digest {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "conformance evidence does not cover the driver profile".to_string(),
            });
        }
        self.conformance
            .verify(
                driver.as_ref(),
                &definition,
                &instance.id,
                &descriptor,
                &request.conformance,
            )
            .await
            .map_err(|error| AgentRuntimeHostError::ConformanceRejected {
                reason: error.to_string(),
            })?;
        let service_profile = descriptor
            .profile
            .intersect(&definition.service_profile_upper_bound);
        let service_digest = profile_digest(&service_profile)?;
        let effective_profile = intersect_profile_layers(
            &service_profile,
            &request.transport_profile,
            &request.host_policy_profile,
            ProfileProvenance {
                service_digest,
                transport_digest: request.transport_profile_digest,
                host_policy_digest: request.host_policy_digest,
            },
        );
        let effective_digest = profile_digest(&effective_profile.profile)?;
        let offer_id =
            AgentServiceOfferId::new(format!("{}-generation-{}", instance.id, generation.0))
                .map_err(|error| AgentRuntimeHostError::InvalidDescriptor {
                    reason: error.to_string(),
                })?;
        let offer = RuntimeOffer {
            id: offer_id,
            service_instance_id: instance.id.clone(),
            instance_revision: instance.revision,
            generation,
            provenance: definition.provenance,
            placement: instance.placement.clone(),
            protocol_revision: descriptor.protocol_revision,
            effective_profile,
            profile_digest: effective_digest,
            conformance: request.conformance,
            available: true,
        };
        instance.observed_state = ServiceInstanceObservedState::Active;
        self.repository
            .commit_activation(instance.clone(), offer.clone())
            .await?;
        self.active_drivers
            .write()
            .await
            .insert((instance.id, generation), ActiveDriver { driver });
        Ok(offer)
    }

    pub async fn offers(&self) -> Result<Vec<RuntimeOffer>, AgentRuntimeHostError> {
        Ok(self
            .repository
            .list_offers()
            .await?
            .into_iter()
            .filter(|offer| offer.available)
            .collect())
    }

    pub async fn deactivate(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
    ) -> Result<AgentServiceInstance, AgentRuntimeHostError> {
        let mut instance = self
            .repository
            .load_instance(instance_id)
            .await?
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_service_instance",
                id: instance_id.to_string(),
            })?;
        if instance.revision != expected_revision {
            return Err(AgentRuntimeHostError::StaleInstanceRevision {
                expected: expected_revision,
                actual: instance.revision,
            });
        }
        instance.desired_state = ServiceInstanceDesiredState::Inactive;
        instance.observed_state = ServiceInstanceObservedState::Inactive;
        let instance = self
            .repository
            .put_instance(instance, Some(expected_revision))
            .await?;
        self.repository.disable_offers(instance_id).await?;
        Ok(instance)
    }

    pub async fn report_unhealthy(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
        reason: impl Into<String>,
    ) -> Result<(), AgentRuntimeHostError> {
        self.repository
            .set_observed_state(
                instance_id,
                expected_revision,
                ServiceInstanceObservedState::Failed {
                    reason: reason.into(),
                },
            )
            .await?;
        self.repository.disable_offers(instance_id).await?;
        Ok(())
    }

    pub async fn bind(
        &self,
        request: BindRuntimeRequest,
    ) -> Result<RuntimeBinding, AgentRuntimeHostError> {
        let existing = self.repository.load_binding(&request.binding_id).await?;
        if let Some(binding) = &existing {
            let same_intent = binding.thread_id == request.thread_id
                && binding.offer_id == request.offer_id
                && binding.bound_surface == request.bound_surface
                && binding.bind_intent == request.intent;
            if !same_intent {
                return Err(AgentRuntimeHostError::DispatchRejected {
                    reason: "binding id is already used by a different bind intent".to_string(),
                });
            }
            if binding.state == RuntimeBindingState::Active {
                return Ok(binding.clone());
            }
            if binding.state != RuntimeBindingState::Pending {
                return Err(AgentRuntimeHostError::DispatchRejected {
                    reason: format!("binding cannot resume from state {:?}", binding.state),
                });
            }
        }
        let offer = self
            .repository
            .load_offer(&request.offer_id)
            .await?
            .ok_or_else(|| AgentRuntimeHostError::OfferUnavailable {
                reason: format!("offer {} does not exist", request.offer_id),
            })?;
        if let Some(binding) = &existing {
            if binding.service_instance_id != offer.service_instance_id
                || binding.instance_revision != offer.instance_revision
                || binding.driver_generation != offer.generation
                || binding.profile_digest != offer.profile_digest
            {
                return Err(AgentRuntimeHostError::DispatchRejected {
                    reason: "pending binding no longer matches its immutable offer".to_string(),
                });
            }
        } else {
            if !offer.available {
                return Err(AgentRuntimeHostError::OfferUnavailable {
                    reason: "offer is disabled".to_string(),
                });
            }
            let current_instance = self
                .repository
                .load_instance(&offer.service_instance_id)
                .await?
                .ok_or_else(|| AgentRuntimeHostError::OfferUnavailable {
                    reason: "offer service instance is missing".to_string(),
                })?;
            if current_instance.revision != offer.instance_revision
                || current_instance.desired_state != ServiceInstanceDesiredState::Active
                || current_instance.observed_state != ServiceInstanceObservedState::Active
                || current_instance.definition_id != offer.provenance.definition_id
                || current_instance.definition_build_digest
                    != offer.provenance.build_digest.as_str()
            {
                return Err(AgentRuntimeHostError::OfferUnavailable {
                    reason: "offer is stale for the current service instance state".to_string(),
                });
            }
        }
        if request.bound_surface.hook_configuration_boundary
            > offer.effective_profile.profile.hooks.configuration_boundary
        {
            return Err(AgentRuntimeHostError::OfferUnavailable {
                reason:
                    "bound HookPlan requires a more dynamic configuration boundary than the offer"
                        .to_string(),
            });
        }
        for requirement in &request.bound_surface.required_hooks {
            if requirement.required && !offer.effective_profile.profile.hooks.satisfies(requirement)
            {
                return Err(AgentRuntimeHostError::OfferUnavailable {
                    reason: format!(
                        "required hook {:?} is not guaranteed by the selected offer",
                        requirement.point
                    ),
                });
            }
        }
        let pending = RuntimeBinding {
            id: request.binding_id.clone(),
            thread_id: request.thread_id.clone(),
            offer_id: offer.id.clone(),
            service_instance_id: offer.service_instance_id.clone(),
            instance_revision: offer.instance_revision,
            driver_generation: offer.generation,
            profile_digest: offer.profile_digest.clone(),
            bound_surface: request.bound_surface.clone(),
            bind_intent: request.intent.clone(),
            applied_surface: None,
            driver_binding_id: None,
            source_thread_id: None,
            state: RuntimeBindingState::Pending,
            lease_epoch: 0,
        };
        if existing.is_none() {
            self.repository.reserve_binding(pending).await?;
        }
        let driver = self.driver_for_offer(&offer).await?;
        let resume_source_thread_id = match &request.intent {
            DriverBindIntent::Resume { source_thread_id } => Some(source_thread_id.clone()),
            _ => None,
        };
        let driver_binding = match driver
            .bind(DriverBindRequest {
                binding_id: request.binding_id.clone(),
                service_instance_id: offer.service_instance_id,
                surface_revision: request.bound_surface.revision,
                surface_digest: request.bound_surface.digest.clone(),
                intent: request.intent,
            })
            .await
        {
            Ok(binding) => binding,
            Err(error) => {
                self.repository
                    .fail_binding(&request.binding_id, offer.generation)
                    .await?;
                return Err(error.into());
            }
        };
        if resume_source_thread_id
            .as_ref()
            .is_some_and(|expected| expected != &driver_binding.source_thread_id)
        {
            self.repository
                .fail_binding(&request.binding_id, offer.generation)
                .await?;
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "driver Resume returned a different source thread identity".to_string(),
            });
        }
        if driver_binding.applied_surface_revision != request.bound_surface.revision
            || driver_binding.applied_surface_digest != request.bound_surface.digest
            || driver_binding.applied_tool_set_revision != request.bound_surface.tool_set_revision
            || driver_binding.applied_tool_set_digest != request.bound_surface.tool_set_digest
            || driver_binding.applied_hook_plan_revision != request.bound_surface.hook_plan_revision
            || driver_binding.applied_hook_plan_digest != request.bound_surface.hook_plan_digest
        {
            self.repository
                .fail_binding(&request.binding_id, offer.generation)
                .await?;
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "driver applied a different surface revision/digest during bind"
                    .to_string(),
            });
        }
        let applied = AppliedSurface {
            revision: driver_binding.applied_surface_revision,
            digest: driver_binding.applied_surface_digest,
            tool_set_revision: driver_binding.applied_tool_set_revision,
            tool_set_digest: driver_binding.applied_tool_set_digest,
            hook_plan_revision: driver_binding.applied_hook_plan_revision,
            hook_plan_digest: driver_binding.applied_hook_plan_digest,
            hooks: driver_binding
                .applied_hooks
                .into_iter()
                .map(|status| HookApplyStatus {
                    point: status.point,
                    acknowledged: status.acknowledged,
                    artifact_digest: status.artifact_digest,
                })
                .collect(),
        };
        let source = RuntimeSourceCoordinate {
            binding_id: request.binding_id.clone(),
            generation: offer.generation,
            thread_id: request.thread_id,
            source_thread_id: driver_binding.source_thread_id,
        };
        self.repository
            .activate_binding(
                &request.binding_id,
                offer.generation,
                applied,
                driver_binding.driver_binding_id,
                source,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn record_apply_receipt(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
    ) -> Result<RuntimeBinding, AgentRuntimeHostError> {
        let binding = self
            .repository
            .load_binding(binding_id)
            .await?
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_binding",
                id: binding_id.to_string(),
            })?;
        if binding.driver_binding_id.is_none() || binding.source_thread_id.is_none() {
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "driver binding/source coordinate has not been established".to_string(),
            });
        }
        if applied.revision != binding.bound_surface.revision
            || applied.digest != binding.bound_surface.digest
            || applied.hook_plan_revision != binding.bound_surface.hook_plan_revision
            || applied.hook_plan_digest != binding.bound_surface.hook_plan_digest
        {
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "surface apply receipt does not match the bound revision/digest"
                    .to_string(),
            });
        }
        self.repository
            .record_apply(binding_id, generation, applied)
            .await
            .map_err(Into::into)
    }

    pub async fn acquire_driver_lease(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<DriverLease, AgentRuntimeHostError> {
        let binding = self.binding(binding_id).await?;
        let now = Utc::now();
        let expires_at = now
            + ChronoDuration::from_std(self.lease_duration).map_err(|error| {
                AgentRuntimeHostError::DispatchRejected {
                    reason: error.to_string(),
                }
            })?;
        self.repository
            .acquire_lease(
                binding_id,
                binding.driver_generation,
                &self.node_id,
                now,
                expires_at,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn renew_driver_lease(
        &self,
        lease: &DriverLease,
    ) -> Result<DriverLease, AgentRuntimeHostError> {
        let now = Utc::now();
        let expires_at = now
            + ChronoDuration::from_std(self.lease_duration).map_err(|error| {
                AgentRuntimeHostError::DispatchRejected {
                    reason: error.to_string(),
                }
            })?;
        self.repository
            .renew_lease(
                &lease.binding_id,
                lease.generation,
                &lease.owner,
                &lease.token,
                now,
                expires_at,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn release_driver_lease(
        &self,
        lease: &DriverLease,
    ) -> Result<(), AgentRuntimeHostError> {
        self.repository
            .release_lease(
                &lease.binding_id,
                lease.generation,
                &lease.owner,
                &lease.token,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn dispatch(
        &self,
        command: RouteDriverCommand,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, AgentRuntimeHostError> {
        let binding = self.binding(&command.envelope.binding_id).await?;
        if binding.driver_generation != command.envelope.generation {
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "command generation is stale".to_string(),
            });
        }
        let source = self
            .repository
            .find_source(&binding.id, binding.driver_generation)
            .await?
            .ok_or_else(|| AgentRuntimeHostError::DispatchRejected {
                reason: "binding source coordinate is missing".to_string(),
            })?;
        if source.source_thread_id != command.envelope.source_thread_id {
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "command source thread does not match binding".to_string(),
            });
        }
        let offer = self
            .repository
            .load_offer(&binding.offer_id)
            .await?
            .ok_or_else(|| AgentRuntimeHostError::OfferUnavailable {
                reason: "bound offer is missing".to_string(),
            })?;
        if !binding.dispatch_admitted(&offer.effective_profile.profile) {
            return Err(AgentRuntimeHostError::DispatchRejected {
                reason: "surface or required hook application is not acknowledged".to_string(),
            });
        }
        let _lease = self
            .repository
            .validate_lease(
                &binding.id,
                binding.driver_generation,
                &command.lease_owner,
                &command.lease_token,
                Utc::now(),
            )
            .await?;
        let adopted_surface = match &command.envelope.command {
            agentdash_agent_runtime_contract::RuntimeCommand::SurfaceAdopt { target, .. } => Some(
                self.ports
                    .surfaces
                    .materialize(DriverSurfaceRequest {
                        binding_id: binding.id.clone(),
                        surface_revision: target.surface_revision,
                        surface_digest: target.surface_digest.clone(),
                    })
                    .await
                    .map_err(|error| AgentRuntimeHostError::DispatchRejected {
                        reason: error.to_string(),
                    })?,
            ),
            _ => None,
        };
        let driver = self.driver_for_offer(&offer).await?;
        let fenced_sink = Arc::new(GenerationFencedEventSink {
            binding_id: binding.id.clone(),
            generation: binding.driver_generation,
            source_thread_id: source.source_thread_id,
            service_instance_id: binding.service_instance_id.clone(),
            instance_revision: binding.instance_revision,
            offer_id: binding.offer_id.clone(),
            repository: self.repository.clone(),
            inner: sink,
        });
        let receipt = driver
            .dispatch(command.envelope, fenced_sink)
            .await
            .map_err(AgentRuntimeHostError::from)?;
        match adopted_surface {
            Some(surface) => {
                let applied = receipt.applied_surface.as_ref().ok_or_else(|| {
                    AgentRuntimeHostError::DispatchRejected {
                        reason: "surface adoption driver receipt is missing applied_surface"
                            .to_string(),
                    }
                })?;
                if applied.descriptor.surface_revision != surface.revision
                    || applied.descriptor.surface_digest != surface.digest
                    || applied.descriptor.tool_set_revision != surface.tools.revision
                    || applied.descriptor.tool_set_digest != surface.tools.digest
                    || applied.descriptor.hook_plan.revision != surface.hooks.revision
                    || applied.descriptor.hook_plan.digest != surface.hooks.digest
                {
                    return Err(AgentRuntimeHostError::DispatchRejected {
                        reason:
                            "surface adoption driver receipt does not match broker materialization"
                                .to_string(),
                    });
                }
                let target_bound = BoundAgentSurfaceReference {
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
                        .map(|hook| agentdash_agent_runtime_contract::HookRequirement {
                            point: hook.point,
                            actions: hook.actions.iter().copied().collect::<BTreeSet<_>>(),
                            minimum_strength: hook.strength,
                            failure_policy: hook.failure_policy,
                            required: hook.required,
                        })
                        .collect(),
                };
                let applied_surface = AppliedSurface {
                    revision: surface.revision,
                    digest: surface.digest,
                    tool_set_revision: surface.tools.revision,
                    tool_set_digest: surface.tools.digest,
                    hook_plan_revision: Some(surface.hooks.revision),
                    hook_plan_digest: Some(surface.hooks.digest),
                    hooks: applied
                        .applied_hooks
                        .iter()
                        .map(|hook| HookApplyStatus {
                            point: hook.point,
                            acknowledged: hook.acknowledged,
                            artifact_digest: hook.artifact_digest.clone(),
                        })
                        .collect(),
                };
                let mut candidate = binding.clone();
                candidate.bound_surface = target_bound.clone();
                candidate.applied_surface = Some(applied_surface.clone());
                if !candidate.dispatch_admitted(&offer.effective_profile.profile) {
                    return Err(AgentRuntimeHostError::DispatchRejected {
                        reason: "adopted surface or required hook application is not acknowledged"
                            .to_string(),
                    });
                }
                self.repository
                    .adopt_surface(
                        &binding.id,
                        binding.driver_generation,
                        &binding.bound_surface,
                        target_bound,
                        applied_surface,
                    )
                    .await?;
            }
            None if receipt.applied_surface.is_some() => {
                return Err(AgentRuntimeHostError::DispatchRejected {
                    reason: "driver returned an unexpected applied_surface receipt".to_string(),
                });
            }
            None => {}
        }
        Ok(receipt)
    }

    pub async fn binding(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<RuntimeBinding, AgentRuntimeHostError> {
        self.repository
            .load_binding(binding_id)
            .await?
            .ok_or_else(|| {
                HostStoreError::NotFound {
                    entity: "agent_runtime_binding",
                    id: binding_id.to_string(),
                }
                .into()
            })
    }

    /// Inspects the driver currently owning a durable binding coordinate.
    ///
    /// Recovery workers use this read path after a crash to reconcile side effects whose dispatch
    /// receipt may have been lost. The durable binding/offer lookup keeps inspection generation-
    /// fenced without inventing a second driver registry.
    pub async fn inspect_binding_driver(
        &self,
        binding_id: &RuntimeBindingId,
        query: agentdash_agent_runtime_contract::DriverInspectionQuery,
    ) -> Result<agentdash_agent_runtime_contract::DriverInspection, AgentRuntimeHostError> {
        let binding = self.binding(binding_id).await?;
        let offer = self
            .repository
            .load_offer(&binding.offer_id)
            .await?
            .ok_or_else(|| AgentRuntimeHostError::OfferUnavailable {
                reason: "bound offer is missing".to_string(),
            })?;
        if offer.generation != binding.driver_generation || !offer.available {
            return Err(AgentRuntimeHostError::OfferUnavailable {
                reason: "bound driver generation is unavailable".to_string(),
            });
        }
        self.driver_for_offer(&offer)
            .await?
            .inspect(query)
            .await
            .map_err(Into::into)
    }

    /// Resolves the single activated driver endpoint owned by a durable service offer.
    ///
    /// Transport adapters use this method to terminate Runtime Wire at the Integration Host;
    /// they must not construct an independent driver or bypass generation fencing.
    pub async fn driver_endpoint(
        &self,
        service_instance_id: &RuntimeServiceInstanceId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, AgentRuntimeHostError> {
        let offer = self
            .repository
            .list_offers()
            .await?
            .into_iter()
            .find(|offer| {
                &offer.service_instance_id == service_instance_id && offer.generation == generation
            })
            .ok_or_else(|| AgentRuntimeHostError::OfferUnavailable {
                reason: format!(
                    "service instance {service_instance_id} has no durable offer for generation {}",
                    generation.0
                ),
            })?;
        self.driver_for_offer(&offer).await
    }

    pub async fn recover_available_drivers(&self) -> Result<usize, AgentRuntimeHostError> {
        let offers = self.offers().await?;
        for offer in &offers {
            self.driver_for_offer(offer).await?;
        }
        Ok(offers.len())
    }

    pub async fn recover_pending_bindings(&self) -> Result<usize, AgentRuntimeHostError> {
        let bindings = self.repository.pending_bindings().await?;
        let mut recovered = 0;
        for binding in bindings {
            self.bind(BindRuntimeRequest {
                binding_id: binding.id,
                thread_id: binding.thread_id,
                offer_id: binding.offer_id,
                bound_surface: binding.bound_surface,
                intent: binding.bind_intent,
            })
            .await?;
            recovered += 1;
        }
        Ok(recovered)
    }

    async fn driver_for_offer(
        &self,
        offer: &RuntimeOffer,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, AgentRuntimeHostError> {
        let key = (offer.service_instance_id.clone(), offer.generation);
        if let Some(active) = self.active_drivers.read().await.get(&key) {
            return Ok(active.driver.clone());
        }
        let _activation_guard = self.driver_activation_lock.lock().await;
        if let Some(active) = self.active_drivers.read().await.get(&key) {
            return Ok(active.driver.clone());
        }
        let instance = self
            .repository
            .load_activation_instance(&offer.service_instance_id, offer.generation)
            .await?
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_service_activation.instance_snapshot",
                id: format!("{}:{}", offer.service_instance_id, offer.generation.0),
            })?;
        if instance.revision != offer.instance_revision {
            return Err(AgentRuntimeHostError::OfferUnavailable {
                reason: "activation snapshot revision does not match the durable offer".to_string(),
            });
        }
        let definition = self.registry.definition(&instance.definition_id)?;
        if definition.provenance != offer.provenance
            || instance.definition_build_digest != definition.provenance.build_digest.as_str()
        {
            return Err(AgentRuntimeHostError::OfferUnavailable {
                reason: "persisted offer provenance does not match the compiled definition"
                    .to_string(),
            });
        }
        validate_config(&definition.config_schema, &instance.config)?;
        validate_credentials(&definition.credential_slots, &instance.credentials)?;
        let scoped_credentials: Arc<dyn AgentRuntimeCredentialBroker> =
            Arc::new(ScopedCredentialBroker::new(
                self.ports.credentials.clone(),
                &definition.credential_slots,
                &instance.credentials,
            ));
        for slot_definition in &definition.credential_slots {
            if let Some(reference) = instance.credentials.get(&slot_definition.slot) {
                scoped_credentials
                    .resolve(&slot_definition.slot, reference, &slot_definition.purpose)
                    .await
                    .map_err(|error| AgentRuntimeHostError::InvalidCredentialBinding {
                        reason: error.to_string(),
                    })?;
            }
        }
        let factory = self.registry.factory(&instance.definition_id)?;
        let driver = factory
            .create(
                ActivatedAgentServiceInstance {
                    instance_id: instance.id.clone(),
                    instance_revision: instance.revision,
                    generation: offer.generation,
                    definition,
                    config: instance.config,
                    credentials: instance.credentials,
                    placement: instance.placement,
                },
                RuntimeDriverHostPorts {
                    credentials: scoped_credentials,
                    surfaces: self.ports.surfaces.clone(),
                    context: self.ports.context.clone(),
                    tools: self.ports.tools.clone(),
                    hooks: self.ports.hooks.clone(),
                },
            )
            .await
            .map_err(|error| AgentRuntimeHostError::Factory {
                reason: error.to_string(),
            })?;
        let descriptor = driver
            .describe(agentdash_agent_runtime_contract::DriverDescribeRequest {
                service_instance_id: offer.service_instance_id.clone(),
            })
            .await?;
        if descriptor.service_instance_id != offer.service_instance_id
            || descriptor.protocol_revision != offer.protocol_revision
            || profile_digest(&descriptor.profile)? != descriptor.profile_digest
            || descriptor.profile_digest != offer.conformance.verified_profile_digest
        {
            return Err(AgentRuntimeHostError::InvalidDescriptor {
                reason: "recovered driver descriptor does not match the durable offer evidence"
                    .to_string(),
            });
        }
        self.active_drivers.write().await.insert(
            key,
            ActiveDriver {
                driver: driver.clone(),
            },
        );
        Ok(driver)
    }
}

struct GenerationFencedEventSink {
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: agentdash_agent_runtime_contract::DriverThreadId,
    service_instance_id: RuntimeServiceInstanceId,
    instance_revision: u64,
    offer_id: AgentServiceOfferId,
    repository: Arc<dyn AgentRuntimeHostRepository>,
    inner: Arc<dyn DriverEventSink>,
}

#[async_trait]
impl DriverEventSink for GenerationFencedEventSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        if event.binding_id != self.binding_id
            || event.generation != self.generation
            || event.source_thread_id != self.source_thread_id
        {
            return Err(DriverError::StaleGeneration);
        }
        let binding_lost = event.facts.iter().any(|fact| {
            matches!(
                fact,
                RuntimeJournalFact::Internal(RuntimeEvent::BindingLost { .. })
            )
        });
        let binding = self
            .repository
            .load_binding(&self.binding_id)
            .await
            .map_err(|error| DriverError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or(DriverError::StaleGeneration)?;
        if binding.driver_generation != self.generation
            || binding.state != crate::RuntimeBindingState::Active
            || binding.source_thread_id.as_ref() != Some(&self.source_thread_id)
            || binding.service_instance_id != self.service_instance_id
            || binding.instance_revision != self.instance_revision
            || binding.offer_id != self.offer_id
        {
            return Err(DriverError::StaleGeneration);
        }
        let mut coordinates = Vec::new();
        for fact in &event.facts {
            let RuntimeJournalFact::Internal(runtime_event) = fact else {
                continue;
            };
            match runtime_event {
                RuntimeEvent::TurnStarted { turn_id, .. }
                | RuntimeEvent::TurnTerminal { turn_id, .. } => {
                    let source_turn_id = event.source_turn_id.clone().ok_or_else(|| {
                        DriverError::ProtocolViolation {
                            reason: "turn lifecycle event is missing source turn coordinate"
                                .to_string(),
                            critical: true,
                        }
                    })?;
                    coordinates.push(RuntimeDriverCoordinate::Turn {
                        runtime_turn_id: turn_id.clone(),
                        source_turn_id,
                    });
                }
                RuntimeEvent::ItemStarted {
                    turn_id, item_id, ..
                }
                | RuntimeEvent::ConversationDelta {
                    turn_id, item_id, ..
                }
                | RuntimeEvent::ItemTerminal {
                    turn_id, item_id, ..
                } => {
                    let source_turn_id = event.source_turn_id.clone().ok_or_else(|| {
                        DriverError::ProtocolViolation {
                            reason: "item lifecycle event is missing source turn coordinate"
                                .to_string(),
                            critical: true,
                        }
                    })?;
                    let source_item_id = event.source_item_id.clone().ok_or_else(|| {
                        DriverError::ProtocolViolation {
                            reason: "item lifecycle event is missing source item coordinate"
                                .to_string(),
                            critical: true,
                        }
                    })?;
                    coordinates.push(RuntimeDriverCoordinate::Turn {
                        runtime_turn_id: turn_id.clone(),
                        source_turn_id,
                    });
                    coordinates.push(RuntimeDriverCoordinate::Item {
                        runtime_item_id: item_id.clone(),
                        source_item_id,
                    });
                }
                _ => {}
            }
        }
        self.inner.emit(event).await?;
        if binding_lost {
            self.repository
                .mark_binding_lost(&self.binding_id, self.generation)
                .await
                .map_err(|error| DriverError::Unavailable {
                    reason: error.to_string(),
                    retryable: true,
                })?;
        }
        for coordinate in coordinates {
            if let Err(error) = self
                .repository
                .record_driver_coordinate(&self.binding_id, self.generation, coordinate)
                .await
            {
                // The authoritative Runtime event has already committed. Asking the Driver to
                // replay it would risk a second lifecycle fact, so coordinate persistence failure
                // is isolated as Host health degradation. Canonical callback requests carry
                // Runtime ids directly and do not depend on this secondary lookup index.
                let health_error = self
                    .repository
                    .fail_binding(&self.binding_id, self.generation)
                    .await
                    .err();
                diag!(
                    Error,
                    Subsystem::AgentRun,
                    binding_id = self.binding_id.to_string(),
                    generation = self.generation.0,
                    error = error.to_string(),
                    health_error = health_error.as_ref().map(ToString::to_string),
                    "Runtime event committed but Host driver-coordinate indexing failed"
                );
                self.inner
                    .emit(DriverEventEnvelope {
                        binding_id: self.binding_id.clone(),
                        generation: self.generation,
                        source_thread_id: self.source_thread_id.clone(),
                        source_turn_id: None,
                        source_item_id: None,
                        source_request_id: None,
                        source_entry_index: None,
                        facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                            binding_id: self.binding_id.clone(),
                            reason: format!(
                                "Host driver-coordinate indexing failed after Runtime acceptance: {error}"
                            ),
                        })],
                    })
                    .await?;
                break;
            }
        }
        Ok(())
    }
}

fn validate_config(
    schema: &serde_json::Value,
    config: &serde_json::Value,
) -> Result<(), AgentRuntimeHostError> {
    let validator =
        validator_for(schema).map_err(|error| AgentRuntimeHostError::InvalidConfiguration {
            reason: format!("definition schema is invalid: {error}"),
        })?;
    let errors = validator
        .iter_errors(config)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AgentRuntimeHostError::InvalidConfiguration {
            reason: errors.join("; "),
        })
    }
}

fn validate_credentials(
    definitions: &[agentdash_integration_api::CredentialSlotDefinition],
    bindings: &BTreeMap<
        agentdash_integration_api::AgentRuntimeCredentialSlot,
        agentdash_integration_api::AgentRuntimeCredentialRef,
    >,
) -> Result<(), AgentRuntimeHostError> {
    for definition in definitions {
        if definition.required && !bindings.contains_key(&definition.slot) {
            return Err(AgentRuntimeHostError::InvalidCredentialBinding {
                reason: format!("required credential slot {} is missing", definition.slot),
            });
        }
    }
    for slot in bindings.keys() {
        if !definitions
            .iter()
            .any(|definition| &definition.slot == slot)
        {
            return Err(AgentRuntimeHostError::InvalidCredentialBinding {
                reason: format!("credential slot {slot} is not declared by the definition"),
            });
        }
    }
    Ok(())
}

pub fn profile_digest(profile: &RuntimeProfile) -> Result<ProfileDigest, AgentRuntimeHostError> {
    Ok(agentdash_agent_runtime_contract::runtime_profile_digest(
        profile,
    ))
}

pub fn empty_hook_apply() -> Vec<HookApplyStatus> {
    Vec::new()
}
