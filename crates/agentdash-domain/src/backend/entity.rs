use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    /// 设备稳定标识，仅作为 backend merge 输入保留，不作为本机唯一身份。
    pub device_id: Option<String>,
    /// 机器级稳定身份，由 Desktop 本地生成并长期保存。
    pub machine_id: Option<String>,
    /// 机器展示标签，通常来自 hostname 或用户命名，不作为唯一键。
    pub machine_label: Option<String>,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendVisibility {
    #[default]
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendShareScopeKind {
    #[default]
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

/// Desktop 本机 runtime 领取/确保 local backend 的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBackendClaim {
    pub owner_user_id: String,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
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
    ExplicitGrant,
}

impl ProjectBackendAccessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitGrant => "explicit_grant",
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
            access_mode: ProjectBackendAccessMode::ExplicitGrant,
            priority: 0,
            root_policy: serde_json::json!({ "kind": "workspace_registry" }),
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
    ManualRegister,
    IdentityDiscovery,
}

impl BackendWorkspaceInventorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManualRegister => "manual_register",
            Self::IdentityDiscovery => "identity_discovery",
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

pub const RUNNER_REGISTRATION_TOKEN_PREFIX: &str = "adrt";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerRegistrationToken {
    pub id: String,
    pub project_id: Uuid,
    pub name: String,
    pub token_secret_hash: String,
    pub token_prefix: String,
    pub created_by_user_id: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_claimed_backend_id: Option<String>,
    pub default_capability_slot: String,
    pub machine_policy: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl RunnerRegistrationToken {
    pub fn new_project_scoped(
        project_id: Uuid,
        name: String,
        created_by_user_id: String,
        expires_at: chrono::DateTime<chrono::Utc>,
        default_capability_slot: String,
        machine_policy: serde_json::Value,
    ) -> RunnerRegistrationTokenIssued {
        let id = format!("rtok_{}", Uuid::new_v4().simple());
        let secret = generate_runner_registration_secret();
        let plaintext = RunnerRegistrationTokenPlaintext {
            token_id: id.clone(),
            secret,
        };
        let now = chrono::Utc::now();
        let registration_token = plaintext.format();
        let token = Self {
            id,
            project_id,
            name,
            token_secret_hash: hash_runner_registration_secret(&plaintext.secret),
            token_prefix: plaintext.prefix(),
            created_by_user_id,
            expires_at,
            revoked_at: None,
            last_used_at: None,
            last_claimed_backend_id: None,
            default_capability_slot,
            machine_policy,
            created_at: now,
            updated_at: now,
        };
        RunnerRegistrationTokenIssued {
            token,
            registration_token,
        }
    }

    pub fn status_at(&self, now: chrono::DateTime<chrono::Utc>) -> RunnerRegistrationTokenStatus {
        if self.revoked_at.is_some() {
            RunnerRegistrationTokenStatus::Revoked
        } else if self.expires_at <= now {
            RunnerRegistrationTokenStatus::Expired
        } else {
            RunnerRegistrationTokenStatus::Active
        }
    }

    pub fn is_active_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.status_at(now) == RunnerRegistrationTokenStatus::Active
    }
}

#[derive(Debug, Clone)]
pub struct RunnerRegistrationTokenIssued {
    pub token: RunnerRegistrationToken,
    pub registration_token: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerRegistrationTokenStatus {
    Active,
    Expired,
    Revoked,
}

impl RunnerRegistrationTokenStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerRegistrationTokenPlaintext {
    pub token_id: String,
    pub secret: String,
}

impl RunnerRegistrationTokenPlaintext {
    pub fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.trim().split('_');
        let prefix = parts.next()?;
        let id_prefix = parts.next()?;
        let id_suffix = parts.next()?;
        let secret = parts.next()?;
        if parts.next().is_some()
            || prefix != RUNNER_REGISTRATION_TOKEN_PREFIX
            || id_prefix.is_empty()
            || id_suffix.is_empty()
            || secret.is_empty()
        {
            return None;
        }
        Some(Self {
            token_id: format!("{id_prefix}_{id_suffix}"),
            secret: secret.to_string(),
        })
    }

    pub fn format(&self) -> String {
        format!(
            "{}_{}_{}",
            RUNNER_REGISTRATION_TOKEN_PREFIX, self.token_id, self.secret
        )
    }

    pub fn prefix(&self) -> String {
        format!("{}_{}", RUNNER_REGISTRATION_TOKEN_PREFIX, self.token_id)
    }
}

pub fn hash_runner_registration_secret(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash.runner_registration_token.v1\n");
    hasher.update(secret.as_bytes());
    hex_encode(&hasher.finalize())
}

pub fn verify_runner_registration_secret(secret: &str, expected_hash: &str) -> bool {
    constant_time_eq(
        hash_runner_registration_secret(secret).as_bytes(),
        expected_hash.as_bytes(),
    )
}

fn generate_runner_registration_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        diff |= (left_byte ^ right_byte) as usize;
    }
    diff == 0
}

#[cfg(test)]
mod runner_registration_token_tests {
    use super::*;

    #[test]
    fn runner_registration_token_plaintext_roundtrips() {
        let plaintext = RunnerRegistrationTokenPlaintext {
            token_id: "rtok_abc123".to_string(),
            secret: "secret123".to_string(),
        };

        let formatted = plaintext.format();
        let parsed =
            RunnerRegistrationTokenPlaintext::parse(&formatted).expect("token should parse");

        assert_eq!(parsed, plaintext);
        assert_eq!(parsed.prefix(), "adrt_rtok_abc123");
    }

    #[test]
    fn runner_registration_token_secret_hash_verifies_without_plaintext() {
        let hash = hash_runner_registration_secret("secret-a");

        assert!(verify_runner_registration_secret("secret-a", &hash));
        assert!(!verify_runner_registration_secret("secret-b", &hash));
    }

    #[test]
    fn runner_registration_token_status_prefers_revoked_then_expired() {
        let now = chrono::Utc::now();
        let issued = RunnerRegistrationToken::new_project_scoped(
            Uuid::new_v4(),
            "runner".to_string(),
            "user-a".to_string(),
            now + chrono::Duration::hours(1),
            "default".to_string(),
            serde_json::json!({}),
        );
        assert_eq!(
            issued.token.status_at(now),
            RunnerRegistrationTokenStatus::Active
        );

        let mut expired = issued.token.clone();
        expired.expires_at = now - chrono::Duration::seconds(1);
        assert_eq!(
            expired.status_at(now),
            RunnerRegistrationTokenStatus::Expired
        );

        expired.revoked_at = Some(now);
        assert_eq!(
            expired.status_at(now),
            RunnerRegistrationTokenStatus::Revoked
        );
    }
}
