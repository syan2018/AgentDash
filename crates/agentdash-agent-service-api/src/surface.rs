use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentConfigurationBoundary,
    AgentHookDefinitionId, AgentPayloadDigest, AgentSurfaceDigest, AgentSurfaceRevision,
    AgentToolName, SemanticFidelity,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentSurfaceContributionKind {
    Instruction,
    Tool,
    Hook,
    Workspace,
    ContextRequirement,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentSurfaceRoute {
    ImmutableDelivery,
    RuntimeToolBroker,
    AgentNativeCallback,
    AgentNativeRegistry,
    HostLifecycle,
    ObservationOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHookPoint {
    BeforeTurn,
    AfterTurn,
    BeforeProviderRequest,
    BeforeTool,
    AfterTool,
    BeforeCompaction,
    AfterCompaction,
    BeforeStop,
    AfterItem,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHookTiming {
    Before,
    After,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHookAction {
    Observe,
    AllowOrDeny,
    RewriteInput,
    RewriteResult,
    AddContext,
    EmitEffect,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolDelivery {
    PromptDeclaration,
    RuntimeBrokerCallback,
    AgentNativeCallback,
    AgentNativeRegistry,
}

impl AgentToolDelivery {
    pub fn route(self) -> AgentSurfaceRoute {
        match self {
            Self::PromptDeclaration => AgentSurfaceRoute::ImmutableDelivery,
            Self::RuntimeBrokerCallback => AgentSurfaceRoute::RuntimeToolBroker,
            Self::AgentNativeCallback => AgentSurfaceRoute::AgentNativeCallback,
            Self::AgentNativeRegistry => AgentSurfaceRoute::AgentNativeRegistry,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolUpdateSemantics {
    Unsupported,
    BindingOnly,
    HotUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentToolSemanticFacet {
    pub delivery: AgentToolDelivery,
    pub invocation: SemanticFidelity,
    pub update: AgentToolUpdateSemantics,
}

impl AgentToolSemanticFacet {
    pub fn satisfies(&self, required: &Self) -> bool {
        self.delivery == required.delivery
            && self.invocation.satisfies(required.invocation)
            && self.update >= required.update
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentHookBlockingSemantics {
    NonBlocking,
    Blocking { fidelity: SemanticFidelity },
}

impl AgentHookBlockingSemantics {
    pub fn satisfies(&self, required: &Self) -> bool {
        match (self, required) {
            (Self::NonBlocking, Self::NonBlocking) => true,
            (Self::Blocking { fidelity }, Self::Blocking { fidelity: required }) => {
                fidelity.satisfies(*required)
            }
            _ => false,
        }
    }

    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::Blocking { .. })
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHookMutationKind {
    RewriteInput,
    RewriteResult,
    AddContext,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHookEffectKind {
    EmitEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentHookSemanticFacet {
    pub point: AgentHookPoint,
    pub timing: AgentHookTiming,
    pub blocking: AgentHookBlockingSemantics,
    pub mutations: BTreeMap<AgentHookMutationKind, SemanticFidelity>,
    pub effects: BTreeMap<AgentHookEffectKind, SemanticFidelity>,
}

impl AgentHookSemanticFacet {
    pub fn satisfies(&self, required: &Self) -> bool {
        self.point == required.point
            && self.timing == required.timing
            && self.blocking.satisfies(&required.blocking)
            && map_satisfies(&self.mutations, &required.mutations)
            && map_satisfies(&self.effects, &required.effects)
    }

    pub fn requires_reverse_callback(&self) -> bool {
        self.blocking.is_blocking() || !self.mutations.is_empty() || !self.effects.is_empty()
    }
}

fn map_satisfies<K: Ord>(
    offered: &BTreeMap<K, SemanticFidelity>,
    required: &BTreeMap<K, SemanticFidelity>,
) -> bool {
    required.iter().all(|(key, required_fidelity)| {
        offered
            .get(key)
            .is_some_and(|fidelity| fidelity.satisfies(*required_fidelity))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "facet", rename_all = "snake_case")]
pub enum AgentSurfaceSemanticFacet {
    Instruction,
    Tool(AgentToolSemanticFacet),
    Hook(AgentHookSemanticFacet),
    Workspace,
    ContextRequirement,
}

impl AgentSurfaceSemanticFacet {
    pub fn kind(&self) -> AgentSurfaceContributionKind {
        match self {
            Self::Instruction => AgentSurfaceContributionKind::Instruction,
            Self::Tool(_) => AgentSurfaceContributionKind::Tool,
            Self::Hook(_) => AgentSurfaceContributionKind::Hook,
            Self::Workspace => AgentSurfaceContributionKind::Workspace,
            Self::ContextRequirement => AgentSurfaceContributionKind::ContextRequirement,
        }
    }

    pub fn satisfies(&self, required: &Self) -> bool {
        match (self, required) {
            (Self::Instruction, Self::Instruction)
            | (Self::Workspace, Self::Workspace)
            | (Self::ContextRequirement, Self::ContextRequirement) => true,
            (Self::Tool(offered), Self::Tool(required)) => offered.satisfies(required),
            (Self::Hook(offered), Self::Hook(required)) => offered.satisfies(required),
            _ => false,
        }
    }

    pub fn required_causal_route(&self) -> Option<AgentSurfaceRoute> {
        match self {
            Self::Tool(tool) => Some(tool.delivery.route()),
            Self::Hook(hook) if hook.requires_reverse_callback() => {
                Some(AgentSurfaceRoute::AgentNativeCallback)
            }
            _ => None,
        }
    }

    pub fn matches_payload(&self, payload: &AgentSurfaceContributionPayload) -> bool {
        match (self, payload) {
            (Self::Instruction, AgentSurfaceContributionPayload::Instruction { .. })
            | (Self::Tool(_), AgentSurfaceContributionPayload::Tool { .. })
            | (Self::Workspace, AgentSurfaceContributionPayload::Workspace { .. })
            | (
                Self::ContextRequirement,
                AgentSurfaceContributionPayload::ContextRequirement { .. },
            ) => true,
            (
                Self::Hook(semantics),
                AgentSurfaceContributionPayload::Hook {
                    point,
                    timing,
                    actions,
                    ..
                },
            ) => {
                semantics.point == *point
                    && semantics.timing == *timing
                    && semantics.blocking.is_blocking()
                        == actions.contains(&AgentHookAction::AllowOrDeny)
                    && semantics
                        .mutations
                        .contains_key(&AgentHookMutationKind::RewriteInput)
                        == actions.contains(&AgentHookAction::RewriteInput)
                    && semantics
                        .mutations
                        .contains_key(&AgentHookMutationKind::RewriteResult)
                        == actions.contains(&AgentHookAction::RewriteResult)
                    && semantics
                        .mutations
                        .contains_key(&AgentHookMutationKind::AddContext)
                        == actions.contains(&AgentHookAction::AddContext)
                    && semantics
                        .effects
                        .contains_key(&AgentHookEffectKind::EmitEffect)
                        == actions.contains(&AgentHookAction::EmitEffect)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentSurfaceContributionPayload {
    Instruction {
        channel: String,
        text: String,
        presentation: agentdash_agent_protocol::AgentSurfaceInstructionPresentation,
    },
    Tool {
        name: AgentToolName,
        description: String,
        input_schema: Value,
        output_schema: Option<Value>,
        protocol_projector: agentdash_agent_protocol::ToolProtocolProjector,
    },
    Hook {
        definition_id: AgentHookDefinitionId,
        point: AgentHookPoint,
        timing: AgentHookTiming,
        actions: BTreeSet<AgentHookAction>,
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::AgentServiceU64")]
        #[ts(type = "AgentServiceU64")]
        deadline_ms: u64,
    },
    Workspace {
        requirement: String,
    },
    ContextRequirement {
        requirement: String,
    },
}

impl AgentSurfaceContributionPayload {
    pub fn kind(&self) -> AgentSurfaceContributionKind {
        match self {
            Self::Instruction { .. } => AgentSurfaceContributionKind::Instruction,
            Self::Tool { .. } => AgentSurfaceContributionKind::Tool,
            Self::Hook { .. } => AgentSurfaceContributionKind::Hook,
            Self::Workspace { .. } => AgentSurfaceContributionKind::Workspace,
            Self::ContextRequirement { .. } => AgentSurfaceContributionKind::ContextRequirement,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSurfaceRequirement {
    pub key: String,
    pub required: bool,
    pub minimum_fidelity: SemanticFidelity,
    pub allowed_routes: BTreeSet<AgentSurfaceRoute>,
    pub semantics: AgentSurfaceSemanticFacet,
    pub payload: AgentSurfaceContributionPayload,
    pub payload_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSurfaceSnapshot {
    pub revision: AgentSurfaceRevision,
    pub digest: AgentSurfaceDigest,
    pub requirements: Vec<AgentSurfaceRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSurfaceCapabilityFacet {
    pub semantics: AgentSurfaceSemanticFacet,
    pub routes: BTreeSet<AgentSurfaceRoute>,
    pub fidelity: SemanticFidelity,
    pub configuration_boundary: AgentConfigurationBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeOffer {
    pub profile_digest: crate::AgentProfileDigest,
    pub contributions: Vec<AgentSurfaceCapabilityFacet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct BoundAgentSurfaceContribution {
    pub key: String,
    pub required: bool,
    pub route: AgentSurfaceRoute,
    pub fidelity: SemanticFidelity,
    pub semantics: AgentSurfaceSemanticFacet,
    pub payload: AgentSurfaceContributionPayload,
    pub payload_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct BoundAgentSurface {
    pub revision: AgentSurfaceRevision,
    pub digest: AgentSurfaceDigest,
    pub offer_profile_digest: crate::AgentProfileDigest,
    pub contributions: Vec<BoundAgentSurfaceContribution>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AppliedContributionStatus {
    Applied,
    Rejected,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AppliedAgentSurfaceContribution {
    pub key: String,
    pub route: AgentSurfaceRoute,
    pub fidelity: SemanticFidelity,
    pub semantics: AgentSurfaceSemanticFacet,
    pub payload_digest: AgentPayloadDigest,
    pub status: AppliedContributionStatus,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AppliedAgentSurface {
    pub revision: AgentSurfaceRevision,
    pub digest: AgentSurfaceDigest,
    pub contributions: Vec<AppliedAgentSurfaceContribution>,
}

impl BoundAgentSurface {
    pub fn accepts_applied(&self, applied: &AppliedAgentSurface) -> bool {
        self.revision == applied.revision
            && self.digest == applied.digest
            && self.contributions.iter().all(|bound| {
                applied.contributions.iter().any(|evidence| {
                    evidence.key == bound.key
                        && evidence.route == bound.route
                        && evidence.payload_digest == bound.payload_digest
                        && evidence.fidelity.satisfies(bound.fidelity)
                        && evidence.semantics.satisfies(&bound.semantics)
                        && (!bound.required
                            || evidence.status == AppliedContributionStatus::Applied)
                })
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentHostCallbackBinding {
    pub route_id: AgentCallbackRouteId,
    pub binding_generation: AgentBindingGeneration,
    pub delivery: AgentSurfaceRoute,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
    pub default_deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ApplyBoundAgentSurface {
    pub command_id: crate::AgentCommandId,
    pub effect_id: crate::AgentEffectIdentity,
    pub idempotency_key: crate::AgentIdempotencyKey,
    pub source: crate::AgentSourceCoordinate,
    pub bound_surface: BoundAgentSurface,
    pub callbacks: AgentHostCallbackBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RevokeBoundAgentSurface {
    pub command_id: crate::AgentCommandId,
    pub effect_id: crate::AgentEffectIdentity,
    pub idempotency_key: crate::AgentIdempotencyKey,
    pub binding_generation: AgentBindingGeneration,
    pub source: crate::AgentSourceCoordinate,
    pub expected_revision: AgentSurfaceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AppliedAgentSurfaceReceipt {
    pub command_id: crate::AgentCommandId,
    pub effect_id: crate::AgentEffectIdentity,
    pub source: crate::AgentSourceCoordinate,
    pub applied: AppliedAgentSurface,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_required_contribution_rejects_observed_evidence() {
        let digest = AgentSurfaceDigest::new("surface").expect("digest");
        let payload_digest = AgentPayloadDigest::new("payload").expect("payload");
        let bound = BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: digest.clone(),
            offer_profile_digest: crate::AgentProfileDigest::new("profile").expect("profile"),
            contributions: vec![BoundAgentSurfaceContribution {
                key: "tool".to_owned(),
                required: true,
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Workspace,
                payload: AgentSurfaceContributionPayload::Workspace {
                    requirement: "root".to_owned(),
                },
                payload_digest: payload_digest.clone(),
            }],
        };
        let applied = AppliedAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest,
            contributions: vec![AppliedAgentSurfaceContribution {
                key: "tool".to_owned(),
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Observed,
                semantics: AgentSurfaceSemanticFacet::Workspace,
                payload_digest,
                status: AppliedContributionStatus::Applied,
                evidence: None,
            }],
        };
        assert!(!bound.accepts_applied(&applied));
    }

    #[test]
    fn route_mismatch_is_not_applied_evidence() {
        let digest = AgentSurfaceDigest::new("surface").expect("digest");
        let payload_digest = AgentPayloadDigest::new("payload").expect("payload");
        let bound = BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: digest.clone(),
            offer_profile_digest: crate::AgentProfileDigest::new("profile").expect("profile"),
            contributions: vec![BoundAgentSurfaceContribution {
                key: "hook".to_owned(),
                required: true,
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Workspace,
                payload: AgentSurfaceContributionPayload::Workspace {
                    requirement: "root".to_owned(),
                },
                payload_digest: payload_digest.clone(),
            }],
        };
        let applied = AppliedAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest,
            contributions: vec![AppliedAgentSurfaceContribution {
                key: "hook".to_owned(),
                route: AgentSurfaceRoute::ObservationOnly,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Workspace,
                payload_digest,
                status: AppliedContributionStatus::Applied,
                evidence: None,
            }],
        };
        assert!(!bound.accepts_applied(&applied));
    }

    #[test]
    fn tool_delivery_is_not_interchangeable_at_equal_fidelity() {
        let required = AgentToolSemanticFacet {
            delivery: AgentToolDelivery::RuntimeBrokerCallback,
            invocation: SemanticFidelity::Exact,
            update: AgentToolUpdateSemantics::BindingOnly,
        };
        let offered = AgentToolSemanticFacet {
            delivery: AgentToolDelivery::AgentNativeCallback,
            invocation: SemanticFidelity::Exact,
            update: AgentToolUpdateSemantics::HotUpdate,
        };

        assert!(!offered.satisfies(&required));
    }

    #[test]
    fn hook_semantics_compare_point_timing_blocking_mutation_and_effect() {
        let required = AgentHookSemanticFacet {
            point: AgentHookPoint::BeforeTool,
            timing: AgentHookTiming::Before,
            blocking: AgentHookBlockingSemantics::Blocking {
                fidelity: SemanticFidelity::Exact,
            },
            mutations: BTreeMap::from([(
                AgentHookMutationKind::RewriteInput,
                SemanticFidelity::Exact,
            )]),
            effects: BTreeMap::from([(AgentHookEffectKind::EmitEffect, SemanticFidelity::Exact)]),
        };
        let mut offered = required.clone();
        offered
            .effects
            .insert(AgentHookEffectKind::EmitEffect, SemanticFidelity::Observed);
        assert!(!offered.satisfies(&required));

        offered
            .effects
            .insert(AgentHookEffectKind::EmitEffect, SemanticFidelity::Exact);
        assert!(offered.satisfies(&required));

        offered.point = AgentHookPoint::AfterTool;
        assert!(!offered.satisfies(&required));
    }
}
