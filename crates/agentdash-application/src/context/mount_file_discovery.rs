//! 通用 Mount 文件发现模块
//!
//! 基于 VFS 的 `RelayVfsService` 扫描约定路径下的已知文件（如 AGENTS.md / MEMORY.md），
//! 返回文件内容供调用方按场景注入到 session context 中。
//!
//! 设计参考 `skill/loader.rs` 的 VFS scan 模式，但抽象为通用的"规则 + 扫描"机制，
//! 不绑定特定文件格式。

use agentdash_spi::{MountCapability, Vfs};

use crate::vfs::types::ResourceRef;
use crate::vfs::{ListOptions, RelayVfsService};

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
    service: &RelayVfsService,
    vfs: &Vfs,
    rules: &[MountFileDiscoveryRule],
) -> MountFileDiscoveryResult {
    let mut result = MountFileDiscoveryResult::default();

    for mount in &vfs.mounts {
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

// ─── 内部辅助 ─────────────────────────────────────────────────

/// 尝试从 mount 中读取单个文件，成功则追加到结果。
async fn try_read_file(
    service: &RelayVfsService,
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

/// 列出指定目录下的一级子目录路径。
async fn list_children_at(
    service: &RelayVfsService,
    vfs: &Vfs,
    mount_id: &str,
    dir_path: &str,
) -> Vec<String> {
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
        Ok(r) => r
            .entries
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path)
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 列出 mount 根目录下的一级子目录名。
async fn list_root_children(service: &RelayVfsService, vfs: &Vfs, mount_id: &str) -> Vec<String> {
    let list_result = service
        .list(
            vfs,
            mount_id,
            ListOptions {
                path: ".".to_string(),
                pattern: None,
                recursive: false,
            },
            None,
            None,
        )
        .await;

    match list_result {
        Ok(r) => r
            .entries
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
