//! ToolExecutor — PiAgent tool call 的本地执行环境
//!
//! 处理来自云端 PiAgent AgentLoop 的工具调用请求，
//! 在本机文件系统和 Shell 环境中执行。
//! 所有操作都受 accessible_roots 安全边界约束。

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentdash_application::vfs::ApplyPatchAffectedPaths;
use agentdash_relay::{FileEntryRelay, SearchHit};

pub(crate) struct SearchParams<'a> {
    pub query: &'a str,
    pub path: Option<&'a str>,
    pub is_regex: bool,
    pub include_glob: Option<&'a str>,
    pub max_results: usize,
    pub context_lines: usize,
}

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

    #[error("Patch 应用失败: {0}")]
    PatchApply(String),
}

impl ToolExecutor {
    pub fn new(accessible_roots: Vec<PathBuf>) -> Self {
        Self { accessible_roots }
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
            if let Ok(root_canonical) = std::fs::canonicalize(root)
                && canonical.starts_with(&root_canonical)
            {
                return Ok(canonical);
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

    pub async fn file_delete(&self, path: &str, workspace_root: &str) -> Result<(), ToolError> {
        let full_path = self.resolve_existing_path(path, workspace_root)?;
        tracing::debug!(path = %full_path.display(), "file_delete");
        tokio::fs::remove_file(&full_path).await?;
        Ok(())
    }

    pub async fn file_rename(
        &self,
        from_path: &str,
        to_path: &str,
        workspace_root: &str,
    ) -> Result<(), ToolError> {
        let source = self.resolve_existing_path(from_path, workspace_root)?;
        let destination = self.resolve_path_for_write(to_path, workspace_root)?;
        tracing::debug!(
            from = %source.display(),
            to = %destination.display(),
            "file_rename"
        );
        if destination.exists() && source != destination {
            return Err(ToolError::InvalidPath(format!("目标文件已存在: {to_path}")));
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(&source, &destination).await?;
        Ok(())
    }

    pub async fn apply_patch(
        &self,
        patch: &str,
        workspace_root: &str,
    ) -> Result<ApplyPatchAffectedPaths, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        tracing::debug!(workspace_root = %ws.display(), "apply_patch");
        let patch_owned = patch.to_string();

        tokio::task::spawn_blocking(move || {
            agentdash_application::vfs::apply_patch_to_fs(&ws, &patch_owned)
        })
        .await
        .map_err(|e| ToolError::PatchApply(format!("patch 任务失败: {e}")))?
        .map_err(|e| ToolError::PatchApply(e.to_string()))
    }

    pub fn resolve_shell_cwd(
        &self,
        workspace_root: &str,
        cwd: Option<&str>,
    ) -> Result<PathBuf, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        let requested = cwd.unwrap_or_default().trim();
        if requested.is_empty() || requested == "." {
            return Ok(ws);
        }

        if is_absolute_like(requested) {
            let absolute = PathBuf::from(requested);
            let canonical = std::fs::canonicalize(&absolute)
                .map_err(|_| ToolError::InvalidPath(requested.to_string()))?;
            if !canonical.starts_with(&ws) {
                return Err(ToolError::PathNotAccessible(requested.to_string()));
            }
            return Ok(canonical);
        }

        resolve_existing_path_with_root(&ws, requested)
    }

    pub async fn shell_exec(
        &self,
        command: &str,
        workspace_root: &str,
        cwd: Option<&str>,
        timeout_ms: Option<u64>,
    ) -> Result<ShellResult, ToolError> {
        let ws = self.resolve_shell_cwd(workspace_root, cwd)?;
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));

        tracing::debug!(
            command = %command,
            workspace_root = workspace_root,
            requested_cwd = ?cwd,
            cwd = %ws.display(),
            "shell_exec"
        );

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

        let glob_matcher =
            pattern.and_then(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()));

        let mut entries = Vec::new();
        collect_entries(&base, &ws, &glob_matcher, recursive, &mut entries).await?;
        Ok(entries)
    }

    pub async fn search(
        &self,
        workspace_root: &str,
        params: &SearchParams<'_>,
    ) -> Result<(Vec<SearchHit>, bool), ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        let search_dir = match params.path {
            Some(p) if !p.trim().is_empty() && p.trim() != "." => {
                resolve_existing_path_with_root(&ws, p)?
            }
            _ => ws.clone(),
        };

        if let Some(rg) = detect_ripgrep().await {
            return run_ripgrep(&rg, &search_dir, &ws, params).await;
        }

        fallback_search(&ws, &search_dir, params).await
    }
}

async fn detect_ripgrep() -> Option<PathBuf> {
    let candidates = if cfg!(windows) {
        vec!["rg.exe", "rg"]
    } else {
        vec!["rg"]
    };
    for name in candidates {
        if let Ok(output) =
            tokio::process::Command::new(if cfg!(windows) { "where" } else { "which" })
                .arg(name)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            && output.status.success()
        {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let first_line = path_str.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Some(PathBuf::from(first_line));
            }
        }
    }
    None
}

async fn run_ripgrep(
    rg_path: &Path,
    search_dir: &Path,
    workspace_root: &Path,
    params: &SearchParams<'_>,
) -> Result<(Vec<SearchHit>, bool), ToolError> {
    let mut cmd = tokio::process::Command::new(rg_path);
    cmd.arg("--json")
        .arg("--max-count")
        .arg(params.max_results.to_string());

    if params.context_lines > 0 {
        cmd.arg("-C").arg(params.context_lines.to_string());
    }
    if !params.is_regex {
        cmd.arg("--fixed-strings");
    }
    if let Some(glob) = params.include_glob {
        cmd.arg("--glob").arg(glob);
    }

    cmd.arg("--").arg(params.query).arg(search_dir);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| ToolError::Timeout(30_000))?
        .map_err(ToolError::Io)?;

    let mut hits = Vec::new();
    let mut truncated = false;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if json.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }

        let data = match json.get("data") {
            Some(d) => d,
            None => continue,
        };

        let abs_path = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let rel_path = Path::new(abs_path)
            .strip_prefix(workspace_root)
            .unwrap_or(Path::new(abs_path))
            .to_string_lossy()
            .replace('\\', "/");

        let line_number = data
            .get("line_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;

        let content = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string();

        hits.push(SearchHit {
            path: rel_path,
            line_number,
            content,
            context_before: Vec::new(),
            context_after: Vec::new(),
        });

        if hits.len() >= params.max_results {
            truncated = true;
            break;
        }
    }

    Ok((hits, truncated))
}

async fn fallback_search(
    workspace_root: &Path,
    search_dir: &Path,
    params: &SearchParams<'_>,
) -> Result<(Vec<SearchHit>, bool), ToolError> {
    let ws = workspace_root.to_path_buf();
    let dir = search_dir.to_path_buf();
    let query = params.query.to_string();
    let regex = if params.is_regex {
        Some(
            regex::Regex::new(&query)
                .map_err(|e| ToolError::InvalidPath(format!("无效正则: {e}")))?,
        )
    } else {
        None
    };
    let max_results = params.max_results;
    let context_lines = params.context_lines;

    tokio::task::spawn_blocking(move || {
        let mut collector = FallbackCollector {
            workspace_root: &ws,
            query: &query,
            regex: regex.as_ref(),
            max_results,
            context_lines,
            hits: Vec::new(),
            truncated: false,
        };
        collector.walk(&dir);
        Ok((collector.hits, collector.truncated))
    })
    .await
    .map_err(|e| ToolError::Io(std::io::Error::other(e)))?
}

const FALLBACK_MAX_FILE_BYTES: u64 = 256 * 1024;
const FALLBACK_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".next",
    "dist",
    "build",
    ".venv",
];

struct FallbackCollector<'a> {
    workspace_root: &'a Path,
    query: &'a str,
    regex: Option<&'a regex::Regex>,
    max_results: usize,
    context_lines: usize,
    hits: Vec<SearchHit>,
    truncated: bool,
}

impl FallbackCollector<'_> {
    fn walk(&mut self, dir: &Path) {
        if self.hits.len() >= self.max_results {
            self.truncated = true;
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if self.hits.len() >= self.max_results {
                self.truncated = true;
                return;
            }
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if ft.is_dir() {
                if FALLBACK_SKIP_DIRS.contains(&name_str.as_ref()) {
                    continue;
                }
                self.walk(&entry.path());
            } else if ft.is_file() {
                self.scan_file(&entry);
            }
        }
    }

    fn scan_file(&mut self, entry: &std::fs::DirEntry) {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => return,
        };
        if meta.len() > FALLBACK_MAX_FILE_BYTES {
            return;
        }
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => return,
        };
        let lines: Vec<&str> = content.lines().collect();
        let rel = entry
            .path()
            .strip_prefix(self.workspace_root)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .replace('\\', "/");

        for (idx, line) in lines.iter().enumerate() {
            let matched = match &self.regex {
                Some(re) => re.is_match(line),
                None => line.contains(self.query),
            };
            if matched {
                let ctx_before: Vec<String> = if self.context_lines > 0 {
                    let start = idx.saturating_sub(self.context_lines);
                    lines[start..idx].iter().map(|s| s.to_string()).collect()
                } else {
                    Vec::new()
                };
                let ctx_after: Vec<String> = if self.context_lines > 0 {
                    let end = (idx + 1 + self.context_lines).min(lines.len());
                    lines[idx + 1..end].iter().map(|s| s.to_string()).collect()
                } else {
                    Vec::new()
                };

                self.hits.push(SearchHit {
                    path: rel.clone(),
                    line_number: idx + 1,
                    content: line.to_string(),
                    context_before: ctx_before,
                    context_after: ctx_after,
                });
                if self.hits.len() >= self.max_results {
                    self.truncated = true;
                    return;
                }
            }
        }
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

    #[tokio::test]
    async fn file_delete_removes_existing_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("demo.txt");
        std::fs::write(&file, "ok").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        executor
            .file_delete("demo.txt", &root)
            .await
            .expect("delete should succeed");

        assert!(!file.exists());
    }

    #[tokio::test]
    async fn file_rename_moves_file_inside_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("demo.txt");
        std::fs::write(&source, "ok").expect("write");
        let destination = temp.path().join("nested").join("renamed.txt");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        executor
            .file_rename("demo.txt", "nested/renamed.txt", &root)
            .await
            .expect("rename should succeed");

        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(destination).expect("read renamed file"),
            "ok"
        );
    }

    #[test]
    fn resolve_shell_cwd_allows_relative_directory_inside_workspace_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let child = temp.path().join("nested");
        std::fs::create_dir_all(&child).expect("mkdir");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let cwd = executor
            .resolve_shell_cwd(&root, Some("nested"))
            .expect("relative cwd should resolve");

        assert_eq!(cwd, std::fs::canonicalize(child).expect("canonical child"));
    }

    #[test]
    fn resolve_shell_cwd_rejects_absolute_directory_outside_workspace_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();
        let outside_path = outside.path().to_string_lossy().to_string();

        let error = executor
            .resolve_shell_cwd(&root, Some(&outside_path))
            .expect_err("outside absolute cwd should be rejected");

        assert!(matches!(error, ToolError::PathNotAccessible(_)));
    }
}
