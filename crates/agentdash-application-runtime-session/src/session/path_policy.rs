use std::path::{Path, PathBuf};

/// 将 default mount root 解析为 session 执行工作目录。
///
/// Session connector 的 working directory 必须是本机路径。`lifecycle://`、
/// `skill-assets://`、`canvas://` 等虚拟 root 需要先经过物化，不能隐式转成
/// `PathBuf` 后交给 connector。
pub fn resolve_session_working_directory(mount_root_ref: &str) -> Result<PathBuf, String> {
    let trimmed = mount_root_ref.trim();
    if trimmed.is_empty() {
        return Err("session working_dir 的 root_ref 不能为空".to_string());
    }
    if let Some((scheme, _)) = trimmed.split_once("://") {
        return Err(format!(
            "session working_dir 不能直接使用虚拟 mount root `{scheme}://`"
        ));
    }
    Ok(PathBuf::from(trimmed))
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

    #[test]
    fn session_working_directory_rejects_virtual_root() {
        let err = resolve_session_working_directory("lifecycle://run/abc")
            .expect_err("virtual root should fail");
        assert!(err.contains("虚拟 mount root"));
    }

    #[test]
    fn session_working_directory_accepts_local_root() {
        assert_eq!(
            resolve_session_working_directory("/workspace/repo")
                .expect("local root")
                .to_string_lossy(),
            "/workspace/repo"
        );
    }
}
