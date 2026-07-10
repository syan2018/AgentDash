use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration, RuntimeServiceInstanceId,
    RuntimeThreadId,
};
use agentdash_integration_api::AgentServiceOfferId;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::{
    AgentRuntimeHostRepository, AgentServiceInstance, AppliedSurface, DriverLease, HostStoreError,
    RuntimeBinding, RuntimeBindingState, RuntimeDriverCoordinate, RuntimeOffer,
    RuntimeSourceCoordinate,
};

#[derive(Default)]
struct MemoryHostState {
    instances: BTreeMap<RuntimeServiceInstanceId, AgentServiceInstance>,
    generations: BTreeMap<RuntimeServiceInstanceId, RuntimeDriverGeneration>,
    activations:
        BTreeMap<(RuntimeServiceInstanceId, RuntimeDriverGeneration), AgentServiceInstance>,
    offers: BTreeMap<AgentServiceOfferId, RuntimeOffer>,
    bindings: BTreeMap<RuntimeBindingId, RuntimeBinding>,
    thread_bindings: BTreeMap<RuntimeThreadId, RuntimeBindingId>,
    sources: BTreeMap<(RuntimeBindingId, RuntimeDriverGeneration), RuntimeSourceCoordinate>,
    driver_coordinates: BTreeMap<
        (RuntimeBindingId, RuntimeDriverGeneration, String, String),
        RuntimeDriverCoordinate,
    >,
    leases: BTreeMap<RuntimeBindingId, DriverLease>,
}

#[derive(Default)]
pub struct MemoryAgentRuntimeHostRepository {
    state: RwLock<MemoryHostState>,
}

impl MemoryAgentRuntimeHostRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AgentRuntimeHostRepository for MemoryAgentRuntimeHostRepository {
    async fn load_instance(
        &self,
        id: &RuntimeServiceInstanceId,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError> {
        Ok(self.state.read().await.instances.get(id).cloned())
    }

    async fn put_instance(
        &self,
        mut instance: AgentServiceInstance,
        expected_revision: Option<u64>,
    ) -> Result<AgentServiceInstance, HostStoreError> {
        let mut state = self.state.write().await;
        let actual = state.instances.get(&instance.id).map(|item| item.revision);
        if actual != expected_revision {
            return Err(HostStoreError::Conflict {
                entity: "agent_service_instance",
                id: instance.id.to_string(),
                expected: expected_revision,
                actual,
            });
        }
        instance.revision = actual
            .map(|revision| {
                revision
                    .checked_add(1)
                    .ok_or_else(|| HostStoreError::Invariant {
                        reason: "service instance revision is exhausted".to_string(),
                    })
            })
            .transpose()?
            .unwrap_or(1);
        state
            .instances
            .insert(instance.id.clone(), instance.clone());
        Ok(instance)
    }

    async fn next_generation(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
    ) -> Result<RuntimeDriverGeneration, HostStoreError> {
        let mut state = self.state.write().await;
        let instance =
            state
                .instances
                .get(instance_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_service_instance",
                    id: instance_id.to_string(),
                })?;
        if instance.revision != expected_revision {
            return Err(HostStoreError::Conflict {
                entity: "agent_service_instance",
                id: instance_id.to_string(),
                expected: Some(expected_revision),
                actual: Some(instance.revision),
            });
        }
        let generation = state
            .generations
            .get(instance_id)
            .map(|generation| {
                generation
                    .0
                    .checked_add(1)
                    .map(RuntimeDriverGeneration)
                    .ok_or_else(|| HostStoreError::Invariant {
                        reason: "driver generation is exhausted".to_string(),
                    })
            })
            .transpose()?
            .unwrap_or(RuntimeDriverGeneration(1));
        state.generations.insert(instance_id.clone(), generation);
        Ok(generation)
    }

    async fn commit_activation(
        &self,
        instance: AgentServiceInstance,
        offer: RuntimeOffer,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        let actual = state.instances.get(&instance.id).map(|item| item.revision);
        if actual != Some(instance.revision) {
            return Err(HostStoreError::Conflict {
                entity: "agent_service_instance",
                id: instance.id.to_string(),
                expected: Some(instance.revision),
                actual,
            });
        }
        if offer.service_instance_id != instance.id
            || offer.instance_revision != instance.revision
            || offer.generation
                != *state.generations.get(&instance.id).ok_or_else(|| {
                    HostStoreError::Invariant {
                        reason: "activation generation was not reserved".to_string(),
                    }
                })?
        {
            return Err(HostStoreError::Invariant {
                reason: "activation offer does not match instance revision/generation".to_string(),
            });
        }
        for existing in state.offers.values_mut() {
            if existing.service_instance_id == instance.id {
                existing.available = false;
            }
        }
        state
            .activations
            .insert((instance.id.clone(), offer.generation), instance.clone());
        state.instances.insert(instance.id.clone(), instance);
        state.offers.insert(offer.id.clone(), offer);
        Ok(())
    }

    async fn load_activation_instance(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError> {
        Ok(self
            .state
            .read()
            .await
            .activations
            .get(&(instance_id.clone(), generation))
            .cloned())
    }

    async fn load_offer(
        &self,
        id: &AgentServiceOfferId,
    ) -> Result<Option<RuntimeOffer>, HostStoreError> {
        Ok(self.state.read().await.offers.get(id).cloned())
    }

    async fn list_offers(&self) -> Result<Vec<RuntimeOffer>, HostStoreError> {
        Ok(self.state.read().await.offers.values().cloned().collect())
    }

    async fn disable_offers(
        &self,
        instance_id: &RuntimeServiceInstanceId,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        for offer in state.offers.values_mut() {
            if &offer.service_instance_id == instance_id {
                offer.available = false;
            }
        }
        Ok(())
    }

    async fn set_observed_state(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
        observed: crate::ServiceInstanceObservedState,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        let instance =
            state
                .instances
                .get_mut(instance_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_service_instance",
                    id: instance_id.to_string(),
                })?;
        if instance.revision != expected_revision {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_service_instance",
                id: instance_id.to_string(),
                expected: Some(expected_revision),
                actual: Some(instance.revision),
            });
        }
        instance.observed_state = observed;
        Ok(())
    }

    async fn reserve_binding(&self, binding: RuntimeBinding) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        if binding.state != RuntimeBindingState::Pending
            || binding.applied_surface.is_some()
            || binding.driver_binding_id.is_some()
            || binding.source_thread_id.is_some()
            || binding.lease_epoch != 0
        {
            return Err(HostStoreError::Invariant {
                reason: "new Host binding must be an unapplied pending reservation".to_string(),
            });
        }
        if state.bindings.contains_key(&binding.id) {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_binding",
                id: binding.id.to_string(),
                expected: None,
                actual: Some(1),
            });
        }
        if state.thread_bindings.contains_key(&binding.thread_id) {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_binding.thread_id",
                id: binding.thread_id.to_string(),
                expected: None,
                actual: Some(1),
            });
        }
        let offer =
            state
                .offers
                .get(&binding.offer_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_offer",
                    id: binding.offer_id.to_string(),
                })?;
        if !offer.available
            || offer.service_instance_id != binding.service_instance_id
            || offer.instance_revision != binding.instance_revision
            || offer.generation != binding.driver_generation
            || offer.profile_digest != binding.profile_digest
        {
            return Err(HostStoreError::Invariant {
                reason: "binding does not match an available offer generation/profile".to_string(),
            });
        }
        let instance = state
            .instances
            .get(&binding.service_instance_id)
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_service_instance",
                id: binding.service_instance_id.to_string(),
            })?;
        if instance.revision != binding.instance_revision
            || instance.desired_state != crate::ServiceInstanceDesiredState::Active
            || instance.observed_state != crate::ServiceInstanceObservedState::Active
        {
            return Err(HostStoreError::Invariant {
                reason: "binding offer is stale for the current service instance".to_string(),
            });
        }
        state
            .thread_bindings
            .insert(binding.thread_id.clone(), binding.id.clone());
        state.bindings.insert(binding.id.clone(), binding);
        Ok(())
    }

    async fn activate_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
        driver_binding_id: agentdash_agent_runtime_contract::DriverBindingId,
        source: RuntimeSourceCoordinate,
    ) -> Result<RuntimeBinding, HostStoreError> {
        let mut state = self.state.write().await;
        let binding =
            state
                .bindings
                .get_mut(binding_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_binding",
                    id: binding_id.to_string(),
                })?;
        if binding.state != RuntimeBindingState::Pending
            || binding.driver_generation != expected_generation
            || source.binding_id != *binding_id
            || source.generation != expected_generation
            || source.thread_id != binding.thread_id
        {
            return Err(HostStoreError::Invariant {
                reason: "binding activation coordinates or generation are stale".to_string(),
            });
        }
        binding.applied_surface = Some(applied);
        binding.driver_binding_id = Some(driver_binding_id);
        binding.source_thread_id = Some(source.source_thread_id.clone());
        binding.state = RuntimeBindingState::Active;
        let activated = binding.clone();
        state
            .sources
            .insert((binding_id.clone(), expected_generation), source);
        Ok(activated)
    }

    async fn load_binding(
        &self,
        id: &RuntimeBindingId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError> {
        Ok(self.state.read().await.bindings.get(id).cloned())
    }

    async fn pending_bindings(&self) -> Result<Vec<RuntimeBinding>, HostStoreError> {
        Ok(self
            .state
            .read()
            .await
            .bindings
            .values()
            .filter(|binding| binding.state == RuntimeBindingState::Pending)
            .cloned()
            .collect())
    }

    async fn record_apply(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
    ) -> Result<RuntimeBinding, HostStoreError> {
        let mut state = self.state.write().await;
        let binding =
            state
                .bindings
                .get_mut(binding_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_binding",
                    id: binding_id.to_string(),
                })?;
        if binding.driver_generation != expected_generation
            || binding.state != RuntimeBindingState::Active
        {
            return Err(HostStoreError::Invariant {
                reason: "surface apply receipt targets a stale or inactive binding".to_string(),
            });
        }
        binding.applied_surface = Some(applied);
        Ok(binding.clone())
    }

    async fn fail_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        let binding =
            state
                .bindings
                .get_mut(binding_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_binding",
                    id: binding_id.to_string(),
                })?;
        if binding.driver_generation != expected_generation {
            return Err(HostStoreError::Invariant {
                reason: "cannot fail a different binding generation".to_string(),
            });
        }
        binding.state = RuntimeBindingState::Failed;
        Ok(())
    }

    async fn find_binding_by_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError> {
        let state = self.state.read().await;
        Ok(state
            .thread_bindings
            .get(thread_id)
            .and_then(|id| state.bindings.get(id))
            .cloned())
    }

    async fn find_source(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<RuntimeSourceCoordinate>, HostStoreError> {
        Ok(self
            .state
            .read()
            .await
            .sources
            .get(&(binding_id.clone(), generation))
            .cloned())
    }

    async fn acquire_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError> {
        let mut state = self.state.write().await;
        let binding = state
            .bindings
            .get(binding_id)
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_binding",
                id: binding_id.to_string(),
            })?;
        if binding.driver_generation != generation || binding.state != RuntimeBindingState::Active {
            return Err(HostStoreError::Invariant {
                reason: "lease binding generation is not active".to_string(),
            });
        }
        if let Some(existing) = state.leases.get(binding_id)
            && existing.expires_at > now
        {
            if existing.owner == owner && existing.generation == generation {
                return Ok(existing.clone());
            }
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_driver_lease",
                id: binding_id.to_string(),
                expected: None,
                actual: Some(existing.epoch),
            });
        }
        let epoch = state
            .leases
            .get(binding_id)
            .map(|lease| {
                lease
                    .epoch
                    .checked_add(1)
                    .ok_or_else(|| HostStoreError::Invariant {
                        reason: "driver lease epoch is exhausted".to_string(),
                    })
            })
            .transpose()?
            .unwrap_or(1);
        let lease = DriverLease {
            binding_id: binding_id.clone(),
            generation,
            owner: owner.to_string(),
            token: uuid::Uuid::new_v4().to_string(),
            epoch,
            expires_at,
        };
        state.leases.insert(binding_id.clone(), lease.clone());
        if let Some(binding) = state.bindings.get_mut(binding_id) {
            binding.lease_epoch = epoch;
        }
        Ok(lease)
    }

    async fn record_driver_coordinate(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        coordinate: RuntimeDriverCoordinate,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        let binding = state
            .bindings
            .get(binding_id)
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_binding",
                id: binding_id.to_string(),
            })?;
        if binding.driver_generation != generation || binding.state != RuntimeBindingState::Active {
            return Err(HostStoreError::Invariant {
                reason: "driver coordinate targets a stale binding generation".to_string(),
            });
        }
        let key = (
            binding_id.clone(),
            generation,
            coordinate.kind().to_string(),
            coordinate.runtime_id().to_string(),
        );
        if let Some(existing) = state.driver_coordinates.get(&key) {
            if existing == &coordinate {
                return Ok(());
            }
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_driver_coordinate",
                id: coordinate.runtime_id().to_string(),
                expected: None,
                actual: Some(generation.0),
            });
        }
        if state.driver_coordinates.iter().any(
            |((existing_binding, existing_generation, existing_kind, _), existing)| {
                existing_binding == binding_id
                    && *existing_generation == generation
                    && existing_kind == coordinate.kind()
                    && existing.source_id() == coordinate.source_id()
            },
        ) {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_driver_coordinate.source_id",
                id: coordinate.source_id().to_string(),
                expected: None,
                actual: Some(generation.0),
            });
        }
        state.driver_coordinates.insert(key, coordinate);
        Ok(())
    }

    async fn validate_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError> {
        let state = self.state.read().await;
        let lease = state
            .leases
            .get(binding_id)
            .ok_or_else(|| HostStoreError::NotFound {
                entity: "agent_runtime_driver_lease",
                id: binding_id.to_string(),
            })?;
        if lease.generation != generation
            || lease.owner != owner
            || lease.token != token
            || lease.expires_at <= now
        {
            return Err(HostStoreError::Invariant {
                reason: "driver lease is stale or expired".to_string(),
            });
        }
        Ok(lease.clone())
    }

    async fn mark_binding_lost(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError> {
        let mut state = self.state.write().await;
        let binding =
            state
                .bindings
                .get_mut(binding_id)
                .ok_or_else(|| HostStoreError::NotFound {
                    entity: "agent_runtime_binding",
                    id: binding_id.to_string(),
                })?;
        if binding.driver_generation != generation {
            return Err(HostStoreError::Invariant {
                reason: "cannot mark a different binding generation lost".to_string(),
            });
        }
        binding.state = RuntimeBindingState::Lost;
        Ok(())
    }

    async fn profile_digest_for_binding(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<ProfileDigest>, HostStoreError> {
        Ok(self
            .state
            .read()
            .await
            .bindings
            .get(binding_id)
            .map(|binding| binding.profile_digest.clone()))
    }
}
