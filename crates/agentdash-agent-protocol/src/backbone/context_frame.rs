use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// AgentDash-owned model-context delivery payload.
///
/// A concrete Agent accepts these frames, consumes `rendered_text` according to the delivery
/// status, and publishes the same value for history, live presentation, and audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextFrame {
    pub id: String,
    pub kind: ContextFrameKind,
    pub delivery_status: ContextDeliveryStatus,
    pub rendered_text: String,
    pub sections: Vec<ContextFrameSection>,
    #[ts(type = "number")]
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextFrameKind {
    Identity,
    UserContext,
    Environment,
    SystemGuidelines,
    AssignmentContext,
    CapabilityStateDelta,
    MemoryContext,
    CompactionSummary,
}

impl ContextFrameKind {
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::UserContext => "user_context",
            Self::Environment => "environment",
            Self::SystemGuidelines => "system_guidelines",
            Self::AssignmentContext => "assignment_context",
            Self::CapabilityStateDelta => "capability_state_delta",
            Self::MemoryContext => "memory_context",
            Self::CompactionSummary => "compaction_summary",
        }
    }

    #[must_use]
    pub const fn frontend_label(self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::UserContext => "User Context",
            Self::Environment => "Environment",
            Self::SystemGuidelines => "System Guidelines",
            Self::AssignmentContext => "Assignment Context",
            Self::CapabilityStateDelta => "Capability State Delta",
            Self::MemoryContext => "Memory Context",
            Self::CompactionSummary => "Compaction Summary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryStatus {
    AppliedBeforePrompt,
    AppliedToCompactedContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextFrameSection {
    ContextFragments {
        fragments: Vec<RuntimeContextFragmentEntry>,
    },
    CapabilityKeyDelta {
        added_capabilities: Vec<String>,
        removed_capabilities: Vec<String>,
        effective_capabilities: Vec<String>,
    },
    ToolPathDelta {
        blocked_tool_paths: Vec<String>,
        unblocked_tool_paths: Vec<String>,
        whitelisted_tool_paths: Vec<String>,
        removed_whitelist_paths: Vec<String>,
    },
    McpServerDelta {
        added_mcp_servers: Vec<String>,
        removed_mcp_servers: Vec<String>,
        changed_mcp_servers: Vec<String>,
    },
    VfsDelta {
        vfs_mounts_added: Vec<String>,
        vfs_mounts_removed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_mount_after: Option<String>,
    },
    ToolSchemaDelta {
        added_tools: Vec<RuntimeToolSchemaEntry>,
        removed_tools: Vec<String>,
        changed_tools: Vec<RuntimeToolSchemaEntry>,
    },
    SkillDelta {
        added_skills: Vec<RuntimeSkillEntry>,
        removed_skills: Vec<RuntimeSkillEntry>,
        changed_skills: Vec<RuntimeSkillEntry>,
    },
    MemoryInventory {
        title: String,
        summary: String,
        mode: RuntimeMemoryInventoryMode,
        sources: Vec<RuntimeMemorySourceEntry>,
        diagnostics: Vec<RuntimeMemoryDiagnosticEntry>,
        added_sources: Vec<RuntimeMemorySourceEntry>,
        removed_sources: Vec<RuntimeMemorySourceEntry>,
        changed_sources: Vec<RuntimeMemorySourceEntry>,
    },
    CompanionAgentRosterDelta {
        added_agents: Vec<RuntimeCompanionAgentEntry>,
        removed_agent_keys: Vec<String>,
        changed_agents: Vec<RuntimeCompanionAgentEntry>,
        effective_agents: Vec<RuntimeCompanionAgentEntry>,
    },
    SystemNotice {
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    CompactionSummary {
        title: String,
        summary: String,
        #[ts(type = "number")]
        tokens_before: u64,
        messages_compacted: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compaction_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null")]
        projection_version: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strategy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trigger: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null")]
        source_start_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null")]
        source_end_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null")]
        first_kept_event_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compacted_until_ref: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null")]
        timestamp_ms: Option<u64>,
    },
}

macro_rules! string_entry { ($name:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)] #[serde(rename_all = "snake_case")] pub struct $name { $($(#[$meta])* pub $field: $ty,)* } }; }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
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
string_entry!(RuntimeContextFragmentEntry { slot: String, label: String, source: String, content: String, #[serde(default, skip_serializing_if = "Option::is_none")] context_usage_kind: Option<String> });
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSkillEntry {
    pub name: String,
    pub capability_key: String,
    pub provider_key: String,
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    pub exposure: SkillContextExposure,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextFrameChanged {
    pub frame: ContextFrame,
}
