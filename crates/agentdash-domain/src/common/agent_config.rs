use serde::{Deserialize, Serialize};

/// Agent 级 System Prompt 注入模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPromptMode {
    /// 在全局 system prompt 之后追加（默认）。
    #[default]
    Append,
    /// 完全替换全局 system prompt。
    Override,
}

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
/// `executor` 字段使用原始字符串，既能表示 vibe-kanban 的 `BaseCodingAgent`
/// （如 `"CLAUDE_CODE"`），也能表示 AgentDash 自有 agent（如 `"PI_AGENT"`）。
/// 路由到具体连接器时由 adapter 层按需转换。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub executor: String,
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
    /// Agent 级工具簇限制。None = 不限制（使用会话类型默认全量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_clusters: Option<Vec<String>>,
    /// Agent 级 system prompt（追加或覆盖全局 prompt）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// system_prompt 注入模式。None 时默认 Append。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
}

/// 已注册的云端原生 Agent executor ID 列表。
/// 这些 Agent 在云端进程内运行，不通过本机后端中继。
const CLOUD_NATIVE_EXECUTORS: &[&str] = &["PI_AGENT"];

impl AgentConfig {
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            provider_id: None,
            model_id: None,
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
            tool_clusters: None,
            system_prompt: None,
            system_prompt_mode: None,
        }
    }

    /// 判断此配置是否指向云端原生 Agent（在云端进程内执行，不经由本机后端中继）。
    pub fn is_cloud_native(&self) -> bool {
        CLOUD_NATIVE_EXECUTORS
            .iter()
            .any(|id| self.executor.eq_ignore_ascii_case(id))
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::new("CLAUDE_CODE")
    }
}
