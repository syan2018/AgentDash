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
