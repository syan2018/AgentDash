use std::path::{Path, PathBuf};

/// 将请求级 working_dir 解析成执行级工作目录。
///
/// 本轮仅收口重复实现，不改变既有语义：非空输入直接按 `Path::join`
/// 处理，因此绝对路径与 `..` 的策略仍保持现状，后续由独立任务收紧。
pub fn resolve_working_dir(mount_root: &Path, requested: Option<&str>) -> PathBuf {
    match requested {
        Some(rel) if !rel.trim().is_empty() => mount_root.join(rel),
        _ => mount_root.to_path_buf(),
    }
}

/// 将执行级工作目录投影为相对 mount root 的 working_dir。
///
/// 返回 `None` 表示工作目录等于 mount root 或无法投影为 root 内相对路径。
/// 该函数刻意只做字符串规范化与前缀裁剪，避免在云端假设远端路径语义。
pub fn to_relative_working_dir(working_directory: &Path, mount_root_ref: &str) -> Option<String> {
    let root = mount_root_ref
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if root.is_empty() {
        return None;
    }
    let wd = working_directory
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    if wd == root {
        return None;
    }
    let prefix = format!("{root}/");
    wd.strip_prefix(&prefix)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_working_dir_defaults_to_mount_root() {
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), None),
            PathBuf::from("/workspace/repo")
        );
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), Some(" ")),
            PathBuf::from("/workspace/repo")
        );
    }

    #[test]
    fn resolve_working_dir_joins_relative_path() {
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), Some("crates/app")),
            PathBuf::from("/workspace/repo").join("crates/app")
        );
    }

    #[test]
    fn relative_working_dir_projects_subdir() {
        assert_eq!(
            to_relative_working_dir(Path::new("/workspace/repo/crates/app"), "/workspace/repo")
                .as_deref(),
            Some("crates/app")
        );
    }

    #[test]
    fn relative_working_dir_uses_none_for_root_or_outside_root() {
        assert_eq!(
            to_relative_working_dir(Path::new("/workspace/repo"), "/workspace/repo"),
            None
        );
        assert_eq!(
            to_relative_working_dir(Path::new("/workspace/other"), "/workspace/repo"),
            None
        );
    }

    #[test]
    fn relative_working_dir_normalizes_windows_separators() {
        assert_eq!(
            to_relative_working_dir(
                Path::new(r"D:\ABCTools_Dev\AgentDashboard\crates"),
                r"D:\ABCTools_Dev\AgentDashboard",
            )
            .as_deref(),
            Some("crates")
        );
    }
}
