use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    ConfigurationBoundary, DriverBindingId, DriverItemId, DriverThreadId, DriverTurnId,
    EffectiveRuntimeProfile, HookPlanDigest, HookPlanRevision, HookPoint, HookRequirement,
    ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration, RuntimeItemId, RuntimeProfile,
    RuntimeServiceInstanceId, RuntimeThreadId, RuntimeTurnId, SurfaceDigest, SurfaceRevision,
};
use agentdash_integration_api::{
    AgentRuntimeCredentialRef, AgentRuntimeCredentialSlot, AgentRuntimePlacement,
    AgentServiceDefinitionId, AgentServiceOfferId, AgentServiceProvenance,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceInstanceDesiredState {
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceInstanceObservedState {
    Inactive,
    Activating,
    Active,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceInstance {
    pub id: RuntimeServiceInstanceId,
    pub definition_id: AgentServiceDefinitionId,
    pub definition_build_digest: String,
    pub config: Value,
    pub credentials: BTreeMap<AgentRuntimeCredentialSlot, AgentRuntimeCredentialRef>,
    pub placement: AgentRuntimePlacement,
    pub desired_state: ServiceInstanceDesiredState,
    pub observed_state: ServiceInstanceObservedState,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PutAgentServiceInstance {
    pub id: RuntimeServiceInstanceId,
    pub definition_id: AgentServiceDefinitionId,
    pub config: Value,
    pub credentials: BTreeMap<AgentRuntimeCredentialSlot, AgentRuntimeCredentialRef>,
    pub placement: AgentRuntimePlacement,
    pub desired_state: ServiceInstanceDesiredState,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConformanceEvidence {
    pub suite_revision: String,
    pub driver_build_digest: String,
    pub verified_profile_digest: ProfileDigest,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeOffer {
    pub id: AgentServiceOfferId,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub instance_revision: u64,
    pub generation: RuntimeDriverGeneration,
    pub provenance: AgentServiceProvenance,
    pub placement: AgentRuntimePlacement,
    pub protocol_revision: u32,
    pub effective_profile: EffectiveRuntimeProfile,
    pub profile_digest: ProfileDigest,
    pub conformance: ConformanceEvidence,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ActivateAgentServiceInstance {
    pub instance_id: RuntimeServiceInstanceId,
    pub expected_revision: u64,
    pub transport_profile: RuntimeProfile,
    pub transport_profile_digest: ProfileDigest,
    pub host_policy_profile: RuntimeProfile,
    pub host_policy_digest: ProfileDigest,
    pub conformance: ConformanceEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BoundAgentSurfaceReference {
    pub revision: SurfaceRevision,
    pub digest: SurfaceDigest,
    pub hook_plan_revision: Option<HookPlanRevision>,
    pub hook_plan_digest: Option<HookPlanDigest>,
    pub hook_artifact_digest: Option<String>,
    pub hook_configuration_boundary: ConfigurationBoundary,
    pub required_hooks: Vec<HookRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HookApplyStatus {
    pub point: HookPoint,
    pub acknowledged: bool,
    pub artifact_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppliedSurface {
    pub revision: SurfaceRevision,
    pub digest: SurfaceDigest,
    pub hook_plan_revision: Option<HookPlanRevision>,
    pub hook_plan_digest: Option<HookPlanDigest>,
    pub hooks: Vec<HookApplyStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBindingState {
    Pending,
    Active,
    Desynchronized,
    Lost,
    Closed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeBinding {
    pub id: RuntimeBindingId,
    pub thread_id: RuntimeThreadId,
    pub offer_id: AgentServiceOfferId,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub instance_revision: u64,
    pub driver_generation: RuntimeDriverGeneration,
    pub profile_digest: ProfileDigest,
    pub bound_surface: BoundAgentSurfaceReference,
    pub applied_surface: Option<AppliedSurface>,
    pub driver_binding_id: Option<DriverBindingId>,
    pub source_thread_id: Option<DriverThreadId>,
    pub state: RuntimeBindingState,
    pub lease_epoch: u64,
}

impl RuntimeBinding {
    pub fn dispatch_admitted(&self, profile: &RuntimeProfile) -> bool {
        if self.state != RuntimeBindingState::Active {
            return false;
        }
        let Some(applied) = &self.applied_surface else {
            return false;
        };
        if applied.revision != self.bound_surface.revision
            || applied.digest != self.bound_surface.digest
            || applied.hook_plan_revision != self.bound_surface.hook_plan_revision
            || applied.hook_plan_digest != self.bound_surface.hook_plan_digest
        {
            return false;
        }
        self.bound_surface.required_hooks.iter().all(|requirement| {
            !requirement.required
                || (applied.hooks.iter().any(|status| {
                    status.point == requirement.point
                        && status.acknowledged
                        && self
                            .bound_surface
                            .hook_artifact_digest
                            .as_ref()
                            .is_none_or(|expected| {
                                status.artifact_digest.as_ref() == Some(expected)
                            })
                }) && profile.hooks.satisfies(requirement))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSourceCoordinate {
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub thread_id: RuntimeThreadId,
    pub source_thread_id: DriverThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeDriverCoordinate {
    Turn {
        runtime_turn_id: RuntimeTurnId,
        source_turn_id: DriverTurnId,
    },
    Item {
        runtime_item_id: RuntimeItemId,
        source_item_id: DriverItemId,
    },
}

impl RuntimeDriverCoordinate {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Turn { .. } => "turn",
            Self::Item { .. } => "item",
        }
    }

    pub fn runtime_id(&self) -> &str {
        match self {
            Self::Turn {
                runtime_turn_id, ..
            } => runtime_turn_id.as_str(),
            Self::Item {
                runtime_item_id, ..
            } => runtime_item_id.as_str(),
        }
    }

    pub fn source_id(&self) -> &str {
        match self {
            Self::Turn { source_turn_id, .. } => source_turn_id.as_str(),
            Self::Item { source_item_id, .. } => source_item_id.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DriverLease {
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub owner: String,
    pub token: String,
    pub epoch: u64,
    pub expires_at: DateTime<Utc>,
}
