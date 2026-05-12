use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{SkillAssetFileKind, SkillAssetSource};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillAssetFile {
    pub id: Uuid,
    pub skill_asset_id: Uuid,
    pub path: String,
    pub content: String,
    pub kind: SkillAssetFileKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SkillAssetFile {
    pub fn new(
        skill_asset_id: Uuid,
        path: impl Into<String>,
        content: impl Into<String>,
        kind: SkillAssetFileKind,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            skill_asset_id,
            path: path.into(),
            content: content.into(),
            kind,
            created_at: now,
            updated_at: now,
        }
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
