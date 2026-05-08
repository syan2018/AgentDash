use serde::{Deserialize, Serialize};

use crate::workflow::ToolCapabilityDirective;

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

// ── AgentPresetConfig ─────────────────────────────────────────────────

/// Agent 配置存储层的权威类型。
///
/// 统一 `Agent.base_config` / `ProjectAgentLink.config_override` / `AgentPreset.config`
/// 三处 JSON blob，所有字段均为 Option 以支持字段级合并（override 覆盖 base）。
///
/// 消费方通过 `to_agent_config()` 提取运行态执行器配置 [`AgentConfig`]。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPresetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// Agent 级能力指令。替代旧 `tool_clusters: Option<Vec<String>>`。
    /// 前端 → API → 存储 → Resolver 全链路使用相同的 `ToolCapabilityDirective` 表示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_directives: Option<Vec<ToolCapabilityDirective>>,
    /// MCP Preset key 引用列表（如 `["github", "jira"]`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_preset_keys: Option<Vec<String>>,
    /// 允许此 Agent 调用的 companion agent 名称白名单。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_companions: Option<Vec<String>>,
}

/// 用于字段级合并的 helper macro — 消除逐字段重复代码。
macro_rules! merge_field {
    ($over:expr, $base:expr, $($field:ident),+ $(,)?) => {
        Self {
            $( $field: $over.$field.clone().or_else(|| $base.$field.clone()), )+
        }
    };
}

impl AgentPresetConfig {
    /// 字段级合并：`self`（override）非 None 的字段优先于 `base`。
    pub fn merge_over(&self, base: &AgentPresetConfig) -> AgentPresetConfig {
        merge_field!(
            self, base,
            executor,
            provider_id,
            model_id,
            agent_id,
            thinking_level,
            permission_policy,
            system_prompt,
            system_prompt_mode,
            display_name,
            capability_directives,
            mcp_preset_keys,
            allowed_companions,
        )
    }

    /// 提取运行态执行器配置 [`AgentConfig`]。
    ///
    /// `fallback_executor` 在 `self.executor` 为 None 时使用（通常来自 `Agent.agent_type`）。
    pub fn to_agent_config(&self, fallback_executor: &str) -> AgentConfig {
        AgentConfig {
            executor: self
                .executor
                .clone()
                .unwrap_or_else(|| fallback_executor.to_string()),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            agent_id: self.agent_id.clone(),
            thinking_level: self.thinking_level,
            permission_policy: self.permission_policy.clone(),
            system_prompt: self.system_prompt.clone(),
            system_prompt_mode: self.system_prompt_mode,
        }
    }

    /// 从旧格式 `serde_json::Value` 反序列化（用于 DB 读取的过渡期）。
    pub fn from_json(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

// ── AgentConfig（运行态执行器配置）──────────────────────────────────────

/// AgentDash 统一执行器配置 — connector 层的运行态接口类型。
///
/// 只包含执行器运行所需的参数（executor / model / prompt 等），
/// 不包含 capability / companion / MCP 等配置（由 `AgentPresetConfig` 承载）。
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
}

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
