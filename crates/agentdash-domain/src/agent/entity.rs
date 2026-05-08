use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::AgentPresetConfig;

/// Agent — 独立的 Agent 实体
///
/// 与 Project 通过 `ProjectAgentLink` 建立多对多关系。
/// 存储 Agent 的基础配置（执行器类型、模型参数、MCP 等），
/// per-project 的覆写和 lifecycle 绑定由关联表管理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    /// 人类可读标识符（如 "code-reviewer"）
    pub name: String,
    /// 执行器类型（如 "PI_AGENT", "claude-code"）
    pub agent_type: String,
    /// 基础配置 JSON — DB 层 jsonb 存储，运行时通过 `preset_config()` 获取类型安全访问。
    pub base_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    pub fn new(name: impl Into<String>, agent_type: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            agent_type: agent_type.into(),
            base_config: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    /// 将 base_config JSON 反序列化为类型安全的 `AgentPresetConfig`。
    pub fn preset_config(&self) -> AgentPresetConfig {
        AgentPresetConfig::from_json(&self.base_config)
    }
}

/// ProjectAgentLink — Project ↔ Agent 多对多关联
///
/// 承载 per-project 的配置覆写、默认 lifecycle 绑定和角色默认标志。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAgentLink {
    pub id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    /// per-project 配置覆写（与 Agent.base_config 合并）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_override: Option<serde_json::Value>,
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
    /// 白名单：允许此 Agent 访问的项目级容器 ID
    /// 空 = 不继承任何项目级容器
    #[serde(default)]
    pub project_container_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectAgentLink {
    pub fn new(project_id: Uuid, agent_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            agent_id,
            config_override: None,
            default_lifecycle_key: None,
            is_default_for_story: false,
            is_default_for_task: false,
            knowledge_enabled: false,
            project_container_ids: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    /// 将 Agent.base_config 与 link.config_override 合并为类型安全的 `AgentPresetConfig`。
    /// override 字段级优先于 base。
    pub fn merged_preset_config(&self, agent: &Agent) -> AgentPresetConfig {
        let base = agent.preset_config();
        match &self.config_override {
            Some(over_json) => {
                let over = AgentPresetConfig::from_json(over_json);
                over.merge_over(&base)
            }
            None => base,
        }
    }
}
