use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// LLM 通信协议类型
///
/// wire protocol 决定后端用哪种 client/bridge 构造模型连接。
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
    /// ChatGPT 账号登录后的 Codex Responses API
    OpenaiCodex,
}

impl WireProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenaiCompatible => "openai_compatible",
            Self::OpenaiCodex => "openai_codex",
        }
    }
}

impl std::str::FromStr for WireProtocol {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            "openai_compatible" => Ok(Self::OpenaiCompatible),
            "openai_codex" => Ok(Self::OpenaiCodex),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for WireProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provider 凭据策略
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialMode {
    /// 仅使用管理员配置的全局 DB Key 或 env key。
    #[default]
    GlobalOnly,
    /// 当前用户配置 BYOK 时优先使用用户 Key，否则使用全局 Key。
    GlobalOrUser,
    /// 必须由当前用户配置 BYOK。
    UserRequired,
}

impl LlmCredentialMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalOnly => "global_only",
            Self::GlobalOrUser => "global_or_user",
            Self::UserRequired => "user_required",
        }
    }
}

impl std::str::FromStr for LlmCredentialMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "global_only" => Ok(Self::GlobalOnly),
            "global_or_user" => Ok(Self::GlobalOrUser),
            "user_required" => Ok(Self::UserRequired),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for LlmCredentialMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 运行态凭据来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialSource {
    GlobalDb,
    GlobalEnv,
    UserByok,
    None,
}

impl LlmCredentialSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalDb => "global_db",
            Self::GlobalEnv => "global_env",
            Self::UserByok => "user_byok",
            Self::None => "none",
        }
    }
}

/// 用户凭据验证状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialVerificationStatus {
    #[default]
    Unverified,
    Verified,
    Failed,
}

impl LlmCredentialVerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for LlmCredentialVerificationStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unverified" => Ok(Self::Unverified),
            "verified" => Ok(Self::Verified),
            "failed" => Ok(Self::Failed),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for LlmCredentialVerificationStatus {
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
    /// 凭据策略
    #[serde(default)]
    pub credential_mode: LlmCredentialMode,
    /// 管理员保存的全局 API Key 密文。
    #[serde(default)]
    pub global_api_key_ciphertext: String,
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
            credential_mode: LlmCredentialMode::GlobalOnly,
            global_api_key_ciphertext: String::new(),
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

    /// 解析全局 env key。DB-backed key 由调用方通过 `LlmSecretCodec` 解密。
    pub fn resolve_env_api_key(&self) -> Option<String> {
        if !self.env_api_key.is_empty() {
            return std::env::var(&self.env_api_key).ok();
        }
        None
    }
}

/// 用户 BYOK 凭据实体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderUserCredential {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub user_id: String,
    pub api_key_ciphertext: String,
    #[serde(default)]
    pub verification_status: LlmCredentialVerificationStatus,
    #[serde(default)]
    pub verification_message: String,
    #[serde(default)]
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LlmProviderUserCredential {
    pub fn new(
        provider_id: Uuid,
        user_id: impl Into<String>,
        api_key_ciphertext: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            provider_id,
            user_id: user_id.into(),
            api_key_ciphertext: api_key_ciphertext.into(),
            verification_status: LlmCredentialVerificationStatus::Unverified,
            verification_message: String::new(),
            verified_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn mark_verification(
        &mut self,
        status: LlmCredentialVerificationStatus,
        message: impl Into<String>,
    ) {
        let now = Utc::now();
        self.verification_status = status;
        self.verification_message = message.into();
        self.verified_at = (status == LlmCredentialVerificationStatus::Verified).then_some(now);
        self.updated_at = now;
    }
}
