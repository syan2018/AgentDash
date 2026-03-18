//! ToolExecutor — PiAgent tool call 的本地执行环境
//!
//! 处理来自云端 PiAgent AgentLoop 的工具调用请求，
//! 在本机文件系统和 Shell 环境中执行。
//! 所有操作都受 accessible_roots 安全边界约束。

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentdash_relay::FileEntryRelay;

#[derive(Debug, Clone)]
pub struct ToolExecutor {
    accessible_roots: Vec<PathBuf>,
}

/// Shell 执行结果
pub struct ShellResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("路径安全检查失败: {0} 不在 accessible_roots 内")]
    PathNotAccessible(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("Shell 执行超时（{0}ms）")]
    Timeout(u64),

    #[error("路径解析失败: {0}")]
    InvalidPath(String),
}

impl ToolExecutor {
    pub fn new(accessible_roots: Vec<PathBuf>) -> Self {
        Self { accessible_roots }
    }

    pub fn accessible_roots(&self) -> &[PathBuf] {
        &self.accessible_roots
    }

    /// 验证 workspace_root 在 accessible_roots 内
    pub fn validate_workspace_root(&self, workspace_root: &str) -> Result<PathBuf, ToolError> {
        let ws_path = PathBuf::from(workspace_root);
        let canonical = std::fs::canonicalize(&ws_path)
            .map_err(|_| ToolError::InvalidPath(workspace_root.to_string()))?;

        if self.accessible_roots.is_empty() {
            return Ok(canonical);
        }

        for root in &self.accessible_roots {
            if let Ok(root_canonical) = std::fs::canonicalize(root) {
                if canonical.starts_with(&root_canonical) {
                    return Ok(canonical);
                }
            }
        }

        Err(ToolError::PathNotAccessible(workspace_root.to_string()))
    }

    pub fn resolve_existing_path(
        &self,
        relative_path: &str,
        workspace_root: &str,
    ) -> Result<PathBuf, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        resolve_existing_path_with_root(&ws, relative_path)
    }

    pub fn resolve_path_for_write(
        &self,
        relative_path: &str,
        workspace_root: &str,
    ) -> Result<PathBuf, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        resolve_path_for_write_with_root(&ws, relative_path)
    }

    pub async fn file_read(&self, path: &str, workspace_root: &str) -> Result<String, ToolError> {
        let full_path = self.resolve_existing_path(path, workspace_root)?;
        tracing::debug!(path = %full_path.display(), "file_read");
        let content = tokio::fs::read_to_string(&full_path).await?;
        Ok(content)
    }

    pub async fn file_write(
        &self,
        path: &str,
        content: &str,
        workspace_root: &str,
    ) -> Result<(), ToolError> {
        let full_path = self.resolve_path_for_write(path, workspace_root)?;
        tracing::debug!(path = %full_path.display(), "file_write");

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full_path, content).await?;
        Ok(())
    }

    pub async fn shell_exec(
        &self,
        command: &str,
        workspace_root: &str,
        timeout_ms: Option<u64>,
    ) -> Result<ShellResult, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));

        tracing::debug!(command = %command, cwd = %ws.display(), "shell_exec");

        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let flag = if cfg!(windows) { "/C" } else { "-c" };

        let child = tokio::process::Command::new(shell)
            .arg(flag)
            .arg(command)
            .current_dir(&ws)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => Ok(ShellResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Ok(Err(e)) => Err(ToolError::Io(e)),
            Err(_) => Err(ToolError::Timeout(timeout_ms.unwrap_or(30_000))),
        }
    }

    pub async fn file_list(
        &self,
        path: &str,
        workspace_root: &str,
        pattern: Option<&str>,
        recursive: bool,
    ) -> Result<Vec<FileEntryRelay>, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        let base = if path.trim().is_empty() || path.trim() == "." {
            ws.clone()
        } else {
            resolve_existing_path_with_root(&ws, path)?
        };

        tracing::debug!(
            path = %base.display(),
            pattern = ?pattern,
            recursive = recursive,
            "file_list"
        );

        let glob_matcher = pattern
            .map(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()))
            .flatten();

        let mut entries = Vec::new();
        collect_entries(&base, &ws, &glob_matcher, recursive, &mut entries).await?;
        Ok(entries)
    }
}

fn resolve_existing_path_with_root(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, ToolError> {
    let normalized = normalize_relative_path(relative_path)?;
    let candidate = if normalized.as_os_str().is_empty() {
        workspace_root.to_path_buf()
    } else {
        workspace_root.join(normalized)
    };

    if !candidate.exists() {
        return Err(ToolError::InvalidPath(relative_path.to_string()));
    }

    let canonical = std::fs::canonicalize(&candidate)?;
    if !canonical.starts_with(workspace_root) {
        return Err(ToolError::PathNotAccessible(relative_path.to_string()));
    }
    Ok(canonical)
}

fn resolve_path_for_write_with_root(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, ToolError> {
    let normalized = normalize_relative_path(relative_path)?;
    if normalized.as_os_str().is_empty() {
        return Err(ToolError::InvalidPath(relative_path.to_string()));
    }

    let candidate = workspace_root.join(&normalized);
    let parent = candidate
        .parent()
        .ok_or_else(|| ToolError::InvalidPath(relative_path.to_string()))?;
    std::fs::create_dir_all(parent)?;
    let canonical_parent = std::fs::canonicalize(parent)?;
    if !canonical_parent.starts_with(workspace_root) {
        return Err(ToolError::PathNotAccessible(relative_path.to_string()));
    }
    Ok(candidate)
}

fn normalize_relative_path(input: &str) -> Result<PathBuf, ToolError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(PathBuf::new());
    }

    if is_absolute_like(trimmed) {
        return Err(ToolError::InvalidPath(trimmed.to_string()));
    }

    let mut normalized = PathBuf::new();
    for part in trimmed.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if !normalized.pop() {
                return Err(ToolError::PathNotAccessible(trimmed.to_string()));
            }
            continue;
        }
        normalized.push(part);
    }
    Ok(normalized)
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

async fn collect_entries(
    dir: &Path,
    workspace_root: &Path,
    glob_matcher: &Option<globset::GlobMatcher>,
    recursive: bool,
    entries: &mut Vec<FileEntryRelay>,
) -> Result<(), ToolError> {
    let mut read_dir = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let file_type = entry.file_type().await?;
        let is_dir = file_type.is_dir();
        let path = entry.path();

        let relative = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let matches = glob_matcher
            .as_ref()
            .map(|matcher| {
                matcher.is_match(&relative)
                    || matcher.is_match(entry.file_name().to_string_lossy().as_ref())
            })
            .unwrap_or(true);

        if matches || is_dir {
            if matches {
                let metadata = entry.metadata().await.ok();
                entries.push(FileEntryRelay {
                    path: relative,
                    size: metadata.as_ref().map(|item| item.len()),
                    modified_at: metadata
                        .as_ref()
                        .and_then(|item| item.modified().ok())
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|duration| duration.as_millis() as i64),
                    is_dir,
                });
            }

            if is_dir && recursive {
                Box::pin(collect_entries(
                    &path,
                    workspace_root,
                    glob_matcher,
                    recursive,
                    entries,
                ))
                .await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_for_write_blocks_escape() {
        let temp = tempfile::tempdir().expect("tempdir");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let error = executor
            .resolve_path_for_write("../escape.txt", &root)
            .expect_err("escape should be rejected");
        assert!(matches!(error, ToolError::PathNotAccessible(_)));
    }

    #[test]
    fn resolve_existing_path_blocks_absolute_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("demo.txt");
        std::fs::write(&file, "ok").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let error = executor
            .resolve_existing_path(file.to_string_lossy().as_ref(), &root)
            .expect_err("absolute path should be rejected");
        assert!(matches!(error, ToolError::InvalidPath(_)));
    }
}
