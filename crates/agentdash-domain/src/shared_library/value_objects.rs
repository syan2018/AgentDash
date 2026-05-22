use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::DomainError;
use crate::common::{MountCapability, SystemPromptMode, ThinkingLevel};
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
    VfsMountTemplate,
    ExtensionTemplate,
}

impl LibraryAssetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentTemplate => "agent_template",
            Self::McpServerTemplate => "mcp_server_template",
            Self::WorkflowTemplate => "workflow_template",
            Self::SkillTemplate => "skill_template",
            Self::VfsMountTemplate => "vfs_mount_template",
            Self::ExtensionTemplate => "extension_template",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "agent_template" => Ok(Self::AgentTemplate),
            "mcp_server_template" => Ok(Self::McpServerTemplate),
            "workflow_template" => Ok(Self::WorkflowTemplate),
            "skill_template" => Ok(Self::SkillTemplate),
            "vfs_mount_template" => Ok(Self::VfsMountTemplate),
            "extension_template" => Ok(Self::ExtensionTemplate),
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
    PluginEmbedded,
}

impl LibraryAssetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::UserAuthored => "user_authored",
            Self::RemoteImported => "remote_imported",
            Self::PluginEmbedded => "plugin_embedded",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "builtin" => Ok(Self::Builtin),
            "user_authored" => Ok(Self::UserAuthored),
            "remote_imported" => Ok(Self::RemoteImported),
            "plugin_embedded" => Ok(Self::PluginEmbedded),
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

impl SharedLibrarySourceStatus {
    pub fn from_installed_source(
        installed: &InstalledAssetSource,
        current_version: Option<&str>,
        current_digest: Option<&str>,
        current_deprecated: bool,
    ) -> Self {
        if current_deprecated || current_version.is_none() || current_digest.is_none() {
            return Self::SourceMissing;
        }
        if current_version == Some(installed.source_version.as_str())
            && current_digest == Some(installed.source_digest.as_str())
        {
            Self::UpToDate
        } else {
            Self::UpdateAvailable
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpToDate => "up_to_date",
            Self::UpdateAvailable => "update_available",
            Self::SourceMissing => "source_missing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "asset_type", content = "payload", rename_all = "snake_case")]
pub enum LibraryAssetPayload {
    AgentTemplate(AgentTemplatePayload),
    McpServerTemplate(McpServerTemplatePayload),
    WorkflowTemplate(WorkflowTemplatePayload),
    SkillTemplate(SkillTemplatePayload),
    VfsMountTemplate(VfsMountTemplatePayload),
    ExtensionTemplate(ExtensionTemplatePayload),
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
            LibraryAssetType::WorkflowTemplate => {
                let value = normalize_workflow_template_payload_value(value)?;
                serde_json::from_value(value)
                    .map(Self::WorkflowTemplate)
                    .map_err(|error| payload_error("workflow_template", error))
            }
            LibraryAssetType::SkillTemplate => serde_json::from_value(value)
                .map(Self::SkillTemplate)
                .map_err(|error| payload_error("skill_template", error)),
            LibraryAssetType::VfsMountTemplate => {
                let payload = serde_json::from_value::<VfsMountTemplatePayload>(value)
                    .map_err(|error| payload_error("vfs_mount_template", error))?;
                payload.validate()?;
                Ok(Self::VfsMountTemplate(payload))
            }
            LibraryAssetType::ExtensionTemplate => {
                let payload = serde_json::from_value::<ExtensionTemplatePayload>(value)
                    .map_err(|error| payload_error("extension_template", error))?;
                payload.validate()?;
                Ok(Self::ExtensionTemplate(payload))
            }
        }
    }

    pub fn validate(asset_type: LibraryAssetType, value: &Value) -> Result<(), DomainError> {
        Self::from_value(asset_type, value.clone()).map(|_| ())
    }
}

pub fn normalize_workflow_template_payload_value(mut value: Value) -> Result<Value, DomainError> {
    let Some(template) = value.get_mut("template") else {
        return Ok(value);
    };
    normalize_workflow_template_value(template)?;
    Ok(value)
}

pub fn normalize_workflow_template_value(value: &mut Value) -> Result<(), DomainError> {
    let Some(lifecycle) = value.get_mut("lifecycle") else {
        return Ok(());
    };
    normalize_workflow_lifecycle_value(lifecycle)
}

pub fn normalize_workflow_lifecycle_value(lifecycle: &mut Value) -> Result<(), DomainError> {
    let Some(object) = lifecycle.as_object_mut() else {
        return Err(DomainError::InvalidConfig(
            "workflow_template.lifecycle 必须是对象".to_string(),
        ));
    };
    if object.contains_key("activities") && object.contains_key("entry_activity_key") {
        return Ok(());
    }

    let entry_step_key = object
        .remove("entry_step_key")
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| {
            DomainError::InvalidConfig(
                "workflow_template.lifecycle.entry_step_key 不能为空".to_string(),
            )
        })?;
    let steps = object
        .remove("steps")
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| {
            DomainError::InvalidConfig("workflow_template.lifecycle.steps 必须是数组".to_string())
        })?;
    let edges = object
        .remove("edges")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();

    object.insert(
        "entry_activity_key".to_string(),
        Value::String(entry_step_key),
    );
    object.insert(
        "activities".to_string(),
        Value::Array(
            steps
                .into_iter()
                .map(legacy_step_to_activity)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    object.insert(
        "transitions".to_string(),
        Value::Array(
            edges
                .into_iter()
                .map(legacy_edge_to_transition)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(())
}

fn legacy_step_to_activity(step: Value) -> Result<Value, DomainError> {
    let object = step.as_object().ok_or_else(|| {
        DomainError::InvalidConfig("workflow_template.lifecycle.steps[] 必须是对象".to_string())
    })?;
    let key = json_string_field(object, "key", "workflow_template.lifecycle.steps[].key")?;
    let workflow_key = json_string_field(
        object,
        "workflow_key",
        "workflow_template.lifecycle.steps[].workflow_key",
    )?;
    let node_type = object
        .get("node_type")
        .and_then(Value::as_str)
        .unwrap_or("agent_node");
    let output_ports = object
        .get("output_ports")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let input_ports = object
        .get("input_ports")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let required_ports = output_ports
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|port| port.get("key").and_then(Value::as_str))
        .map(|key| Value::String(key.to_string()))
        .collect::<Vec<_>>();

    let mut activity = Map::new();
    activity.insert("key".to_string(), Value::String(key));
    activity.insert(
        "description".to_string(),
        object
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
            .into(),
    );
    activity.insert(
        "executor".to_string(),
        json!({
            "kind": "agent",
            "workflow_key": workflow_key,
            "session_policy": if node_type == "phase_node" { "continue_root" } else { "spawn_child" },
        }),
    );
    activity.insert("input_ports".to_string(), input_ports);
    activity.insert("output_ports".to_string(), output_ports);
    activity.insert(
        "completion_policy".to_string(),
        if required_ports.is_empty() {
            json!({ "kind": "executor_terminal" })
        } else {
            json!({ "kind": "output_ports", "required_ports": required_ports })
        },
    );
    Ok(Value::Object(activity))
}

fn legacy_edge_to_transition(edge: Value) -> Result<Value, DomainError> {
    let object = edge.as_object().ok_or_else(|| {
        DomainError::InvalidConfig("workflow_template.lifecycle.edges[] 必须是对象".to_string())
    })?;
    let from = json_string_field(
        object,
        "from_node",
        "workflow_template.lifecycle.edges[].from_node",
    )?;
    let to = json_string_field(
        object,
        "to_node",
        "workflow_template.lifecycle.edges[].to_node",
    )?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("artifact");
    let artifact_bindings = if kind == "artifact" {
        let from_port = json_string_field(
            object,
            "from_port",
            "workflow_template.lifecycle.edges[].from_port",
        )?;
        let to_port = json_string_field(
            object,
            "to_port",
            "workflow_template.lifecycle.edges[].to_port",
        )?;
        json!([{
            "from_port": from_port,
            "to_port": to_port,
            "alias": "latest"
        }])
    } else {
        Value::Array(Vec::new())
    };

    Ok(json!({
        "from": from,
        "to": to,
        "kind": if kind == "artifact" { "artifact" } else { "flow" },
        "condition": { "kind": "always" },
        "artifact_bindings": artifact_bindings,
    }))
}

fn json_string_field(
    object: &Map<String, Value>,
    field: &str,
    field_path: &str,
) -> Result<String, DomainError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainError::InvalidConfig(format!("{field_path} 不能为空")))
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VfsMountTemplatePayload {
    Inline {
        mount_id: String,
        display_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default)]
        capabilities: Vec<MountCapability>,
        files: Vec<InlineMountFilePayload>,
    },
    ExternalService {
        mount_id: String,
        display_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default)]
        capabilities: Vec<MountCapability>,
        service_id: String,
        root_ref: String,
    },
}

impl VfsMountTemplatePayload {
    pub fn mount_id(&self) -> &str {
        match self {
            Self::Inline { mount_id, .. } | Self::ExternalService { mount_id, .. } => {
                mount_id.as_str()
            }
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Inline { display_name, .. } | Self::ExternalService { display_name, .. } => {
                display_name.as_str()
            }
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Inline { description, .. } | Self::ExternalService { description, .. } => {
                description.as_deref()
            }
        }
    }

    pub fn capabilities(&self) -> &[MountCapability] {
        match self {
            Self::Inline { capabilities, .. } | Self::ExternalService { capabilities, .. } => {
                capabilities
            }
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        require_non_empty("vfs_mount_template.mount_id", self.mount_id())?;
        require_non_empty("vfs_mount_template.display_name", self.display_name())?;
        match self {
            Self::Inline { files, .. } => {
                if files.is_empty() {
                    return Err(DomainError::InvalidConfig(
                        "vfs_mount_template.files 不能为空".to_string(),
                    ));
                }
                for (index, file) in files.iter().enumerate() {
                    file.validate(index)?;
                }
            }
            Self::ExternalService {
                service_id,
                root_ref,
                ..
            } => {
                require_non_empty("vfs_mount_template.service_id", service_id)?;
                require_non_empty("vfs_mount_template.root_ref", root_ref)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InlineMountFilePayload {
    pub path: String,
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

impl InlineMountFilePayload {
    fn validate(&self, index: usize) -> Result<(), DomainError> {
        require_non_empty(
            &format!("vfs_mount_template.files[{index}].path"),
            &self.path,
        )?;
        match self.content_kind.as_str() {
            "text" => {
                if self.content.is_none() {
                    return Err(DomainError::InvalidConfig(format!(
                        "vfs_mount_template.files[{index}].content 不能为空"
                    )));
                }
            }
            "binary" => {
                if self.data_base64.is_none() {
                    return Err(DomainError::InvalidConfig(format!(
                        "vfs_mount_template.files[{index}].data_base64 不能为空"
                    )));
                }
                require_non_empty(
                    &format!("vfs_mount_template.files[{index}].mime_type"),
                    self.mime_type.as_deref().unwrap_or_default(),
                )?;
            }
            other => {
                return Err(DomainError::InvalidConfig(format!(
                    "vfs_mount_template.files[{index}].content_kind 非法: {other}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionTemplatePayload {
    pub manifest_version: String,
    pub extension_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<ExtensionCommandDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<ExtensionFlagDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_renderers: Vec<ExtensionMessageRendererDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_directives: Vec<ToolCapabilityDirective>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_refs: Vec<ExtensionAssetRef>,
}

impl ExtensionTemplatePayload {
    pub fn validate(&self) -> Result<(), DomainError> {
        require_non_empty(
            "extension_template.manifest_version",
            &self.manifest_version,
        )?;
        require_non_empty("extension_template.extension_id", &self.extension_id)?;
        for command in &self.commands {
            command.validate()?;
        }
        for flag in &self.flags {
            flag.validate()?;
        }
        for renderer in &self.message_renderers {
            renderer.validate()?;
        }
        for asset_ref in &self.asset_refs {
            asset_ref.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionCommandDefinition {
    pub name: String,
    pub description: String,
    pub handler: ExtensionCommandHandler,
}

impl ExtensionCommandDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        require_non_empty("extension_template.commands[].name", &self.name)?;
        if self.name.starts_with('/') || self.name.contains('/') {
            return Err(DomainError::InvalidConfig(
                "extension_template command name 不应包含 `/`".to_string(),
            ));
        }
        self.handler.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionCommandHandler {
    InjectMessage { content: String },
}

impl ExtensionCommandHandler {
    fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::InjectMessage { content } => {
                require_non_empty("extension_template.commands[].handler.content", content)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionFlagType {
    Bool,
    String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionFlagDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub flag_type: ExtensionFlagType,
    pub default: Value,
    pub description: String,
}

impl ExtensionFlagDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        require_non_empty("extension_template.flags[].name", &self.name)?;
        let valid_default = match self.flag_type {
            ExtensionFlagType::Bool => self.default.is_boolean(),
            ExtensionFlagType::String => self.default.is_string(),
        };
        if !valid_default {
            return Err(DomainError::InvalidConfig(format!(
                "extension_template flag `{}` 的 default 与 type 不匹配",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionMessageRendererDefinition {
    pub custom_type: String,
    pub renderer: ExtensionRendererDeclaration,
}

impl ExtensionMessageRendererDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        require_non_empty(
            "extension_template.message_renderers[].custom_type",
            &self.custom_type,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionRendererDeclaration {
    JsonCard,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionAssetRef {
    pub asset_type: String,
    pub key: String,
    #[serde(default)]
    pub required: bool,
}

impl ExtensionAssetRef {
    fn validate(&self) -> Result<(), DomainError> {
        require_non_empty(
            "extension_template.asset_refs[].asset_type",
            &self.asset_type,
        )?;
        require_non_empty("extension_template.asset_refs[].key", &self.key)
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuiltinSeed {
    pub asset_type: LibraryAssetType,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    pub source_ref: String,
    pub payload_digest: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginLibraryAssetSeed {
    pub asset_type: LibraryAssetType,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    pub payload: Value,
}

impl PluginLibraryAssetSeed {
    pub fn validate(&self) -> Result<(), DomainError> {
        LibraryAssetPayload::validate(self.asset_type, &self.payload)
    }
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
    fn validates_extension_template_payload() {
        let payload = json!({
            "manifest_version": "1",
            "extension_id": "gitlab-review",
            "commands": [{
                "name": "gitlab-review:prepare",
                "description": "准备 review",
                "handler": { "kind": "inject_message", "content": "请准备 review。" }
            }],
            "flags": [{
                "name": "gitlab-review.verbose",
                "type": "bool",
                "default": false,
                "description": "详细输出"
            }],
            "message_renderers": [{
                "custom_type": "gitlab-review.summary",
                "renderer": { "kind": "json_card" }
            }]
        });

        let typed = LibraryAssetPayload::from_value(LibraryAssetType::ExtensionTemplate, payload)
            .expect("valid extension template");

        assert!(matches!(typed, LibraryAssetPayload::ExtensionTemplate(_)));
    }

    #[test]
    fn rejects_extension_flag_default_type_mismatch() {
        let payload = json!({
            "manifest_version": "1",
            "extension_id": "bad",
            "flags": [{
                "name": "bad.verbose",
                "type": "bool",
                "default": "yes",
                "description": "bad"
            }]
        });

        let result = LibraryAssetPayload::from_value(LibraryAssetType::ExtensionTemplate, payload);

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

    #[test]
    fn normalizes_legacy_workflow_template_lifecycle_payload() {
        let payload = json!({
            "template": {
                "key": "review_flow",
                "name": "Review Flow",
                "description": "desc",
                "binding_kinds": ["story"],
                "workflows": [],
                "lifecycle": {
                    "key": "review_flow",
                    "name": "Review Flow",
                    "description": "desc",
                    "entry_step_key": "plan",
                    "steps": [{
                        "key": "plan",
                        "workflow_key": "review_plan",
                        "node_type": "agent_node",
                        "output_ports": [{"key": "proposal", "description": "Proposal"}]
                    }, {
                        "key": "apply",
                        "workflow_key": "review_apply",
                        "node_type": "phase_node",
                        "input_ports": [{"key": "proposal", "description": "Proposal"}]
                    }],
                    "edges": [{
                        "kind": "artifact",
                        "from_node": "plan",
                        "from_port": "proposal",
                        "to_node": "apply",
                        "to_port": "proposal"
                    }]
                }
            }
        });

        let normalized = normalize_workflow_template_payload_value(payload).expect("normalize");
        let lifecycle = &normalized["template"]["lifecycle"];

        assert_eq!(lifecycle["entry_activity_key"], "plan");
        assert!(lifecycle.get("entry_step_key").is_none());
        assert_eq!(lifecycle["activities"][0]["executor"]["kind"], "agent");
        assert_eq!(
            lifecycle["activities"][0]["completion_policy"]["kind"],
            "output_ports"
        );
        assert_eq!(
            lifecycle["activities"][1]["executor"]["session_policy"],
            "continue_root"
        );
        assert_eq!(lifecycle["transitions"][0]["kind"], "artifact");
        assert_eq!(
            lifecycle["transitions"][0]["artifact_bindings"][0]["from_port"],
            "proposal"
        );
    }
}
