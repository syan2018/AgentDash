use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedSkillFileKind {
    Skill,
    Reference,
    Script,
    Asset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSkillFile {
    pub relative_path: &'static str,
    pub content: &'static str,
    pub kind: EmbeddedSkillFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSkillBundle {
    pub name: &'static str,
    pub root_path: &'static str,
    pub entry_path: &'static str,
    pub files: &'static [EmbeddedSkillFile],
}

impl EmbeddedSkillBundle {
    pub fn entry_full_path(&self) -> String {
        self.full_path(self.entry_path)
    }

    pub fn full_path(&self, relative_path: &str) -> String {
        let root = self.root_path.trim().trim_matches('/');
        let relative = relative_path.trim().trim_matches('/');
        format!("{root}/{relative}")
    }

    pub fn materialized_files(&self) -> Vec<MaterializedEmbeddedSkillFile> {
        self.files
            .iter()
            .map(|file| MaterializedEmbeddedSkillFile {
                path: self.full_path(file.relative_path),
                content: file.content.to_string(),
                kind: file.kind,
            })
            .collect()
    }

    pub fn owns_path(&self, path: &str) -> bool {
        let Some(normalized) = normalize_embedded_skill_path(path) else {
            return false;
        };
        self.files
            .iter()
            .any(|file| self.full_path(file.relative_path) == normalized)
    }

    pub fn validate(&self) -> Result<(), EmbeddedSkillBundleError> {
        validate_path("root_path", self.root_path)?;
        validate_path("entry_path", self.entry_path)?;
        if self.name.trim().is_empty() {
            return Err(EmbeddedSkillBundleError::InvalidName);
        }
        if self.files.is_empty() {
            return Err(EmbeddedSkillBundleError::EmptyFiles {
                bundle_name: self.name.to_string(),
            });
        }

        let mut found_entry = false;
        for file in self.files {
            validate_path("relative_path", file.relative_path)?;
            if file.relative_path == self.entry_path {
                found_entry = true;
                if file.kind != EmbeddedSkillFileKind::Skill {
                    return Err(EmbeddedSkillBundleError::EntryNotSkill {
                        bundle_name: self.name.to_string(),
                        entry_path: self.entry_path.to_string(),
                    });
                }
            }
        }

        if !found_entry {
            return Err(EmbeddedSkillBundleError::MissingEntry {
                bundle_name: self.name.to_string(),
                entry_path: self.entry_path.to_string(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedEmbeddedSkillFile {
    pub path: String,
    pub content: String,
    pub kind: EmbeddedSkillFileKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmbeddedSkillSyncReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
}

impl EmbeddedSkillSyncReport {
    pub fn changed(&self) -> bool {
        self.added > 0 || self.updated > 0
    }
}

pub trait EmbeddedSkillTargetFile {
    fn path(&self) -> &str;
    fn content(&self) -> &str;
    fn set_path(&mut self, path: String);
    fn set_content(&mut self, content: String);
    fn from_path_content(path: String, content: String) -> Self;
}

pub fn ensure_embedded_skill_bundle<T>(
    files: &mut Vec<T>,
    bundle: &EmbeddedSkillBundle,
) -> Result<EmbeddedSkillSyncReport, EmbeddedSkillBundleError>
where
    T: EmbeddedSkillTargetFile,
{
    bundle.validate()?;

    let mut report = EmbeddedSkillSyncReport::default();
    for bundled_file in bundle.materialized_files() {
        let existing = files.iter_mut().find(|file| {
            normalize_embedded_skill_path(file.path()).as_deref()
                == Some(bundled_file.path.as_str())
        });

        if let Some(existing) = existing {
            if existing.path() == bundled_file.path && existing.content() == bundled_file.content {
                report.unchanged += 1;
                continue;
            }

            existing.set_path(bundled_file.path);
            existing.set_content(bundled_file.content);
            report.updated += 1;
        } else {
            files.push(T::from_path_content(
                bundled_file.path,
                bundled_file.content,
            ));
            report.added += 1;
        }
    }

    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddedSkillBundleError {
    #[error("embedded skill bundle name 不能为空")]
    InvalidName,
    #[error("embedded skill bundle `{bundle_name}` 文件列表不能为空")]
    EmptyFiles { bundle_name: String },
    #[error("embedded skill bundle path `{field}` 非法: {path}")]
    InvalidPath { field: &'static str, path: String },
    #[error("embedded skill bundle `{bundle_name}` 缺少入口文件: {entry_path}")]
    MissingEntry {
        bundle_name: String,
        entry_path: String,
    },
    #[error("embedded skill bundle `{bundle_name}` 的入口文件不是 Skill 类型: {entry_path}")]
    EntryNotSkill {
        bundle_name: String,
        entry_path: String,
    },
}

fn validate_path(field: &'static str, path: &str) -> Result<(), EmbeddedSkillBundleError> {
    if normalize_embedded_skill_path(path).is_some() {
        Ok(())
    } else {
        Err(EmbeddedSkillBundleError::InvalidPath {
            field,
            path: path.to_string(),
        })
    }
}

fn normalize_embedded_skill_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_matches('/').to_string();
    if normalized.is_empty() || normalized.contains(':') {
        return None;
    }

    for segment in normalized.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return None;
        }
    }

    Some(normalized)
}

impl fmt::Display for EmbeddedSkillFileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddedSkillFileKind::Skill => f.write_str("skill"),
            EmbeddedSkillFileKind::Reference => f.write_str("reference"),
            EmbeddedSkillFileKind::Script => f.write_str("script"),
            EmbeddedSkillFileKind::Asset => f.write_str("asset"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FakeFile {
        path: String,
        content: String,
    }

    impl EmbeddedSkillTargetFile for FakeFile {
        fn path(&self) -> &str {
            &self.path
        }

        fn content(&self) -> &str {
            &self.content
        }

        fn set_path(&mut self, path: String) {
            self.path = path;
        }

        fn set_content(&mut self, content: String) {
            self.content = content;
        }

        fn from_path_content(path: String, content: String) -> Self {
            Self { path, content }
        }
    }

    const FILES: &[EmbeddedSkillFile] = &[
        EmbeddedSkillFile {
            relative_path: "SKILL.md",
            content: "skill",
            kind: EmbeddedSkillFileKind::Skill,
        },
        EmbeddedSkillFile {
            relative_path: "references/api.md",
            content: "api",
            kind: EmbeddedSkillFileKind::Reference,
        },
    ];

    const BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
        name: "demo",
        root_path: "skills/demo",
        entry_path: "SKILL.md",
        files: FILES,
    };

    #[test]
    fn materializer_adds_all_bundle_files() {
        let mut files = Vec::<FakeFile>::new();

        let report = ensure_embedded_skill_bundle(&mut files, &BUNDLE).expect("valid bundle");

        assert_eq!(report.added, 2);
        assert_eq!(files[0].path, "skills/demo/SKILL.md");
        assert_eq!(files[1].path, "skills/demo/references/api.md");
    }

    #[test]
    fn materializer_updates_existing_bundle_files() {
        let mut files = vec![FakeFile {
            path: "skills\\demo\\SKILL.md".to_string(),
            content: "old".to_string(),
        }];

        let report = ensure_embedded_skill_bundle(&mut files, &BUNDLE).expect("valid bundle");

        assert_eq!(report.added, 1);
        assert_eq!(report.updated, 1);
        assert_eq!(files[0].path, "skills/demo/SKILL.md");
        assert_eq!(files[0].content, "skill");
    }

    #[test]
    fn validate_rejects_missing_entry() {
        const BAD_FILES: &[EmbeddedSkillFile] = &[EmbeddedSkillFile {
            relative_path: "references/api.md",
            content: "api",
            kind: EmbeddedSkillFileKind::Reference,
        }];
        let bundle = EmbeddedSkillBundle {
            name: "demo",
            root_path: "skills/demo",
            entry_path: "SKILL.md",
            files: BAD_FILES,
        };

        let err = bundle.validate().expect_err("missing entry should fail");

        assert!(matches!(err, EmbeddedSkillBundleError::MissingEntry { .. }));
    }
}
