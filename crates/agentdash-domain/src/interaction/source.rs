use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{InteractionError, InteractionResult, SOURCE_BUNDLE_FORMAT_V1};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

impl SourceFile {
    pub fn new(
        path: impl Into<String>,
        content: impl Into<String>,
        media_type: Option<String>,
    ) -> InteractionResult<Self> {
        Ok(Self {
            path: normalize_source_path(&path.into())?,
            content: content.into(),
            media_type,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSandboxConfig {
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub import_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBundle {
    pub format_version: u16,
    pub entry_file: String,
    pub files: Vec<SourceFile>,
    pub sandbox: SourceSandboxConfig,
    pub digest: String,
}

impl SourceBundle {
    pub fn new(
        entry_file: impl Into<String>,
        files: Vec<SourceFile>,
        sandbox: SourceSandboxConfig,
    ) -> InteractionResult<Self> {
        let entry_file = normalize_source_path(&entry_file.into())?;
        let mut files_by_path = BTreeMap::new();
        for mut file in files {
            file.path = normalize_source_path(&file.path)?;
            if files_by_path.insert(file.path.clone(), file).is_some() {
                return Err(InteractionError::InvalidSourcePath {
                    path: entry_file,
                    reason: "source bundle 中存在重复 path",
                });
            }
        }
        if !files_by_path.contains_key(&entry_file) {
            return Err(InteractionError::MissingEntryFile { entry_file });
        }

        let files = files_by_path.into_values().collect::<Vec<_>>();
        let sandbox = canonicalize_sandbox(sandbox)?;
        let digest = source_bundle_digest(&entry_file, &files, &sandbox)?;
        Ok(Self {
            format_version: SOURCE_BUNDLE_FORMAT_V1,
            entry_file,
            files,
            sandbox,
            digest,
        })
    }

    pub fn verify_digest(&self) -> InteractionResult<()> {
        if self.format_version != SOURCE_BUNDLE_FORMAT_V1 {
            return Err(InteractionError::InvalidField {
                field: "source_bundle.format_version",
                reason: "只支持 V1 source bundle",
            });
        }
        let rebuilt = Self::new(
            self.entry_file.clone(),
            self.files.clone(),
            self.sandbox.clone(),
        )?;
        if rebuilt.entry_file != self.entry_file
            || rebuilt.files != self.files
            || rebuilt.sandbox != self.sandbox
        {
            return Err(InteractionError::InvalidField {
                field: "source_bundle",
                reason: "source bundle 必须使用 canonical path/order/config",
            });
        }
        let expected = rebuilt.digest;
        if self.digest == expected {
            Ok(())
        } else {
            Err(InteractionError::InvalidDigest {
                field: "source_bundle.digest",
            })
        }
    }
}

fn canonicalize_sandbox(
    mut sandbox: SourceSandboxConfig,
) -> InteractionResult<SourceSandboxConfig> {
    for library in &mut sandbox.libraries {
        *library = library.trim().to_string();
        if library.is_empty() {
            return Err(InteractionError::InvalidField {
                field: "source_bundle.sandbox.libraries",
                reason: "library 不能为空",
            });
        }
    }
    sandbox.libraries.sort();
    sandbox.libraries.dedup();
    for (specifier, target) in &sandbox.import_map {
        if specifier.trim() != specifier
            || specifier.is_empty()
            || target.trim() != target
            || target.is_empty()
        {
            return Err(InteractionError::InvalidField {
                field: "source_bundle.sandbox.import_map",
                reason: "specifier/target 必须非空且已规范化",
            });
        }
    }
    Ok(sandbox)
}

pub fn normalize_source_path(raw: &str) -> InteractionResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(InteractionError::InvalidSourcePath {
            path: raw.to_string(),
            reason: "path 不能为空",
        });
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') || trimmed.contains('\\') {
        return Err(InteractionError::InvalidSourcePath {
            path: raw.to_string(),
            reason: "path 必须是使用正斜杠的相对路径",
        });
    }

    let mut normalized = Vec::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if normalized.pop().is_none() {
                    return Err(InteractionError::InvalidSourcePath {
                        path: raw.to_string(),
                        reason: "path 不能逃逸 source root",
                    });
                }
            }
            value if value.contains('\0') => {
                return Err(InteractionError::InvalidSourcePath {
                    path: raw.to_string(),
                    reason: "path 不能包含 NUL",
                });
            }
            value => normalized.push(value),
        }
    }
    if normalized.is_empty() {
        return Err(InteractionError::InvalidSourcePath {
            path: raw.to_string(),
            reason: "path 不能为空",
        });
    }
    Ok(normalized.join("/"))
}

fn source_bundle_digest(
    entry_file: &str,
    files: &[SourceFile],
    sandbox: &SourceSandboxConfig,
) -> InteractionResult<String> {
    #[derive(Serialize)]
    struct DigestInput<'a> {
        format_version: u16,
        entry_file: &'a str,
        files: &'a [SourceFile],
        sandbox: &'a SourceSandboxConfig,
    }

    let bytes = serde_json::to_vec(&DigestInput {
        format_version: SOURCE_BUNDLE_FORMAT_V1,
        entry_file,
        files,
        sandbox,
    })
    .map_err(|error| InteractionError::Serialization {
        context: "source_bundle_digest",
        message: error.to_string(),
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_normalizes_order_and_produces_stable_digest() {
        let first = SourceBundle::new(
            "./src/main.tsx",
            vec![
                SourceFile::new("src/z.ts", "z", None).expect("valid z source"),
                SourceFile::new("src/main.tsx", "main", None).expect("valid entry source"),
            ],
            SourceSandboxConfig::default(),
        )
        .expect("valid source bundle");
        let second = SourceBundle::new(
            "src/main.tsx",
            vec![
                SourceFile::new("src/main.tsx", "main", None).expect("valid entry source"),
                SourceFile::new("src/z.ts", "z", None).expect("valid z source"),
            ],
            SourceSandboxConfig::default(),
        )
        .expect("valid source bundle");

        assert_eq!(first.files, second.files);
        assert_eq!(first.digest, second.digest);
        first.verify_digest().expect("digest should verify");
    }

    #[test]
    fn bundle_rejects_paths_that_escape_the_source_root() {
        let error =
            SourceFile::new("../secret.txt", "secret", None).expect_err("escaping path must fail");
        assert!(matches!(error, InteractionError::InvalidSourcePath { .. }));
    }

    #[test]
    fn bundle_digest_normalizes_library_set() {
        let files = vec![SourceFile::new("main.rhai", "42", None).expect("source")];
        let first = SourceBundle::new(
            "main.rhai",
            files.clone(),
            SourceSandboxConfig {
                libraries: vec!["z".into(), " a ".into(), "z".into()],
                import_map: BTreeMap::new(),
            },
        )
        .expect("bundle");
        let second = SourceBundle::new(
            "main.rhai",
            files,
            SourceSandboxConfig {
                libraries: vec!["a".into(), "z".into()],
                import_map: BTreeMap::new(),
            },
        )
        .expect("bundle");
        assert_eq!(first.sandbox.libraries, vec!["a", "z"]);
        assert_eq!(first.digest, second.digest);
    }
}
