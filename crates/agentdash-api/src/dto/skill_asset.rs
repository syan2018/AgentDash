use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::skill_asset::{SkillAsset, SkillAssetFile, SkillAssetSource};

use super::InstalledAssetSourceResponse;

#[derive(Debug, Serialize)]
pub struct SkillAssetResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_source: Option<RemoteSkillAssetSourceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSourceResponse>,
    pub disable_model_invocation: bool,
    pub files: Vec<SkillAssetFileDto>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RemoteSkillAssetSourceDto {
    pub source_type: &'static str,
    pub url: String,
    pub imported_at: DateTime<Utc>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAssetFileDto {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default = "default_text_content_kind")]
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl From<SkillAssetFile> for SkillAssetFileDto {
    fn from(file: SkillAssetFile) -> Self {
        let content_kind = file.content_kind_str().to_string();
        let content = file.text_content().map(ToString::to_string);
        let mime_type = file.mime_type().map(ToString::to_string);
        Self {
            path: file.path,
            content,
            content_kind,
            mime_type,
            size_bytes: file.size_bytes,
            kind: Some(file.kind.tag().to_string()),
        }
    }
}

fn default_text_content_kind() -> String {
    "text".to_string()
}

impl From<SkillAsset> for SkillAssetResponse {
    fn from(asset: SkillAsset) -> Self {
        let source = asset.source.tag();
        let builtin_key = match &asset.source {
            SkillAssetSource::BuiltinSeed { key } => Some(key.clone()),
            _ => None,
        };
        let remote_source = asset
            .source
            .remote_source()
            .map(|source| RemoteSkillAssetSourceDto {
                source_type: source.source_type,
                url: source.url.to_string(),
                imported_at: *source.imported_at,
                digest: source.digest.to_string(),
            });
        Self {
            id: asset.id,
            project_id: asset.project_id,
            key: asset.key,
            display_name: asset.display_name,
            description: asset.description,
            source,
            builtin_key,
            remote_source,
            installed_source: asset.installed_source.map(Into::into),
            disable_model_invocation: asset.disable_model_invocation,
            files: asset.files.into_iter().map(Into::into).collect(),
            created_at: asset.created_at,
            updated_at: asset.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillAssetRequest {
    pub key: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default)]
    pub files: Vec<SkillAssetFileDto>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRemoteSkillAssetRequest {
    pub url: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateSkillAssetRequest {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub disable_model_invocation: Option<bool>,
    #[serde(default)]
    pub files: Option<Vec<SkillAssetFileDto>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListSkillAssetQuery {
    /// 期望值：`"user"` / `"builtin_seed"` / `"github"` / None（不过滤）
    #[serde(default)]
    pub source: Option<String>,
}
