use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_container::{ContextContainerDefinition, MountDerivationPolicy};
use crate::session_composition::SessionComposition;

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
    /// 项目级上下文容器定义
    #[serde(default)]
    pub context_containers: Vec<ContextContainerDefinition>,
    /// 项目级挂载派生策略
    #[serde(default)]
    pub mount_policy: MountDerivationPolicy,
    /// 项目级会话编排默认配置
    #[serde(default)]
    pub session_composition: SessionComposition,
}

/// Agent 预设配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPreset {
    pub name: String,
    pub agent_type: String,
    pub config: serde_json::Value,
}
