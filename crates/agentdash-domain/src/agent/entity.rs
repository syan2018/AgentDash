use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{AgentPresetConfig, error::DomainError};
use crate::shared_library::InstalledAssetSource;

/// ProjectAgent — Project 内可运行、可编辑的 Agent 实例。
///
/// 跨 Project 复用只发生在 Shared Library 的 `AgentTemplate`；运行路径只消费
/// 已安装或手工创建的 ProjectAgent。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAgent {
    pub id: Uuid,
    pub project_id: Uuid,
    /// 人类可读标识符（如 "code-reviewer"）
    pub name: String,
    /// 执行器类型（如 "PI_AGENT", "claude-code"）
    pub agent_type: String,
    /// Project 内配置 JSON — DB 层 TEXT 存储，运行时通过 `preset_config()` 获取类型安全访问。
    pub config: serde_json::Value,
    /// Marketplace / Shared Library 安装来源；手工创建的 Project Agent 为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    /// 此 Agent 在此 Project 下的默认 lifecycle key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_lifecycle_key: Option<String>,
    /// 是否为此 Project 的 Story 默认 Agent
    #[serde(default)]
    pub is_default_for_story: bool,
    /// 是否为此 Project 的 Task 默认 Agent
    #[serde(default)]
    pub is_default_for_task: bool,
    /// 是否启用 Agent 跨 session 知识库（按 Project × Agent 隔离）
    /// 默认 false — 大多数 Agent 是无状态的
    #[serde(default)]
    pub knowledge_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectAgent {
    pub fn new(project_id: Uuid, name: impl Into<String>, agent_type: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            agent_type: agent_type.into(),
            config: serde_json::json!({}),
            installed_source: None,
            default_lifecycle_key: None,
            is_default_for_story: false,
            is_default_for_task: false,
            knowledge_enabled: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// 将 ProjectAgent.config JSON 反序列化为类型安全的 `AgentPresetConfig`。
    pub fn preset_config(&self) -> Result<AgentPresetConfig, DomainError> {
        AgentPresetConfig::from_json(&self.config)
    }
}
