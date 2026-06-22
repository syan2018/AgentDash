//! 通用 Mount 文件发现模块
//!
//! 基于 VFS 的 `VfsService` 扫描约定路径下的已知文件（如 AGENTS.md / MEMORY.md），
//! 返回文件内容供调用方按场景注入到 session context 中。
//!
//! 设计参考 `skill/loader.rs` 的 VFS scan 模式，但抽象为通用的"规则 + 扫描"机制，
//! 不绑定特定文件格式。

use std::collections::VecDeque;

use agentdash_spi::{Mount, MountCapability, Vfs};
use agentdash_spi::{SkillDiscoveryVfsFile, SkillDiscoveryVfsRule};

use crate::vfs::types::ResourceRef;
use crate::vfs::{
    ListOptions, PROVIDER_CANVAS_FS, PROVIDER_INLINE_FS, PROVIDER_LIFECYCLE_VFS, PROVIDER_RELAY_FS,
    PROVIDER_SKILL_ASSET_FS, RuntimeFileEntry, VfsService, normalize_mount_relative_path,
};

const AUTO_DISCOVERY_METADATA_KEY: &str = "agentdash_auto_discovery";
const DISCOVERY_POLICY_METADATA_KEY: &str = "agentdash_discovery_policy";

// ─── 规则定义 ─────────────────────────────────────────────────

/// 单条文件发现规则。
///
/// 调用方通过组合多条规则，描述需要在 mount 中搜索哪些约定文件。
pub struct MountFileDiscoveryRule {
    /// 规则标识（如 `"agents_md"` / `"memory_md"`），用于结果归类。
    pub key: &'static str,
    /// 目标文件名列表（如 `["AGENTS.md"]`）。
    pub file_names: &'static [&'static str],
    /// 是否扫描 mount 根目录。
    pub scan_root: bool,
    /// 是否扫描根目录的一级子目录（用于 monorepo 场景）。
    pub scan_children: bool,
    /// 在这些前缀目录下扫描一级子目录。
    /// 例如 `[".agents/skills", "skills"]` → 扫描 `.agents/skills/*/` 和 `skills/*/`
    pub scan_prefixes: &'static [&'static str],
    /// 单文件大小上限（字节），超过则跳过并记录诊断。
    pub max_size_bytes: u64,
}

// ─── 结果定义 ─────────────────────────────────────────────────

/// 一个被发现的文件。
#[derive(Debug, Clone)]
pub struct DiscoveredMountFile {
    pub rule_key: String,
    pub mount_id: String,
    /// 相对于 mount 根的路径（如 `"AGENTS.md"` 或 `"packages/foo/AGENTS.md"`）。
    pub path: String,
    pub content: String,
}

/// 诊断条目（不阻断发现流程）。
#[derive(Debug, Clone)]
pub struct MountFileDiscoveryDiagnostic {
    pub rule_key: String,
    pub mount_id: String,
    pub path: String,
    pub message: String,
}

/// 发现结果。
#[derive(Debug, Default)]
pub struct MountFileDiscoveryResult {
    pub files: Vec<DiscoveredMountFile>,
    pub diagnostics: Vec<MountFileDiscoveryDiagnostic>,
}

// ─── 内置规则常量 ──────────────────────────────────────────────

pub static BUILTIN_GUIDELINE_RULES: &[MountFileDiscoveryRule] = &[
    MountFileDiscoveryRule {
        key: "agents_md",
        file_names: &["AGENTS.md"],
        scan_root: true,
        scan_children: true,
        scan_prefixes: &[],
        max_size_bytes: 64 * 1024,
    },
    MountFileDiscoveryRule {
        key: "memory_md",
        file_names: &["MEMORY.md"],
        scan_root: true,
        scan_children: true,
        scan_prefixes: &[],
        max_size_bytes: 64 * 1024,
    },
];

pub static BUILTIN_SKILL_RULES: &[MountFileDiscoveryRule] = &[MountFileDiscoveryRule {
    key: "skill_md",
    file_names: &["SKILL.md"],
    scan_root: false,
    scan_children: false,
    scan_prefixes: &[".agents/skills", "skills"],
    max_size_bytes: 64 * 1024,
}];

// ─── 公共 API ─────────────────────────────────────────────────

/// 扫描所有可读 mount，按规则列表发现约定文件。
///
/// 对每个有 Read 能力的 mount：
/// 1. 若 `scan_root`，在根目录尝试读取每个 `file_names`；
/// 2. 若 `scan_children` 且 mount 有 List 能力，列出根目录一级子目录，
///    在每个子目录中尝试读取。
///
/// 同一 `rule_key` 下可能发现多个文件（来自根 + 不同子目录），全部保留。
pub async fn discover_mount_files(
    service: &VfsService,
    vfs: &Vfs,
    rules: &[MountFileDiscoveryRule],
) -> MountFileDiscoveryResult {
    let mut result = MountFileDiscoveryResult::default();

    for mount in &vfs.mounts {
        if !should_scan_mount_for_discovery(mount) {
            tracing::debug!(
                mount_id = %mount.id,
                provider = %mount.provider,
                "跳过 mount 自动文件发现"
            );
            continue;
        }

        let has_read = mount.capabilities.contains(&MountCapability::Read);
        if !has_read {
            continue;
        }
        let has_list = mount.capabilities.contains(&MountCapability::List);

        for rule in rules {
            // 根目录扫描
            if rule.scan_root {
                for file_name in rule.file_names {
                    try_read_file(service, vfs, &mount.id, file_name, rule, &mut result).await;
                }
            }

            // 一级子目录扫描
            if rule.scan_children && has_list {
                let children = list_root_children(service, vfs, &mount.id).await;
                for child_dir in &children {
                    for file_name in rule.file_names {
                        let path = format!("{child_dir}/{file_name}");
                        try_read_file(service, vfs, &mount.id, &path, rule, &mut result).await;
                    }
                }
            }

            // 前缀目录扫描（skill 模式：prefix/*/file_name）
            if !rule.scan_prefixes.is_empty() && has_list {
                for prefix in rule.scan_prefixes {
                    let children = list_children_at(service, vfs, &mount.id, prefix).await;
                    for child_dir in &children {
                        for file_name in rule.file_names {
                            let path = format!("{child_dir}/{file_name}");
                            try_read_file(service, vfs, &mount.id, &path, rule, &mut result).await;
                        }
                    }
                }
            }
        }
    }

    result
}

/// 按 dynamic skill provider 声明的 VFS 规则扫描文件。
///
/// Provider 规则只描述“在允许自动发现的 mount 内找什么”。是否允许扫描某个
/// mount 仍由 `should_scan_mount_for_discovery` 决定，避免 KM / 外部文档等
/// 高成本 mount 在 session capability 构建时被递归扫爆。
pub async fn discover_skill_vfs_files(
    service: &VfsService,
    vfs: &Vfs,
    rules: &[SkillDiscoveryVfsRule],
) -> (
    Vec<SkillDiscoveryVfsFile>,
    Vec<MountFileDiscoveryDiagnostic>,
) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();

    for mount in &vfs.mounts {
        if !should_scan_mount_for_discovery(mount) {
            tracing::debug!(
                mount_id = %mount.id,
                provider = %mount.provider,
                "跳过 dynamic skill VFS discovery mount"
            );
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
                    )
                    .await
                } else {
                    let children = list_children_at(service, vfs, &mount.id, &prefix).await;
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

// ─── 内部辅助 ─────────────────────────────────────────────────

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
    diagnostics: &mut Vec<MountFileDiscoveryDiagnostic>,
) -> Result<String, ()> {
    normalize_mount_relative_path(raw_path, allow_empty).map_err(|error| {
        diagnostics.push(MountFileDiscoveryDiagnostic {
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
    files: &mut Vec<SkillDiscoveryVfsFile>,
    diagnostics: &mut Vec<MountFileDiscoveryDiagnostic>,
) {
    let target = ResourceRef {
        mount_id: mount_id.to_string(),
        path: path.to_string(),
    };
    let read = match service.read_text(vfs, &target, None, None).await {
        Ok(r) => r,
        Err(_) => return,
    };

    let content_len = read.content.len() as u64;
    if content_len > max_size_bytes {
        diagnostics.push(MountFileDiscoveryDiagnostic {
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
        let entries = list_entries_at(service, vfs, mount_id, &path).await;
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

/// 尝试从 mount 中读取单个文件，成功则追加到结果。
async fn try_read_file(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    path: &str,
    rule: &MountFileDiscoveryRule,
    result: &mut MountFileDiscoveryResult,
) {
    let target = ResourceRef {
        mount_id: mount_id.to_string(),
        path: path.to_string(),
    };
    let read = match service.read_text(vfs, &target, None, None).await {
        Ok(r) => r,
        Err(_) => return, // 文件不存在或不可读，静默跳过
    };

    let content_len = read.content.len() as u64;
    if content_len > rule.max_size_bytes {
        result.diagnostics.push(MountFileDiscoveryDiagnostic {
            rule_key: rule.key.to_string(),
            mount_id: mount_id.to_string(),
            path: path.to_string(),
            message: format!(
                "文件过大（{content_len} bytes > {} bytes），已跳过",
                rule.max_size_bytes
            ),
        });
        return;
    }

    if read.content.trim().is_empty() {
        return;
    }

    result.files.push(DiscoveredMountFile {
        rule_key: rule.key.to_string(),
        mount_id: mount_id.to_string(),
        path: path.to_string(),
        content: read.content,
    });
}

async fn list_entries_at(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    dir_path: &str,
) -> Vec<RuntimeFileEntry> {
    let list_result = service
        .list(
            vfs,
            mount_id,
            ListOptions {
                path: dir_path.to_string(),
                pattern: None,
                recursive: false,
            },
            None,
            None,
        )
        .await;

    match list_result {
        Ok(r) => r.entries,
        Err(_) => Vec::new(),
    }
}

/// 列出指定目录下的一级子目录路径。
async fn list_children_at(
    service: &VfsService,
    vfs: &Vfs,
    mount_id: &str,
    dir_path: &str,
) -> Vec<String> {
    list_entries_at(service, vfs, mount_id, dir_path)
        .await
        .into_iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path)
        .collect()
}

/// 列出 mount 根目录下的一级子目录名。
async fn list_root_children(service: &VfsService, vfs: &Vfs, mount_id: &str) -> Vec<String> {
    list_children_at(service, vfs, mount_id, ".").await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(provider: &str, metadata: serde_json::Value) -> Mount {
        Mount {
            id: provider.to_string(),
            provider: provider.to_string(),
            backend_id: String::new(),
            root_ref: ".".to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: provider.to_string(),
            metadata,
        }
    }

    #[test]
    fn builtin_guideline_rules_cover_agents_and_memory() {
        assert_eq!(BUILTIN_GUIDELINE_RULES.len(), 2);
        assert_eq!(BUILTIN_GUIDELINE_RULES[0].key, "agents_md");
        assert_eq!(BUILTIN_GUIDELINE_RULES[0].file_names, &["AGENTS.md"]);
        assert!(BUILTIN_GUIDELINE_RULES[0].scan_root);
        assert!(BUILTIN_GUIDELINE_RULES[0].scan_children);

        assert_eq!(BUILTIN_GUIDELINE_RULES[1].key, "memory_md");
        assert_eq!(BUILTIN_GUIDELINE_RULES[1].file_names, &["MEMORY.md"]);
    }

    #[test]
    fn discovery_result_default_is_empty() {
        let result = MountFileDiscoveryResult::default();
        assert!(result.files.is_empty());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn discovery_scans_builtin_low_cost_mounts_by_default() {
        for provider in [
            PROVIDER_RELAY_FS,
            PROVIDER_INLINE_FS,
            PROVIDER_LIFECYCLE_VFS,
            PROVIDER_CANVAS_FS,
            PROVIDER_SKILL_ASSET_FS,
        ] {
            assert!(
                should_scan_mount_for_discovery(&mount(provider, serde_json::Value::Null)),
                "{provider} should be auto-discoverable"
            );
        }
    }

    #[test]
    fn discovery_skips_external_mounts_by_default() {
        assert!(!should_scan_mount_for_discovery(&mount(
            "external_docs",
            serde_json::Value::Null
        )));
        assert!(!should_scan_mount_for_discovery(&mount(
            "custom_remote_provider",
            serde_json::Value::Null
        )));
    }

    #[test]
    fn discovery_metadata_can_override_default_policy() {
        assert!(should_scan_mount_for_discovery(&mount(
            "external_docs",
            serde_json::json!({ AUTO_DISCOVERY_METADATA_KEY: true })
        )));
        assert!(!should_scan_mount_for_discovery(&mount(
            PROVIDER_INLINE_FS,
            serde_json::json!({ DISCOVERY_POLICY_METADATA_KEY: "manual" })
        )));
        assert!(should_scan_mount_for_discovery(&mount(
            "custom_remote_provider",
            serde_json::json!({ DISCOVERY_POLICY_METADATA_KEY: "auto" })
        )));
    }

    #[test]
    fn dynamic_rule_file_name_matching_uses_leaf_name() {
        assert!(matches_any_file_name(
            "Tools/example/nested/SKILL.md",
            &["SKILL.md".to_string()]
        ));
        assert!(!matches_any_file_name(
            "Tools/example/nested/README.md",
            &["SKILL.md".to_string()]
        ));
    }

    #[test]
    fn discovery_policy_models_external_mounts_as_manual_by_default() {
        assert!(!should_scan_mount_for_discovery(&mount(
            "km",
            serde_json::Value::Null
        )));
        assert!(!should_scan_mount_for_discovery(&mount(
            "external_service",
            serde_json::json!({ DISCOVERY_POLICY_METADATA_KEY: "manual" })
        )));
        assert!(should_scan_mount_for_discovery(&mount(
            "km",
            serde_json::json!({ DISCOVERY_POLICY_METADATA_KEY: "auto" })
        )));
    }
}
