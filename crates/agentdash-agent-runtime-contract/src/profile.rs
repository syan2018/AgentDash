use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub fn runtime_profile_digest(profile: &RuntimeProfile) -> crate::ProfileDigest {
    use sha2::{Digest, Sha256};

    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                serde_json::Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key.clone(), canonicalize(value)))
                        .collect(),
                )
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(canonicalize).collect())
            }
            other => other.clone(),
        }
    }

    let value = serde_json::to_value(profile).expect("RuntimeProfile serialization cannot fail");
    let bytes = serde_json::to_vec(&canonicalize(&value))
        .expect("RuntimeProfile canonical serialization cannot fail");
    crate::ProfileDigest::new(format!("sha256:{:x}", Sha256::digest(bytes)))
        .expect("RuntimeProfile digest is non-empty")
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceRuntimeClass {
    Turn,
    Conversation,
    Interactive,
    ManagedThread,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMechanism {
    Native,
    HostAdaptedExact,
    HostAdaptedBoundary,
    Observed,
    PromptOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticStrength {
    ObservedOnly,
    BoundaryAdapted,
    ExactDurableBoundary,
    ExactSynchronous,
}

impl SemanticStrength {
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationBoundary {
    StaticService,
    Binding,
    ThreadStart,
    TurnStart,
    HotReplace,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextFidelity {
    Opaque,
    EventProjected,
    AgentReplay,
    DriverExact,
    PlatformExact,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum InputModality {
    Text,
    Image,
    Audio,
    FileReference,
    Resource,
    Skill,
    Mention,
    Structured,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum InstructionChannel {
    System,
    Developer,
    AdditionalContext,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolChannel {
    DirectCallback,
    McpFacade,
    DriverNative,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCapability {
    Read,
    Write,
    Search,
    MultipleRoots,
    VirtualFileSystem,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCapability {
    ThreadStart,
    ThreadResume,
    ThreadFork,
    ThreadRead,
    TurnStart,
    TurnSteer,
    TurnInterrupt,
    ToolSetReplace,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextCapability {
    Read,
    Export,
    Import,
    PrepareCompaction,
    ActivateCheckpoint,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryCapability {
    Usage,
    Reasoning,
    Deltas,
    Diagnostics,
    ConfigurationEvidence,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    BeforeThreadStart,
    AfterThreadStart,
    BeforeTurn,
    AfterTurn,
    BeforeProviderRequest,
    BeforeTool,
    AfterTool,
    BeforeContextCompact,
    AfterContextCompact,
    BeforeStop,
    AfterItem,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    Observe,
    AddContext,
    Block,
    RewriteInput,
    RewriteResult,
    RequestApproval,
    ContinueTurn,
    RefreshSurface,
    EmitEffect,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    FailClosed,
    FailOpenWithDiagnostic,
    RetryDurableEffect,
    ObserveOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookRequirement {
    pub point: HookPoint,
    pub actions: BTreeSet<HookAction>,
    pub minimum_strength: SemanticStrength,
    pub failure_policy: HookFailurePolicy,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookPointCapability {
    pub point: HookPoint,
    pub actions: BTreeSet<HookAction>,
    pub strength: SemanticStrength,
    pub mechanism: DeliveryMechanism,
    pub failure_policies: BTreeSet<HookFailurePolicy>,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct HookProfile {
    pub points: Vec<HookPointCapability>,
    pub configuration_boundary: ConfigurationBoundary,
}

impl HookProfile {
    pub fn satisfies(&self, requirement: &HookRequirement) -> bool {
        self.points.iter().any(|capability| {
            capability.point == requirement.point
                && capability.acknowledged
                && capability.strength.satisfies(requirement.minimum_strength)
                && requirement.actions.is_subset(&capability.actions)
                && capability
                    .failure_policies
                    .contains(&requirement.failure_policy)
                && !matches!(capability.mechanism, DeliveryMechanism::PromptOnly)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct InputProfile {
    pub modalities: BTreeSet<InputModality>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct InstructionProfile {
    pub channels: BTreeSet<InstructionChannel>,
    pub configuration_boundary: ConfigurationBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ToolProfile {
    pub channels: BTreeSet<ToolChannel>,
    pub configuration_boundary: ConfigurationBoundary,
    pub cancellation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceProfile {
    pub capabilities: BTreeSet<WorkspaceCapability>,
    pub mechanism: DeliveryMechanism,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct InteractionProfile {
    pub kinds: BTreeSet<crate::RuntimeInteractionKind>,
    pub durable_correlation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextProfile {
    pub capabilities: BTreeSet<ContextCapability>,
    pub fidelity: ContextFidelity,
    pub activation_idempotent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeProfile {
    pub reference_class: ReferenceRuntimeClass,
    pub input: InputProfile,
    pub instruction: InstructionProfile,
    pub tools: ToolProfile,
    pub workspace: WorkspaceProfile,
    pub interactions: InteractionProfile,
    pub lifecycle: BTreeSet<LifecycleCapability>,
    pub hooks: HookProfile,
    pub context: ContextProfile,
    pub telemetry_config: BTreeSet<TelemetryCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLayer {
    Service,
    Transport,
    HostPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ProfileProvenance {
    pub service_digest: crate::ProfileDigest,
    pub transport_digest: crate::ProfileDigest,
    pub host_policy_digest: crate::ProfileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct EffectiveRuntimeProfile {
    pub profile: RuntimeProfile,
    pub provenance: ProfileProvenance,
}

pub fn intersect_profile_layers(
    service: &RuntimeProfile,
    transport: &RuntimeProfile,
    host_policy: &RuntimeProfile,
    provenance: ProfileProvenance,
) -> EffectiveRuntimeProfile {
    EffectiveRuntimeProfile {
        profile: service.intersect(transport).intersect(host_policy),
        provenance,
    }
}

impl RuntimeProfile {
    /// Computes the guarantees common to a service, placement transport, and host policy.
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            reference_class: self.reference_class.min(other.reference_class),
            input: InputProfile {
                modalities: intersection(&self.input.modalities, &other.input.modalities),
            },
            instruction: InstructionProfile {
                channels: intersection(&self.instruction.channels, &other.instruction.channels),
                configuration_boundary: self
                    .instruction
                    .configuration_boundary
                    .min(other.instruction.configuration_boundary),
            },
            tools: ToolProfile {
                channels: intersection(&self.tools.channels, &other.tools.channels),
                configuration_boundary: self
                    .tools
                    .configuration_boundary
                    .min(other.tools.configuration_boundary),
                cancellation: self.tools.cancellation && other.tools.cancellation,
            },
            workspace: WorkspaceProfile {
                capabilities: intersection(
                    &self.workspace.capabilities,
                    &other.workspace.capabilities,
                ),
                mechanism: self.workspace.mechanism.max(other.workspace.mechanism),
            },
            interactions: InteractionProfile {
                kinds: intersection(&self.interactions.kinds, &other.interactions.kinds),
                durable_correlation: self.interactions.durable_correlation
                    && other.interactions.durable_correlation,
            },
            lifecycle: intersection(&self.lifecycle, &other.lifecycle),
            hooks: intersect_hooks(&self.hooks, &other.hooks),
            context: ContextProfile {
                capabilities: intersection(&self.context.capabilities, &other.context.capabilities),
                fidelity: self.context.fidelity.min(other.context.fidelity),
                activation_idempotent: self.context.activation_idempotent
                    && other.context.activation_idempotent,
            },
            telemetry_config: intersection(&self.telemetry_config, &other.telemetry_config),
        }
    }
}

fn intersection<T: Ord + Clone>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> BTreeSet<T> {
    left.intersection(right).cloned().collect()
}

fn intersect_hooks(left: &HookProfile, right: &HookProfile) -> HookProfile {
    let mut points = Vec::new();
    for left_point in &left.points {
        if let Some(right_point) = right
            .points
            .iter()
            .find(|item| item.point == left_point.point)
        {
            points.push(HookPointCapability {
                point: left_point.point,
                actions: intersection(&left_point.actions, &right_point.actions),
                strength: left_point.strength.min(right_point.strength),
                mechanism: left_point.mechanism.max(right_point.mechanism),
                failure_policies: intersection(
                    &left_point.failure_policies,
                    &right_point.failure_policies,
                ),
                acknowledged: left_point.acknowledged && right_point.acknowledged,
            });
        }
    }
    HookProfile {
        points,
        configuration_boundary: left
            .configuration_boundary
            .min(right.configuration_boundary),
    }
}
