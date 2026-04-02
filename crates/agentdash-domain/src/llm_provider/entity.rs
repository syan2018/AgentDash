use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// LLM 通信协议类型
///
/// 仅 3 种 wire protocol，决定后端用哪种 rig client 构造 bridge。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireProtocol {
    /// Anthropic 原生 API (`x-api-key` header)
    Anthropic,
    /// Google Gemini 原生 API
    Gemini,
    /// OpenAI Chat Completions / Responses API 及所有兼容端点
    /// (OpenAI, DeepSeek, Groq, xAI, Ollama, Azure, …)
    OpenaiCompatible,
}

impl WireProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenaiCompatible => "openai_compatible",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "anthropic" => Some(Self::Anthropic),
            "gemini" => Some(Self::Gemini),
            "openai_compatible" => Some(Self::OpenaiCompatible),
            _ => None,
        }
    }
}

impl std::fmt::Display for WireProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// LLM Provider 配置实体
///
/// 存储一个 LLM 服务端点的完整连接配置。
/// 同一 protocol 可以有多个实例（如多个 OpenAI-compatible 代理）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProvider {
    pub id: Uuid,
    /// 显示名称, e.g. "Anthropic Claude", "My Azure Proxy"
    pub name: String,
    /// 唯一标识符 (slug), e.g. "anthropic", "my-azure-proxy"
    /// 用于 model discovery 流和前端引用
    pub slug: String,
    /// wire protocol 类型
    pub protocol: WireProtocol,
    /// API 密钥 (可为空, 如本地 Ollama)
    #[serde(default)]
    pub api_key: String,
    /// 端点 URL (空字符串 = 使用协议默认值)
    #[serde(default)]
    pub base_url: String,
    /// 仅 openai_compatible: "responses" | "completions" | ""(自动推断)
    #[serde(default)]
    pub wire_api: String,
    /// 默认模型 ID
    #[serde(default)]
    pub default_model: String,
    /// 用户自定义模型列表 (JSON)
    #[serde(default)]
    pub models: serde_json::Value,
    /// 屏蔽的模型 ID 列表 (JSON)
    #[serde(default)]
    pub blocked_models: serde_json::Value,
    /// 环境变量 fallback 名称, e.g. "ANTHROPIC_API_KEY"
    #[serde(default)]
    pub env_api_key: String,
    /// 模型发现端点 (空 = 从 base_url 推导或不支持)
    #[serde(default)]
    pub discovery_url: String,
    /// 排序权重 (越小越前)
    #[serde(default)]
    pub sort_order: i32,
    /// 启用/禁用
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

impl LlmProvider {
    pub fn new(name: impl Into<String>, slug: impl Into<String>, protocol: WireProtocol) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            slug: slug.into(),
            protocol,
            api_key: String::new(),
            base_url: String::new(),
            wire_api: String::new(),
            default_model: String::new(),
            models: serde_json::json!([]),
            blocked_models: serde_json::json!([]),
            env_api_key: String::new(),
            discovery_url: String::new(),
            sort_order: 0,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析生效的 API key: 优先使用配置值，为空时回退到环境变量
    pub fn resolve_api_key(&self) -> Option<String> {
        if !self.api_key.is_empty() {
            return Some(self.api_key.clone());
        }
        if !self.env_api_key.is_empty() {
            return std::env::var(&self.env_api_key).ok();
        }
        None
    }
}
