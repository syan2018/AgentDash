use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use agentdash_domain::mcp_preset as domain;
use agentdash_domain::shared_library::InstalledAssetSource;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct McpHttpHeader {
    pub name: String,
    pub value: String,
}

impl From<domain::McpHttpHeader> for McpHttpHeader {
    fn from(header: domain::McpHttpHeader) -> Self {
        Self {
            name: header.name,
            value: header.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct McpEnvVar {
    pub name: String,
    pub value: String,
}

impl From<domain::McpEnvVar> for McpEnvVar {
    fn from(env: domain::McpEnvVar) -> Self {
        Self {
            name: env.name,
            value: env.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfigDto {
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
    },
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<McpEnvVar>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        cwd: Option<String>,
    },
}

impl From<domain::McpTransportConfig> for McpTransportConfigDto {
    fn from(config: domain::McpTransportConfig) -> Self {
        match config {
            domain::McpTransportConfig::Http { url, headers } => Self::Http {
                url,
                headers: headers.into_iter().map(Into::into).collect(),
            },
            domain::McpTransportConfig::Sse { url, headers } => Self::Sse {
                url,
                headers: headers.into_iter().map(Into::into).collect(),
            },
            domain::McpTransportConfig::Stdio {
                command,
                args,
                env,
                cwd,
            } => Self::Stdio {
                command,
                args,
                env: env.into_iter().map(Into::into).collect(),
                cwd,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct McpRuntimeBindingConfigDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<McpRuntimeBindingRuleDto>,
}

impl From<domain::McpRuntimeBindingConfig> for McpRuntimeBindingConfigDto {
    fn from(config: domain::McpRuntimeBindingConfig) -> Self {
        Self {
            mount_id: config.mount_id,
            bindings: config.bindings.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct McpRuntimeBindingRuleDto {
    pub source: McpRuntimeBindingSourceDto,
    pub target: McpRuntimeBindingTargetDto,
    #[serde(default)]
    pub required: bool,
}

impl From<domain::McpRuntimeBindingRule> for McpRuntimeBindingRuleDto {
    fn from(rule: domain::McpRuntimeBindingRule) -> Self {
        Self {
            source: rule.source.into(),
            target: rule.target.into(),
            required: rule.required,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpRuntimeBindingSourceDto {
    VfsRootRef,
    #[serde(rename = "runtime_backend_anchor_backend_id", alias = "vfs_backend_id")]
    RuntimeBackendAnchorBackendId,
    WorkspaceId,
    WorkspaceBindingId,
    WorkspaceIdentity {
        path: Vec<String>,
    },
    WorkspaceDetectedFact {
        path: Vec<String>,
    },
}

impl From<domain::McpRuntimeBindingSource> for McpRuntimeBindingSourceDto {
    fn from(source: domain::McpRuntimeBindingSource) -> Self {
        match source {
            domain::McpRuntimeBindingSource::VfsRootRef => Self::VfsRootRef,
            domain::McpRuntimeBindingSource::RuntimeBackendAnchorBackendId => {
                Self::RuntimeBackendAnchorBackendId
            }
            domain::McpRuntimeBindingSource::WorkspaceId => Self::WorkspaceId,
            domain::McpRuntimeBindingSource::WorkspaceBindingId => Self::WorkspaceBindingId,
            domain::McpRuntimeBindingSource::WorkspaceIdentity { path } => {
                Self::WorkspaceIdentity { path }
            }
            domain::McpRuntimeBindingSource::WorkspaceDetectedFact { path } => {
                Self::WorkspaceDetectedFact { path }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpRuntimeBindingTargetDto {
    HttpQuery { name: String },
    HttpHeader { name: String },
    StdioEnv { name: String },
    StdioCwd,
}

impl From<domain::McpRuntimeBindingTarget> for McpRuntimeBindingTargetDto {
    fn from(target: domain::McpRuntimeBindingTarget) -> Self {
        match target {
            domain::McpRuntimeBindingTarget::HttpQuery { name } => Self::HttpQuery { name },
            domain::McpRuntimeBindingTarget::HttpHeader { name } => Self::HttpHeader { name },
            domain::McpRuntimeBindingTarget::StdioEnv { name } => Self::StdioEnv { name },
            domain::McpRuntimeBindingTarget::StdioCwd => Self::StdioCwd,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpRoutePolicy {
    #[default]
    Auto,
    Relay,
    Direct,
}

impl From<domain::McpRoutePolicy> for McpRoutePolicy {
    fn from(policy: domain::McpRoutePolicy) -> Self {
        match policy {
            domain::McpRoutePolicy::Auto => Self::Auto,
            domain::McpRoutePolicy::Relay => Self::Relay,
            domain::McpRoutePolicy::Direct => Self::Direct,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpPresetSourceTag {
    Builtin,
    User,
}

impl From<domain::McpPresetSource> for McpPresetSourceTag {
    fn from(source: domain::McpPresetSource) -> Self {
        match source {
            domain::McpPresetSource::Builtin { .. } => Self::Builtin,
            domain::McpPresetSource::User => Self::User,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct InstalledAssetSourceResponse {
    pub library_asset_id: String,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: DateTime<Utc>,
}

impl From<InstalledAssetSource> for InstalledAssetSourceResponse {
    fn from(source: InstalledAssetSource) -> Self {
        Self {
            library_asset_id: source.library_asset_id.to_string(),
            source_ref: source.source_ref,
            source_version: source.source_version,
            source_digest: source.source_digest,
            installed_at: source.installed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
pub struct McpPresetResponse {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub transport: McpTransportConfigDto,
    pub route_policy: McpRoutePolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
    pub source: McpPresetSourceTag,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub builtin_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_source: Option<InstalledAssetSourceResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<domain::McpPreset> for McpPresetResponse {
    fn from(preset: domain::McpPreset) -> Self {
        let source = preset.source.clone().into();
        let builtin_key = match &preset.source {
            domain::McpPresetSource::Builtin { key } => Some(key.clone()),
            domain::McpPresetSource::User => None,
        };
        Self {
            id: preset.id.to_string(),
            project_id: preset.project_id.to_string(),
            key: preset.key,
            display_name: preset.display_name,
            description: preset.description,
            transport: preset.transport.into(),
            route_policy: preset.route_policy.into(),
            runtime_binding: preset.runtime_binding.map(Into::into),
            source,
            builtin_key,
            installed_source: preset.installed_source.map(Into::into),
            created_at: preset.created_at,
            updated_at: preset.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DeleteMcpPresetResponse {
    pub deleted: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateMcpPresetRequest {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    pub transport: McpTransportConfigDto,
    #[serde(default)]
    pub route_policy: McpRoutePolicy,
    #[serde(default)]
    #[ts(optional)]
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
}

#[derive(Debug, Clone, Deserialize, Default, TS)]
pub struct UpdateMcpPresetRequest {
    #[serde(default)]
    #[ts(optional)]
    pub key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(optional)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    #[ts(optional)]
    pub transport: Option<McpTransportConfigDto>,
    #[serde(default)]
    #[ts(optional)]
    pub route_policy: Option<McpRoutePolicy>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(optional)]
    pub runtime_binding: Option<Option<McpRuntimeBindingConfigDto>>,
}

#[derive(Debug, Clone, Deserialize, Default, TS)]
pub struct CloneMcpPresetRequest {
    #[serde(default)]
    #[ts(optional)]
    pub key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default, TS)]
pub struct ListMcpPresetQuery {
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<McpPresetSourceTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProbeMcpPresetRequest {
    pub transport: McpTransportConfigDto,
    #[serde(default)]
    #[ts(optional)]
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeMcpPresetResponse {
    Ok {
        latency_ms: u32,
        tools: Vec<ProbeMcpToolInfo>,
    },
    Error {
        error: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ProbeMcpToolInfo {
    pub name: String,
    pub description: String,
}

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_request_description_triple_state_missing() {
        let raw = r#"{"key":"new-key"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse missing");
        assert!(parsed.description.is_none());
        assert_eq!(parsed.key.as_deref(), Some("new-key"));
    }

    #[test]
    fn update_request_description_triple_state_null() {
        let raw = r#"{"description":null}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse null");
        assert_eq!(parsed.description, Some(None));
    }

    #[test]
    fn update_request_description_triple_state_value() {
        let raw = r#"{"description":"updated"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse value");
        assert_eq!(parsed.description, Some(Some("updated".to_string())));
    }

    #[test]
    fn update_request_runtime_binding_triple_state_missing() {
        let raw = r#"{"key":"new-key"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse missing");
        assert!(parsed.runtime_binding.is_none());
    }

    #[test]
    fn update_request_runtime_binding_triple_state_null() {
        let raw = r#"{"runtime_binding":null}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse null");
        assert_eq!(parsed.runtime_binding, Some(None));
    }

    #[test]
    fn update_request_runtime_binding_triple_state_value() {
        let raw = r#"{
            "runtime_binding": {
                "mount_id": "main",
                "bindings": [{
                    "source": { "kind": "workspace_detected_fact", "path": ["p4", "client_name"] },
                    "target": { "kind": "http_query", "name": "p4_client" },
                    "required": true
                }]
            }
        }"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse value");
        let binding = parsed
            .runtime_binding
            .expect("field should be present")
            .expect("binding should be object");
        assert_eq!(binding.mount_id.as_deref(), Some("main"));
        assert_eq!(binding.bindings.len(), 1);
        assert!(binding.bindings[0].required);
    }
}
