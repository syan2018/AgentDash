use serde::{Deserialize, Serialize};

/// 后端连接配置
///
/// 中控层维护的后端列表，每个后端代表一个独立的 AgentDash 数据源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    /// 鉴权令牌（预留，当前阶段可为空）
    pub auth_token: Option<String>,
    /// 是否启用
    pub enabled: bool,
    /// 后端类型标识
    pub backend_type: BackendType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    /// 本地后端（同机 SQLite）
    Local,
    /// 远程后端（通过 HTTP 连接）
    Remote,
}

/// 视图配置
///
/// 用户自定义的跨后端看板视图，聚合来自不同后端的 Story。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewConfig {
    pub id: String,
    pub name: String,
    /// 此视图包含哪些 Backend 的数据
    pub backend_ids: Vec<String>,
    /// 过滤条件（预留扩展）
    pub filters: serde_json::Value,
    /// 排序规则
    pub sort_by: Option<String>,
}

/// 用户偏好
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    pub default_view_id: Option<String>,
    pub theme: Option<String>,
    pub sidebar_collapsed: bool,
}
