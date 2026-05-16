use std::collections::{BTreeSet, HashSet};
use std::fmt;

use super::types::ResourceRef;
use crate::runtime::{Mount, MountCapability, Vfs};

const URI_SEPARATOR: &str = "://";

/// Link 跳转最大深度，防止循环与链条过长。
const MAX_LINK_DEPTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MountId(String);

impl MountId {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("mount ID 不能为空".to_string());
        }
        if trimmed.contains(URI_SEPARATOR)
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.chars().any(char::is_whitespace)
        {
            return Err(format!("mount ID `{trimmed}` 含非法字符"));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<MountId> for String {
    fn from(value: MountId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MountRelativePath(String);

impl MountRelativePath {
    pub fn parse(input: &str, allow_empty: bool) -> Result<Self, String> {
        normalize_mount_relative_path(input, allow_empty).map(Self)
    }

    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for MountRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<MountRelativePath> for String {
    fn from(value: MountRelativePath) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VfsUri {
    pub mount_id: MountId,
    pub path: MountRelativePath,
}

impl VfsUri {
    pub fn parse(input: &str, vfs: &Vfs, allow_empty_path: bool) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("路径不能为空".to_string());
        }

        let (mount_id, raw_path) = if let Some(sep_pos) = trimmed.find(URI_SEPARATOR) {
            let mount_id = MountId::parse(&trimmed[..sep_pos])?;
            let path = trimmed[sep_pos + URI_SEPARATOR.len()..].trim_start_matches('/');
            (mount_id, path.to_string())
        } else {
            let mount_id = MountId::parse(&resolve_mount_id(vfs, None)?)?;
            (mount_id, trimmed.to_string())
        };
        let path = MountRelativePath::parse(&raw_path, allow_empty_path)?;
        Ok(Self { mount_id, path })
    }

    pub fn into_resource_ref(self) -> ResourceRef {
        ResourceRef {
            mount_id: self.mount_id.into(),
            path: self.path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootRef {
    LocalPath(String),
    ProviderUri { scheme: String, remainder: String },
}

impl RootRef {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("root_ref 不能为空".to_string());
        }
        if let Some(sep_pos) = trimmed.find(URI_SEPARATOR) {
            let scheme = trimmed[..sep_pos].trim();
            let remainder = trimmed[sep_pos + URI_SEPARATOR.len()..].trim();
            if scheme.is_empty() || remainder.is_empty() {
                return Err(format!("root_ref URI 格式错误: {trimmed}"));
            }
            if scheme
                .chars()
                .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '+'))
            {
                return Err(format!("root_ref scheme `{scheme}` 含非法字符"));
            }
            Ok(Self::ProviderUri {
                scheme: scheme.to_string(),
                remainder: remainder.to_string(),
            })
        } else {
            Ok(Self::LocalPath(trimmed.to_string()))
        }
    }

    pub fn is_local_path(&self) -> bool {
        matches!(self, Self::LocalPath(_))
    }

    pub fn scheme(&self) -> Option<&str> {
        match self {
            Self::LocalPath(_) => None,
            Self::ProviderUri { scheme, .. } => Some(scheme),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPolicy {
    VfsRead,
    VfsWrite,
    VfsList,
    VfsSearchBase,
    PatchTarget,
    PatchMoveTarget,
    ShellCwd { allow_absolute_inside_root: bool },
    SessionWorkingDir,
    MaterializationTarget,
}

impl PathPolicy {
    pub fn allows_empty(self) -> bool {
        matches!(
            self,
            Self::VfsList | Self::VfsSearchBase | Self::ShellCwd { .. } | Self::SessionWorkingDir
        )
    }

    pub fn parse_mount_relative_path(self, input: &str) -> Result<MountRelativePath, String> {
        MountRelativePath::parse(input, self.allows_empty())
    }
}

/// 解析统一 URI 路径为 `ResourceRef`。
///
/// 支持两种格式：
/// - `mount_id://relative/path` — 显式指定 mount
/// - `relative/path` — 使用 VFS 的默认 mount
///
/// 命中 VFS 的 `links` 表时会透明跳转到目标 `(mount_id, path)`，
/// 最多跳转 `MAX_LINK_DEPTH` 次。
pub fn parse_mount_uri(input: &str, vfs: &Vfs) -> Result<ResourceRef, String> {
    let raw_ref = VfsUri::parse(input, vfs, true)?.into_resource_ref();

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
            path: normalize_mount_relative_path(&next_path, true)?,
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

pub fn validate_vfs(vfs: &Vfs) -> Result<(), String> {
    let mut ids = HashSet::new();
    for mount in &vfs.mounts {
        MountId::parse(&mount.id)?;
        if !ids.insert(mount.id.as_str()) {
            return Err(format!("VFS mount id 重复: {}", mount.id));
        }
        if mount.provider.trim().is_empty() {
            return Err(format!("mount `{}` provider 不能为空", mount.id));
        }
        validate_reserved_mount_id(mount)?;
        validate_mount_root_ref(mount)?;
        validate_provider_capabilities(mount)?;
    }

    match vfs.default_mount_id.as_deref() {
        Some(default_mount_id) if vfs.mounts.iter().any(|mount| mount.id == default_mount_id) => {}
        Some(default_mount_id) => {
            return Err(format!(
                "default_mount_id 指向不存在的 mount: {default_mount_id}"
            ));
        }
        None if !vfs.mounts.is_empty() => {
            return Err("VFS 包含 mount 时必须设置 default_mount_id".to_string());
        }
        None => {}
    }

    validate_links(vfs)
}

fn validate_reserved_mount_id(mount: &Mount) -> Result<(), String> {
    let valid = match mount.id.as_str() {
        "main" => matches!(mount.provider.as_str(), "relay_fs" | "local_fs"),
        "lifecycle" => mount.provider == "lifecycle_vfs",
        "skill-assets" => mount.provider == "skill_asset_fs",
        id if id.starts_with("cvs-") => mount.provider == "canvas_fs",
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "mount id `{}` 是系统保留 ID，不能由 provider `{}` 使用",
            mount.id, mount.provider
        ))
    }
}

fn validate_mount_root_ref(mount: &Mount) -> Result<(), String> {
    let root_ref = RootRef::parse(&mount.root_ref)
        .map_err(|error| format!("mount `{}` root_ref 无效: {error}", mount.id))?;
    match mount.provider.as_str() {
        "relay_fs" | "local_fs" if !root_ref.is_local_path() => Err(format!(
            "mount `{}` provider `{}` 需要本机路径 root_ref",
            mount.id, mount.provider
        )),
        "lifecycle_vfs" if root_ref.scheme() != Some("lifecycle") => Err(format!(
            "mount `{}` lifecycle_vfs root_ref 必须使用 lifecycle://",
            mount.id
        )),
        "skill_asset_fs" if root_ref.scheme() != Some("skill-assets") => Err(format!(
            "mount `{}` skill_asset_fs root_ref 必须使用 skill-assets://",
            mount.id
        )),
        "canvas_fs" if root_ref.scheme() != Some("canvas") => Err(format!(
            "mount `{}` canvas_fs root_ref 必须使用 canvas://",
            mount.id
        )),
        "inline_fs" if root_ref.is_local_path() => Err(format!(
            "mount `{}` inline_fs root_ref 必须使用 provider URI",
            mount.id
        )),
        _ => Ok(()),
    }
}

fn validate_provider_capabilities(mount: &Mount) -> Result<(), String> {
    for capability in &mount.capabilities {
        let supported = match mount.provider.as_str() {
            "relay_fs" | "local_fs" => matches!(
                capability,
                MountCapability::Read
                    | MountCapability::Write
                    | MountCapability::List
                    | MountCapability::Search
                    | MountCapability::Exec
            ),
            "inline_fs" | "lifecycle_vfs" | "skill_asset_fs" | "canvas_fs" => matches!(
                capability,
                MountCapability::Read
                    | MountCapability::Write
                    | MountCapability::List
                    | MountCapability::Search
            ),
            _ => true,
        };
        if !supported {
            return Err(format!(
                "mount `{}` provider `{}` 不支持 capability `{}`",
                mount.id,
                mount.provider,
                capability_name(capability)
            ));
        }
    }
    Ok(())
}

fn validate_links(vfs: &Vfs) -> Result<(), String> {
    let ids: BTreeSet<&str> = vfs.mounts.iter().map(|mount| mount.id.as_str()).collect();
    let mut link_keys = HashSet::new();
    for link in &vfs.links {
        if !ids.contains(link.from_mount_id.as_str()) {
            return Err(format!(
                "VFS link from_mount_id 不存在: {}",
                link.from_mount_id
            ));
        }
        if !ids.contains(link.to_mount_id.as_str()) {
            return Err(format!("VFS link to_mount_id 不存在: {}", link.to_mount_id));
        }
        let from_path = normalize_mount_relative_path(&link.from_path, true)?;
        let to_path = normalize_mount_relative_path(&link.to_path, true)?;
        let key = (link.from_mount_id.clone(), from_path.clone());
        if !link_keys.insert(key) {
            return Err(format!(
                "VFS link 重复: {}://{}",
                link.from_mount_id, from_path
            ));
        }
        detect_link_cycle(vfs, &link.from_mount_id, &from_path)?;
        if to_path.is_empty() && link.to_mount_id.is_empty() {
            return Err("VFS link target 不能为空".to_string());
        }
    }
    Ok(())
}

fn detect_link_cycle(vfs: &Vfs, mount_id: &str, path: &str) -> Result<(), String> {
    let mut current = ResourceRef {
        mount_id: mount_id.to_string(),
        path: path.to_string(),
    };
    let mut seen = HashSet::new();
    for _ in 0..=MAX_LINK_DEPTH {
        let key = (current.mount_id.clone(), current.path.clone());
        if !seen.insert(key) {
            return Err(format!("VFS link 存在循环: {mount_id}://{path}"));
        }
        let Some(link) = vfs.find_link(&current.mount_id, &current.path) else {
            return Ok(());
        };
        current = ResourceRef {
            mount_id: link.to_mount_id.clone(),
            path: normalize_mount_relative_path(&link.to_path, true)?,
        };
    }
    Err(format!(
        "VFS link 解析超过最大深度 {MAX_LINK_DEPTH}: {mount_id}://{path}"
    ))
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
            root_ref: format!("context://inline/{id}"),
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

    #[test]
    fn parse_mount_uri_normalizes_before_returning() {
        let vfs = make_vfs(vec![]);
        let result = parse_mount_uri("src://docs//guide/../intro.md", &vfs).expect("resolve");
        assert_eq!(result.mount_id, "src");
        assert_eq!(result.path, "docs/intro.md");
    }

    #[test]
    fn typed_paths_reject_absolute_and_escape() {
        assert!(MountRelativePath::parse("../secret", false).is_err());
        assert!(MountRelativePath::parse("C:/repo/file.rs", false).is_err());
        assert_eq!(
            MountRelativePath::parse("./src//lib.rs", false)
                .expect("normalize")
                .as_str(),
            "src/lib.rs"
        );
    }

    #[test]
    fn root_ref_distinguishes_local_and_provider_uri() {
        assert!(matches!(
            RootRef::parse("/workspace/repo").expect("root"),
            RootRef::LocalPath(_)
        ));
        assert_eq!(
            RootRef::parse("lifecycle://run/123")
                .expect("root")
                .scheme(),
            Some("lifecycle")
        );
    }

    #[test]
    fn validate_vfs_rejects_duplicate_mounts_and_bad_default() {
        let mut vfs = make_vfs(vec![]);
        vfs.mounts.push(vfs.mounts[0].clone());
        let err = validate_vfs(&vfs).expect_err("duplicate id");
        assert!(err.contains("重复"));

        let mut vfs = make_vfs(vec![]);
        vfs.default_mount_id = Some("missing".to_string());
        let err = validate_vfs(&vfs).expect_err("bad default");
        assert!(err.contains("default_mount_id"));
    }

    #[test]
    fn validate_vfs_requires_default_mount_when_mounts_exist() {
        let mut vfs = make_vfs(vec![]);
        vfs.default_mount_id = None;

        let err = validate_vfs(&vfs).expect_err("missing default");

        assert!(err.contains("default_mount_id"));
    }

    #[test]
    fn validate_vfs_rejects_reserved_mount_id_conflict() {
        let mut vfs = make_vfs(vec![]);
        vfs.mounts[0].id = "lifecycle".to_string();

        let err = validate_vfs(&vfs).expect_err("reserved conflict");

        assert!(err.contains("系统保留"));
    }

    #[test]
    fn validate_vfs_rejects_builtin_provider_unsupported_capability() {
        let mut vfs = make_vfs(vec![]);
        vfs.mounts[0].capabilities.push(MountCapability::Exec);

        let err = validate_vfs(&vfs).expect_err("unsupported capability");

        assert!(err.contains("不支持 capability"));
    }
}
