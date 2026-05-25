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

impl From<McpHttpHeader> for domain::McpHttpHeader {
    fn from(header: McpHttpHeader) -> Self {
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

impl From<McpEnvVar> for domain::McpEnvVar {
    fn from(env: McpEnvVar) -> Self {
        Self {
            name: env.name,
            value: env.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]

pub enum McpTransportConfig {
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
    },
}

impl From<domain::McpTransportConfig> for McpTransportConfig {
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
            domain::McpTransportConfig::Stdio { command, args, env } => Self::Stdio {
                command,
                args,
                env: env.into_iter().map(Into::into).collect(),
            },
        }
    }
}

impl From<McpTransportConfig> for domain::McpTransportConfig {
    fn from(config: McpTransportConfig) -> Self {
        match config {
            McpTransportConfig::Http { url, headers } => Self::Http {
                url,
                headers: headers.into_iter().map(Into::into).collect(),
            },
            McpTransportConfig::Sse { url, headers } => Self::Sse {
                url,
                headers: headers.into_iter().map(Into::into).collect(),
            },
            McpTransportConfig::Stdio { command, args, env } => Self::Stdio {
                command,
                args,
                env: env.into_iter().map(Into::into).collect(),
            },
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

impl From<McpRoutePolicy> for domain::McpRoutePolicy {
    fn from(policy: McpRoutePolicy) -> Self {
        match policy {
            McpRoutePolicy::Auto => Self::Auto,
            McpRoutePolicy::Relay => Self::Relay,
            McpRoutePolicy::Direct => Self::Direct,
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
    pub transport: McpTransportConfig,
    pub route_policy: McpRoutePolicy,
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
            source,
            builtin_key,
            installed_source: preset.installed_source.map(Into::into),
            created_at: preset.created_at,
            updated_at: preset.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]

pub struct CreateMcpPresetRequest {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub route_policy: McpRoutePolicy,
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
    pub transport: Option<McpTransportConfig>,
    #[serde(default)]
    #[ts(optional)]
    pub route_policy: Option<McpRoutePolicy>,
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
}
