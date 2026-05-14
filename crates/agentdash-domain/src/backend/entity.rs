use serde::{Deserialize, Serialize};

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
    /// 稳定设备标识，由 Desktop 生成并按 server profile 隔离保存。
    pub device_id: Option<String>,
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
    pub device_id: String,
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
