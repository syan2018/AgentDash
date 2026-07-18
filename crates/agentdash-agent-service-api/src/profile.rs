use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AgentProfileDigest, InitialContextContributionKind};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticFidelity {
    Unsupported,
    Observed,
    Approximation,
    Exact,
}

impl SemanticFidelity {
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfigurationBoundary {
    StaticService,
    Binding,
    Create,
    Turn,
    HotUpdate,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleCapability {
    Create,
    Start,
    Resume,
    Close,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommandCapability {
    SubmitInput,
    Steer,
    Interrupt,
    RequestCompaction,
    ResolveInteraction,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentForkCutoffKind {
    Head,
    CompletedTurn,
    Item,
    SourceCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentForkCapability {
    pub cutoffs: BTreeMap<AgentForkCutoffKind, SemanticFidelity>,
    pub lineage_fidelity: SemanticFidelity,
    pub native_durability: SemanticFidelity,
}

impl AgentForkCapability {
    pub fn supports_exact(&self, cutoff: AgentForkCutoffKind) -> bool {
        self.cutoffs
            .get(&cutoff)
            .is_some_and(|fidelity| fidelity.satisfies(SemanticFidelity::Exact))
            && self.lineage_fidelity.satisfies(SemanticFidelity::Exact)
            && self.native_durability.satisfies(SemanticFidelity::Exact)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentCompactionMode {
    AgentOwnedNative,
    ExactContextRevision,
    ObservedOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentSourceChangeLevel {
    OrderedDurableTail,
    OrderedLiveStream,
    SnapshotOnly,
    ObservationOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextDeliveryFidelity {
    Unsupported,
    CanonicalRendered,
    TypedNative,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextAppliedEvidence {
    Unsupported,
    PackageDigest,
    PackageAndMaterializedDigest,
}

impl InitialContextAppliedEvidence {
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

impl InitialContextDeliveryFidelity {
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InitialContextProfile {
    pub contribution_fidelity:
        BTreeMap<InitialContextContributionKind, InitialContextDeliveryFidelity>,
    pub applied_evidence: InitialContextAppliedEvidence,
    pub renderer_versions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentSurfaceProfile {
    pub facets: Vec<crate::AgentSurfaceCapabilityFacet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCapabilityProfile {
    pub lifecycle: BTreeSet<AgentLifecycleCapability>,
    pub commands: BTreeSet<AgentCommandCapability>,
    pub fork: AgentForkCapability,
    pub compaction: BTreeMap<AgentCompactionMode, SemanticFidelity>,
    pub source_changes: AgentSourceChangeLevel,
    pub initial_context: InitialContextProfile,
    pub surface: AgentSurfaceProfile,
    pub inspect_effects: SemanticFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceDescriptor {
    pub definition_id: crate::AgentServiceDefinitionId,
    pub title: String,
    pub protocol_revision: u32,
    pub profile: AgentCapabilityProfile,
    pub profile_digest: AgentProfileDigest,
    pub configuration_boundary: AgentConfigurationBoundary,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weaker_fidelity_never_satisfies_exact() {
        for fidelity in [
            SemanticFidelity::Unsupported,
            SemanticFidelity::Observed,
            SemanticFidelity::Approximation,
        ] {
            assert!(!fidelity.satisfies(SemanticFidelity::Exact));
        }
    }

    #[test]
    fn rendered_context_does_not_satisfy_typed_native() {
        assert!(
            !InitialContextDeliveryFidelity::CanonicalRendered
                .satisfies(InitialContextDeliveryFidelity::TypedNative)
        );
    }

    #[test]
    fn context_evidence_is_a_typed_guarantee() {
        assert!(
            InitialContextAppliedEvidence::PackageAndMaterializedDigest
                .satisfies(InitialContextAppliedEvidence::PackageDigest)
        );
        assert!(
            !InitialContextAppliedEvidence::Unsupported
                .satisfies(InitialContextAppliedEvidence::PackageDigest)
        );
    }
}
