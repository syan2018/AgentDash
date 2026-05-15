use std::fmt;
use std::path::{Component, Path, PathBuf};

/// 将请求级 working_dir 解析成执行级工作目录。
pub fn resolve_working_dir(
    mount_root: &Path,
    requested: Option<&str>,
) -> Result<PathBuf, WorkingDirPolicyError> {
    let Some(raw) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(mount_root.to_path_buf());
    };

    let requested_path = Path::new(raw);
    let mut relative = PathBuf::new();
    for component in requested_path.components() {
        match component {
            Component::Normal(segment) => relative.push(segment),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(WorkingDirPolicyError::ParentSegment {
                    requested: raw.to_string(),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(WorkingDirPolicyError::AbsoluteOrRoot {
                    requested: raw.to_string(),
                });
            }
        }
    }

    Ok(mount_root.join(relative))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingDirPolicyError {
    AbsoluteOrRoot { requested: String },
    ParentSegment { requested: String },
}

impl fmt::Display for WorkingDirPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbsoluteOrRoot { requested } => {
                write!(f, "working_dir `{requested}` 不能是绝对路径或根路径")
            }
            Self::ParentSegment { requested } => {
                write!(f, "working_dir `{requested}` 不能包含 `..` 越界片段")
            }
        }
    }
}

impl std::error::Error for WorkingDirPolicyError {}

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
            resolve_working_dir(Path::new("/workspace/repo"), None).unwrap(),
            PathBuf::from("/workspace/repo")
        );
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), Some(" ")).unwrap(),
            PathBuf::from("/workspace/repo")
        );
    }

    #[test]
    fn resolve_working_dir_joins_relative_path() {
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), Some("crates/app")).unwrap(),
            PathBuf::from("/workspace/repo").join("crates/app")
        );
        assert_eq!(
            resolve_working_dir(Path::new("/workspace/repo"), Some("./crates/app")).unwrap(),
            PathBuf::from("/workspace/repo").join("crates/app")
        );
    }

    #[test]
    fn resolve_working_dir_rejects_parent_segments() {
        let error = resolve_working_dir(Path::new("/workspace/repo"), Some("../outside"))
            .expect_err("parent segment must be rejected");
        assert!(matches!(error, WorkingDirPolicyError::ParentSegment { .. }));
    }

    #[test]
    fn resolve_working_dir_rejects_absolute_paths() {
        #[cfg(windows)]
        let outside = r"C:\outside";
        #[cfg(not(windows))]
        let outside = "/tmp/outside";

        let error = resolve_working_dir(Path::new("/workspace/repo"), Some(outside))
            .expect_err("absolute path must be rejected");
        assert!(matches!(
            error,
            WorkingDirPolicyError::AbsoluteOrRoot { .. }
        ));
    }

    #[test]
    fn resolve_working_dir_rejects_rooted_paths() {
        let error = resolve_working_dir(Path::new("/workspace/repo"), Some("/outside"))
            .expect_err("rooted path must be rejected");
        assert!(matches!(
            error,
            WorkingDirPolicyError::AbsoluteOrRoot { .. }
        ));
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
