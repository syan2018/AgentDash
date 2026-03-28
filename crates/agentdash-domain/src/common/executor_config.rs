use serde::{Deserialize, Serialize};

/// 思考/推理级别 — 跨层通用值对象。
///
/// 在 Domain 层定义，避免各层重复声明或依赖具体 Agent 运行时。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

/// AgentDash 统一执行器配置。
///
/// `executor` 字段使用原始字符串，既能表示 vibe-kanban 的 `BaseCodingAgent` 变体
/// （如 `"CLAUDE_CODE"`），也能表示 AgentDash 自有 agent（如 `"PI_AGENT"`）。
/// 路由到具体连接器时由 adapter 层按需转换。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
}

impl ExecutorConfig {
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            variant: None,
            provider_id: None,
            model_id: None,
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
        }
    }
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self::new("CLAUDE_CODE")
    }
}
