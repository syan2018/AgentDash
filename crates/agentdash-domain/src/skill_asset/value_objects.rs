use serde::{Deserialize, Serialize};

/// Skill 资产来源。
///
/// `BuiltinSeed` 表示从源码内嵌模板 bootstrap 到项目后的种子资产。它仍是项目内资产，
/// 因而允许项目编辑者直接修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillAssetSource {
    BuiltinSeed { key: String },
    User,
}

impl SkillAssetSource {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::BuiltinSeed { .. } => "builtin_seed",
            Self::User => "user",
        }
    }

    pub fn builtin_key(&self) -> Option<&str> {
        match self {
            Self::BuiltinSeed { key } => Some(key.as_str()),
            Self::User => None,
        }
    }

    pub fn is_builtin_seed(&self) -> bool {
        matches!(self, Self::BuiltinSeed { .. })
    }
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
