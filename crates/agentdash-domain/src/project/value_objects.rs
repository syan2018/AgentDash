use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 项目级配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// 默认 Agent 类型（如 "claude-code"）
    pub default_agent_type: Option<String>,
    /// 默认 Workspace ID
    pub default_workspace_id: Option<Uuid>,
    /// Agent 预设列表
    #[serde(default)]
    pub agent_presets: Vec<AgentPreset>,
}

/// Agent 预设配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPreset {
    pub name: String,
    pub agent_type: String,
    pub config: serde_json::Value,
}
