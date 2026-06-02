use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::DomainError;
use crate::common::{MountCapability, SystemPromptMode, ThinkingLevel};
use crate::extension_package::ExtensionPackageMetadata;
use crate::mcp_preset::{McpRoutePolicy, McpTransportConfig};
use crate::skill_asset::SkillAssetFileKind;
use crate::workflow::ToolCapabilityDirective;

pub const EXTENSION_PERMISSION_LOCAL_PROFILE_READ: &str = "local.profile.read";
pub const EXTENSION_PERMISSION_PROCESS_EXECUTE: &str = "process.execute";

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

/// 校验 lifecycle 对象使用当前格式 (`entry_activity_key` / `activities`)。
pub fn normalize_workflow_lifecycle_value(lifecycle: &mut Value) -> Result<(), DomainError> {
    let Some(object) = lifecycle.as_object_mut() else {
        return Err(DomainError::InvalidConfig(
            "workflow_template.lifecycle 必须是对象".to_string(),
        ));
    };
    if !object.contains_key("entry_activity_key") {
        return Err(DomainError::InvalidConfig(
            "workflow_template.lifecycle.entry_activity_key 不能为空".to_string(),
        ));
    }
    if !object.contains_key("activities") {
        return Err(DomainError::InvalidConfig(
            "workflow_template.lifecycle.activities 必须存在".to_string(),
        ));
    }
    Ok(())
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
    pub package: ExtensionPackageMetadata,
    pub asset_version: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_actions: Vec<ExtensionRuntimeActionDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_channels: Vec<ExtensionProtocolChannelDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extension_dependencies: Vec<ExtensionDependencyDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_tabs: Vec<ExtensionWorkspaceTabDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<ExtensionPermissionDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundles: Vec<ExtensionBundleRef>,
}

impl ExtensionTemplatePayload {
    pub fn validate(&self) -> Result<(), DomainError> {
        require_non_empty(
            "extension_template.manifest_version",
            &self.manifest_version,
        )?;
        require_non_empty("extension_template.extension_id", &self.extension_id)?;
        self.package.validate()?;
        require_non_empty("extension_template.asset_version", &self.asset_version)?;
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
        for action in &self.runtime_actions {
            action.validate()?;
        }
        for channel in &self.protocol_channels {
            channel.validate()?;
        }
        for dependency in &self.extension_dependencies {
            dependency.validate()?;
        }
        for tab in &self.workspace_tabs {
            tab.validate()?;
        }
        for permission in &self.permissions {
            permission.validate()?;
        }
        for bundle in &self.bundles {
            bundle.validate()?;
        }
        Ok(())
    }

    pub fn requires_package_artifact(&self) -> bool {
        !self.runtime_actions.is_empty()
            || !self.protocol_channels.is_empty()
            || !self.workspace_tabs.is_empty()
            || !self.bundles.is_empty()
    }

    pub fn grants_local_profile_read(&self) -> bool {
        self.permissions.iter().any(|permission| {
            matches!(
                permission,
                ExtensionPermissionDeclaration::LocalProfile {
                    access: ExtensionPermissionAccess::Read | ExtensionPermissionAccess::ReadWrite
                }
            )
        })
    }

    pub fn action_declares_local_profile_read(&self, action_key: &str) -> bool {
        self.runtime_actions
            .iter()
            .find(|action| action.action_key == action_key)
            .map(|action| {
                action
                    .permissions
                    .iter()
                    .any(|permission| permission == EXTENSION_PERMISSION_LOCAL_PROFILE_READ)
            })
            .unwrap_or(false)
    }

    pub fn allows_local_profile_read_for_action(&self, action_key: &str) -> bool {
        self.evaluate_action_permission(action_key, EXTENSION_PERMISSION_LOCAL_PROFILE_READ)
            .allowed
    }

    pub fn evaluate_action_permission(
        &self,
        action_key: &str,
        requested_permission: &str,
    ) -> ExtensionPermissionDecision {
        let action = self
            .runtime_actions
            .iter()
            .find(|action| action.action_key == action_key);
        let capability_family = classify_extension_permission_key(requested_permission);
        let has_action_declaration = action
            .map(|action| {
                action
                    .permissions
                    .iter()
                    .any(|permission| permission == requested_permission)
            })
            .unwrap_or(false);
        let reason = if action.is_none() {
            ExtensionPermissionDecisionReason::MissingRuntimeAction
        } else if capability_family == "unknown" {
            ExtensionPermissionDecisionReason::UnknownPermission
        } else if !has_action_declaration {
            ExtensionPermissionDecisionReason::MissingActionDeclaration
        } else {
            ExtensionPermissionDecisionReason::Allowed
        };
        ExtensionPermissionDecision {
            requested_permission: requested_permission.to_string(),
            action_key: action_key.to_string(),
            capability_family: capability_family.to_string(),
            allowed: reason == ExtensionPermissionDecisionReason::Allowed,
            reason,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionPermissionDecision {
    pub requested_permission: String,
    pub action_key: String,
    pub capability_family: String,
    pub allowed: bool,
    pub reason: ExtensionPermissionDecisionReason,
}

impl ExtensionPermissionDecision {
    pub fn denial_message(&self) -> String {
        match self.reason {
            ExtensionPermissionDecisionReason::Allowed => "permission allowed".to_string(),
            ExtensionPermissionDecisionReason::MissingRuntimeAction => {
                format!("extension action `{}` 不存在", self.action_key)
            }
            ExtensionPermissionDecisionReason::MissingExtensionGrant => format!(
                "extension 顶层未声明 {} capability",
                self.requested_permission
            ),
            ExtensionPermissionDecisionReason::MissingActionDeclaration => format!(
                "extension action `{}` 未声明 {}",
                self.action_key, self.requested_permission
            ),
            ExtensionPermissionDecisionReason::UnknownPermission => {
                format!(
                    "extension action 声明了未知权限: {}",
                    self.requested_permission
                )
            }
        }
    }
}

fn classify_extension_permission_key(permission: &str) -> &'static str {
    if permission == EXTENSION_PERMISSION_LOCAL_PROFILE_READ {
        "local_profile"
    } else if permission.starts_with("http.fetch") {
        "http"
    } else if permission.starts_with("workspace.vfs.") {
        "workspace"
    } else if permission.starts_with("env.read") {
        "env"
    } else if permission == EXTENSION_PERMISSION_PROCESS_EXECUTE
        || permission.starts_with("process.run")
    {
        "process"
    } else if permission.starts_with("runtime.invoke") {
        "runtime_action"
    } else if permission.starts_with("extension.channel.invoke") {
        "extension_channel"
    } else {
        "unknown"
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionPermissionDecisionReason {
    Allowed,
    MissingRuntimeAction,
    MissingExtensionGrant,
    MissingActionDeclaration,
    UnknownPermission,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRuntimeActionKind {
    SessionRuntime,
    Setup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionRuntimeActionDefinition {
    pub action_key: String,
    pub kind: ExtensionRuntimeActionKind,
    pub description: String,
    #[serde(default = "empty_json_schema")]
    pub input_schema: Value,
    #[serde(default = "empty_json_schema")]
    pub output_schema: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
}

impl ExtensionRuntimeActionDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        validate_runtime_action_key(
            "extension_template.runtime_actions[].action_key",
            &self.action_key,
        )?;
        require_non_empty(
            "extension_template.runtime_actions[].description",
            &self.description,
        )?;
        validate_json_schema(
            "extension_template.runtime_actions[].input_schema",
            &self.input_schema,
        )?;
        validate_json_schema(
            "extension_template.runtime_actions[].output_schema",
            &self.output_schema,
        )?;
        for permission in &self.permissions {
            validate_permission_key(
                "extension_template.runtime_actions[].permissions[]",
                permission,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionProtocolChannelDefinition {
    pub channel_key: String,
    pub version: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<ExtensionProtocolChannelMethodDefinition>,
}

impl ExtensionProtocolChannelDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        validate_namespaced_extension_key(
            "extension_template.protocol_channels[].channel_key",
            &self.channel_key,
        )?;
        require_non_empty(
            "extension_template.protocol_channels[].version",
            &self.version,
        )?;
        require_non_empty(
            "extension_template.protocol_channels[].description",
            &self.description,
        )?;
        if self.methods.is_empty() {
            return Err(DomainError::InvalidConfig(
                "extension_template.protocol_channels[].methods 不能为空".to_string(),
            ));
        }
        for method in &self.methods {
            method.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionProtocolChannelMethodDefinition {
    pub name: String,
    pub description: String,
    #[serde(default = "empty_json_schema")]
    pub input_schema: Value,
    #[serde(default = "empty_json_schema")]
    pub output_schema: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
}

impl ExtensionProtocolChannelMethodDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        validate_protocol_method_name(
            "extension_template.protocol_channels[].methods[].name",
            &self.name,
        )?;
        require_non_empty(
            "extension_template.protocol_channels[].methods[].description",
            &self.description,
        )?;
        validate_json_schema(
            "extension_template.protocol_channels[].methods[].input_schema",
            &self.input_schema,
        )?;
        validate_json_schema(
            "extension_template.protocol_channels[].methods[].output_schema",
            &self.output_schema,
        )?;
        for permission in &self.permissions {
            validate_permission_key(
                "extension_template.protocol_channels[].methods[].permissions[]",
                permission,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionDependencyDeclaration {
    pub alias: String,
    pub extension_id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
}

impl ExtensionDependencyDeclaration {
    fn validate(&self) -> Result<(), DomainError> {
        validate_extension_alias(
            "extension_template.extension_dependencies[].alias",
            &self.alias,
        )?;
        validate_extension_id(
            "extension_template.extension_dependencies[].extension_id",
            &self.extension_id,
        )?;
        require_non_empty(
            "extension_template.extension_dependencies[].version",
            &self.version,
        )?;
        if self.channels.is_empty() {
            return Err(DomainError::InvalidConfig(
                "extension_template.extension_dependencies[].channels 不能为空".to_string(),
            ));
        }
        for channel in &self.channels {
            validate_namespaced_extension_key(
                "extension_template.extension_dependencies[].channels[]",
                channel,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionWorkspaceTabDefinition {
    pub type_id: String,
    pub label: String,
    pub uri_scheme: String,
    pub renderer: ExtensionWorkspaceTabRendererDeclaration,
}

impl ExtensionWorkspaceTabDefinition {
    fn validate(&self) -> Result<(), DomainError> {
        validate_extension_qualified_id(
            "extension_template.workspace_tabs[].type_id",
            &self.type_id,
        )?;
        require_non_empty("extension_template.workspace_tabs[].label", &self.label)?;
        validate_uri_scheme(
            "extension_template.workspace_tabs[].uri_scheme",
            &self.uri_scheme,
        )?;
        self.renderer.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionWorkspaceTabRendererDeclaration {
    Webview { entry: String },
    CanvasPanel { entry: String },
}

impl ExtensionWorkspaceTabRendererDeclaration {
    fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Webview { entry } => {
                require_non_empty("extension_template.workspace_tabs[].renderer.entry", entry)
            }
            Self::CanvasPanel { entry } => {
                require_non_empty("extension_template.workspace_tabs[].renderer.entry", entry)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionPermissionAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionProcessPermissionAccess {
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionPermissionDeclaration {
    LocalProfile {
        access: ExtensionPermissionAccess,
    },
    Http {
        hosts: Vec<String>,
        access: ExtensionPermissionAccess,
    },
    Workspace {
        access: ExtensionPermissionAccess,
    },
    Env {
        names: Vec<String>,
        access: ExtensionPermissionAccess,
    },
    Process {
        access: ExtensionProcessPermissionAccess,
    },
    RuntimeAction {
        action_key: String,
    },
    ExtensionChannel {
        channel_key: String,
        methods: Vec<String>,
    },
}

impl ExtensionPermissionDeclaration {
    fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::LocalProfile { .. } | Self::Workspace { .. } | Self::Process { .. } => Ok(()),
            Self::Http { hosts, .. } => {
                validate_non_empty_string_list("extension_template.permissions[].hosts", hosts)
            }
            Self::Env { names, .. } => {
                validate_non_empty_string_list("extension_template.permissions[].names", names)
            }
            Self::RuntimeAction { action_key } => validate_runtime_action_key(
                "extension_template.permissions[].action_key",
                action_key,
            ),
            Self::ExtensionChannel {
                channel_key,
                methods,
            } => {
                validate_namespaced_extension_key(
                    "extension_template.permissions[].channel_key",
                    channel_key,
                )?;
                if methods.is_empty() {
                    return Err(DomainError::InvalidConfig(
                        "extension_template.permissions[].methods 不能为空".to_string(),
                    ));
                }
                for method in methods {
                    validate_protocol_method_name(
                        "extension_template.permissions[].methods[]",
                        method,
                    )?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBundleKind {
    ExtensionHost,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionBundleRef {
    pub kind: ExtensionBundleKind,
    pub entry: String,
    pub digest: String,
}

impl ExtensionBundleRef {
    fn validate(&self) -> Result<(), DomainError> {
        require_non_empty("extension_template.bundles[].entry", &self.entry)?;
        validate_bundle_digest("extension_template.bundles[].digest", &self.digest)
    }
}

fn empty_json_schema() -> Value {
    Value::Object(Default::default())
}

fn validate_json_schema(field: &str, value: &Value) -> Result<(), DomainError> {
    if value.is_object() || value.is_boolean() {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须是 JSON Schema 对象或布尔值"
        )))
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    }
    Ok(())
}

fn validate_runtime_action_key(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let valid = value.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    });
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须由小写字母、数字、下划线、短横线和点分段组成: {value}"
        )))
    }
}

fn validate_extension_qualified_id(field: &str, value: &str) -> Result<(), DomainError> {
    validate_runtime_action_key(field, value)
}

fn validate_namespaced_extension_key(field: &str, value: &str) -> Result<(), DomainError> {
    validate_runtime_action_key(field, value)?;
    if value.split('.').count() < 2 {
        return Err(DomainError::InvalidConfig(format!(
            "{field} 必须包含 provider namespace: {value}"
        )));
    }
    Ok(())
}

fn validate_extension_id(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let valid = value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须由小写字母、数字、下划线和短横线组成: {value}"
        )))
    }
}

fn validate_extension_alias(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    };
    let valid = first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须以小写字母开头，并只包含小写字母、数字、下划线和短横线: {value}"
        )))
    }
}

fn validate_protocol_method_name(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    };
    let valid = first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须是合法 method 名称: {value}"
        )))
    }
}

fn validate_permission_key(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let valid = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '*' | '='));
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须是稳定 permission key: {value}"
        )))
    }
}

fn validate_non_empty_string_list(field: &str, values: &[String]) -> Result<(), DomainError> {
    if values.is_empty() {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    }
    for value in values {
        require_non_empty(field, value)?;
    }
    Ok(())
}

fn validate_uri_scheme(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    };
    let valid = first.is_ascii_lowercase()
        && chars.all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '+' || c == '.' || c == '-'
        });
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须是小写 URI scheme: {value}"
        )))
    }
}

fn validate_bundle_digest(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(DomainError::InvalidConfig(format!(
            "{field} 必须使用 sha256:<hex> 格式"
        )));
    };
    let valid = hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须包含 64 位 sha256 十六进制摘要"
        )))
    }
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
            "manifest_version": "2",
            "extension_id": "gitlab-review",
            "package": {
                "name": "gitlab-review",
                "version": "0.1.0"
            },
            "asset_version": "0.1.0",
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
            }],
            "runtime_actions": [{
                "action_key": "gitlab-review.prepare",
                "kind": "session_runtime",
                "description": "准备 review runtime action",
                "input_schema": {},
                "output_schema": {},
                "permissions": [
                    "local.profile.read",
                    "http.fetch:gitlab.example",
                    "env.read:GITLAB_TOKEN",
                    "process.execute",
                    "extension.channel.invoke:gitlab-review.api.listMergeRequests"
                ]
            }],
            "protocol_channels": [{
                "channel_key": "gitlab-review.api",
                "version": "1.0.0",
                "description": "GitLab review API channel",
                "methods": [{
                    "name": "listMergeRequests",
                    "description": "列出 merge requests",
                    "input_schema": true,
                    "output_schema": true,
                    "permissions": ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"]
                }]
            }],
            "extension_dependencies": [{
                "alias": "gitlab",
                "extension_id": "gitlab-review",
                "version": "^1.0.0",
                "channels": ["gitlab-review.api"]
            }],
            "workspace_tabs": [{
                "type_id": "gitlab-review.summary-panel",
                "label": "Review",
                "uri_scheme": "gitlab-review",
                "renderer": { "kind": "webview", "entry": "dist/panel/index.html" }
            }],
            "permissions": [{
                "kind": "local_profile",
                "access": "read"
            }, {
                "kind": "http",
                "hosts": ["gitlab.example"],
                "access": "read"
            }, {
                "kind": "env",
                "names": ["GITLAB_TOKEN"],
                "access": "read"
            }, {
                "kind": "process",
                "access": "execute"
            }, {
                "kind": "extension_channel",
                "channel_key": "gitlab-review.api",
                "methods": ["listMergeRequests"]
            }],
            "bundles": [{
                "kind": "extension_host",
                "entry": "dist/extension.js",
                "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }]
        });

        let typed = LibraryAssetPayload::from_value(LibraryAssetType::ExtensionTemplate, payload)
            .expect("valid extension template");

        assert!(matches!(typed, LibraryAssetPayload::ExtensionTemplate(_)));
    }

    #[test]
    fn classifies_extension_template_package_requirement() {
        let declaration_only = extension_template_from_json(json!({
            "commands": [{
                "name": "demo.say",
                "description": "Say hello",
                "handler": { "kind": "inject_message", "content": "hello" }
            }],
            "flags": [{
                "name": "demo.enabled",
                "type": "bool",
                "default": true,
                "description": "Enabled"
            }]
        }));
        assert!(!declaration_only.requires_package_artifact());

        let runtime_action = extension_template_from_json(json!({
            "runtime_actions": [{
                "action_key": "demo.run",
                "kind": "session_runtime",
                "description": "Run demo",
                "input_schema": {},
                "output_schema": {}
            }]
        }));
        assert!(runtime_action.requires_package_artifact());

        let protocol_channel = extension_template_from_json(json!({
            "protocol_channels": [{
                "channel_key": "demo.api",
                "version": "1.0.0",
                "description": "Demo API",
                "methods": [{
                    "name": "ping",
                    "description": "Ping",
                    "input_schema": {},
                    "output_schema": {}
                }]
            }]
        }));
        assert!(protocol_channel.requires_package_artifact());

        let workspace_tab = extension_template_from_json(json!({
            "workspace_tabs": [{
                "type_id": "demo.panel",
                "label": "Demo",
                "uri_scheme": "demo",
                "renderer": { "kind": "webview", "entry": "dist/panel/index.html" }
            }]
        }));
        assert!(workspace_tab.requires_package_artifact());

        let bundle = extension_template_from_json(json!({
            "bundles": [{
                "kind": "extension_host",
                "entry": "dist/extension.js",
                "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }]
        }));
        assert!(bundle.requires_package_artifact());
    }

    #[test]
    fn evaluates_local_profile_permission_contract() {
        let top_and_action = extension_template_for_permission(true, true);
        let decision = top_and_action.evaluate_action_permission(
            "gitlab-review.prepare",
            EXTENSION_PERMISSION_LOCAL_PROFILE_READ,
        );
        assert!(decision.allowed);
        assert_eq!(decision.reason, ExtensionPermissionDecisionReason::Allowed);

        let top_only = extension_template_for_permission(true, false);
        let decision = top_only.evaluate_action_permission(
            "gitlab-review.prepare",
            EXTENSION_PERMISSION_LOCAL_PROFILE_READ,
        );
        assert!(!decision.allowed);
        assert_eq!(
            decision.reason,
            ExtensionPermissionDecisionReason::MissingActionDeclaration
        );

        let action_only = extension_template_for_permission(false, true);
        let decision = action_only.evaluate_action_permission(
            "gitlab-review.prepare",
            EXTENSION_PERMISSION_LOCAL_PROFILE_READ,
        );
        assert!(decision.allowed);
        assert_eq!(decision.reason, ExtensionPermissionDecisionReason::Allowed);
        assert_eq!(decision.capability_family, "local_profile");

        let unknown = top_and_action
            .evaluate_action_permission("gitlab-review.prepare", "local.profile.admin");
        assert!(!unknown.allowed);
        assert_eq!(
            unknown.reason,
            ExtensionPermissionDecisionReason::UnknownPermission
        );
    }

    #[test]
    fn rejects_invalid_extension_runtime_contracts() {
        let bad_action = LibraryAssetPayload::from_value(
            LibraryAssetType::ExtensionTemplate,
            json!({
                "manifest_version": "2",
                "extension_id": "bad",
                "package": {
                    "name": "bad",
                    "version": "0.1.0"
                },
                "asset_version": "0.1.0",
                "runtime_actions": [{
                    "action_key": "Bad.Action",
                    "kind": "session_runtime",
                    "description": "bad",
                    "input_schema": {},
                    "output_schema": {}
                }]
            }),
        );
        assert!(bad_action.is_err());

        let bad_tab = LibraryAssetPayload::from_value(
            LibraryAssetType::ExtensionTemplate,
            json!({
                "manifest_version": "2",
                "extension_id": "bad",
                "package": {
                    "name": "bad",
                    "version": "0.1.0"
                },
                "asset_version": "0.1.0",
                "workspace_tabs": [{
                    "type_id": "bad.panel",
                    "label": "Bad",
                    "uri_scheme": "Bad",
                    "renderer": { "kind": "webview", "entry": "dist/index.html" }
                }]
            }),
        );
        assert!(bad_tab.is_err());

        let bad_bundle = LibraryAssetPayload::from_value(
            LibraryAssetType::ExtensionTemplate,
            json!({
                "manifest_version": "2",
                "extension_id": "bad",
                "package": {
                    "name": "bad",
                    "version": "0.1.0"
                },
                "asset_version": "0.1.0",
                "bundles": [{
                    "kind": "extension_host",
                    "entry": "dist/extension.js",
                    "digest": "sha256:not-a-digest"
                }]
            }),
        );
        assert!(bad_bundle.is_err());
    }

    #[test]
    fn rejects_extension_flag_default_type_mismatch() {
        let payload = json!({
            "manifest_version": "2",
            "extension_id": "bad",
            "package": {
                "name": "bad",
                "version": "0.1.0"
            },
            "asset_version": "0.1.0",
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

    fn extension_template_from_json(extra: serde_json::Value) -> ExtensionTemplatePayload {
        let mut payload = json!({
            "manifest_version": "2",
            "extension_id": "demo",
            "package": {
                "name": "@agentdash/demo",
                "version": "0.1.0"
            },
            "asset_version": "0.1.0"
        });
        let payload_object = payload.as_object_mut().expect("payload object");
        let extra_object = extra.as_object().expect("extra object");
        for (key, value) in extra_object {
            payload_object.insert(key.clone(), value.clone());
        }

        match LibraryAssetPayload::from_value(LibraryAssetType::ExtensionTemplate, payload)
            .expect("valid extension template")
        {
            LibraryAssetPayload::ExtensionTemplate(payload) => payload,
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    fn extension_template_for_permission(
        include_top_level_permission: bool,
        include_action_permission: bool,
    ) -> ExtensionTemplatePayload {
        let permissions = if include_top_level_permission {
            vec![ExtensionPermissionDeclaration::LocalProfile {
                access: ExtensionPermissionAccess::Read,
            }]
        } else {
            vec![]
        };
        let action_permissions = if include_action_permission {
            vec![EXTENSION_PERMISSION_LOCAL_PROFILE_READ.to_string()]
        } else {
            vec![]
        };
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "gitlab-review".to_string(),
            package: ExtensionPackageMetadata {
                name: "gitlab-review".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: "gitlab-review.prepare".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "Prepare review".to_string(),
                input_schema: json!({}),
                output_schema: json!({}),
                permissions: action_permissions,
            }],
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions,
            bundles: vec![],
        }
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
    fn normalize_accepts_current_format_lifecycle_payload() {
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
                    "entry_activity_key": "plan",
                    "activities": [{
                        "key": "plan",
                        "executor": { "kind": "agent", "procedure_key": "review_plan" },
                        "output_ports": [{"key": "proposal", "description": "Proposal"}]
                    }],
                    "transitions": []
                }
            }
        });

        let normalized = normalize_workflow_template_payload_value(payload).expect("normalize");
        assert_eq!(normalized["template"]["lifecycle"]["entry_activity_key"], "plan");
    }

    #[test]
    fn normalize_rejects_legacy_format_lifecycle_payload() {
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
                    "steps": [{ "key": "plan", "procedure_key": "review_plan" }]
                }
            }
        });

        let result = normalize_workflow_template_payload_value(payload);
        assert!(result.is_err());
    }
}
