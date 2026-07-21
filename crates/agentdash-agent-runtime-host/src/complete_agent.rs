use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime::{
    ManagedRuntimeAgentBinding, ManagedRuntimeCreateOutcome, ManagedRuntimeDispatchContext,
    ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError, ManagedRuntimeLifecycleInspection,
    ManagedRuntimeLifecyclePort, ManagedRuntimeRebindOutcome, ManagedRuntimeResumeOutcome,
    bind_complete_agent_surface,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentBindingGeneration, AgentCallbackRouteId, AgentChangePage,
    AgentChangesQuery, AgentCommandEnvelope, AgentCommandId, AgentCommandMeta, AgentCommandReceipt,
    AgentEffectIdentity, AgentEffectInspectionState, AgentForkPoint, AgentHostCallbackBinding,
    AgentHostCallbackError, AgentHostCallbackErrorCode, AgentHostCallbackMeta, AgentIdempotencyKey,
    AgentPayloadDigest, AgentProfileDigest, AgentReadQuery, AgentReceiptState, AgentRuntimeOffer,
    AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceError, AgentServiceInstanceId,
    AgentSourceCoordinate, AgentSurfaceProfile, AgentSurfaceRoute, AgentSurfaceSemanticFacet,
    AgentSurfaceSnapshot, AgentTerminalOutcome, AppliedAgentCommandReceipt, AppliedAgentSurface,
    AppliedForkAgentReceipt, ApplyBoundAgentSurface, BoundAgentSurface,
    CompleteAgentLiveAttachmentId, CompleteAgentService, CreateAgentCommand, ForkAgentCommand,
    ForkAgentReceipt, InitialAgentContextPackage, ResumeAgentCommand, SemanticFidelity,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    CompleteAgentCallbackRoute, CompleteAgentLiveCatalogError, CompleteAgentLiveSelection,
    CompleteAgentRemoteBindingFact, CompleteAgentServiceVerification,
    SharedCompleteAgentLiveCatalog,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompleteAgentBindingId(String);

impl CompleteAgentBindingId {
    pub fn new(value: impl Into<String>) -> Result<Self, CompleteAgentHostError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Complete Agent binding id must not be empty".to_owned(),
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentBindingTarget {
    pub logical_instance_id: AgentServiceInstanceId,
    pub live_attachment_id: CompleteAgentLiveAttachmentId,
    pub definition_id: AgentServiceDefinitionId,
    pub verified_build_digest: AgentPayloadDigest,
    pub verified_profile_digest: AgentProfileDigest,
    pub offer_profile_digest: AgentProfileDigest,
    pub placement: CompleteAgentPlacement,
    pub remote_binding: Option<CompleteAgentRemoteBindingFact>,
}

impl CompleteAgentBindingTarget {
    pub fn host_incarnation_id(&self) -> &str {
        self.placement.host_incarnation_id()
    }

    pub(crate) fn is_valid(&self) -> bool {
        !self.logical_instance_id.as_str().trim().is_empty()
            && !self.live_attachment_id.as_str().trim().is_empty()
            && !self.definition_id.as_str().trim().is_empty()
            && !self.verified_build_digest.as_str().trim().is_empty()
            && !self.verified_profile_digest.as_str().trim().is_empty()
            && !self.offer_profile_digest.as_str().trim().is_empty()
            && self.placement.is_valid()
            && self.verified_profile_digest == self.offer_profile_digest
            && match (&self.placement, &self.remote_binding) {
                (
                    CompleteAgentPlacement::Remote {
                        transport_id,
                        host_incarnation_id,
                        ..
                    },
                    Some(remote),
                ) => {
                    remote.local_service_instance_id == self.logical_instance_id
                        && remote.remote_binding_generation.0 > 0
                        && remote.host_incarnation_id == *host_incarnation_id
                        && remote.transport_id == *transport_id
                }
                (CompleteAgentPlacement::Remote { .. }, None) => false,
                (_, None) => true,
                (_, Some(_)) => false,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentBinding {
    pub id: CompleteAgentBindingId,
    pub target: CompleteAgentBindingTarget,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface: BoundAgentSurface,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub state: CompleteAgentBindingState,
}

impl CompleteAgentBinding {
    pub fn dispatch_admitted(&self) -> bool {
        self.state == CompleteAgentBindingState::Available
            && self
                .applied_surface
                .as_ref()
                .is_some_and(|applied| self.bound_surface.accepts_applied(applied))
    }

    fn managed(&self) -> Option<ManagedRuntimeAgentBinding> {
        self.dispatch_admitted()
            .then(|| ManagedRuntimeAgentBinding {
                source: self.source.clone(),
                generation: self.generation,
                applied_surface: self
                    .applied_surface
                    .clone()
                    .expect("dispatch-admitted binding has applied surface"),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentBindingState {
    PendingSurface,
    Available,
    Desynchronized,
    Lost,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTarget {
    pub runtime_thread_id: RuntimeThreadId,
    pub target: CompleteAgentBindingTarget,
    pub generation: AgentBindingGeneration,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface: BoundAgentSurface,
    pub callbacks: AgentHostCallbackBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTargetProvisioning {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub target: CompleteAgentRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRuntimeTargetProvisioningRequest {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub runtime_thread_id: RuntimeThreadId,
    pub target: CompleteAgentBindingTarget,
    pub desired_surface: AgentSurfaceSnapshot,
    pub callback_deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeTargetRecovery {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub previous_target: CompleteAgentRuntimeTarget,
    pub recovered_target: CompleteAgentRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRuntimeTargetRecoveryRequest {
    pub idempotency_key: AgentIdempotencyKey,
    pub request_digest: AgentPayloadDigest,
    pub runtime_thread_id: RuntimeThreadId,
    pub expected_generation: AgentBindingGeneration,
    pub target: CompleteAgentBindingTarget,
    pub desired_surface: AgentSurfaceSnapshot,
    pub callback_deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerifiedServiceRegistration {
    pub instance_id: AgentServiceInstanceId,
    pub descriptor: AgentServiceDescriptor,
    pub placement: CompleteAgentPlacement,
    pub verification: CompleteAgentServiceVerification,
    pub remote_binding: Option<CompleteAgentRemoteBindingFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompleteAgentPlacement {
    InProcess {
        host_incarnation_id: String,
    },
    LocalProcess {
        host_id: String,
        host_incarnation_id: String,
    },
    Remote {
        host_id: String,
        transport_id: String,
        host_incarnation_id: String,
    },
}

impl CompleteAgentPlacement {
    pub fn host_incarnation_id(&self) -> &str {
        match self {
            Self::InProcess {
                host_incarnation_id,
            }
            | Self::LocalProcess {
                host_incarnation_id,
                ..
            }
            | Self::Remote {
                host_incarnation_id,
                ..
            } => host_incarnation_id,
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::InProcess {
                host_incarnation_id,
            } => !host_incarnation_id.trim().is_empty(),
            Self::LocalProcess {
                host_id,
                host_incarnation_id,
            } => !host_id.trim().is_empty() && !host_incarnation_id.trim().is_empty(),
            Self::Remote {
                host_id,
                transport_id,
                host_incarnation_id,
            } => {
                !host_id.trim().is_empty()
                    && !transport_id.trim().is_empty()
                    && !host_incarnation_id.trim().is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentHostError {
    #[error("Complete Agent live attachment is unavailable: {attachment_id}")]
    UnavailableAttachment {
        attachment_id: CompleteAgentLiveAttachmentId,
    },
    #[error("Complete Agent binding was not found: {binding_id}")]
    UnknownBinding { binding_id: String },
    #[error("Complete Agent generation is stale: expected {expected:?}, actual {actual:?}")]
    StaleGeneration {
        expected: AgentBindingGeneration,
        actual: AgentBindingGeneration,
    },
    #[error("Complete Agent binding is not dispatchable: {reason}")]
    DispatchRejected { reason: String },
    #[error("Complete Agent Runtime target provisioning conflicts with current process state")]
    ProvisioningConflict,
    #[error("Complete Agent host invariant failed: {reason}")]
    Invariant { reason: String },
    #[error(transparent)]
    LiveCatalog(#[from] CompleteAgentLiveCatalogError),
    #[error(transparent)]
    Service(#[from] AgentServiceError),
}

#[derive(Default)]
struct CompleteAgentHostLiveState {
    runtime_targets: BTreeMap<RuntimeThreadId, CompleteAgentRuntimeTarget>,
    bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    callback_routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    lost_runtime_threads: BTreeSet<RuntimeThreadId>,
}

/// Process-local Complete Agent router.
///
/// Dropping this value removes every route and generation. Concrete Agents remain authoritative
/// for source history, surface application, and effect receipts; Product remains authoritative for
/// the stable service/source association used to rebuild a route.
pub struct CompleteAgentHost {
    live_catalog: SharedCompleteAgentLiveCatalog,
    state: RwLock<CompleteAgentHostLiveState>,
}

impl CompleteAgentHost {
    pub fn new(live_catalog: SharedCompleteAgentLiveCatalog) -> Self {
        Self {
            live_catalog,
            state: RwLock::new(CompleteAgentHostLiveState::default()),
        }
    }

    pub async fn attach_verified_service(
        &self,
        registration: CompleteAgentVerifiedServiceRegistration,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentHostError> {
        Ok(self.live_catalog.attach(registration, service).await?)
    }

    pub async fn provision_runtime_target(
        &self,
        request: CompleteAgentRuntimeTargetProvisioningRequest,
    ) -> Result<CompleteAgentRuntimeTargetProvisioning, CompleteAgentHostError> {
        if request.callback_deadline_ms == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime target callback deadline must be positive".to_owned(),
            });
        }
        let selection = self.live_selection(&request.target).await?;
        let bound_surface = bind_complete_agent_surface(&request.desired_surface, &selection.offer)
            .map_err(|error| CompleteAgentHostError::DispatchRejected {
                reason: error.to_string(),
            })?;
        let mut state = self.state.write().await;
        if let Some(existing) = state
            .runtime_targets
            .get(&request.runtime_thread_id)
            .cloned()
        {
            if !state
                .lost_runtime_threads
                .contains(&request.runtime_thread_id)
                && existing.target == request.target
                && existing.bound_surface == bound_surface
            {
                return Ok(CompleteAgentRuntimeTargetProvisioning {
                    idempotency_key: request.idempotency_key,
                    request_digest: request.request_digest,
                    target: existing,
                });
            }
            if !state
                .lost_runtime_threads
                .contains(&request.runtime_thread_id)
            {
                return Err(CompleteAgentHostError::ProvisioningConflict);
            }
        }
        let generation = state
            .runtime_targets
            .get(&request.runtime_thread_id)
            .map_or(Ok(AgentBindingGeneration(1)), |target| {
                target
                    .generation
                    .0
                    .checked_add(1)
                    .map(AgentBindingGeneration)
                    .ok_or_else(|| CompleteAgentHostError::Invariant {
                        reason: "Runtime target generation is exhausted".to_owned(),
                    })
            })?;
        let target = runtime_target(
            request.runtime_thread_id.clone(),
            request.target,
            generation,
            selection.offer.profile_digest,
            bound_surface,
            request.callback_deadline_ms,
        )?;
        state
            .runtime_targets
            .insert(request.runtime_thread_id.clone(), target.clone());
        state
            .lost_runtime_threads
            .remove(&request.runtime_thread_id);
        Ok(CompleteAgentRuntimeTargetProvisioning {
            idempotency_key: request.idempotency_key,
            request_digest: request.request_digest,
            target,
        })
    }

    pub async fn prepare_runtime_surface_rebind(
        &self,
        request: CompleteAgentRuntimeTargetRecoveryRequest,
    ) -> Result<CompleteAgentRuntimeTargetRecovery, CompleteAgentHostError> {
        if request.callback_deadline_ms == 0 || request.expected_generation.0 == 0 {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Runtime surface rebind requires positive generation and deadline"
                    .to_owned(),
            });
        }
        let selection = self.live_selection(&request.target).await?;
        let bound_surface = bind_complete_agent_surface(&request.desired_surface, &selection.offer)
            .map_err(|error| CompleteAgentHostError::DispatchRejected {
                reason: error.to_string(),
            })?;
        let mut state = self.state.write().await;
        let previous = state
            .runtime_targets
            .get(&request.runtime_thread_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                reason: format!(
                    "Runtime target {} is not registered",
                    request.runtime_thread_id
                ),
            })?;
        if previous.target != request.target {
            return Err(CompleteAgentHostError::ProvisioningConflict);
        }
        if previous.bound_surface == bound_surface {
            return Ok(CompleteAgentRuntimeTargetRecovery {
                idempotency_key: request.idempotency_key,
                request_digest: request.request_digest,
                previous_target: previous.clone(),
                recovered_target: previous,
            });
        }
        if previous.generation != request.expected_generation {
            return Err(CompleteAgentHostError::StaleGeneration {
                expected: request.expected_generation,
                actual: previous.generation,
            });
        }
        let generation =
            AgentBindingGeneration(previous.generation.0.checked_add(1).ok_or_else(|| {
                CompleteAgentHostError::Invariant {
                    reason: "Runtime target generation is exhausted".to_owned(),
                }
            })?);
        let recovered = runtime_target(
            request.runtime_thread_id.clone(),
            request.target,
            generation,
            selection.offer.profile_digest,
            bound_surface,
            request.callback_deadline_ms,
        )?;
        state
            .runtime_targets
            .insert(request.runtime_thread_id, recovered.clone());
        Ok(CompleteAgentRuntimeTargetRecovery {
            idempotency_key: request.idempotency_key,
            request_digest: request.request_digest,
            previous_target: previous,
            recovered_target: recovered,
        })
    }

    pub async fn runtime_target(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<CompleteAgentRuntimeTarget, CompleteAgentHostError> {
        self.state
            .read()
            .await
            .runtime_targets
            .get(runtime_thread_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                reason: format!("Runtime target {runtime_thread_id} is not registered"),
            })
    }

    pub async fn resolve_callback_route(
        &self,
        meta: &AgentHostCallbackMeta,
    ) -> Result<(CompleteAgentCallbackRoute, CompleteAgentBinding), AgentHostCallbackError> {
        let state = self.state.read().await;
        let route = state
            .callback_routes
            .get(&meta.route_id)
            .cloned()
            .ok_or_else(|| {
                AgentHostCallbackError::new(
                    AgentHostCallbackErrorCode::UnknownRoute,
                    "callback route is not registered in this Host incarnation",
                    false,
                )
            })?;
        if route.generation != meta.binding_generation {
            return Err(AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::StaleBindingGeneration,
                "callback binding generation is stale",
                false,
            ));
        }
        if route.source != meta.source {
            return Err(AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback source does not match the registered route",
                false,
            ));
        }
        let binding = state
            .bindings
            .get(&route.binding_id)
            .cloned()
            .ok_or_else(|| {
                AgentHostCallbackError::new(
                    AgentHostCallbackErrorCode::Internal,
                    "callback route has no owning binding",
                    false,
                )
            })?;
        Ok((route, binding))
    }

    pub async fn runtime_source_association(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<(AgentServiceInstanceId, AgentSourceCoordinate), CompleteAgentHostError> {
        let (target, binding) = self.current_binding(runtime_thread_id).await?;
        Ok((target.target.logical_instance_id, binding.source))
    }

    pub async fn runtime_binding_generation(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        source: &AgentSourceCoordinate,
    ) -> Result<AgentBindingGeneration, CompleteAgentHostError> {
        let (_, binding) = self.current_binding(runtime_thread_id).await?;
        if &binding.source != source {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Agent command route is not current for this source".to_owned(),
            });
        }
        Ok(binding.generation)
    }

    pub async fn restore_runtime_source_route(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        source: AgentSourceCoordinate,
        effect_id: AgentEffectIdentity,
        _dispatch_owner: String,
        _lease_duration_ms: u64,
    ) -> Result<AgentBindingGeneration, CompleteAgentHostError> {
        let target = self.runtime_target(runtime_thread_id).await?;
        self.apply_surface_and_bind(&target, source, effect_id)
            .await
            .map(|binding| binding.generation)
    }

    pub async fn fork_runtime_source(
        &self,
        parent_runtime_thread_id: &RuntimeThreadId,
        parent_source: &AgentSourceCoordinate,
        child_runtime_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
        effect_id: AgentEffectIdentity,
        dispatch_owner: String,
        lease_duration_ms: u64,
    ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
        let (_, parent_binding) = self
            .current_binding(parent_runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        if &parent_binding.source != parent_source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "parent concrete Agent route does not match Product association".to_owned(),
            });
        }
        ManagedRuntimeLifecyclePort::fork(
            self,
            ManagedRuntimeDispatchContext {
                runtime_thread_id: parent_runtime_thread_id.clone(),
                effect_id,
                dispatch_owner,
                now_ms: current_time_ms(),
                lease_duration_ms,
            },
            parent_binding
                .managed()
                .ok_or(ManagedRuntimeLifecycleError::NotFound)?,
            child_runtime_thread_id,
            cutoff,
        )
        .await
    }

    pub async fn apply_prepared_runtime_surface(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        effect_id: AgentEffectIdentity,
        _dispatch_owner: String,
        _lease_duration_ms: u64,
    ) -> Result<ManagedRuntimeRebindOutcome, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        if let Ok((_, existing)) = self.current_binding(runtime_thread_id).await
            && let Some(binding) = existing.managed()
            && binding.generation == target.generation
        {
            return Ok(ManagedRuntimeRebindOutcome {
                receipt: synthetic_surface_command_receipt(
                    &effect_id,
                    binding.source.clone(),
                    binding.applied_surface.revision.0,
                )?,
                previous_binding: binding.clone(),
                binding,
            });
        }
        let previous_generation = target
            .generation
            .0
            .checked_sub(1)
            .filter(|value| *value > 0)
            .ok_or(ManagedRuntimeLifecycleError::NotFound)?;
        let previous_id = runtime_binding_id(
            runtime_thread_id,
            AgentBindingGeneration(previous_generation),
        )
        .map_err(map_lifecycle_host_error)?;
        let previous = self
            .state
            .read()
            .await
            .bindings
            .get(&previous_id)
            .and_then(CompleteAgentBinding::managed)
            .ok_or(ManagedRuntimeLifecycleError::NotFound)?;
        let binding = self
            .apply_surface_and_bind(&target, previous.source.clone(), effect_id.clone())
            .await
            .map_err(map_lifecycle_host_error)?;
        Ok(ManagedRuntimeRebindOutcome {
            receipt: synthetic_surface_command_receipt(
                &effect_id,
                binding.source.clone(),
                binding.applied_surface.revision.0,
            )?,
            previous_binding: previous,
            binding,
        })
    }

    pub async fn mark_target_bindings_lost(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<(), CompleteAgentHostError> {
        let mut state = self.state.write().await;
        let threads = state
            .runtime_targets
            .iter()
            .filter(|(_, current)| &current.target == target)
            .map(|(thread, _)| thread.clone())
            .collect::<Vec<_>>();
        for thread in threads {
            if let Some((route_id, generation)) = state
                .runtime_targets
                .get(&thread)
                .map(|current| (current.callbacks.route_id.clone(), current.generation))
            {
                state.callback_routes.remove(&route_id);
                if let Ok(binding_id) = runtime_binding_id(&thread, generation) {
                    state.bindings.remove(&binding_id);
                }
            }
            state.lost_runtime_threads.insert(thread);
        }
        let lost_binding_ids = state
            .bindings
            .iter()
            .filter(|(_, binding)| &binding.target == target)
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        state
            .callback_routes
            .retain(|_, route| !lost_binding_ids.contains(&route.binding_id));
        state
            .bindings
            .retain(|_, binding| &binding.target != target);
        Ok(())
    }

    pub async fn lost_runtime_threads_for_profile(
        &self,
        profile_digest: &AgentProfileDigest,
    ) -> Result<Vec<RuntimeThreadId>, CompleteAgentHostError> {
        let state = self.state.read().await;
        Ok(state
            .lost_runtime_threads
            .iter()
            .filter(|thread| {
                state
                    .runtime_targets
                    .get(*thread)
                    .is_some_and(|target| &target.profile_digest == profile_digest)
            })
            .cloned()
            .collect())
    }

    async fn current_binding(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<(CompleteAgentRuntimeTarget, CompleteAgentBinding), CompleteAgentHostError> {
        let state = self.state.read().await;
        let target = state
            .runtime_targets
            .get(runtime_thread_id)
            .cloned()
            .ok_or_else(|| CompleteAgentHostError::DispatchRejected {
                reason: format!("Runtime target {runtime_thread_id} is not registered"),
            })?;
        let binding_id = runtime_binding_id(runtime_thread_id, target.generation)?;
        let binding = state.bindings.get(&binding_id).cloned().ok_or_else(|| {
            CompleteAgentHostError::UnknownBinding {
                binding_id: binding_id.as_str().to_owned(),
            }
        })?;
        if binding.target != target.target
            || binding.generation != target.generation
            || binding.profile_digest != target.profile_digest
            || binding.bound_surface != target.bound_surface
            || !binding.dispatch_admitted()
        {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "Runtime binding is not admitted by the current target".to_owned(),
            });
        }
        Ok((target, binding))
    }

    async fn runtime_binding(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        expected: &ManagedRuntimeAgentBinding,
    ) -> Result<(CompleteAgentRuntimeTarget, CompleteAgentBinding), CompleteAgentHostError> {
        let (target, binding) = self.current_binding(runtime_thread_id).await?;
        if binding.generation != expected.generation
            || binding.source != expected.source
            || binding.applied_surface.as_ref() != Some(&expected.applied_surface)
        {
            return Err(CompleteAgentHostError::StaleGeneration {
                expected: binding.generation,
                actual: expected.generation,
            });
        }
        Ok((target, binding))
    }

    async fn live_selection(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentHostError> {
        validate_binding_target(target)?;
        let selection = self
            .live_catalog
            .resolve(&target.live_attachment_id)
            .await
            .ok_or_else(|| CompleteAgentHostError::UnavailableAttachment {
                attachment_id: target.live_attachment_id.clone(),
            })?;
        if selection.target != *target {
            return Err(CompleteAgentHostError::DispatchRejected {
                reason: "live attachment does not match the pinned target facts".to_owned(),
            });
        }
        Ok(selection)
    }

    async fn service(
        &self,
        target: &CompleteAgentBindingTarget,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentHostError> {
        Ok(self.live_selection(target).await?.service())
    }

    async fn apply_surface_and_bind(
        &self,
        target: &CompleteAgentRuntimeTarget,
        source: AgentSourceCoordinate,
        effect_id: AgentEffectIdentity,
    ) -> Result<ManagedRuntimeAgentBinding, CompleteAgentHostError> {
        let binding_id = runtime_binding_id(&target.runtime_thread_id, target.generation)?;
        if let Some(existing) = self.state.read().await.bindings.get(&binding_id).cloned()
            && existing.source == source
            && existing.target == target.target
            && let Some(binding) = existing.managed()
        {
            return Ok(binding);
        }
        let service = self.service(&target.target).await?;
        let surface_effect_id = derived_effect_id(&effect_id, "surface")?;
        let inspection = service.inspect(surface_effect_id.clone()).await?;
        if !inspection.validate() || inspection.effect_id != surface_effect_id {
            return Err(CompleteAgentHostError::Invariant {
                reason: "Agent returned invalid surface effect inspection".to_owned(),
            });
        }
        let receipt = match inspection.state {
            AgentEffectInspectionState::NotApplied => {
                service
                    .apply_surface(ApplyBoundAgentSurface {
                        command_id: derived_command_id(&effect_id, "surface")?,
                        effect_id: surface_effect_id.clone(),
                        idempotency_key: derived_idempotency_key(&effect_id, "surface")?,
                        source: source.clone(),
                        bound_surface: target.bound_surface.clone(),
                        callbacks: target.callbacks.clone(),
                    })
                    .await?
            }
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::SurfaceApply { receipt },
            } => receipt,
            AgentEffectInspectionState::Accepted { .. } | AgentEffectInspectionState::Unknown => {
                return Err(CompleteAgentHostError::DispatchRejected {
                    reason: "surface application is not yet inspectably applied".to_owned(),
                });
            }
            AgentEffectInspectionState::Applied { .. } => {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "surface effect inspection returned another effect kind".to_owned(),
                });
            }
        };
        if receipt.effect_id != surface_effect_id
            || receipt.source != source
            || !target.bound_surface.accepts_applied(&receipt.applied)
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface receipt does not match the current Host route".to_owned(),
            });
        }
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target: target.target.clone(),
            generation: target.generation,
            source: source.clone(),
            profile_digest: target.profile_digest.clone(),
            bound_surface: target.bound_surface.clone(),
            applied_surface: Some(receipt.applied.clone()),
            state: CompleteAgentBindingState::Available,
        };
        let route = CompleteAgentCallbackRoute::from_binding(
            target.runtime_thread_id.clone(),
            binding_id.clone(),
            target.callbacks.clone(),
            source.clone(),
            target.bound_surface.clone(),
        )
        .map_err(|error| CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        })?;
        let mut state = self.state.write().await;
        if state.runtime_targets.get(&target.runtime_thread_id) != Some(target) {
            return Err(CompleteAgentHostError::StaleGeneration {
                expected: target.generation,
                actual: state
                    .runtime_targets
                    .get(&target.runtime_thread_id)
                    .map_or(AgentBindingGeneration(0), |current| current.generation),
            });
        }
        state.bindings.insert(binding_id, binding);
        state.callback_routes.insert(route.route_id.clone(), route);
        Ok(ManagedRuntimeAgentBinding {
            source,
            generation: target.generation,
            applied_surface: receipt.applied,
        })
    }
}

#[async_trait]
impl ManagedRuntimeLifecyclePort for CompleteAgentHost {
    async fn create(
        &self,
        context: ManagedRuntimeDispatchContext,
        initial_context: Option<InitialAgentContextPackage>,
    ) -> Result<ManagedRuntimeCreateOutcome, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let inspection = inspect(&service, &context.effect_id).await?;
        let receipt = match inspection {
            AgentEffectInspectionState::NotApplied => service
                .create(CreateAgentCommand {
                    meta: lifecycle_meta(&context, target.generation)?,
                    requested_source: None,
                    initial_context,
                })
                .await
                .map_err(agent_lifecycle_error)?,
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Create { receipt },
            } => agent_receipt_from_applied(receipt),
            AgentEffectInspectionState::Accepted { .. } | AgentEffectInspectionState::Unknown => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Create effect is not yet inspectably applied".to_owned(),
                });
            }
            AgentEffectInspectionState::Applied { .. } => {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Create effect inspection returned another effect kind".to_owned(),
                });
            }
        };
        ensure_applied_receipt(&receipt, "Create")?;
        let binding = self
            .apply_surface_and_bind(&target, receipt.source.clone(), context.effect_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        let descriptor = service.describe().await.map_err(agent_lifecycle_error)?;
        Ok(ManagedRuntimeCreateOutcome {
            initial_context: receipt.initial_context.clone(),
            receipt,
            binding,
            contribution_fidelity: descriptor.profile.initial_context.contribution_fidelity,
        })
    }

    async fn resume(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError> {
        let (target, _) = self
            .runtime_binding(&context.runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let inspection = inspect(&service, &context.effect_id).await?;
        let receipt = match inspection {
            AgentEffectInspectionState::NotApplied => service
                .resume(ResumeAgentCommand {
                    meta: lifecycle_meta(&context, binding.generation)?,
                    source: binding.source.clone(),
                })
                .await
                .map_err(agent_lifecycle_error)?,
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Resume { receipt },
            } => agent_receipt_from_applied(receipt),
            AgentEffectInspectionState::Accepted { .. } | AgentEffectInspectionState::Unknown => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Resume effect is not yet inspectably applied".to_owned(),
                });
            }
            AgentEffectInspectionState::Applied { .. } => {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Resume effect inspection returned another effect kind".to_owned(),
                });
            }
        };
        ensure_applied_receipt(&receipt, "Resume")?;
        if receipt.source != binding.source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "Resume receipt belongs to another source".to_owned(),
            });
        }
        Ok(ManagedRuntimeResumeOutcome { receipt, binding })
    }

    async fn rebind(
        &self,
        context: ManagedRuntimeDispatchContext,
        _previous_binding: ManagedRuntimeAgentBinding,
    ) -> Result<ManagedRuntimeRebindOutcome, ManagedRuntimeLifecycleError> {
        self.apply_prepared_runtime_surface(
            &context.runtime_thread_id,
            context.effect_id,
            context.dispatch_owner,
            context.lease_duration_ms,
        )
        .await
    }

    async fn fork(
        &self,
        context: ManagedRuntimeDispatchContext,
        parent: ManagedRuntimeAgentBinding,
        child_thread_id: RuntimeThreadId,
        cutoff: AgentForkPoint,
    ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
        let (parent_target, _) = self
            .runtime_binding(&context.runtime_thread_id, &parent)
            .await
            .map_err(map_lifecycle_host_error)?;
        let service = self
            .service(&parent_target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        let inspection = inspect(&service, &context.effect_id).await?;
        let receipt = match inspection {
            AgentEffectInspectionState::NotApplied => service
                .fork(ForkAgentCommand {
                    meta: lifecycle_meta(&context, parent.generation)?,
                    source: parent.source.clone(),
                    requested_child_source: None,
                    cutoff: cutoff.clone(),
                })
                .await
                .map_err(agent_lifecycle_error)?,
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Fork { receipt },
            } => fork_receipt_from_applied(receipt),
            AgentEffectInspectionState::Accepted { .. } | AgentEffectInspectionState::Unknown => {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Fork effect is not yet inspectably applied".to_owned(),
                });
            }
            AgentEffectInspectionState::Applied { .. } => {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Fork effect inspection returned another effect kind".to_owned(),
                });
            }
        };
        let child_source = receipt.child_source.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "applied Fork receipt has no child source".to_owned(),
            }
        })?;
        let child_history_digest = receipt.child_history_digest.clone().ok_or_else(|| {
            ManagedRuntimeLifecycleError::InspectionRequired {
                reason: "applied Fork receipt has no child history digest".to_owned(),
            }
        })?;
        if receipt.parent_source != parent.source || receipt.cutoff != cutoff {
            return Err(ManagedRuntimeLifecycleError::ForkChildKnown {
                child_source,
                child_history_digest: Some(child_history_digest),
                reason: "Fork receipt does not match the requested parent/cutoff".to_owned(),
            });
        }
        let child_target = runtime_target(
            child_thread_id.clone(),
            parent_target.target,
            AgentBindingGeneration(1),
            parent_target.profile_digest,
            parent_target.bound_surface,
            parent_target.callbacks.default_deadline_ms,
        )
        .map_err(map_lifecycle_host_error)?;
        {
            let mut state = self.state.write().await;
            if let Some(existing) = state.runtime_targets.get(&child_thread_id)
                && existing != &child_target
            {
                return Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "child Runtime target already exists with different facts".to_owned(),
                });
            }
            state
                .runtime_targets
                .insert(child_thread_id, child_target.clone());
        }
        let child_binding = self
            .apply_surface_and_bind(
                &child_target,
                child_source.clone(),
                context.effect_id.clone(),
            )
            .await
            .map_err(
                |error| ManagedRuntimeLifecycleError::ForkInspectionRequired {
                    child_source: child_source.clone(),
                    child_history_digest: Some(child_history_digest.clone()),
                    reason: error.to_string(),
                },
            )?;
        Ok(ManagedRuntimeForkOutcome {
            receipt,
            child_binding,
            child_history_digest,
        })
    }

    async fn execute(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: ManagedRuntimeAgentBinding,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError> {
        let (target, host_binding) = self
            .runtime_binding(&context.runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        if command.source != host_binding.source || command.meta.effect_id != context.effect_id {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "Agent command coordinates do not match the current route".to_owned(),
            });
        }
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        match inspect(&service, &context.effect_id).await? {
            AgentEffectInspectionState::NotApplied => service
                .execute(command)
                .await
                .map_err(agent_lifecycle_error),
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Command { receipt },
            } => Ok(agent_receipt_from_applied(receipt)),
            AgentEffectInspectionState::Accepted { source } => Ok(AgentCommandReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                source,
                state: AgentReceiptState::Accepted,
                snapshot_revision: None,
                initial_context: None,
            }),
            AgentEffectInspectionState::Unknown => {
                Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "Agent command effect is unknown".to_owned(),
                })
            }
            AgentEffectInspectionState::Applied { .. } => {
                Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "Agent command inspection returned another effect kind".to_owned(),
                })
            }
        }
    }

    async fn inspect(
        &self,
        context: ManagedRuntimeDispatchContext,
        binding: Option<ManagedRuntimeAgentBinding>,
    ) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError> {
        let target = self
            .runtime_target(&context.runtime_thread_id)
            .await
            .map_err(map_lifecycle_host_error)?;
        if let Some(binding) = &binding {
            self.runtime_binding(&context.runtime_thread_id, binding)
                .await
                .map_err(map_lifecycle_host_error)?;
        }
        let service = self
            .service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?;
        match inspect(&service, &context.effect_id).await? {
            AgentEffectInspectionState::NotApplied => {
                Ok(ManagedRuntimeLifecycleInspection::NotApplied)
            }
            AgentEffectInspectionState::Accepted { .. } => {
                Ok(ManagedRuntimeLifecycleInspection::Accepted)
            }
            AgentEffectInspectionState::Unknown => Ok(ManagedRuntimeLifecycleInspection::Unknown),
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Command { receipt },
            } => Ok(ManagedRuntimeLifecycleInspection::CommandApplied(
                agent_receipt_from_applied(receipt),
            )),
            AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Resume { receipt },
            } => {
                let binding = binding.ok_or(ManagedRuntimeLifecycleError::NotFound)?;
                Ok(ManagedRuntimeLifecycleInspection::ResumeApplied(
                    ManagedRuntimeResumeOutcome {
                        receipt: agent_receipt_from_applied(receipt),
                        binding,
                    },
                ))
            }
            AgentEffectInspectionState::Applied { .. } => {
                Err(ManagedRuntimeLifecycleError::Invalid {
                    reason: "inspection outcome requires its typed Product operation context"
                        .to_owned(),
                })
            }
        }
    }

    async fn read(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentReadQuery,
    ) -> Result<agentdash_agent_service_api::AgentSnapshot, ManagedRuntimeLifecycleError> {
        let (target, host_binding) = self
            .runtime_binding(&runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        if query.source != host_binding.source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "read source does not match Runtime binding".to_owned(),
            });
        }
        self.service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?
            .read(query)
            .await
            .map_err(agent_lifecycle_error)
    }

    async fn changes(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, ManagedRuntimeLifecycleError> {
        let (target, host_binding) = self
            .runtime_binding(&runtime_thread_id, &binding)
            .await
            .map_err(map_lifecycle_host_error)?;
        if query.source != host_binding.source {
            return Err(ManagedRuntimeLifecycleError::Invalid {
                reason: "changes source does not match Runtime binding".to_owned(),
            });
        }
        self.service(&target.target)
            .await
            .map_err(map_lifecycle_host_error)?
            .changes(query)
            .await
            .map_err(agent_lifecycle_error)
    }

    async fn is_ready(
        &self,
        runtime_thread_id: RuntimeThreadId,
        binding: ManagedRuntimeAgentBinding,
    ) -> Result<bool, ManagedRuntimeLifecycleError> {
        self.runtime_binding(&runtime_thread_id, &binding)
            .await
            .map(|(_, binding)| binding.dispatch_admitted())
            .map_err(map_lifecycle_host_error)
    }
}

fn runtime_target(
    runtime_thread_id: RuntimeThreadId,
    target: CompleteAgentBindingTarget,
    generation: AgentBindingGeneration,
    profile_digest: AgentProfileDigest,
    bound_surface: BoundAgentSurface,
    callback_deadline_ms: u64,
) -> Result<CompleteAgentRuntimeTarget, CompleteAgentHostError> {
    let route_id = callback_route_id(&runtime_thread_id, &target, generation, &bound_surface)?;
    Ok(CompleteAgentRuntimeTarget {
        runtime_thread_id,
        target,
        generation,
        profile_digest,
        bound_surface,
        callbacks: AgentHostCallbackBinding {
            route_id,
            binding_generation: generation,
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: callback_deadline_ms,
        },
    })
}

fn runtime_binding_id(
    runtime_thread_id: &RuntimeThreadId,
    generation: AgentBindingGeneration,
) -> Result<CompleteAgentBindingId, CompleteAgentHostError> {
    CompleteAgentBindingId::new(format!(
        "runtime-binding:{runtime_thread_id}:{}",
        generation.0
    ))
}

fn callback_route_id(
    runtime_thread_id: &RuntimeThreadId,
    target: &CompleteAgentBindingTarget,
    generation: AgentBindingGeneration,
    bound_surface: &BoundAgentSurface,
) -> Result<AgentCallbackRouteId, CompleteAgentHostError> {
    let mut digest = Sha256::new();
    digest.update(runtime_thread_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(target.live_attachment_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(target.host_incarnation_id().as_bytes());
    digest.update([0]);
    digest.update(generation.0.to_be_bytes());
    digest.update([0]);
    digest.update(bound_surface.digest.as_str().as_bytes());
    AgentCallbackRouteId::new(format!("runtime-callback:{:x}", digest.finalize())).map_err(
        |error| CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        },
    )
}

fn lifecycle_meta(
    context: &ManagedRuntimeDispatchContext,
    generation: AgentBindingGeneration,
) -> Result<AgentCommandMeta, ManagedRuntimeLifecycleError> {
    Ok(AgentCommandMeta {
        command_id: derived_command_id(&context.effect_id, "lifecycle")
            .map_err(map_lifecycle_host_error)?,
        effect_id: context.effect_id.clone(),
        idempotency_key: derived_idempotency_key(&context.effect_id, "lifecycle")
            .map_err(map_lifecycle_host_error)?,
        binding_generation: generation,
        expected_snapshot_revision: None,
    })
}

fn derived_command_id(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentCommandId, CompleteAgentHostError> {
    AgentCommandId::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

fn derived_effect_id(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentEffectIdentity, CompleteAgentHostError> {
    AgentEffectIdentity::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

fn derived_idempotency_key(
    effect_id: &AgentEffectIdentity,
    suffix: &str,
) -> Result<AgentIdempotencyKey, CompleteAgentHostError> {
    AgentIdempotencyKey::new(format!("{}:{suffix}", effect_id.as_str())).map_err(|error| {
        CompleteAgentHostError::Invariant {
            reason: error.to_string(),
        }
    })
}

async fn inspect(
    service: &Arc<dyn CompleteAgentService>,
    effect_id: &AgentEffectIdentity,
) -> Result<AgentEffectInspectionState, ManagedRuntimeLifecycleError> {
    let inspection = service
        .inspect(effect_id.clone())
        .await
        .map_err(agent_lifecycle_error)?;
    if !inspection.validate() || &inspection.effect_id != effect_id {
        return Err(ManagedRuntimeLifecycleError::Invalid {
            reason: "Agent returned invalid effect inspection evidence".to_owned(),
        });
    }
    Ok(inspection.state)
}

fn agent_receipt_from_applied(receipt: AppliedAgentCommandReceipt) -> AgentCommandReceipt {
    AgentCommandReceipt {
        command_id: receipt.command_id,
        effect_id: receipt.effect_id,
        source: receipt.source,
        state: AgentReceiptState::AlreadyApplied {
            terminal: receipt.terminal,
        },
        snapshot_revision: receipt.snapshot_revision,
        initial_context: receipt.initial_context,
    }
}

fn fork_receipt_from_applied(receipt: AppliedForkAgentReceipt) -> ForkAgentReceipt {
    ForkAgentReceipt {
        command_id: receipt.command_id,
        effect_id: receipt.effect_id,
        parent_source: receipt.parent_source,
        child_source: Some(receipt.child_source),
        cutoff: receipt.cutoff,
        child_history_digest: Some(receipt.child_history_digest),
        state: AgentReceiptState::AlreadyApplied {
            terminal: receipt.terminal,
        },
    }
}

fn ensure_applied_receipt(
    receipt: &AgentCommandReceipt,
    operation: &str,
) -> Result<(), ManagedRuntimeLifecycleError> {
    if matches!(
        receipt.state,
        AgentReceiptState::AlreadyApplied { .. }
            | AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded | AgentTerminalOutcome::Closed
            }
    ) {
        Ok(())
    } else {
        Err(ManagedRuntimeLifecycleError::InspectionRequired {
            reason: format!("{operation} is not yet inspectably applied"),
        })
    }
}

fn synthetic_surface_command_receipt(
    effect_id: &AgentEffectIdentity,
    source: AgentSourceCoordinate,
    revision: u64,
) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError> {
    Ok(AgentCommandReceipt {
        command_id: derived_command_id(effect_id, "surface").map_err(map_lifecycle_host_error)?,
        effect_id: derived_effect_id(effect_id, "surface").map_err(map_lifecycle_host_error)?,
        source,
        state: AgentReceiptState::AlreadyApplied { terminal: None },
        snapshot_revision: Some(agentdash_agent_service_api::AgentSnapshotRevision(revision)),
        initial_context: None,
    })
}

fn map_lifecycle_host_error(error: CompleteAgentHostError) -> ManagedRuntimeLifecycleError {
    match error {
        CompleteAgentHostError::UnknownBinding { .. } => ManagedRuntimeLifecycleError::NotFound,
        CompleteAgentHostError::StaleGeneration { .. } => {
            ManagedRuntimeLifecycleError::StaleGeneration
        }
        CompleteAgentHostError::Service(error) => agent_lifecycle_error(error),
        CompleteAgentHostError::UnavailableAttachment { .. } => {
            ManagedRuntimeLifecycleError::Unavailable {
                reason: error.to_string(),
            }
        }
        CompleteAgentHostError::LiveCatalog(error) => ManagedRuntimeLifecycleError::Invalid {
            reason: error.to_string(),
        },
        CompleteAgentHostError::DispatchRejected { reason }
        | CompleteAgentHostError::Invariant { reason } => {
            ManagedRuntimeLifecycleError::Invalid { reason }
        }
        CompleteAgentHostError::ProvisioningConflict => ManagedRuntimeLifecycleError::Invalid {
            reason: error.to_string(),
        },
    }
}

fn agent_lifecycle_error(error: AgentServiceError) -> ManagedRuntimeLifecycleError {
    ManagedRuntimeLifecycleError::Unavailable {
        reason: error.to_string(),
    }
}

fn validate_binding_target(
    target: &CompleteAgentBindingTarget,
) -> Result<(), CompleteAgentHostError> {
    if !target.is_valid() {
        return Err(CompleteAgentHostError::Invariant {
            reason: "Complete Agent binding target snapshot is invalid".to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn validate_service_descriptor(
    descriptor: &AgentServiceDescriptor,
) -> Result<(), CompleteAgentHostError> {
    validate_surface_profile(&descriptor.profile.surface)
}

pub(crate) fn runtime_offer_from_descriptor(
    descriptor: &AgentServiceDescriptor,
) -> Result<AgentRuntimeOffer, CompleteAgentHostError> {
    let surface = &descriptor.profile.surface;
    validate_surface_profile(surface)?;
    Ok(AgentRuntimeOffer {
        profile_digest: descriptor.profile_digest.clone(),
        contributions: surface.facets.clone(),
    })
}

fn validate_surface_profile(surface: &AgentSurfaceProfile) -> Result<(), CompleteAgentHostError> {
    for facet in &surface.facets {
        if facet.routes.is_empty() {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet has no materialization route".to_owned(),
            });
        }
        if facet.fidelity == SemanticFidelity::Unsupported {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet cannot declare unsupported fidelity".to_owned(),
            });
        }
        if facet
            .semantics
            .required_causal_route()
            .is_some_and(|required| !facet.routes.contains(&required))
        {
            return Err(CompleteAgentHostError::Invariant {
                reason: "surface capability facet omits its semantic causal route".to_owned(),
            });
        }
        match &facet.semantics {
            AgentSurfaceSemanticFacet::Tool(tool)
                if tool.invocation == SemanticFidelity::Unsupported
                    || tool.update
                        == agentdash_agent_service_api::AgentToolUpdateSemantics::Unsupported =>
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "tool capability facet declares unsupported semantics".to_owned(),
                });
            }
            AgentSurfaceSemanticFacet::Hook(hook)
                if matches!(
                    hook.blocking,
                    agentdash_agent_service_api::AgentHookBlockingSemantics::Blocking {
                        fidelity: SemanticFidelity::Unsupported
                    }
                ) || hook
                    .mutations
                    .values()
                    .chain(hook.effects.values())
                    .any(|fidelity| *fidelity == SemanticFidelity::Unsupported) =>
            {
                return Err(CompleteAgentHostError::Invariant {
                    reason: "hook capability facet declares unsupported semantics".to_owned(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn current_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
