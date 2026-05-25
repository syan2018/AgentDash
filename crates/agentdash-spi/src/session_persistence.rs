use std::collections::BTreeSet;
use std::io;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_domain::session_binding::StorySessionId;
use agentdash_domain::workflow::MountDirective;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::context::capability::CompanionAgentEntry;
use crate::{
    AgentConfig, SessionMcpServer, ToolCapability, ToolCapabilityFilter, ToolCluster, Vfs,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCapabilityStateTransition {
    pub id: String,
    pub run_id: Uuid,
    pub lifecycle_key: String,
    pub phase_node: String,
    pub capability_keys: BTreeSet<String>,
    pub transition: RuntimeCapabilityTransition,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityDimensionKey(pub String);

impl CapabilityDimensionKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityArtifactSource {
    pub kind: String,
}

impl CapabilityArtifactSource {
    pub fn workflow() -> Self {
        Self {
            kind: "workflow".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDeclarationRecord {
    pub dimension: CapabilityDimensionKey,
    pub declaration_type: String,
    pub source: CapabilityArtifactSource,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityContributionRecord {
    pub dimension: CapabilityDimensionKey,
    pub contribution_type: String,
    pub source: CapabilityArtifactSource,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilityEffectRecord {
    pub dimension: CapabilityDimensionKey,
    pub effect_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilityTransition {
    #[serde(default)]
    pub declarations: Vec<CapabilityDeclarationRecord>,
    #[serde(default)]
    pub effects: Vec<RuntimeCapabilityEffectRecord>,
}

pub const CAPABILITY_DIMENSION_TOOL: &str = "tool";
pub const CAPABILITY_DIMENSION_MCP: &str = "mcp";
pub const CAPABILITY_DIMENSION_COMPANION: &str = "companion";
pub const CAPABILITY_DIMENSION_VFS: &str = "vfs";

pub const DECLARATION_TYPE_CAPABILITY_DIRECTIVE: &str = "capability_directive";
pub const DECLARATION_TYPE_MOUNT_OPERATION: &str = "mount_operation";

pub const EFFECT_TYPE_SET_TOOL_ACCESS: &str = "set_tool_access";
pub const EFFECT_TYPE_SET_MCP_SERVER_SET: &str = "set_server_set";
pub const EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER: &str = "set_agent_roster";
pub const EFFECT_TYPE_APPLY_VFS_OVERLAY: &str = "apply_vfs_overlay";
pub const EFFECT_TYPE_APPLY_MOUNT_OPERATIONS: &str = "apply_mount_operations";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToolAccessEffect {
    pub capabilities: BTreeSet<ToolCapability>,
    pub enabled_clusters: BTreeSet<ToolCluster>,
    pub tool_policy: std::collections::BTreeMap<String, ToolCapabilityFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpServerSetEffect {
    pub servers: Vec<SessionMcpServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCompanionAgentRosterEffect {
    pub agents: Vec<CompanionAgentEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyVfsOverlayEffect {
    pub overlay: Vfs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyMountOperationsEffect {
    pub operations: Vec<MountDirective>,
}

impl RuntimeCapabilityTransition {
    pub fn from_records(
        declarations: Vec<CapabilityDeclarationRecord>,
        effects: Vec<RuntimeCapabilityEffectRecord>,
    ) -> Self {
        Self {
            declarations,
            effects,
        }
    }
}

impl CapabilityDeclarationRecord {
    pub fn typed(
        dimension: &str,
        declaration_type: &str,
        source: CapabilityArtifactSource,
        payload: &impl Serialize,
    ) -> Result<Self, String> {
        Ok(Self {
            dimension: CapabilityDimensionKey::new(dimension),
            declaration_type: declaration_type.to_string(),
            source,
            payload: serde_json::to_value(payload).map_err(|error| {
                format!(
                    "{dimension}.{declaration_type} declaration payload serialize failed: {error}"
                )
            })?,
        })
    }
}

impl RuntimeCapabilityEffectRecord {
    pub fn typed(
        dimension: &str,
        effect_type: &str,
        payload: &impl Serialize,
    ) -> Result<Self, String> {
        Ok(Self {
            dimension: CapabilityDimensionKey::new(dimension),
            effect_type: effect_type.to_string(),
            payload: serde_json::to_value(payload).map_err(|error| {
                format!("{dimension}.{effect_type} payload serialize failed: {error}")
            })?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    #[default]
    Auto,
    Source,
    User,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBootstrapState {
    #[default]
    Plain,
    Pending,
    Bootstrapped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSessionContext {
    pub dispatch_id: String,
    pub parent_session_id: StorySessionId,
    pub parent_turn_id: String,
    pub companion_label: String,
    pub slice_mode: String,
    pub adoption_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
    #[serde(default)]
    pub inherited_fragment_labels: Vec<String>,
    #[serde(default)]
    pub inherited_constraint_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_source: TitleSource,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub last_event_seq: u64,
    #[serde(default)]
    pub last_execution_status: ExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_terminal_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<AgentConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_context: Option<CompanionSessionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_layout: Option<serde_json::Value>,
    #[serde(default)]
    pub visible_canvas_mount_ids: Vec<String>,
    #[serde(default)]
    pub bootstrap_state: SessionBootstrapState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl ExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Interrupted)
    }
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Interrupted => write!(f, "interrupted"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCommandStatus {
    Requested,
    Applied,
    Failed,
}

impl RuntimeCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Applied => "applied",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for RuntimeCommandStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "requested" => Ok(Self::Requested),
            "applied" => Ok(Self::Applied),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown runtime command status: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCommandRecord {
    pub id: Uuid,
    pub session_id: String,
    pub transition_id: String,
    pub phase_node: String,
    pub status: RuntimeCommandStatus,
    pub transition: PendingCapabilityStateTransition,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub applied_at_ms: Option<i64>,
    pub failed_at_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalEffectType {
    HookEffects,
    SessionTerminalCallback,
    HookAutoResume,
}

impl TerminalEffectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HookEffects => "hook_effects",
            Self::SessionTerminalCallback => "session_terminal_callback",
            Self::HookAutoResume => "hook_auto_resume",
        }
    }
}

impl TryFrom<&str> for TerminalEffectType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "hook_effects" => Ok(Self::HookEffects),
            "session_terminal_callback" => Ok(Self::SessionTerminalCallback),
            "hook_auto_resume" => Ok(Self::HookAutoResume),
            other => Err(format!("unknown terminal effect type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalEffectStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    DeadLetter,
}

impl TerminalEffectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::DeadLetter => "dead_letter",
        }
    }
}

impl TryFrom<&str> for TerminalEffectStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "dead_letter" => Ok(Self::DeadLetter),
            other => Err(format!("unknown terminal effect status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TerminalEffectRecord {
    pub id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub effect_type: TerminalEffectType,
    pub payload: serde_json::Value,
    pub status: TerminalEffectStatus,
    pub attempt_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTerminalEffectRecord {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub effect_type: TerminalEffectType,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSessionEvent {
    pub session_id: String,
    pub event_seq: u64,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub session_update_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub notification: BackboneEnvelope,
}

#[derive(Debug, Clone)]
pub struct SessionEventBacklog {
    pub snapshot_seq: u64,
    pub events: Vec<PersistedSessionEvent>,
}

#[derive(Debug, Clone)]
pub struct SessionEventPage {
    pub snapshot_seq: u64,
    pub events: Vec<PersistedSessionEvent>,
    pub has_more: bool,
    pub next_after_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCompactionStatus {
    Started,
    ProjectionCommitted,
    Failed,
    Superseded,
    RolledBack,
}

impl SessionCompactionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::ProjectionCommitted => "projection_committed",
            Self::Failed => "failed",
            Self::Superseded => "superseded",
            Self::RolledBack => "rolled_back",
        }
    }
}

impl TryFrom<&str> for SessionCompactionStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "started" => Ok(Self::Started),
            "projection_committed" => Ok(Self::ProjectionCommitted),
            "failed" => Ok(Self::Failed),
            "superseded" => Ok(Self::Superseded),
            "rolled_back" => Ok(Self::RolledBack),
            other => Err(format!("unknown session compaction status: {other}")),
        }
    }
}

pub const SESSION_PROJECTION_KIND_MODEL_CONTEXT: &str = "model_context";
pub const SESSION_PROJECTION_KIND_TIMELINE: &str = "timeline";
pub const SESSION_PROJECTION_KIND_AUDIT: &str = "audit";
pub const SESSION_PROJECTION_KIND_HANDOFF: &str = "handoff";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionCompactionRecord {
    pub id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    pub projection_kind: String,
    pub projection_version: u64,
    pub lifecycle_item_id: String,
    pub start_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_event_seq: Option<u64>,
    pub status: SessionCompactionStatus,
    pub trigger: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_head_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_start_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_end_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_kept_event_seq: Option<u64>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub replacement_projection_json: Value,
    #[serde(default)]
    pub token_stats_json: Value,
    #[serde(default)]
    pub diagnostics_json: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionSegmentRecord {
    pub id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    pub projection_kind: String,
    pub projection_version: u64,
    pub sort_order: u64,
    pub segment_type: String,
    pub origin: String,
    pub synthetic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_start_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_end_event_seq: Option<u64>,
    #[serde(default)]
    pub source_refs_json: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by_compaction_id: Option<String>,
    #[serde(default)]
    pub content_json: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<u64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionHeadRecord {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    pub projection_kind: String,
    pub projection_version: u64,
    pub head_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_compaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by_event_seq: Option<u64>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLineageRelationKind {
    Fork,
    Companion,
    SpawnedAgent,
    RollbackBranch,
}

impl SessionLineageRelationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fork => "fork",
            Self::Companion => "companion",
            Self::SpawnedAgent => "spawned_agent",
            Self::RollbackBranch => "rollback_branch",
        }
    }
}

impl TryFrom<&str> for SessionLineageRelationKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "fork" => Ok(Self::Fork),
            "companion" => Ok(Self::Companion),
            "spawned_agent" => Ok(Self::SpawnedAgent),
            "rollback_branch" => Ok(Self::RollbackBranch),
            other => Err(format!("unknown session lineage relation kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLineageStatus {
    Open,
    Closed,
    Archived,
}

impl SessionLineageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Archived => "archived",
        }
    }
}

impl TryFrom<&str> for SessionLineageStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "archived" => Ok(Self::Archived),
            other => Err(format!("unknown session lineage status: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionLineageRecord {
    pub child_session_id: String,
    pub parent_session_id: String,
    pub relation_kind: SessionLineageRelationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_point_event_seq: Option<u64>,
    #[serde(default)]
    pub fork_point_ref_json: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_point_compaction_id: Option<String>,
    pub status: SessionLineageStatus,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub metadata_json: Value,
}

#[derive(Debug, Clone)]
pub struct NewCompactionProjectionCommit {
    pub completed_event: BackboneEnvelope,
    pub compaction: SessionCompactionRecord,
    pub segments: Vec<SessionProjectionSegmentRecord>,
    pub head: SessionProjectionHeadRecord,
}

#[derive(Debug, Clone)]
pub struct CompactionProjectionCommitResult {
    pub event: PersistedSessionEvent,
    pub compaction: SessionCompactionRecord,
    pub segments: Vec<SessionProjectionSegmentRecord>,
    pub head: SessionProjectionHeadRecord,
}

#[async_trait]
pub trait SessionMetaStore: Send + Sync {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()>;
    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>>;
    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>>;
    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()>;
    async fn delete_session(&self, session_id: &str) -> io::Result<()>;
}

#[async_trait]
pub trait SessionEventStore: Send + Sync {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent>;
    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog>;
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;
    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>>;
}

#[async_trait]
pub trait SessionTerminalEffectStore: Send + Sync {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord>;
    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> io::Result<()>;
    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> io::Result<()>;
    async fn mark_terminal_effect_failed(&self, effect_id: Uuid, error: String) -> io::Result<()>;
    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> io::Result<()>;
    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>>;
}

#[async_trait]
pub trait SessionRuntimeCommandStore: Send + Sync {
    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord>;
    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> io::Result<()>;
    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> io::Result<()>;
    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
}

#[async_trait]
pub trait SessionCompactionStore: Send + Sync {
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> io::Result<Option<SessionCompactionRecord>>;
    async fn list_compactions(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Vec<SessionCompactionRecord>>;
}

#[async_trait]
pub trait SessionProjectionStore: Send + Sync {
    async fn list_projection_segments(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
        projection_version: u64,
    ) -> io::Result<Vec<SessionProjectionSegmentRecord>>;
    async fn read_projection_head(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Option<SessionProjectionHeadRecord>>;
    async fn upsert_projection_head(&self, head: SessionProjectionHeadRecord) -> io::Result<()>;
    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> io::Result<CompactionProjectionCommitResult>;
}

#[async_trait]
pub trait SessionLineageStore: Send + Sync {
    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> io::Result<()>;
    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> io::Result<Option<SessionLineageRecord>>;
    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> io::Result<()>;
}

#[async_trait]
pub trait SessionPersistence: Send + Sync {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()>;
    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>>;
    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>>;
    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()>;
    async fn delete_session(&self, session_id: &str) -> io::Result<()>;
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent>;
    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog>;
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;
    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>>;
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord>;
    async fn mark_terminal_effect_running(&self, effect_id: Uuid) -> io::Result<()>;
    async fn mark_terminal_effect_succeeded(&self, effect_id: Uuid) -> io::Result<()>;
    async fn mark_terminal_effect_failed(&self, effect_id: Uuid, error: String) -> io::Result<()>;
    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: Uuid,
        error: String,
    ) -> io::Result<()>;
    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>>;
    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord>;
    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
    async fn mark_runtime_commands_applied(&self, command_ids: &[Uuid]) -> io::Result<()>;
    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[Uuid],
        error: String,
    ) -> io::Result<()>;
    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>>;
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> io::Result<Option<SessionCompactionRecord>>;
    async fn list_compactions(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Vec<SessionCompactionRecord>>;
    async fn list_projection_segments(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
        projection_version: u64,
    ) -> io::Result<Vec<SessionProjectionSegmentRecord>>;
    async fn read_projection_head(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        projection_kind: &str,
    ) -> io::Result<Option<SessionProjectionHeadRecord>>;
    async fn upsert_projection_head(&self, head: SessionProjectionHeadRecord) -> io::Result<()>;
    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> io::Result<CompactionProjectionCommitResult>;
    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> io::Result<()>;
    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> io::Result<Option<SessionLineageRecord>>;
    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>>;
    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> io::Result<()>;
}
