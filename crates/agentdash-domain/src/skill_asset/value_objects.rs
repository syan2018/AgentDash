use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Skill 资产来源。
///
/// `BuiltinSeed` 表示从源码内嵌模板 bootstrap 到项目后的种子资产。它仍是项目内资产，
/// 因而允许项目编辑者直接修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillAssetSource {
    BuiltinSeed {
        key: String,
    },
    Github {
        url: String,
        imported_at: DateTime<Utc>,
        digest: String,
    },
    Clawhub {
        url: String,
        imported_at: DateTime<Utc>,
        digest: String,
    },
    SkillsSh {
        url: String,
        imported_at: DateTime<Utc>,
        digest: String,
    },
    User,
}

impl SkillAssetSource {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::BuiltinSeed { .. } => "builtin_seed",
            Self::Github { .. } => "github",
            Self::Clawhub { .. } => "clawhub",
            Self::SkillsSh { .. } => "skills_sh",
            Self::User => "user",
        }
    }

    pub fn builtin_key(&self) -> Option<&str> {
        match self {
            Self::BuiltinSeed { key } => Some(key.as_str()),
            _ => None,
        }
    }

    pub fn is_builtin_seed(&self) -> bool {
        matches!(self, Self::BuiltinSeed { .. })
    }

    pub fn remote_source(&self) -> Option<RemoteSkillAssetSource<'_>> {
        match self {
            Self::Github {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSource {
                source_type: "github",
                url,
                imported_at,
                digest,
            }),
            Self::Clawhub {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSource {
                source_type: "clawhub",
                url,
                imported_at,
                digest,
            }),
            Self::SkillsSh {
                url,
                imported_at,
                digest,
            } => Some(RemoteSkillAssetSource {
                source_type: "skills_sh",
                url,
                imported_at,
                digest,
            }),
            Self::BuiltinSeed { .. } | Self::User => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RemoteSkillAssetSource<'a> {
    pub source_type: &'static str,
    pub url: &'a str,
    pub imported_at: &'a DateTime<Utc>,
    pub digest: &'a str,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillAssetFileKind {
    Skill,
    Reference,
    Script,
    Asset,
}

impl SkillAssetFileKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Reference => "reference",
            Self::Script => "script",
            Self::Asset => "asset",
        }
    }

    pub fn from_path(path: &str) -> Self {
        if path == "SKILL.md" {
            return Self::Skill;
        }
        if path.starts_with("scripts/") {
            return Self::Script;
        }
        if path.starts_with("assets/") {
            return Self::Asset;
        }
        Self::Reference
    }
}
