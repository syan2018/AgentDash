use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_domain::skill_asset as domain;

use crate::shared_library::InstalledAssetSourceDto;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillAssetSource {
    BuiltinSeed,
    User,
    Github,
    Clawhub,
    SkillsSh,
}

impl From<&domain::SkillAssetSource> for SkillAssetSource {
    fn from(source: &domain::SkillAssetSource) -> Self {
        match source {
            domain::SkillAssetSource::BuiltinSeed { .. } => Self::BuiltinSeed,
            domain::SkillAssetSource::Github { .. } => Self::Github,
            domain::SkillAssetSource::Clawhub { .. } => Self::Clawhub,
            domain::SkillAssetSource::SkillsSh { .. } => Self::SkillsSh,
            domain::SkillAssetSource::User => Self::User,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSkillAssetSourceType {
    Github,
    Clawhub,
    SkillsSh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillAssetFileContentKind {
    Text,
    Binary,
}

fn default_text_content_kind() -> SkillAssetFileContentKind {
    SkillAssetFileContentKind::Text
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillAssetFileKind {
    Skill,
    Reference,
    Script,
    Asset,
}

impl From<domain::SkillAssetFileKind> for SkillAssetFileKind {
    fn from(kind: domain::SkillAssetFileKind) -> Self {
        match kind {
            domain::SkillAssetFileKind::Skill => Self::Skill,
            domain::SkillAssetFileKind::Reference => Self::Reference,
            domain::SkillAssetFileKind::Script => Self::Script,
            domain::SkillAssetFileKind::Asset => Self::Asset,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct RemoteSkillAssetSourceDto {
    pub source_type: RemoteSkillAssetSourceType,
    pub url: String,
    pub imported_at: DateTime<Utc>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SkillAssetFileDto {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content: Option<String>,
    #[serde(default = "default_text_content_kind")]
    pub content_kind: SkillAssetFileContentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
    #[serde(default)]
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<SkillAssetFileKind>,
}

impl From<domain::SkillAssetFile> for SkillAssetFileDto {
    fn from(file: domain::SkillAssetFile) -> Self {
        let content_kind = match file.content_kind_str() {
            "binary" => SkillAssetFileContentKind::Binary,
            _ => SkillAssetFileContentKind::Text,
        };
        let content = file.text_content().map(ToString::to_string);
        let mime_type = file.mime_type().map(ToString::to_string);
        Self {
            path: file.path,
            content,
            content_kind,
            mime_type,
            size_bytes: file.size_bytes,
            kind: Some(file.kind.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SkillAssetDto {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub source: SkillAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub builtin_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remote_source: Option<RemoteSkillAssetSourceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_source: Option<InstalledAssetSourceDto>,
    pub disable_model_invocation: bool,
    pub files: Vec<SkillAssetFileDto>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<domain::SkillAsset> for SkillAssetDto {
    fn from(asset: domain::SkillAsset) -> Self {
        let source = SkillAssetSource::from(&asset.source);
        let builtin_key = match &asset.source {
            domain::SkillAssetSource::BuiltinSeed { key } => Some(key.clone()),
            _ => None,
        };
        let remote_source = match &asset.source {
            domain::SkillAssetSource::Github {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSourceDto {
                source_type: RemoteSkillAssetSourceType::Github,
                url: url.clone(),
                imported_at: *imported_at,
                digest: digest.clone(),
            }),
            domain::SkillAssetSource::Clawhub {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSourceDto {
                source_type: RemoteSkillAssetSourceType::Clawhub,
                url: url.clone(),
                imported_at: *imported_at,
                digest: digest.clone(),
            }),
            domain::SkillAssetSource::SkillsSh {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSourceDto {
                source_type: RemoteSkillAssetSourceType::SkillsSh,
                url: url.clone(),
                imported_at: *imported_at,
                digest: digest.clone(),
            }),
            domain::SkillAssetSource::BuiltinSeed { .. } | domain::SkillAssetSource::User => None,
        };
        Self {
            id: asset.id.to_string(),
            project_id: asset.project_id.to_string(),
            key: asset.key,
            display_name: asset.display_name,
            description: asset.description,
            source,
            builtin_key,
            remote_source,
            installed_source: asset
                .installed_source
                .map(|source| InstalledAssetSourceDto {
                    library_asset_id: source.library_asset_id.to_string(),
                    source_ref: source.source_ref,
                    source_version: source.source_version,
                    source_digest: source.source_digest,
                    installed_at: source.installed_at.to_rfc3339(),
                }),
            disable_model_invocation: asset.disable_model_invocation,
            files: asset.files.into_iter().map(Into::into).collect(),
            created_at: asset.created_at,
            updated_at: asset.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateSkillAssetRequest {
    pub key: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    #[ts(optional)]
    pub disable_model_invocation: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub files: Option<Vec<SkillAssetFileDto>>,
}

#[derive(Debug, Clone, Deserialize, Default, TS)]
pub struct UpdateSkillAssetRequest {
    #[serde(default)]
    #[ts(optional)]
    pub key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub disable_model_invocation: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub files: Option<Vec<SkillAssetFileDto>>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ImportRemoteSkillAssetRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Default, TS)]
pub struct ListSkillAssetQuery {
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<SkillAssetSource>,
}
