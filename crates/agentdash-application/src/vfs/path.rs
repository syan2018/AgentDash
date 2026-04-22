use super::types::ResourceRef;
use crate::runtime::{Mount, MountCapability, Vfs};

const URI_SEPARATOR: &str = "://";

/// Link 跳转最大深度，防止循环与链条过长。
const MAX_LINK_DEPTH: usize = 5;

/// 解析统一 URI 路径为 `ResourceRef`。
///
/// 支持两种格式：
/// - `mount_id://relative/path` — 显式指定 mount
/// - `relative/path` — 使用 VFS 的默认 mount
///
/// 命中 VFS 的 `links` 表时会透明跳转到目标 `(mount_id, path)`，
/// 最多跳转 `MAX_LINK_DEPTH` 次。
pub fn parse_mount_uri(input: &str, vfs: &Vfs) -> Result<ResourceRef, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("路径不能为空".to_string());
    }

    let raw_ref = if let Some(sep_pos) = trimmed.find(URI_SEPARATOR) {
        let mount_id = &trimmed[..sep_pos];
        if mount_id.is_empty() {
            return Err("URI 格式错误: mount ID 不能为空".to_string());
        }
        let path = &trimmed[sep_pos + URI_SEPARATOR.len()..];
        let path = path.trim_start_matches('/');
        ResourceRef {
            mount_id: mount_id.to_string(),
            path: path.to_string(),
        }
    } else {
        let mount_id = resolve_mount_id(vfs, None)?;
        ResourceRef {
            mount_id,
            path: trimmed.to_string(),
        }
    };

    resolve_links(vfs, raw_ref)
}

/// 透明跳转 VFS link 表，直到无命中或达到深度上限。
fn resolve_links(vfs: &Vfs, mut current: ResourceRef) -> Result<ResourceRef, String> {
    for _ in 0..MAX_LINK_DEPTH {
        let Some(link) = vfs.find_link(&current.mount_id, &current.path) else {
            return Ok(current);
        };

        // 计算目标路径：若 from_path 是前缀，将尾部拼到 to_path。
        let suffix = if link.from_path.is_empty() {
            current.path.clone()
        } else if current.path == link.from_path {
            String::new()
        } else if let Some(tail) = current.path.strip_prefix(&format!("{}/", link.from_path)) {
            tail.to_string()
        } else {
            // find_link 的匹配规则应保证不会走到这里，兜底。
            String::new()
        };

        let next_path = if suffix.is_empty() {
            link.to_path.clone()
        } else if link.to_path.is_empty() {
            suffix
        } else {
            format!("{}/{}", link.to_path.trim_end_matches('/'), suffix)
        };

        current = ResourceRef {
            mount_id: link.to_mount_id.clone(),
            path: next_path,
        };
    }

    Err(format!(
        "VFS link 解析超过最大深度 {MAX_LINK_DEPTH}，疑似存在循环引用"
    ))
}

/// 将 mount_id + path 格式化为 URI 字符串（如 `lifecycle://active/steps/start`）。
pub fn format_mount_uri(mount_id: &str, path: &str) -> String {
    if path.is_empty() {
        format!("{mount_id}://")
    } else {
        format!("{mount_id}://{path}")
    }
}

pub fn resolve_mount<'a>(
    vfs: &'a Vfs,
    mount_id: &str,
    capability: MountCapability,
) -> Result<&'a Mount, String> {
    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| format!("mount 不存在: {mount_id}"))?;
    if !mount.supports(capability) {
        return Err(format!("mount `{}` 不支持该能力", mount.id));
    }
    Ok(mount)
}

pub fn resolve_mount_id(vfs: &Vfs, mount: Option<&str>) -> Result<String, String> {
    if let Some(mount_id) = mount.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(mount_id.to_string());
    }
    vfs.default_mount_id
        .clone()
        .or_else(|| {
            if vfs.mounts.len() == 1 {
                vfs.mounts.first().map(|mount| mount.id.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            if vfs.mounts.is_empty() {
                "当前会话没有可用 mount".to_string()
            } else {
                format!(
                    "VFS 存在 {} 个 mount 但未设置 default_mount_id，需显式指定",
                    vfs.mounts.len()
                )
            }
        })
}

pub fn capability_name(capability: &MountCapability) -> &'static str {
    match capability {
        MountCapability::Read => "read",
        MountCapability::Write => "write",
        MountCapability::List => "list",
        MountCapability::Search => "search",
        MountCapability::Exec => "exec",
        MountCapability::Watch => "watch",
    }
}

pub fn normalize_mount_relative_path(input: &str, allow_empty: bool) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "." {
        return if allow_empty {
            Ok(String::new())
        } else {
            Err("路径不能为空".to_string())
        };
    }

    if is_absolute_like(trimmed) {
        return Err("路径必须是相对于 mount 根目录的相对路径".to_string());
    }

    let mut parts = Vec::new();
    for part in trimmed.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.pop().is_none() {
                return Err("路径越界：不允许访问 mount 之外的路径".to_string());
            }
            continue;
        }
        parts.push(part.to_string());
    }

    if parts.is_empty() {
        if allow_empty {
            Ok(String::new())
        } else {
            Err("路径不能为空".to_string())
        }
    } else {
        Ok(parts.join("/"))
    }
}

fn is_absolute_like(raw: &str) -> bool {
    raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.starts_with("//")
        || raw.starts_with("\\\\")
        || raw
            .as_bytes()
            .get(1)
            .zip(raw.as_bytes().get(2))
            .is_some_and(|(second, third)| *second == b':' && (*third == b'\\' || *third == b'/'))
}

pub fn join_root_ref(root_ref: &str, relative_path: &str) -> String {
    if relative_path.is_empty() {
        return root_ref.to_string();
    }

    let use_backslash = root_ref.contains('\\');
    let root = root_ref.trim_end_matches(['/', '\\']);
    let rel = if use_backslash {
        relative_path.replace('/', "\\")
    } else {
        relative_path.replace('\\', "/")
    };

    if use_backslash {
        format!("{root}\\{rel}")
    } else {
        format!("{root}/{rel}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountLink};

    fn make_mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "inline_fs".to_string(),
            backend_id: String::new(),
            root_ref: String::new(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn make_vfs(links: Vec<MountLink>) -> Vfs {
        Vfs {
            mounts: vec![make_mount("src"), make_mount("dst")],
            default_mount_id: Some("src".to_string()),
            links,
            ..Default::default()
        }
    }

    #[test]
    fn resolves_exact_link_target() {
        let vfs = make_vfs(vec![MountLink {
            from_mount_id: "src".to_string(),
            from_path: "alias.md".to_string(),
            to_mount_id: "dst".to_string(),
            to_path: "real/file.md".to_string(),
        }]);

        let result = parse_mount_uri("src://alias.md", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "dst");
        assert_eq!(result.path, "real/file.md");
    }

    #[test]
    fn resolves_directory_prefix_link_with_suffix() {
        let vfs = make_vfs(vec![MountLink {
            from_mount_id: "src".to_string(),
            from_path: "docs".to_string(),
            to_mount_id: "dst".to_string(),
            to_path: "shared/docs".to_string(),
        }]);

        let result = parse_mount_uri("src://docs/guide/intro.md", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "dst");
        assert_eq!(result.path, "shared/docs/guide/intro.md");
    }

    #[test]
    fn non_matching_path_stays_on_origin() {
        let vfs = make_vfs(vec![MountLink {
            from_mount_id: "src".to_string(),
            from_path: "docs".to_string(),
            to_mount_id: "dst".to_string(),
            to_path: "shared/docs".to_string(),
        }]);

        let result = parse_mount_uri("src://readme.md", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "src");
        assert_eq!(result.path, "readme.md");
    }

    #[test]
    fn cycle_detection_returns_error() {
        let vfs = make_vfs(vec![
            MountLink {
                from_mount_id: "src".to_string(),
                from_path: "a".to_string(),
                to_mount_id: "dst".to_string(),
                to_path: "b".to_string(),
            },
            MountLink {
                from_mount_id: "dst".to_string(),
                from_path: "b".to_string(),
                to_mount_id: "src".to_string(),
                to_path: "a".to_string(),
            },
        ]);

        let err = parse_mount_uri("src://a", &vfs).expect_err("should detect cycle");
        assert!(err.contains("超过最大深度"));
    }

    #[test]
    fn chained_links_resolve_within_depth() {
        let vfs = Vfs {
            mounts: vec![make_mount("a"), make_mount("b"), make_mount("c")],
            default_mount_id: Some("a".to_string()),
            links: vec![
                MountLink {
                    from_mount_id: "a".to_string(),
                    from_path: "x".to_string(),
                    to_mount_id: "b".to_string(),
                    to_path: "y".to_string(),
                },
                MountLink {
                    from_mount_id: "b".to_string(),
                    from_path: "y".to_string(),
                    to_mount_id: "c".to_string(),
                    to_path: "z".to_string(),
                },
            ],
            ..Default::default()
        };

        let result = parse_mount_uri("a://x", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "c");
        assert_eq!(result.path, "z");
    }

    #[test]
    fn default_mount_without_link_is_unchanged() {
        let vfs = make_vfs(vec![]);
        let result = parse_mount_uri("notes.md", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "src");
        assert_eq!(result.path, "notes.md");
    }
}
