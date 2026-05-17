use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;
use crate::common::{SystemPromptMode, ThinkingLevel};
use crate::mcp_preset::{McpRoutePolicy, McpTransportConfig};
use crate::skill_asset::SkillAssetFileKind;
use crate::workflow::ToolCapabilityDirective;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetType {
    AgentTemplate,
    McpServerTemplate,
    WorkflowTemplate,
    SkillTemplate,
}

impl LibraryAssetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentTemplate => "agent_template",
            Self::McpServerTemplate => "mcp_server_template",
            Self::WorkflowTemplate => "workflow_template",
            Self::SkillTemplate => "skill_template",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "agent_template" => Ok(Self::AgentTemplate),
            "mcp_server_template" => Ok(Self::McpServerTemplate),
            "workflow_template" => Ok(Self::WorkflowTemplate),
            "skill_template" => Ok(Self::SkillTemplate),
            other => Err(DomainError::InvalidConfig(format!(
                "library_assets.asset_type 非法: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetScope {
    Builtin,
    System,
    Org,
    User,
}

impl LibraryAssetScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::System => "system",
            Self::Org => "org",
            Self::User => "user",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "builtin" => Ok(Self::Builtin),
            "system" => Ok(Self::System),
            "org" => Ok(Self::Org),
            "user" => Ok(Self::User),
            other => Err(DomainError::InvalidConfig(format!(
                "library_assets.scope 非法: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetSource {
    Builtin,
    UserAuthored,
    RemoteImported,
}

impl LibraryAssetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::UserAuthored => "user_authored",
            Self::RemoteImported => "remote_imported",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "builtin" => Ok(Self::Builtin),
            "user_authored" => Ok(Self::UserAuthored),
            "remote_imported" => Ok(Self::RemoteImported),
            other => Err(DomainError::InvalidConfig(format!(
                "library_assets.source 非法: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledAssetSource {
    pub library_asset_id: Uuid,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: DateTime<Utc>,
}

impl InstalledAssetSource {
    pub fn new(
        library_asset_id: Uuid,
        source_ref: impl Into<String>,
        source_version: impl Into<String>,
        source_digest: impl Into<String>,
    ) -> Self {
        Self {
            library_asset_id,
            source_ref: source_ref.into(),
            source_version: source_version.into(),
            source_digest: source_digest.into(),
            installed_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharedLibrarySourceStatus {
    UpToDate,
    UpdateAvailable,
    SourceMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "asset_type", content = "payload", rename_all = "snake_case")]
pub enum LibraryAssetPayload {
    AgentTemplate(AgentTemplatePayload),
    McpServerTemplate(McpServerTemplatePayload),
    WorkflowTemplate(WorkflowTemplatePayload),
    SkillTemplate(SkillTemplatePayload),
}

impl LibraryAssetPayload {
    pub fn from_value(asset_type: LibraryAssetType, value: Value) -> Result<Self, DomainError> {
        match asset_type {
            LibraryAssetType::AgentTemplate => serde_json::from_value(value)
                .map(Self::AgentTemplate)
                .map_err(|error| payload_error("agent_template", error)),
            LibraryAssetType::McpServerTemplate => serde_json::from_value(value)
                .map(Self::McpServerTemplate)
                .map_err(|error| payload_error("mcp_server_template", error)),
            LibraryAssetType::WorkflowTemplate => serde_json::from_value(value)
                .map(Self::WorkflowTemplate)
                .map_err(|error| payload_error("workflow_template", error)),
            LibraryAssetType::SkillTemplate => serde_json::from_value(value)
                .map(Self::SkillTemplate)
                .map_err(|error| payload_error("skill_template", error)),
        }
    }

    pub fn validate(asset_type: LibraryAssetType, value: &Value) -> Result<(), DomainError> {
        Self::from_value(asset_type, value.clone()).map(|_| ())
    }
}

fn payload_error(asset_type: &str, error: serde_json::Error) -> DomainError {
    DomainError::InvalidConfig(format!(
        "library_assets.payload 与 {asset_type} schema 不匹配: {error}"
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct AgentTemplatePayload {
    pub config: AgentTemplateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct AgentTemplateConfig {
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_directives: Vec<ToolCapabilityDirective>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_slots: Vec<AgentMcpSlotTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMcpSlotTemplate {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ProjectAgentConfigOverride {
    pub override_executor: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    pub override_model: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub override_thinking_level: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    pub override_permission_policy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    pub override_system_prompt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerTemplatePayload {
    pub transport: McpTransportConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_policy: Option<McpRoutePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTemplatePayload {
    pub template: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTemplatePayload {
    pub files: Vec<SkillTemplateFilePayload>,
    #[serde(default)]
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTemplateFilePayload {
    pub path: String,
    pub content: String,
    pub kind: SkillAssetFileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuiltinSeed {
    pub asset_type: LibraryAssetType,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    pub payload_digest: String,
    pub payload: Value,
}

impl BuiltinSeed {
    pub fn validate(&self) -> Result<(), DomainError> {
        LibraryAssetPayload::validate(self.asset_type, &self.payload)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn validates_payload_by_asset_type() {
        let payload = json!({
            "transport": { "type": "http", "url": "https://example.com/mcp" },
            "route_policy": "direct",
            "capabilities": ["search"]
        });

        let typed = LibraryAssetPayload::from_value(LibraryAssetType::McpServerTemplate, payload)
            .expect("valid mcp template");

        match typed {
            LibraryAssetPayload::McpServerTemplate(payload) => {
                assert_eq!(payload.capabilities, vec!["search".to_string()]);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_payload_for_type() {
        let result = LibraryAssetPayload::from_value(
            LibraryAssetType::SkillTemplate,
            json!({"files": "not array"}),
        );

        assert!(result.is_err());
    }

    #[test]
    fn project_override_requires_explicit_flags() {
        let override_config = ProjectAgentConfigOverride {
            override_model: true,
            model_id: Some("gpt-5.4".to_string()),
            ..Default::default()
        };

        assert!(override_config.override_model);
        assert_eq!(override_config.model_id.as_deref(), Some("gpt-5.4"));
        assert!(!override_config.override_system_prompt);
    }
}
