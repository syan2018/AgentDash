use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_domain::llm_provider as domain;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderProtocol {
    Anthropic,
    Gemini,
    OpenaiCompatible,
    OpenaiCodex,
}

impl From<domain::WireProtocol> for LlmProviderProtocol {
    fn from(protocol: domain::WireProtocol) -> Self {
        match protocol {
            domain::WireProtocol::Anthropic => Self::Anthropic,
            domain::WireProtocol::Gemini => Self::Gemini,
            domain::WireProtocol::OpenaiCompatible => Self::OpenaiCompatible,
            domain::WireProtocol::OpenaiCodex => Self::OpenaiCodex,
        }
    }
}

impl From<LlmProviderProtocol> for domain::WireProtocol {
    fn from(protocol: LlmProviderProtocol) -> Self {
        match protocol {
            LlmProviderProtocol::Anthropic => Self::Anthropic,
            LlmProviderProtocol::Gemini => Self::Gemini,
            LlmProviderProtocol::OpenaiCompatible => Self::OpenaiCompatible,
            LlmProviderProtocol::OpenaiCodex => Self::OpenaiCodex,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialModeDto {
    GlobalOnly,
    GlobalOrUser,
    UserRequired,
}

impl From<domain::LlmCredentialMode> for LlmCredentialModeDto {
    fn from(mode: domain::LlmCredentialMode) -> Self {
        match mode {
            domain::LlmCredentialMode::GlobalOnly => Self::GlobalOnly,
            domain::LlmCredentialMode::GlobalOrUser => Self::GlobalOrUser,
            domain::LlmCredentialMode::UserRequired => Self::UserRequired,
        }
    }
}

impl From<LlmCredentialModeDto> for domain::LlmCredentialMode {
    fn from(mode: LlmCredentialModeDto) -> Self {
        match mode {
            LlmCredentialModeDto::GlobalOnly => Self::GlobalOnly,
            LlmCredentialModeDto::GlobalOrUser => Self::GlobalOrUser,
            LlmCredentialModeDto::UserRequired => Self::UserRequired,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialSourceDto {
    GlobalDb,
    GlobalEnv,
    UserByok,
    None,
}

impl From<domain::LlmCredentialSource> for LlmCredentialSourceDto {
    fn from(source: domain::LlmCredentialSource) -> Self {
        match source {
            domain::LlmCredentialSource::GlobalDb => Self::GlobalDb,
            domain::LlmCredentialSource::GlobalEnv => Self::GlobalEnv,
            domain::LlmCredentialSource::UserByok => Self::UserByok,
            domain::LlmCredentialSource::None => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmCredentialVerificationStatusDto {
    Unverified,
    Verified,
    Failed,
}

impl From<domain::LlmCredentialVerificationStatus> for LlmCredentialVerificationStatusDto {
    fn from(status: domain::LlmCredentialVerificationStatus) -> Self {
        match status {
            domain::LlmCredentialVerificationStatus::Unverified => Self::Unverified,
            domain::LlmCredentialVerificationStatus::Verified => Self::Verified,
            domain::LlmCredentialVerificationStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LlmProviderAdminDto {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub protocol: LlmProviderProtocol,
    pub credential_mode: LlmCredentialModeDto,
    pub global_api_key_configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub global_api_key_preview: Option<String>,
    pub global_api_key_source: LlmCredentialSourceDto,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub models: Value,
    pub blocked_models: Value,
    pub env_api_key: String,
    pub discovery_url: String,
    pub sort_order: i32,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct EffectiveLlmModelProfileDto {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub reasoning: bool,
    pub supports_image: bool,
    pub context_window: u64,
    pub blocked: bool,
    pub discovered: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct EffectiveLlmProviderDto {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub protocol: LlmProviderProtocol,
    pub credential_mode: LlmCredentialModeDto,
    pub base_url: String,
    pub wire_api: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_wire_api: Option<String>,
    pub default_model: String,
    pub models: Value,
    pub effective_models: Vec<EffectiveLlmModelProfileDto>,
    pub model_discovery_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_discovery_message: Option<String>,
    pub blocked_models: Value,
    pub discovery_url: String,
    pub enabled: bool,
    pub executable: bool,
    pub effective_api_key_source: LlmCredentialSourceDto,
    pub user_api_key_configured: bool,
    pub user_credential_verification_status: LlmCredentialVerificationStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_api_key_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_credential_verification_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_credential_verified_at: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateLlmProviderRequest {
    pub name: String,
    pub slug: String,
    pub protocol: LlmProviderProtocol,
    #[serde(default)]
    #[ts(optional)]
    pub credential_mode: Option<LlmCredentialModeDto>,
    #[serde(default)]
    #[ts(optional)]
    pub global_api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub base_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub wire_api: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub default_model: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub models: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub blocked_models: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub env_api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub discovery_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct UpdateLlmProviderRequest {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub protocol: Option<LlmProviderProtocol>,
    #[serde(default)]
    #[ts(optional)]
    pub credential_mode: Option<LlmCredentialModeDto>,
    #[serde(default)]
    #[ts(optional)]
    pub global_api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub base_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub wire_api: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub default_model: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub models: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub blocked_models: Option<Value>,
    #[serde(default)]
    #[ts(optional)]
    pub env_api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub discovery_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ReorderLlmProvidersRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ProbeLlmProviderModelsRequest {
    pub protocol: LlmProviderProtocol,
    #[serde(default)]
    #[ts(optional)]
    pub api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub base_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub discovery_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub env_api_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProbeLlmProviderModelDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct UpsertLlmProviderUserCredentialRequest {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteLlmProviderUserCredentialResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexOAuthFlowStatusDto {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StartCodexOAuthResponse {
    pub flow_id: String,
    pub auth_url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CodexOAuthStatusResponse {
    pub flow_id: String,
    pub status: CodexOAuthFlowStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}
