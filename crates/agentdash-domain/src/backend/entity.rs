use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workspace::WorkspaceIdentityKind;

/// 后端连接配置
///
/// 中控层维护的后端列表，每个后端代表一个独立的 AgentDash 数据源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    /// Relay 握手鉴权令牌，由云端生成并维护
    pub auth_token: Option<String>,
    /// 是否启用
    pub enabled: bool,
    /// 后端类型标识
    pub backend_type: BackendType,
    /// 注册此后端的用户标识（None 表示共享/系统级后端）
    pub owner_user_id: Option<String>,
    /// Desktop / runtime profile 标识，用于区分同一用户在不同 server/profile 下的本机端。
    pub profile_id: Option<String>,
    /// 旧版稳定设备标识，仅作为 legacy merge 输入保留，不再作为本机唯一身份。
    pub device_id: Option<String>,
    /// 机器级稳定身份，由 Desktop 本地生成并长期保存。
    pub machine_id: Option<String>,
    /// 机器展示标签，通常来自 hostname 或用户命名，不作为唯一键。
    pub machine_label: Option<String>,
    /// 旧机器身份候选，用于从 hostname / 旧 device_id 合并到 machine_id。
    pub legacy_machine_ids: Vec<String>,
    /// 本机 backend 可见性。个人本机为 private，共享本机使用 shared / system。
    pub visibility: BackendVisibility,
    /// backend scope 类型。personal runtime 使用 user。
    pub share_scope_kind: BackendShareScopeKind,
    /// scope id。user/project scope 填具体 id，system scope 为空。
    pub share_scope_id: Option<String>,
    /// 同一 machine/scope 下的能力槽位，默认 default。
    pub capability_slot: String,
    /// 设备元信息（OS、arch、app version 等），仅用于诊断与展示。
    pub device: serde_json::Value,
    /// 最近一次由 Desktop ensure/claim 的时间。
    pub last_claimed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendVisibility {
    Private,
    Shared,
    System,
}

impl BackendVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
            Self::System => "system",
        }
    }
}

impl Default for BackendVisibility {
    fn default() -> Self {
        Self::Private
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendShareScopeKind {
    User,
    Project,
    System,
}

impl BackendShareScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::System => "system",
        }
    }
}

impl Default for BackendShareScopeKind {
    fn default() -> Self {
        Self::User
    }
}

/// 视图配置
///
/// 用户自定义的跨后端看板视图，聚合来自不同后端的 Story。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewConfig {
    pub id: String,
    pub name: String,
    pub backend_ids: Vec<String>,
    pub filters: serde_json::Value,
    pub sort_by: Option<String>,
}

/// 用户偏好
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    pub default_view_id: Option<String>,
    pub theme: Option<String>,
    pub sidebar_collapsed: bool,
}

/// Desktop 本机 runtime 领取/确保 local backend 的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBackendClaim {
    pub owner_user_id: String,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub legacy_machine_ids: Vec<String>,
    pub visibility: BackendVisibility,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub backend_id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: String,
    pub device: serde_json::Value,
    pub rotate_token: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatus {
    Online,
    Offline,
    Starting,
    Degraded,
    Stopping,
    Error,
}

impl std::fmt::Display for RuntimeHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Starting => write!(f, "starting"),
            Self::Degraded => write!(f, "degraded"),
            Self::Stopping => write!(f, "stopping"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub backend_id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub status: RuntimeHealthStatus,
    pub version: Option<String>,
    pub capabilities: serde_json::Value,
    pub accessible_roots: Vec<String>,
    pub device: serde_json::Value,
    pub connected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub disconnect_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealthOnlineUpdate {
    pub backend_id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub version: String,
    pub capabilities: serde_json::Value,
    pub accessible_roots: Vec<String>,
    pub device: serde_json::Value,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendExecutionSelectionMode {
    Explicit,
    AutoIdle,
    WorkspaceBinding,
}

impl BackendExecutionSelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::AutoIdle => "auto_idle",
            Self::WorkspaceBinding => "workspace_binding",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendExecutionLeaseState {
    Claimed,
    Running,
    Released,
    Lost,
    Failed,
}

impl BackendExecutionLeaseState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Released => "released",
            Self::Lost => "lost",
            Self::Failed => "failed",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Claimed | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendExecutionTerminalKind {
    Completed,
    Failed,
    Interrupted,
}

impl BackendExecutionTerminalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackendExecutionLease {
    pub id: Uuid,
    pub backend_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub executor_id: String,
    pub workspace_id: Option<Uuid>,
    pub root_ref: Option<String>,
    pub selection_mode: BackendExecutionSelectionMode,
    pub state: BackendExecutionLeaseState,
    pub claim_reason: Option<String>,
    pub terminal_kind: Option<BackendExecutionTerminalKind>,
    pub release_reason: Option<String>,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
    pub activated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl BackendExecutionLease {
    pub fn claimed(
        backend_id: String,
        session_id: String,
        turn_id: String,
        executor_id: String,
        selection_mode: BackendExecutionSelectionMode,
        claim_reason: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            backend_id,
            session_id,
            turn_id,
            executor_id,
            workspace_id: None,
            root_ref: None,
            selection_mode,
            state: BackendExecutionLeaseState::Claimed,
            claim_reason,
            terminal_kind: None,
            release_reason: None,
            claimed_at: now,
            activated_at: None,
            released_at: None,
            last_seen_at: now,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBackendAccessStatus {
    Active,
    Paused,
    Revoked,
}

impl ProjectBackendAccessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBackendAccessMode {
    UseInventory,
}

impl ProjectBackendAccessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UseInventory => "use_inventory",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBackendAccess {
    pub id: Uuid,
    pub project_id: Uuid,
    pub backend_id: String,
    pub status: ProjectBackendAccessStatus,
    pub access_mode: ProjectBackendAccessMode,
    pub priority: i32,
    pub root_policy: serde_json::Value,
    pub capability_policy: serde_json::Value,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ProjectBackendAccess {
    pub fn new(project_id: Uuid, backend_id: String, created_by: Option<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            backend_id,
            status: ProjectBackendAccessStatus::Active,
            access_mode: ProjectBackendAccessMode::UseInventory,
            priority: 0,
            root_policy: serde_json::json!({ "kind": "backend_inventory" }),
            capability_policy: serde_json::json!({}),
            note: None,
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == ProjectBackendAccessStatus::Active
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendWorkspaceInventoryStatus {
    Available,
    Stale,
    Offline,
    Error,
}

impl BackendWorkspaceInventoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Stale => "stale",
            Self::Offline => "offline",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendWorkspaceInventorySource {
    RuntimeRegister,
    ManualRefresh,
    ScheduledRefresh,
    CapabilityExpansionAck,
}

impl BackendWorkspaceInventorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeRegister => "runtime_register",
            Self::ManualRefresh => "manual_refresh",
            Self::ScheduledRefresh => "scheduled_refresh",
            Self::CapabilityExpansionAck => "capability_expansion_ack",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendWorkspaceInventory {
    pub id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: serde_json::Value,
    pub detected_facts: serde_json::Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub source: BackendWorkspaceInventorySource,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl BackendWorkspaceInventory {
    pub fn available(
        backend_id: String,
        root_ref: String,
        identity_kind: WorkspaceIdentityKind,
        identity_payload: serde_json::Value,
        detected_facts: serde_json::Value,
        source: BackendWorkspaceInventorySource,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            backend_id,
            root_ref,
            identity_kind,
            identity_payload,
            detected_facts,
            status: BackendWorkspaceInventoryStatus::Available,
            source,
            last_seen_at: now,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}
