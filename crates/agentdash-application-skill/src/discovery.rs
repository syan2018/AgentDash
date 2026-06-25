use std::collections::VecDeque;

use agentdash_application_vfs::{
    ListOptions, PROVIDER_CANVAS_FS, PROVIDER_INLINE_FS, PROVIDER_LIFECYCLE_VFS, PROVIDER_RELAY_FS,
    PROVIDER_SKILL_ASSET_FS, ResourceRef, RuntimeFileEntry, VfsService,
    normalize_mount_relative_path,
};
use agentdash_spi::{
    AuthIdentity, Mount, MountCapability, SkillDiscoveryDiagnostic, SkillDiscoveryVfsFile,
    SkillDiscoveryVfsRule, Vfs,
};

pub use crate::skill::{
    LoadSkillsResult, SkillDiagnostic, load_skills_from_local_dirs, load_skills_from_vfs,
};

const AUTO_DISCOVERY_METADATA_KEY: &str = "agentdash_auto_discovery";
const DISCOVERY_POLICY_METADATA_KEY: &str = "agentdash_discovery_policy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillVfsDiscoveryDiagnostic {
    pub rule_key: String,
    pub mount_id: String,
    pub path: String,
    pub message: String,
}

impl SkillVfsDiscoveryDiagnostic {
    pub fn into_skill_discovery_diagnostic(
        self,
        provider_key: impl Into<String>,
    ) -> SkillDiscoveryDiagnostic {
        SkillDiscoveryDiagnostic {
            provider_key: provider_key.into(),
            code: "vfs_discovery_scan".to_string(),
            message: self.message,
            local_name: None,
            file_path: Some(format!("{}://{}", self.mount_id, self.path)),
        }
    }
}

pub async fn discover_skill_vfs_files(
    service: &VfsService,
    vfs: &Vfs,
    rules: &[SkillDiscoveryVfsRule],
    identity: Option<&AuthIdentity>,
) -> (Vec<SkillDiscoveryVfsFile>, Vec<SkillVfsDiscoveryDiagnostic>) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();

    for mount in &vfs.mounts {
        if !should_scan_mount_for_discovery(mount) {
            continue;
        }
        if !mount.capabilities.contains(&MountCapability::Read) {
            continue;
        }

        let has_list = mount.capabilities.contains(&MountCapability::List);
        for rule in rules {
            let rule_key = normalized_rule_key(rule);

            for exact_path in &rule.exact_paths {
                let Ok(path) =
                    normalize_rule_path(&rule_key, &mount.id, exact_path, false, &mut diagnostics)
                else {
                    continue;
                };
                try_read_skill_vfs_file(
                    service,
                    vfs,
                    &mount.id,
                    &path,
                    &rule_key,
                    rule.max_size_bytes,
                    identity,
                    &mut files,
                    &mut diagnostics,
                )
                .await;
            }

            if !has_list || rule.scan_prefixes.is_empty() || rule.file_names.is_empty() {
                continue;
            }

            let max_files = rule.max_files.unwrap_or(usize::MAX);
            let mut emitted_for_rule = 0usize;
            for prefix in &rule.scan_prefixes {
                if emitted_for_rule >= max_files {
                    break;
                }
                let Ok(prefix) =
                    normalize_rule_path(&rule_key, &mount.id, prefix, true, &mut diagnostics)
                else {
                    continue;
                };

                let candidates = if rule.recursive {
                    list_recursive_files(
                        service,
                        vfs,
                        &mount.id,
                        &prefix,
                        rule.max_depth.unwrap_or(8),
                        max_files.saturating_sub(emitted_for_rule),
                        identity,
                    )
                    .await
                } else {
                    let children =
                        list_children_at(service, vfs, &mount.id, &prefix, identity).await;
                    children
                        .into_iter()
                        .flat_map(|child_dir| {
                            rule.file_names
                                .iter()
                                .map(move |file_name| format!("{child_dir}/{file_name}"))
                        })
                        .collect()
                };

                for candidate in candidates {
                    if emitted_for_rule >= max_files {
                        break;
                    }
                    if !matches_any_file_name(&candidate, &rule.file_names) {
                        continue;
                    }
                    let before_len = files.len();
                    try_read_skill_vfs_file(
                        service,
                        vfs,
                        &mount.id,
                        &candidate,
                        &rule_key,
                        rule.max_size_bytes,
                        identity,
                        &mut files,
                        &mut diagnostics,
                    )
                    .await;
                    if files.len() > before_len {
                        emitted_for_rule += 1;
                    }
                }
            }
        }
    }

    (files, diagnostics)
}

fn should_scan_mount_for_discovery(mount: &Mount) -> bool {
    match mount.metadata.get(AUTO_DISCOVERY_METADATA_KEY) {
        Some(serde_json::Value::Bool(enabled)) => return *enabled,
        Some(serde_json::Value::String(value)) => {
            match value.trim().to_ascii_lowercase().as_str() {
                "true" | "allow" | "auto" => return true,
                "false" | "deny" | "skip" | "manual" => return false,
                _ => {}
            }
        }
        _ => {}
    }

    match mount
        .metadata
        .get(DISCOVERY_POLICY_METADATA_KEY)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("auto") | Some("allow") => return true,
        Some("manual") | Some("skip") | Some("deny") => return false,
        _ => {}
    }

    matches!(
        mount.provider.as_str(),
        PROVIDER_RELAY_FS
            | PROVIDER_INLINE_FS
            | PROVIDER_LIFECYCLE_VFS
            | PROVIDER_CANVAS_FS
            | PROVIDER_SKILL_ASSET_FS
    )
}

fn normalized_rule_key(rule: &SkillDiscoveryVfsRule) -> String {
    let key = rule.key.trim();
    if key.is_empty() {
        "skill_discovery".to_string()
    } else {
        key.to_string()
    }
}

fn normalize_rule_path(
    rule_key: &str,
    mount_id: &str,
    raw_path: &str,
    allow_empty: bool,
    diagnostics: &mut Vec<SkillVfsDiscoveryDiagnostic>,
) -> Result<String, ()> {
    normalize_mount_relative_path(raw_path, allow_empty).map_err(|error| {
        diagnostics.push(SkillVfsDiscoveryDiagnostic {
            rule_key: rule_key.to_string(),
            mount_id: mount_id.to_string(),
            path: raw_path.to_string(),
            message: format!("discovery path 非法: {error}"),
        });
    })
}

async fn try_read_skill_vfs_file(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    path: &str,
    rule_key: &str,
    max_size_bytes: u64,
    identity: Option<&AuthIdentity>,
    files: &mut Vec<SkillDiscoveryVfsFile>,
    diagnostics: &mut Vec<SkillVfsDiscoveryDiagnostic>,
) {
    let target = ResourceRef {
        mount_id: mount_id.to_string(),
        path: path.to_string(),
    };
    let read = match service.read_text(vfs, &target, None, identity).await {
        Ok(read) => read,
        Err(_) => return,
    };

    let content_len = read.content.len() as u64;
    if content_len > max_size_bytes {
        diagnostics.push(SkillVfsDiscoveryDiagnostic {
            rule_key: rule_key.to_string(),
            mount_id: mount_id.to_string(),
            path: path.to_string(),
            message: format!("文件过大（{content_len} bytes > {max_size_bytes} bytes），已跳过"),
        });
        return;
    }
    if read.content.trim().is_empty() {
        return;
    }

    files.push(SkillDiscoveryVfsFile {
        rule_key: rule_key.to_string(),
        mount_id: mount_id.to_string(),
        path: read.path,
        content: read.content,
    });
}

async fn list_recursive_files(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    root_path: &str,
    max_depth: usize,
    max_files: usize,
    identity: Option<&AuthIdentity>,
) -> Vec<String> {
    if max_files == 0 {
        return Vec::new();
    }

    let mut files = Vec::new();
    let mut queue = VecDeque::from([(root_path.to_string(), 0usize)]);
    while let Some((path, depth)) = queue.pop_front() {
        if depth > max_depth {
            continue;
        }
        let entries = list_entries_at(service, vfs, mount_id, &path, identity).await;
        for entry in entries {
            if entry.is_dir {
                if depth < max_depth {
                    queue.push_back((entry.path, depth + 1));
                }
            } else {
                files.push(entry.path);
                if files.len() >= max_files {
                    return files;
                }
            }
        }
    }

    files
}

fn matches_any_file_name(path: &str, file_names: &[String]) -> bool {
    let Some(name) = path.rsplit('/').next() else {
        return false;
    };
    file_names.iter().any(|file_name| file_name == name)
}

async fn list_children_at(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    dir_path: &str,
    identity: Option<&AuthIdentity>,
) -> Vec<String> {
    list_entries_at(service, vfs, mount_id, dir_path, identity)
        .await
        .into_iter()
        .filter(|entry| entry.is_dir)
        .map(|entry| entry.path)
        .collect()
}

async fn list_entries_at(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    dir_path: &str,
    identity: Option<&AuthIdentity>,
) -> Vec<RuntimeFileEntry> {
    service
        .list(
            vfs,
            mount_id,
            ListOptions {
                path: dir_path.to_string(),
                pattern: None,
                recursive: false,
            },
            None,
            identity,
        )
        .await
        .map(|result| result.entries)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_converts_to_skill_discovery_diagnostic() {
        let diagnostic = SkillVfsDiscoveryDiagnostic {
            rule_key: "skills".to_string(),
            mount_id: "workspace".to_string(),
            path: "skills/demo/SKILL.md".to_string(),
            message: "too large".to_string(),
        }
        .into_skill_discovery_diagnostic("provider");

        assert_eq!(diagnostic.provider_key, "provider");
        assert_eq!(diagnostic.code, "vfs_discovery_scan");
        assert_eq!(
            diagnostic.file_path.as_deref(),
            Some("workspace://skills/demo/SKILL.md")
        );
    }
}
