//! ToolExecutor — PiAgent tool call 的本地执行环境
//!
//! 处理来自云端 PiAgent AgentLoop 的工具调用请求，
//! 在本机文件系统和 Shell 环境中执行。
//! 所有执行类操作都受 session mount root 边界约束。

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentdash_application::vfs::{ApplyPatchAffectedPaths, FsPatchTarget, apply_patch_to_target};
use agentdash_relay::{FileEntryRelay, SearchHit, ShellOutputStream};
use ignore::WalkBuilder;
use tokio::io::{AsyncBufReadExt, BufReader};

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
    workspace_roots_configured: bool,
    canonical_workspace_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDiscoveryIntent {
    ImplicitWorkspaceScan,
    ExplicitSubtreeScan,
}

#[derive(Debug, Clone, Copy)]
struct FileDiscoveryPolicy {
    intent: FileDiscoveryIntent,
}

const HARD_EXCLUDE_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];
const BUILTIN_NOISE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
];

/// Shell 执行结果
pub struct ShellResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// 文件 bytes 读取结果
pub struct BinaryFileResult {
    pub data: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("路径安全检查失败: {0} 不在当前执行 workspace 边界内")]
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

fn canonicalize_workspace_roots(workspace_roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut canonical_roots = Vec::new();
    for root in workspace_roots {
        let Ok(canonical) = std::fs::canonicalize(root) else {
            continue;
        };
        if !canonical.is_dir() || canonical_roots.contains(&canonical) {
            continue;
        }
        canonical_roots.push(canonical);
    }
    canonical_roots
}

impl ToolExecutor {
    pub fn new(workspace_roots: Vec<PathBuf>) -> Self {
        let workspace_roots_configured = !workspace_roots.is_empty();
        let canonical_workspace_roots = canonicalize_workspace_roots(workspace_roots);
        Self {
            workspace_roots_configured,
            canonical_workspace_roots,
        }
    }

    /// 验证执行类操作的 workspace root，并在配置了 workspace roots 时检查其归属。
    pub fn validate_workspace_root(&self, workspace_root: &str) -> Result<PathBuf, ToolError> {
        let trimmed = workspace_root.trim();
        if trimmed.is_empty() {
            return Err(ToolError::InvalidPath(
                "workspace root 不能为空".to_string(),
            ));
        }

        let ws_path = PathBuf::from(trimmed);
        let canonical = std::fs::canonicalize(&ws_path)
            .map_err(|_| ToolError::InvalidPath(workspace_root.to_string()))?;

        if !canonical.is_dir() {
            return Err(ToolError::InvalidPath(format!(
                "workspace root 不是目录: {workspace_root}"
            )));
        }

        if !self.workspace_roots_configured {
            return Ok(canonical);
        }

        for root in &self.canonical_workspace_roots {
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }

        Err(ToolError::PathNotAccessible(format!(
            "workspace root 未登记: {workspace_root}"
        )))
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

    pub async fn file_read_binary(
        &self,
        path: &str,
        workspace_root: &str,
    ) -> Result<BinaryFileResult, ToolError> {
        let full_path = self.resolve_existing_path(path, workspace_root)?;
        if !full_path.is_file() {
            return Err(ToolError::InvalidPath(path.to_string()));
        }
        tracing::debug!(path = %full_path.display(), "file_read_binary");
        let data = tokio::fs::read(&full_path).await?;
        Ok(BinaryFileResult {
            data,
            mime_type: infer_mime_type(path),
        })
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
        let target = FsPatchTarget::new(&ws).map_err(|e| ToolError::PatchApply(e.to_string()))?;
        apply_patch_to_target(&target, patch)
            .await
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
            return Err(ToolError::InvalidPath(
                "shell cwd 必须是相对于 workspace root 的路径".to_string(),
            ));
        }

        resolve_existing_path_with_root(&ws, requested)
    }

    #[allow(dead_code)]
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

        let child = shell_command(command, &ws)
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

    /// 流式 shell 执行 — 逐行推送 stdout/stderr 到回调，完成后返回最终结果。
    pub async fn shell_exec_streaming<F>(
        &self,
        command: &str,
        workspace_root: &str,
        cwd: Option<&str>,
        timeout_ms: Option<u64>,
        mut on_output: F,
    ) -> Result<ShellResult, ToolError>
    where
        F: FnMut(&str, ShellOutputStream) + Send,
    {
        let ws = self.resolve_shell_cwd(workspace_root, cwd)?;
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));

        tracing::debug!(
            command = %command,
            cwd = %ws.display(),
            "shell_exec_streaming"
        );

        let mut child = shell_command(command, &ws)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);
        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        let read_loop = async {
            let mut stdout_done = false;
            let mut stderr_done = false;
            let mut stdout_line = Vec::new();
            let mut stderr_line = Vec::new();

            while !stdout_done || !stderr_done {
                tokio::select! {
                    read = stdout_reader.read_until(b'\n', &mut stdout_line), if !stdout_done => {
                        match read {
                            Ok(0) => {
                                stdout_done = true;
                            }
                            Ok(_) => {
                                let chunk = decode_output_chunk(&stdout_line);
                                stdout_line.clear();
                                on_output(&chunk, ShellOutputStream::Stdout);
                                stdout_buf.push_str(&chunk);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "stdout read error");
                                return Err(e);
                            }
                        }
                    }
                    read = stderr_reader.read_until(b'\n', &mut stderr_line), if !stderr_done => {
                        match read {
                            Ok(0) => {
                                stderr_done = true;
                            }
                            Ok(_) => {
                                let chunk = decode_output_chunk(&stderr_line);
                                stderr_line.clear();
                                on_output(&chunk, ShellOutputStream::Stderr);
                                stderr_buf.push_str(&chunk);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "stderr read error");
                                return Err(e);
                            }
                        }
                    }
                }
            }

            Ok::<(), std::io::Error>(())
        };

        match tokio::time::timeout(timeout, read_loop).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(ToolError::Io(e)),
            Err(_) => {
                let _ = child.kill().await;
                return Err(ToolError::Timeout(timeout_ms.unwrap_or(30_000)));
            }
        }

        let status = child.wait().await.map_err(ToolError::Io)?;

        Ok(ShellResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_buf,
            stderr: stderr_buf,
        })
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
        let policy = FileDiscoveryPolicy::from_base(&base, &ws);
        if !policy.allows_path(&base, &ws) {
            return Ok(Vec::new());
        }

        tracing::debug!(
            path = %base.display(),
            pattern = ?pattern,
            recursive = recursive,
            "file_list"
        );

        let pattern = pattern.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            collect_entries(&base, &ws, pattern.as_deref(), recursive, policy)
        })
        .await
        .map_err(|e| ToolError::Io(std::io::Error::other(e)))?
    }

    pub async fn search(
        &self,
        workspace_root: &str,
        params: &SearchParams<'_>,
    ) -> Result<(Vec<SearchHit>, bool), ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        search_workspace_with_ripgrep(&ws, params, detect_ripgrep().await).await
    }

    #[cfg(test)]
    async fn search_with_ripgrep_path(
        &self,
        workspace_root: &str,
        params: &SearchParams<'_>,
        rg_path: Option<PathBuf>,
    ) -> Result<(Vec<SearchHit>, bool), ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        search_workspace_with_ripgrep(&ws, params, rg_path).await
    }
}

pub(crate) fn resolve_detect_workspace_root(workspace_root: &str) -> Result<PathBuf, ToolError> {
    let trimmed = workspace_root.trim();
    if trimmed.is_empty() {
        return Err(ToolError::InvalidPath(
            "workspace root 不能为空".to_string(),
        ));
    }

    let canonical =
        std::fs::canonicalize(trimmed).map_err(|_| ToolError::InvalidPath(trimmed.to_string()))?;
    if !canonical.is_dir() {
        return Err(ToolError::InvalidPath(format!(
            "workspace root 不是目录: {trimmed}"
        )));
    }

    std::fs::read_dir(&canonical).map_err(ToolError::Io)?;
    Ok(canonical)
}

async fn search_workspace_with_ripgrep(
    workspace_root: &Path,
    params: &SearchParams<'_>,
    rg_path: Option<PathBuf>,
) -> Result<(Vec<SearchHit>, bool), ToolError> {
    let search_dir = match params.path {
        Some(p) if !p.trim().is_empty() && p.trim() != "." => {
            resolve_existing_path_with_root(workspace_root, p)?
        }
        _ => workspace_root.to_path_buf(),
    };
    let policy = FileDiscoveryPolicy::from_base(&search_dir, workspace_root);
    if !policy.allows_path(&search_dir, workspace_root) {
        return Ok((Vec::new(), false));
    }

    let rg = rg_path.ok_or_else(ripgrep_unavailable_error)?;
    run_ripgrep(&rg, &search_dir, workspace_root, params, policy).await
}

fn ripgrep_unavailable_error() -> ToolError {
    ToolError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "未找到 rg/ripgrep",
    ))
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
    policy: FileDiscoveryPolicy,
) -> Result<(Vec<SearchHit>, bool), ToolError> {
    let mut cmd = tokio::process::Command::new(rg_path);
    cmd.arg("--json")
        .arg("--max-count")
        .arg(params.max_results.to_string());
    for arg in ripgrep_policy_args(policy) {
        cmd.arg(arg);
    }

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

        let Ok(rel_path) = workspace_relative_path(Path::new(abs_path), workspace_root) else {
            tracing::warn!(
                path = abs_path,
                workspace_root = %workspace_root.display(),
                "ripgrep returned path outside workspace root"
            );
            continue;
        };

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
    let canonical_parent = canonical_existing_write_parent(parent, relative_path)?;
    if !canonical_parent.starts_with(workspace_root) {
        return Err(ToolError::PathNotAccessible(relative_path.to_string()));
    }
    Ok(candidate)
}

fn canonical_existing_write_parent(
    parent: &Path,
    relative_path: &str,
) -> Result<PathBuf, ToolError> {
    let mut current = parent;
    loop {
        if current.exists() {
            return std::fs::canonicalize(current).map_err(ToolError::Io);
        }
        current = current
            .parent()
            .ok_or_else(|| ToolError::InvalidPath(relative_path.to_string()))?;
    }
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

fn workspace_relative_path(path: &Path, workspace_root: &Path) -> Result<String, ToolError> {
    path.strip_prefix(workspace_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| {
            ToolError::PathNotAccessible(format!(
                "{} is outside {}",
                path.display(),
                workspace_root.display()
            ))
        })
}

fn collect_entries(
    dir: &Path,
    workspace_root: &Path,
    glob_pattern: Option<&str>,
    recursive: bool,
    policy: FileDiscoveryPolicy,
) -> Result<Vec<FileEntryRelay>, ToolError> {
    let glob_matcher =
        glob_pattern.and_then(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()));
    let walker = build_walk(dir, workspace_root, policy, recursive);
    let mut entries = Vec::new();

    for result in walker.build() {
        let entry = result.map_err(|e| ToolError::Io(std::io::Error::other(e)))?;
        let path = entry.path();
        if path == dir {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();
        let relative = workspace_relative_path(path, workspace_root)?;

        let matches = glob_matcher
            .as_ref()
            .map(|matcher| {
                matcher.is_match(&relative)
                    || path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| matcher.is_match(name))
            })
            .unwrap_or(true);

        if matches {
            let metadata = std::fs::metadata(path).ok();
            entries.push(FileEntryRelay {
                path: relative,
                size: metadata.as_ref().map(|item| item.len()),
                modified_at: metadata
                    .as_ref()
                    .and_then(|item| item.modified().ok())
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as i64),
                is_dir,
                content_kind: file_content_kind(path, is_dir),
                mime_type: file_mime_type(path, is_dir),
            });
        }
    }

    Ok(entries)
}

fn build_walk(
    dir: &Path,
    workspace_root: &Path,
    policy: FileDiscoveryPolicy,
    recursive: bool,
) -> WalkBuilder {
    let mut builder = WalkBuilder::new(dir);
    let respect_workspace_ignores = policy.respects_workspace_ignores();
    builder
        .hidden(false)
        .ignore(respect_workspace_ignores)
        .git_ignore(respect_workspace_ignores)
        .git_global(respect_workspace_ignores)
        .git_exclude(respect_workspace_ignores)
        .require_git(false)
        .parents(respect_workspace_ignores);
    if !recursive {
        builder.max_depth(Some(1));
    }
    let workspace_root = workspace_root.to_path_buf();
    builder.filter_entry(move |entry| policy.allows_path(entry.path(), &workspace_root));
    builder
}

impl FileDiscoveryPolicy {
    fn from_base(base: &Path, workspace_root: &Path) -> Self {
        let intent = if base == workspace_root {
            FileDiscoveryIntent::ImplicitWorkspaceScan
        } else {
            FileDiscoveryIntent::ExplicitSubtreeScan
        };
        Self { intent }
    }

    fn respects_workspace_ignores(self) -> bool {
        self.intent == FileDiscoveryIntent::ImplicitWorkspaceScan
    }

    fn allows_path(self, path: &Path, workspace_root: &Path) -> bool {
        if path_has_named_segment(path, workspace_root, HARD_EXCLUDE_DIRS) {
            return false;
        }
        if self.intent == FileDiscoveryIntent::ImplicitWorkspaceScan
            && path_has_named_segment(path, workspace_root, BUILTIN_NOISE_DIRS)
        {
            return false;
        }
        true
    }
}

fn path_has_named_segment(path: &Path, workspace_root: &Path, names: &[&str]) -> bool {
    let relative = path.strip_prefix(workspace_root).unwrap_or(path);
    relative.components().any(|component| {
        let std::path::Component::Normal(segment) = component else {
            return false;
        };
        segment_matches(segment, names)
    })
}

fn segment_matches(segment: &std::ffi::OsStr, names: &[&str]) -> bool {
    segment
        .to_str()
        .is_some_and(|value| names.iter().any(|name| value.eq_ignore_ascii_case(name)))
}

fn ripgrep_policy_args(policy: FileDiscoveryPolicy) -> Vec<String> {
    let mut args = vec!["--hidden".to_string()];
    if policy.respects_workspace_ignores() {
        args.push("--no-require-git".to_string());
    }
    if !policy.respects_workspace_ignores() {
        args.push("--no-ignore".to_string());
    }
    push_ripgrep_exclude_globs(&mut args, HARD_EXCLUDE_DIRS);
    if policy.intent == FileDiscoveryIntent::ImplicitWorkspaceScan {
        push_ripgrep_exclude_globs(&mut args, BUILTIN_NOISE_DIRS);
    }
    args
}

fn push_ripgrep_exclude_globs(args: &mut Vec<String>, dirs: &[&str]) {
    for dir in dirs {
        args.push("--glob".to_string());
        args.push(format!("!{dir}/**"));
        args.push("--glob".to_string());
        args.push(format!("!**/{dir}/**"));
    }
}

fn file_content_kind(path: &Path, is_dir: bool) -> Option<String> {
    if is_dir {
        return None;
    }
    is_image_path(path).then(|| "binary".to_string())
}

fn file_mime_type(path: &Path, is_dir: bool) -> Option<String> {
    if is_dir {
        return None;
    }
    let mime_type = infer_mime_type(&path.to_string_lossy());
    (mime_type != "application/octet-stream").then_some(mime_type)
}

fn infer_mime_type(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn is_image_path(path: &Path) -> bool {
    infer_mime_type(&path.to_string_lossy()).starts_with("image/")
}

fn shell_command(command: &str, cwd: &Path) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut shell = tokio::process::Command::new("powershell.exe");
        let command = format!(
            "$OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = $OutputEncoding; {command}"
        );
        shell
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(command)
            .current_dir(cwd);
        shell
    }

    #[cfg(not(windows))]
    {
        let mut shell = tokio::process::Command::new("sh");
        shell.arg("-c").arg(command).current_dir(cwd);
        shell
    }
}

fn decode_output_chunk(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(path, content).expect("write file");
    }

    fn entry_paths(entries: &[FileEntryRelay]) -> Vec<&str> {
        entries.iter().map(|entry| entry.path.as_str()).collect()
    }

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
    fn resolve_path_for_write_does_not_create_parent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let resolved = executor
            .resolve_path_for_write("nested/demo.txt", &root)
            .expect("write path should resolve");
        let expected = std::fs::canonicalize(temp.path())
            .expect("canonical workspace")
            .join("nested")
            .join("demo.txt");

        assert_eq!(resolved, expected);
        assert!(
            !nested.exists(),
            "write path resolution must not create parent directories"
        );
    }

    #[test]
    fn validate_workspace_root_allows_mount_root_when_workspace_roots_empty() {
        let workspace = tempfile::tempdir().expect("workspace");
        let executor = ToolExecutor::new(Vec::new());

        let resolved = executor
            .validate_workspace_root(workspace.path().to_string_lossy().as_ref())
            .expect("empty workspace roots should not block explicit mount root");

        assert_eq!(
            resolved,
            std::fs::canonicalize(workspace.path()).expect("canonical workspace")
        );
    }

    #[test]
    fn registered_roots_are_canonicalized_and_deduped_on_construction() {
        let workspace = tempfile::tempdir().expect("workspace");
        let duplicate = workspace.path().join(".");
        let executor = ToolExecutor::new(vec![workspace.path().to_path_buf(), duplicate]);

        assert!(executor.workspace_roots_configured);
        assert_eq!(executor.canonical_workspace_roots.len(), 1);
        assert_eq!(
            executor.canonical_workspace_roots[0],
            std::fs::canonicalize(workspace.path()).expect("canonical workspace")
        );
    }

    #[test]
    fn unavailable_registered_roots_do_not_open_workspace_boundary() {
        let workspace = tempfile::tempdir().expect("workspace");
        let unavailable_parent = tempfile::tempdir().expect("unavailable parent");
        let unavailable_root = unavailable_parent.path().join("missing");
        let executor = ToolExecutor::new(vec![unavailable_root]);
        let root = workspace.path().to_string_lossy().to_string();

        assert!(executor.workspace_roots_configured);
        assert!(executor.canonical_workspace_roots.is_empty());

        let error = executor
            .validate_workspace_root(&root)
            .expect_err("unavailable configured roots should fail closed");
        assert!(matches!(error, ToolError::PathNotAccessible(_)));
    }

    #[test]
    fn validate_workspace_root_rejects_unregistered_mount_root_when_roots_exist() {
        let registered = tempfile::tempdir().expect("registered");
        let workspace = tempfile::tempdir().expect("workspace");
        let executor = ToolExecutor::new(vec![registered.path().to_path_buf()]);

        let error = executor
            .validate_workspace_root(workspace.path().to_string_lossy().as_ref())
            .expect_err("unregistered mount root should be rejected");

        assert!(matches!(error, ToolError::PathNotAccessible(_)));
    }

    #[test]
    fn detect_workspace_root_only_requires_readable_directory() {
        let registered = tempfile::tempdir().expect("registered");
        let workspace = tempfile::tempdir().expect("workspace");
        let executor = ToolExecutor::new(vec![registered.path().to_path_buf()]);

        executor
            .validate_workspace_root(workspace.path().to_string_lossy().as_ref())
            .expect_err("execution boundary still rejects unregistered root");

        let detected = resolve_detect_workspace_root(workspace.path().to_string_lossy().as_ref())
            .expect("detect path should not require workspace roots registration");

        assert_eq!(
            detected,
            std::fs::canonicalize(workspace.path()).expect("canonical workspace")
        );
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
    async fn file_read_binary_returns_svg_bytes_and_mime() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("diagram.svg");
        std::fs::write(&file, "<svg></svg>").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let result = executor
            .file_read_binary("diagram.svg", &root)
            .await
            .expect("read binary");

        assert_eq!(result.data, b"<svg></svg>");
        assert_eq!(result.mime_type, "image/svg+xml");
    }

    #[tokio::test]
    async fn file_list_marks_svg_as_image_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("diagram.svg");
        std::fs::write(&file, "<svg></svg>").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let entries = executor
            .file_list(".", &root, None, false)
            .await
            .expect("list");
        let entry = entries
            .iter()
            .find(|entry| entry.path == "diagram.svg")
            .expect("svg entry");

        assert_eq!(entry.content_kind.as_deref(), Some("binary"));
        assert_eq!(entry.mime_type.as_deref(), Some("image/svg+xml"));
    }

    #[tokio::test]
    async fn file_list_default_skips_workspace_ignored_and_builtin_noise() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join(".gitignore"), "ignored/\n");
        write_file(&temp.path().join("src/main.rs"), "fn main() {}\n");
        write_file(&temp.path().join("ignored/generated.rs"), "ignored\n");
        write_file(&temp.path().join("node_modules/pkg/index.js"), "ignored\n");
        write_file(&temp.path().join("target/debug/app.d"), "ignored\n");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let entries = executor
            .file_list(".", &root, None, true)
            .await
            .expect("list");
        let paths = entry_paths(&entries);

        assert!(paths.contains(&"src/main.rs"));
        assert!(
            !paths.iter().any(|path| path.starts_with("ignored/")),
            "gitignored subtree should be skipped: {paths:?}"
        );
        assert!(
            !paths.iter().any(|path| path.starts_with("node_modules/")),
            "builtin dependency subtree should be skipped: {paths:?}"
        );
        assert!(
            !paths.iter().any(|path| path.starts_with("target/")),
            "builtin build subtree should be skipped: {paths:?}"
        );
    }

    #[tokio::test]
    async fn file_list_explicit_path_enters_ordinary_ignored_subtree() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join(".gitignore"), "ignored/\n");
        write_file(
            &temp.path().join("ignored/generated.rs"),
            "visible by explicit path\n",
        );
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let entries = executor
            .file_list("ignored", &root, None, true)
            .await
            .expect("list explicit ignored subtree");
        let paths = entry_paths(&entries);

        assert!(paths.contains(&"ignored/generated.rs"));
    }

    #[tokio::test]
    async fn file_list_keeps_vcs_metadata_hard_excluded() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join(".git/HEAD"), "ref: refs/heads/main\n");
        write_file(&temp.path().join("src/main.rs"), "fn main() {}\n");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let default_entries = executor
            .file_list(".", &root, None, true)
            .await
            .expect("default list");
        let default_paths = entry_paths(&default_entries);
        assert!(!default_paths.iter().any(|path| path.starts_with(".git/")));

        let explicit_entries = executor
            .file_list(".git", &root, None, true)
            .await
            .expect("explicit vcs list");
        assert!(explicit_entries.is_empty());
    }

    #[tokio::test]
    async fn search_default_skips_ignored_subtree_but_explicit_path_finds_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join(".gitignore"), "ignored/\n");
        write_file(&temp.path().join("src/main.rs"), "needle in source\n");
        write_file(
            &temp.path().join("ignored/generated.rs"),
            "needle in generated\n",
        );
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();

        let params = SearchParams {
            query: "needle",
            path: None,
            is_regex: false,
            include_glob: None,
            max_results: 20,
            context_lines: 0,
        };
        let (default_hits, _) = executor.search(&root, &params).await.expect("search");
        assert!(default_hits.iter().any(|hit| hit.path == "src/main.rs"));
        assert!(
            !default_hits
                .iter()
                .any(|hit| hit.path.starts_with("ignored/")),
            "default search should skip ignored subtree: {default_hits:?}"
        );

        let explicit_params = SearchParams {
            path: Some("ignored"),
            ..params
        };
        let (explicit_hits, _) = executor
            .search(&root, &explicit_params)
            .await
            .expect("explicit search");
        assert!(
            explicit_hits
                .iter()
                .any(|hit| hit.path == "ignored/generated.rs"),
            "explicit ignored subtree should be searchable: {explicit_hits:?}"
        );
    }

    #[tokio::test]
    async fn search_requires_ripgrep_when_unavailable() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join("src/main.rs"), "needle in source\n");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();
        let params = SearchParams {
            query: "needle",
            path: None,
            is_regex: false,
            include_glob: None,
            max_results: 20,
            context_lines: 0,
        };

        let error = executor
            .search_with_ripgrep_path(&root, &params, None)
            .await
            .expect_err("search should require ripgrep");
        assert!(matches!(
            error,
            ToolError::Io(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn ripgrep_policy_args_enter_explicit_ignored_subtree_without_vcs_metadata() {
        let implicit = ripgrep_policy_args(FileDiscoveryPolicy {
            intent: FileDiscoveryIntent::ImplicitWorkspaceScan,
        });
        assert!(implicit.contains(&"--hidden".to_string()));
        assert!(implicit.contains(&"--no-require-git".to_string()));
        assert!(!implicit.contains(&"--no-ignore".to_string()));
        assert!(implicit.contains(&"!**/node_modules/**".to_string()));
        assert!(implicit.contains(&"!**/.git/**".to_string()));

        let explicit = ripgrep_policy_args(FileDiscoveryPolicy {
            intent: FileDiscoveryIntent::ExplicitSubtreeScan,
        });
        assert!(explicit.contains(&"--no-ignore".to_string()));
        assert!(!explicit.contains(&"--no-require-git".to_string()));
        assert!(!explicit.contains(&"!**/node_modules/**".to_string()));
        assert!(explicit.contains(&"!**/.git/**".to_string()));
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

        assert!(matches!(error, ToolError::InvalidPath(_)));
    }

    #[test]
    fn resolve_shell_cwd_rejects_absolute_directory_inside_workspace_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let child = temp.path().join("nested");
        std::fs::create_dir_all(&child).expect("mkdir");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();
        let child_path = child.to_string_lossy().to_string();

        let error = executor
            .resolve_shell_cwd(&root, Some(&child_path))
            .expect_err("absolute cwd should be rejected even inside root");

        assert!(matches!(error, ToolError::InvalidPath(_)));
    }

    #[tokio::test]
    async fn shell_exec_handles_quoted_absolute_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("space dir");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("demo file.txt");
        std::fs::write(&file, "quoted ok").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();
        let file_path = file.to_string_lossy();
        let command = if cfg!(windows) {
            format!("Get-Content -LiteralPath '{file_path}'")
        } else {
            format!("cat \"{file_path}\"")
        };

        let result = executor
            .shell_exec(&command, &root, None, Some(10_000))
            .await
            .expect("quoted absolute path command should run");

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "quoted ok");
    }

    #[tokio::test]
    async fn shell_exec_streaming_captures_stdout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("demo.txt");
        std::fs::write(&file, "stream ok").expect("write");
        let executor = ToolExecutor::new(vec![temp.path().to_path_buf()]);
        let root = temp.path().to_string_lossy().to_string();
        let file_path = file.to_string_lossy();
        let command = if cfg!(windows) {
            format!("Get-Content -LiteralPath '{file_path}'")
        } else {
            format!("cat \"{file_path}\"")
        };
        let mut streamed = String::new();

        let result = executor
            .shell_exec_streaming(&command, &root, None, Some(10_000), |delta, stream| {
                if matches!(stream, ShellOutputStream::Stdout) {
                    streamed.push_str(delta);
                }
            })
            .await
            .expect("streaming stdout should run");

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "stream ok");
        assert_eq!(streamed.trim(), "stream ok");
    }
}
