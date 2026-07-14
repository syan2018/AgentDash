use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// AgentDash-owned context presentation payload.
///
/// This is a presentation/audit projection of the materialized agent surface. It is not the
/// model context itself and must never be used as the execution adapter's input contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextFrame {
    pub id: String,
    pub kind: ContextFrameKind,
    pub source: ContextFrameSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_mode: Option<String>,
    pub delivery_status: ContextDeliveryStatus,
    pub delivery_channel: ContextDeliveryChannel,
    pub message_role: ContextMessageRole,
    #[serde(default)]
    pub delivery_metadata: ContextDeliveryMetadata,
    pub rendered_text: String,
    #[serde(default)]
    pub sections: Vec<ContextFrameSection>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextFrameKind {
    Identity,
    UserContext,
    Environment,
    SystemGuidelines,
    CompactionSummary,
    AssignmentContext,
    CapabilityStateDelta,
    MemoryContext,
    PendingAction,
    AutoResume,
}

impl ContextFrameKind {
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::UserContext => "user_context",
            Self::Environment => "environment",
            Self::SystemGuidelines => "system_guidelines",
            Self::CompactionSummary => "compaction_summary",
            Self::AssignmentContext => "assignment_context",
            Self::CapabilityStateDelta => "capability_state_delta",
            Self::MemoryContext => "memory_context",
            Self::PendingAction => "pending_action",
            Self::AutoResume => "auto_resume",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextFrameSource {
    RuntimeContextUpdate,
    CompanionResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryStatus {
    Accepted,
    PreparedForConnector,
    QueuedForTransformContext,
    AppliedBeforePrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryChannel {
    TurnStart,
    ConnectorContext,
    TransformContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextMessageRole {
    System,
    Developer,
    Context,
    User,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextDeliveryMetadata {
    pub delivery_phase: ContextDeliveryPhase,
    pub delivery_order: u32,
    pub cache_policy: ContextCachePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_revision: Option<String>,
    pub model_channel: ContextModelChannel,
    pub agent_consumption: ContextAgentConsumption,
    pub frontend_label: String,
    #[serde(default)]
    pub connector_profile: ContextConnectorProfile,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextDeliveryPlan {
    pub plan_id: String,
    pub target_agent: ContextDeliveryTarget,
    #[serde(default)]
    pub entries: Vec<ContextDeliveryEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextDeliveryTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub profile: ContextConnectorProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextDeliveryEntry {
    pub frame_id: String,
    pub frame_kind: ContextFrameKind,
    pub delivery_phase: ContextDeliveryPhase,
    pub delivery_order: u32,
    pub cache_policy: ContextCachePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_revision: Option<String>,
    pub model_channel: ContextModelChannel,
    pub agent_consumption: ContextAgentConsumption,
    pub frontend_label: String,
    #[serde(default)]
    pub connector_profile: ContextConnectorProfile,
}

impl ContextFrame {
    #[must_use]
    pub fn delivery_entry(&self) -> ContextDeliveryEntry {
        ContextDeliveryEntry {
            frame_id: self.id.clone(),
            frame_kind: self.kind,
            delivery_phase: self.delivery_metadata.delivery_phase,
            delivery_order: self.delivery_metadata.delivery_order,
            cache_policy: self.delivery_metadata.cache_policy,
            cache_key: self.delivery_metadata.cache_key.clone(),
            cache_revision: self.delivery_metadata.cache_revision.clone(),
            model_channel: self.delivery_metadata.model_channel,
            agent_consumption: self.delivery_metadata.agent_consumption.clone(),
            frontend_label: self.delivery_metadata.frontend_label.clone(),
            connector_profile: self.delivery_metadata.connector_profile.clone(),
        }
    }
}

impl ContextDeliveryMetadata {
    #[must_use]
    pub fn for_frame(
        kind: ContextFrameKind,
        delivery_channel: ContextDeliveryChannel,
        message_role: ContextMessageRole,
    ) -> Self {
        let (delivery_phase, delivery_order, cache_policy, frontend_label) = match kind {
            ContextFrameKind::Identity => (
                ContextDeliveryPhase::StableSystem,
                10,
                ContextCachePolicy::Static,
                "Identity",
            ),
            ContextFrameKind::UserContext => (
                ContextDeliveryPhase::StableSystem,
                12,
                ContextCachePolicy::Static,
                "User Context",
            ),
            ContextFrameKind::Environment => (
                ContextDeliveryPhase::SessionPolicy,
                15,
                ContextCachePolicy::SessionDigest,
                "Environment",
            ),
            ContextFrameKind::SystemGuidelines => (
                ContextDeliveryPhase::SessionPolicy,
                20,
                ContextCachePolicy::SessionDigest,
                "System Guidelines",
            ),
            ContextFrameKind::CompactionSummary => (
                ContextDeliveryPhase::RunState,
                30,
                ContextCachePolicy::RuntimeStateDigest,
                "Compaction Summary",
            ),
            ContextFrameKind::AssignmentContext => (
                ContextDeliveryPhase::Assignment,
                40,
                ContextCachePolicy::AssignmentRevision,
                "Assignment Context",
            ),
            ContextFrameKind::CapabilityStateDelta => (
                ContextDeliveryPhase::DiscoveredInventory,
                50,
                ContextCachePolicy::DiscoveryDigest,
                "Capability State Delta",
            ),
            ContextFrameKind::MemoryContext => (
                ContextDeliveryPhase::DiscoveredInventory,
                60,
                ContextCachePolicy::DiscoveryDigest,
                "Memory Context",
            ),
            ContextFrameKind::PendingAction => (
                ContextDeliveryPhase::TurnRuntime,
                70,
                ContextCachePolicy::TurnEphemeral,
                "Pending Action",
            ),
            ContextFrameKind::AutoResume => (
                ContextDeliveryPhase::TurnRuntime,
                80,
                ContextCachePolicy::TurnEphemeral,
                "Auto Resume",
            ),
        };
        let model_channel = match kind {
            ContextFrameKind::Identity
            | ContextFrameKind::UserContext
            | ContextFrameKind::Environment
            | ContextFrameKind::SystemGuidelines => ContextModelChannel::System,
            ContextFrameKind::AutoResume | ContextFrameKind::PendingAction => {
                ContextModelChannel::User
            }
            ContextFrameKind::MemoryContext
            | ContextFrameKind::CompactionSummary
            | ContextFrameKind::AssignmentContext => ContextModelChannel::Context,
            ContextFrameKind::CapabilityStateDelta => match message_role {
                ContextMessageRole::System => ContextModelChannel::System,
                ContextMessageRole::Developer => ContextModelChannel::Developer,
                ContextMessageRole::User => ContextModelChannel::User,
                ContextMessageRole::Context => ContextModelChannel::Context,
            },
        };
        let _ = delivery_channel;
        Self {
            delivery_phase,
            delivery_order,
            cache_policy,
            model_channel,
            frontend_label: frontend_label.to_string(),
            agent_consumption: ContextAgentConsumption {
                reason: format!("default_{}_delivery", kind.as_key()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryPhase {
    StableSystem,
    SessionPolicy,
    RunState,
    Assignment,
    DiscoveredInventory,
    #[default]
    TurnRuntime,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextCachePolicy {
    Static,
    SessionDigest,
    RuntimeStateDigest,
    AssignmentRevision,
    DiscoveryDigest,
    TurnEphemeral,
    #[default]
    Uncached,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextModelChannel {
    System,
    Developer,
    #[default]
    Context,
    User,
    AuditOnly,
    Ignored,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextAgentConsumption {
    #[serde(default)]
    pub target: String,
    pub mode: ContextAgentConsumptionMode,
    #[serde(default)]
    pub reason: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextAgentConsumptionMode {
    #[default]
    Consume,
    AuditOnly,
    Ignore,
    ConnectorNative,
    SystemAppend,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextConnectorProfile {
    #[serde(default)]
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared_consumption_modes: Vec<ContextAgentConsumptionMode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextFrameSection {
    Identity {
        title: String,
        summary: String,
        #[serde(default)]
        fragments: Vec<RuntimeContextFragmentEntry>,
    },
    AssignmentContext {
        title: String,
        summary: String,
        #[serde(default)]
        fragments: Vec<RuntimeContextFragmentEntry>,
    },
    CapabilityKeyDelta {
        #[serde(default)]
        added_capabilities: Vec<String>,
        #[serde(default)]
        removed_capabilities: Vec<String>,
        #[serde(default)]
        effective_capabilities: Vec<String>,
    },
    ToolPathDelta {
        #[serde(default)]
        blocked_tool_paths: Vec<String>,
        #[serde(default)]
        unblocked_tool_paths: Vec<String>,
        #[serde(default)]
        whitelisted_tool_paths: Vec<String>,
        #[serde(default)]
        removed_whitelist_paths: Vec<String>,
    },
    McpServerDelta {
        #[serde(default)]
        added_mcp_servers: Vec<String>,
        #[serde(default)]
        removed_mcp_servers: Vec<String>,
        #[serde(default)]
        changed_mcp_servers: Vec<String>,
    },
    VfsDelta {
        #[serde(default)]
        vfs_mounts_added: Vec<String>,
        #[serde(default)]
        vfs_mounts_removed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_after: Option<String>,
    },
    ToolSchemaDelta {
        #[serde(default)]
        added_tools: Vec<RuntimeToolSchemaEntry>,
    },
    SkillDelta {
        #[serde(default)]
        added_skills: Vec<RuntimeSkillEntry>,
        #[serde(default)]
        removed_skills: Vec<RuntimeSkillEntry>,
        #[serde(default)]
        changed_skills: Vec<RuntimeSkillEntry>,
    },
    MemoryInventory {
        title: String,
        summary: String,
        mode: RuntimeMemoryInventoryMode,
        #[serde(default)]
        sources: Vec<RuntimeMemorySourceEntry>,
        #[serde(default)]
        diagnostics: Vec<RuntimeMemoryDiagnosticEntry>,
        #[serde(default)]
        added_sources: Vec<RuntimeMemorySourceEntry>,
        #[serde(default)]
        removed_sources: Vec<RuntimeMemorySourceEntry>,
        #[serde(default)]
        changed_sources: Vec<RuntimeMemorySourceEntry>,
    },
    CompanionAgentRosterDelta {
        #[serde(default)]
        added_agents: Vec<RuntimeCompanionAgentEntry>,
        #[serde(default)]
        removed_agent_keys: Vec<String>,
        #[serde(default)]
        changed_agents: Vec<RuntimeCompanionAgentEntry>,
        #[serde(default)]
        effective_agents: Vec<RuntimeCompanionAgentEntry>,
    },
    SystemNotice {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    PendingAction {
        title: String,
        summary: String,
        action_id: String,
        action_type: String,
        status: String,
        revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        #[serde(default)]
        instructions: Vec<String>,
        #[serde(default)]
        injections: Vec<RuntimeHookInjectionEntry>,
    },
    AutoResume {
        title: String,
        summary: String,
        reason: String,
        prompt: String,
    },
    CompactionSummary {
        title: String,
        summary: String,
        tokens_before: u64,
        messages_compacted: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compaction_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projection_version: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strategy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trigger: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_start_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_end_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first_kept_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compacted_until_ref: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    Environment {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        date: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        platform: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        executor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
    UserPreferences {
        title: String,
        summary: String,
        #[serde(default)]
        items: Vec<String>,
    },
    ProjectGuidelines {
        title: String,
        summary: String,
        #[serde(default)]
        entries: Vec<ProjectGuidelineEntry>,
    },
    UserContext {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default)]
        groups: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        extra: serde_json::Value,
    },
}

macro_rules! string_entry { ($name:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)] #[serde(rename_all = "snake_case")] pub struct $name { $($(#[$meta])* pub $field: $ty,)* } }; }
string_entry!(ProjectGuidelineEntry {
    path: String,
    content: String
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeToolSchemaEntry {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}
string_entry!(RuntimeHookInjectionEntry { slot: String, source: String, content: String, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });
string_entry!(RuntimeContextFragmentEntry { slot: String, label: String, source: String, content: String, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSkillEntry {
    pub name: String,
    #[serde(default)]
    pub capability_key: String,
    #[serde(default)]
    pub provider_key: String,
    #[serde(default)]
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub exposure: SkillContextExposure,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage_kind: Option<String>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkillContextExposure {
    #[default]
    DefaultExposed,
    ExplicitOnly,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMemoryInventoryMode {
    #[default]
    Snapshot,
    Delta,
}
string_entry!(RuntimeMemorySourceEntry { provider_key: String, source_key: String, display_name: String, source_uri: String, index_uri: String, mount_id: String, scope: String, index_status: String, trust_level: String, revision: String, #[serde(default, skip_serializing_if = "Option::is_none")] summary: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });
string_entry!(RuntimeMemoryDiagnosticEntry { provider_key: String, code: String, message: String, #[serde(default, skip_serializing_if = "Option::is_none")] source_key: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] uri: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });
string_entry!(RuntimeCompanionAgentEntry { agent_key: String, executor: String, display_name: String, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextFrameChanged {
    pub frame: ContextFrame,
}
