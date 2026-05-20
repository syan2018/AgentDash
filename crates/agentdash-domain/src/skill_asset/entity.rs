use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{SkillAssetFileKind, SkillAssetSource};
use crate::common::StoredFileContent;
use crate::inline_file::{InlineFile, InlineFileOwnerKind};
use crate::shared_library::InstalledAssetSource;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillAssetFile {
    pub id: Uuid,
    pub skill_asset_id: Uuid,
    pub path: String,
    pub content: StoredFileContent,
    pub kind: SkillAssetFileKind,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SkillAssetFile {
    pub const INLINE_CONTAINER_ID: &'static str = "files";

    pub fn new(
        skill_asset_id: Uuid,
        path: impl Into<String>,
        content: impl Into<String>,
        kind: SkillAssetFileKind,
    ) -> Self {
        Self::new_text(skill_asset_id, path, content, kind)
    }

    pub fn new_text(
        skill_asset_id: Uuid,
        path: impl Into<String>,
        content: impl Into<String>,
        kind: SkillAssetFileKind,
    ) -> Self {
        let content = StoredFileContent::text(content);
        Self::new_with_content(skill_asset_id, path, content, kind)
    }

    pub fn new_binary(
        skill_asset_id: Uuid,
        path: impl Into<String>,
        bytes: Vec<u8>,
        mime_type: impl Into<String>,
        kind: SkillAssetFileKind,
    ) -> Self {
        let content = StoredFileContent::binary(bytes, mime_type);
        Self::new_with_content(skill_asset_id, path, content, kind)
    }

    pub fn new_with_content(
        skill_asset_id: Uuid,
        path: impl Into<String>,
        content: StoredFileContent,
        kind: SkillAssetFileKind,
    ) -> Self {
        let now = Utc::now();
        let size_bytes = content.size_bytes();
        Self {
            id: Uuid::new_v4(),
            skill_asset_id,
            path: path.into(),
            content,
            kind,
            size_bytes,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn from_inline_file(file: InlineFile) -> Option<Self> {
        if file.owner_kind != InlineFileOwnerKind::SkillAsset
            || file.container_id != Self::INLINE_CONTAINER_ID
        {
            return None;
        }
        Some(Self {
            id: file.id,
            skill_asset_id: file.owner_id,
            kind: SkillAssetFileKind::from_path(&file.path),
            path: file.path,
            content: file.content,
            size_bytes: file.size_bytes,
            created_at: file.updated_at,
            updated_at: file.updated_at,
        })
    }

    pub fn into_inline_file(self) -> InlineFile {
        InlineFile {
            id: self.id,
            owner_kind: InlineFileOwnerKind::SkillAsset,
            owner_id: self.skill_asset_id,
            container_id: Self::INLINE_CONTAINER_ID.to_string(),
            path: self.path,
            content: self.content,
            size_bytes: self.size_bytes,
            updated_at: self.updated_at,
        }
    }

    pub fn content_kind_str(&self) -> &'static str {
        self.content.kind().as_str()
    }

    pub fn text_content(&self) -> Option<&str> {
        self.content.text_content()
    }

    pub fn binary_content(&self) -> Option<&[u8]> {
        self.content.binary_content()
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.content.mime_type()
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillAsset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub source: SkillAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub disable_model_invocation: bool,
    pub files: Vec<SkillAssetFile>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SkillAsset {
    pub fn new_user(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        disable_model_invocation: bool,
    ) -> Self {
        Self::new(
            project_id,
            key,
            display_name,
            description,
            SkillAssetSource::User,
            disable_model_invocation,
        )
    }

    pub fn new_builtin_seed(
        project_id: Uuid,
        builtin_key: impl Into<String>,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        disable_model_invocation: bool,
    ) -> Self {
        Self::new(
            project_id,
            key,
            display_name,
            description,
            SkillAssetSource::BuiltinSeed {
                key: builtin_key.into(),
            },
            disable_model_invocation,
        )
    }

    pub fn new_github_import(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        disable_model_invocation: bool,
        url: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self::new(
            project_id,
            key,
            display_name,
            description,
            SkillAssetSource::Github {
                url: url.into(),
                imported_at: Utc::now(),
                digest: digest.into(),
            },
            disable_model_invocation,
        )
    }

    pub fn new_clawhub_import(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        disable_model_invocation: bool,
        url: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self::new(
            project_id,
            key,
            display_name,
            description,
            SkillAssetSource::Clawhub {
                url: url.into(),
                imported_at: Utc::now(),
                digest: digest.into(),
            },
            disable_model_invocation,
        )
    }

    pub fn new_skills_sh_import(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        disable_model_invocation: bool,
        url: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self::new(
            project_id,
            key,
            display_name,
            description,
            SkillAssetSource::SkillsSh {
                url: url.into(),
                imported_at: Utc::now(),
                digest: digest.into(),
            },
            disable_model_invocation,
        )
    }

    fn new(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        source: SkillAssetSource,
        disable_model_invocation: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            key: key.into(),
            display_name: display_name.into(),
            description: description.into(),
            source,
            installed_source: None,
            disable_model_invocation,
            files: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_builtin_seed(&self) -> bool {
        self.source.is_builtin_seed()
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}
