use super::types::ResourceRef;
use crate::runtime::{AddressSpace, Mount, MountCapability};

const URI_SEPARATOR: &str = "://";

/// 解析统一 URI 路径为 `ResourceRef`。
///
/// 支持两种格式：
/// - `mount_id://relative/path` — 显式指定 mount
/// - `relative/path` — 使用 address space 的默认 mount
pub fn parse_mount_uri(input: &str, address_space: &AddressSpace) -> Result<ResourceRef, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("路径不能为空".to_string());
    }

    if let Some(sep_pos) = trimmed.find(URI_SEPARATOR) {
        let mount_id = &trimmed[..sep_pos];
        if mount_id.is_empty() {
            return Err("URI 格式错误: mount ID 不能为空".to_string());
        }
        let path = &trimmed[sep_pos + URI_SEPARATOR.len()..];
        let path = path.trim_start_matches('/');
        return Ok(ResourceRef {
            mount_id: mount_id.to_string(),
            path: path.to_string(),
        });
    }

    let mount_id = resolve_mount_id(address_space, None)?;
    Ok(ResourceRef {
        mount_id,
        path: trimmed.to_string(),
    })
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
    address_space: &'a AddressSpace,
    mount_id: &str,
    capability: MountCapability,
) -> Result<&'a Mount, String> {
    let mount = address_space
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| format!("mount 不存在: {mount_id}"))?;
    if !mount.supports(capability) {
        return Err(format!("mount `{}` 不支持该能力", mount.id));
    }
    Ok(mount)
}

pub fn resolve_mount_id(
    address_space: &AddressSpace,
    mount: Option<&str>,
) -> Result<String, String> {
    if let Some(mount_id) = mount.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(mount_id.to_string());
    }
    address_space
        .default_mount_id
        .clone()
        .or_else(|| address_space.mounts.first().map(|mount| mount.id.clone()))
        .ok_or_else(|| "当前会话没有可用 mount".to_string())
}

pub fn capability_name(capability: &MountCapability) -> &'static str {
    match capability {
        MountCapability::Read => "read",
        MountCapability::Write => "write",
        MountCapability::List => "list",
        MountCapability::Search => "search",
        MountCapability::Exec => "exec",
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
