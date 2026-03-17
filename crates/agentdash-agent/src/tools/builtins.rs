use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;
use walkdir::WalkDir;

use crate::tools::registry::ToolRegistry;
use crate::tools::schema::schema_value;
use crate::tools::support::{
    resolve_existing_path, resolve_path_for_write, truncate_chars, workspace_display,
};
use tokio_util::sync::CancellationToken;

use crate::types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
};

/// 应从 WalkDir 遍历中排除的目录名
const WALK_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".next",
    ".nuxt",
    "dist",
    "build",
    ".tox",
    ".venv",
    "venv",
];

fn ok_text(text: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

fn canonical_workspace_root(workspace_root: PathBuf) -> PathBuf {
    workspace_root.canonicalize().unwrap_or(workspace_root)
}

fn err_text(text: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: true,
        details: None,
    }
}

/// 判断一个 WalkDir entry 是否应该被跳过（通过目录名）
fn should_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_dir() {
        if let Some(name) = entry.file_name().to_str() {
            return WALK_SKIP_DIRS.contains(&name);
        }
    }
    false
}

// ─── ReadFileTool ───────────────────────────────────────────

#[derive(Clone)]
pub struct ReadFileTool {
    workspace_root: PathBuf,
}

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root: canonical_workspace_root(workspace_root),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileParams {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[async_trait]
impl AgentTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "读取工作空间内文件内容，支持按行范围读取"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ReadFileParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ReadFileParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let path = resolve_existing_path(&self.workspace_root, &params.path)
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("读取文件失败: {e}")))?;
        let lines = content.lines().collect::<Vec<_>>();
        let start = params.start_line.unwrap_or(1).max(1);
        let end = params.end_line.unwrap_or(lines.len()).max(start);
        let mut selected = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let line_no = index + 1;
            if line_no >= start && line_no <= end {
                selected.push(format!("{:>4} | {}", line_no, line));
            }
        }
        Ok(ok_text(format!(
            "文件: {}\n{}",
            workspace_display(&self.workspace_root, &path),
            selected.join("\n")
        )))
    }
}

// ─── WriteFileTool ──────────────────────────────────────────

#[derive(Clone)]
pub struct WriteFileTool {
    workspace_root: PathBuf,
}

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root: canonical_workspace_root(workspace_root),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteFileParams {
    pub path: String,
    pub content: String,
    pub append: Option<bool>,
}

#[async_trait]
impl AgentTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "向工作空间内文件写入内容，可覆盖或追加"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WriteFileParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WriteFileParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let path = resolve_path_for_write(&self.workspace_root, &params.path)
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        if params.append.unwrap_or(false) {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("打开文件失败: {e}")))?;
            file.write_all(params.content.as_bytes())
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("追加文件失败: {e}")))?;
        } else {
            tokio::fs::write(&path, params.content.as_bytes())
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("写入文件失败: {e}")))?;
        }
        Ok(ok_text(format!(
            "已写入文件: {}",
            workspace_display(&self.workspace_root, &path)
        )))
    }
}

// ─── ListDirectoryTool ──────────────────────────────────────

#[derive(Clone)]
pub struct ListDirectoryTool {
    workspace_root: PathBuf,
}

impl ListDirectoryTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root: canonical_workspace_root(workspace_root),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirectoryParams {
    pub path: Option<String>,
    pub recursive: Option<bool>,
    pub max_depth: Option<usize>,
}

#[async_trait]
impl AgentTool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }
    fn description(&self) -> &str {
        "列出工作空间目录内容，支持递归和深度限制"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ListDirectoryParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ListDirectoryParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let requested = params.path.as_deref().unwrap_or(".");
        let path = resolve_existing_path(&self.workspace_root, requested)
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        if !path.is_dir() {
            return Ok(err_text(format!(
                "目标不是目录: {}",
                workspace_display(&self.workspace_root, &path)
            )));
        }
        let recursive = params.recursive.unwrap_or(false);
        let max_depth = if recursive {
            params.max_depth.unwrap_or(3)
        } else {
            1
        };

        // WalkDir 是同步阻塞操作，放入 spawn_blocking 避免阻塞 tokio runtime
        let ws_root = self.workspace_root.clone();
        let walk_path = path.clone();
        let entries = tokio::task::spawn_blocking(move || {
            let mut entries = Vec::new();
            for entry in WalkDir::new(&walk_path)
                .max_depth(max_depth)
                .into_iter()
                .filter_entry(|e| !should_skip_dir(e))
                .filter_map(Result::ok)
                .skip(1)
            {
                let entry_path = entry.path();
                let rel = workspace_display(&ws_root, entry_path);
                let kind = if entry.file_type().is_dir() {
                    "dir"
                } else {
                    "file"
                };
                entries.push(format!("[{kind}] {rel}"));
            }
            entries
        })
        .await
        .map_err(|e| AgentToolError::ExecutionFailed(format!("目录遍历失败: {e}")))?;

        if cancel.is_cancelled() {
            return Ok(err_text("操作已取消"));
        }

        Ok(ok_text(format!(
            "目录: {}\n{}",
            workspace_display(&self.workspace_root, &path),
            if entries.is_empty() {
                "(空目录)".to_string()
            } else {
                entries.join("\n")
            }
        )))
    }
}

// ─── ShellTool ──────────────────────────────────────────────

#[derive(Clone)]
pub struct ShellTool {
    workspace_root: PathBuf,
    default_timeout: Duration,
    max_output_chars: usize,
}

impl ShellTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root: canonical_workspace_root(workspace_root),
            default_timeout: Duration::from_secs(30),
            max_output_chars: 50_000,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellParams {
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[async_trait]
impl AgentTool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "在工作空间内执行 shell 命令，带超时与输出截断"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ShellParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let cwd = match params.cwd.as_deref() {
            Some(path) => resolve_existing_path(&self.workspace_root, path)
                .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?,
            None => self.workspace_root.clone(),
        };
        let timeout = Duration::from_secs(
            params
                .timeout_secs
                .unwrap_or(self.default_timeout.as_secs())
                .max(1),
        );

        let mut command = if cfg!(windows) {
            let mut cmd = Command::new("powershell.exe");
            cmd.arg("-Command").arg(&params.command);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.arg("-lc").arg(&params.command);
            cmd
        };
        command
            .current_dir(&cwd)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // 同时等待命令完成、超时、和取消 — 三者谁先到谁赢
        let output = tokio::select! {
            result = tokio::time::timeout(timeout, command.output()) => {
                match result {
                    Ok(Ok(output)) => output,
                    Ok(Err(e)) => return Err(AgentToolError::ExecutionFailed(format!("执行命令失败: {e}"))),
                    Err(_) => return Ok(err_text(format!(
                        "命令执行超时（>{}s）: {}",
                        timeout.as_secs(),
                        params.command
                    ))),
                }
            }
            _ = cancel.cancelled() => {
                return Ok(err_text("命令执行已取消"));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let merged = if stderr.trim().is_empty() {
            stdout
        } else if stdout.trim().is_empty() {
            format!("[stderr]\n{stderr}")
        } else {
            format!("[stdout]\n{stdout}\n\n[stderr]\n{stderr}")
        };
        let (body, truncated) = truncate_chars(&merged, self.max_output_chars);
        let message = format!(
            "命令: {}\n工作目录: {}\n退出码: {}{}\n{}",
            params.command,
            workspace_display(&self.workspace_root, &cwd),
            output.status.code().unwrap_or(-1),
            if truncated {
                "（输出已截断）"
            } else {
                ""
            },
            body
        );

        Ok(AgentToolResult {
            content: vec![ContentPart::text(message)],
            is_error: !output.status.success(),
            details: None,
        })
    }
}

// ─── SearchTool ─────────────────────────────────────────────

#[derive(Clone)]
pub struct SearchTool {
    workspace_root: PathBuf,
    max_output_chars: usize,
}

impl SearchTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root: canonical_workspace_root(workspace_root),
            max_output_chars: 20_000,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    pub pattern: String,
    pub path: Option<String>,
    pub include: Option<String>,
    pub exclude: Option<Vec<String>>,
    pub max_results: Option<usize>,
    pub regex: Option<bool>,
}

#[async_trait]
impl AgentTool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "在工作空间内搜索文本或正则，返回命中文件和行号"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<SearchParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: SearchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let root =
            resolve_existing_path(&self.workspace_root, params.path.as_deref().unwrap_or("."))
                .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        let matcher = build_glob_matcher(params.include.as_deref(), params.exclude.as_deref())
            .map_err(|e| AgentToolError::InvalidArguments(format!("glob 解析失败: {e}")))?;
        let regex = if params.regex.unwrap_or(false) {
            Some(
                Regex::new(&params.pattern)
                    .map_err(|e| AgentToolError::InvalidArguments(format!("正则无效: {e}")))?,
            )
        } else {
            None
        };
        let max_results = params.max_results.unwrap_or(50).max(1);

        // Phase 1: 用 spawn_blocking 收集候选文件路径，避免 WalkDir 阻塞 runtime
        let ws_root = self.workspace_root.clone();
        let search_root = root.clone();
        let file_paths: Vec<PathBuf> = tokio::task::spawn_blocking(move || {
            WalkDir::new(&search_root)
                .into_iter()
                .filter_entry(|e| !should_skip_dir(e))
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .filter(|e| matcher.is_match(&ws_root, e.path()))
                .map(|e| e.into_path())
                .collect()
        })
        .await
        .map_err(|e| AgentToolError::ExecutionFailed(format!("文件遍历失败: {e}")))?;

        // Phase 2: 逐文件异步读取+匹配，每个文件后检查取消
        let mut hits = Vec::new();
        for file_path in &file_paths {
            if cancel.is_cancelled() {
                return Ok(err_text("搜索已取消"));
            }

            let content = match tokio::fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (index, line) in content.lines().enumerate() {
                let matched = match &regex {
                    Some(compiled) => compiled.is_match(line),
                    None => line.contains(&params.pattern),
                };
                if matched {
                    hits.push(format!(
                        "{}:{}: {}",
                        workspace_display(&self.workspace_root, file_path),
                        index + 1,
                        line.trim()
                    ));
                    if hits.len() >= max_results {
                        break;
                    }
                }
            }

            if hits.len() >= max_results {
                break;
            }
        }

        let body = if hits.is_empty() {
            "未找到匹配结果".to_string()
        } else {
            hits.join("\n")
        };
        let (body, truncated) = truncate_chars(&body, self.max_output_chars);
        Ok(ok_text(format!(
            "搜索根目录: {}\n模式: {}{}\n{}",
            workspace_display(&self.workspace_root, &root),
            params.pattern,
            if truncated {
                "（输出已截断）"
            } else {
                ""
            },
            body
        )))
    }
}

// ─── GlobMatcher ────────────────────────────────────────────

struct GlobMatcher {
    include: GlobSet,
    exclude: Option<GlobSet>,
}

impl GlobMatcher {
    fn is_match(&self, workspace_root: &Path, path: &Path) -> bool {
        let rel = workspace_display(workspace_root, path);
        self.include.is_match(&rel)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|exclude| exclude.is_match(&rel))
    }
}

fn build_glob_matcher(
    include: Option<&str>,
    exclude: Option<&[String]>,
) -> Result<GlobMatcher, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    if let Some(pattern) = include {
        builder.add(Glob::new(pattern)?);
    } else {
        builder.add(Glob::new("**")?);
    }
    let include = builder.build()?;

    let exclude = if let Some(items) = exclude {
        let mut exclude_builder = GlobSetBuilder::new();
        for item in items {
            exclude_builder.add(Glob::new(item)?);
        }
        Some(exclude_builder.build()?)
    } else {
        None
    };

    Ok(GlobMatcher { include, exclude })
}

// ─── BuiltinToolset ─────────────────────────────────────────

pub struct BuiltinToolset {
    tools: Vec<DynAgentTool>,
}

impl BuiltinToolset {
    pub fn for_workspace(workspace_root: PathBuf) -> Self {
        let mut registry = ToolRegistry::new();
        registry.register(ReadFileTool::new(workspace_root.clone()));
        registry.register(WriteFileTool::new(workspace_root.clone()));
        registry.register(ListDirectoryTool::new(workspace_root.clone()));
        registry.register(ShellTool::new(workspace_root.clone()));
        registry.register(SearchTool::new(workspace_root));
        Self {
            tools: registry.all(),
        }
    }

    pub fn into_tools(self) -> Vec<DynAgentTool> {
        self.tools
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn read_write_file_tool_roundtrip() {
        let cancel = CancellationToken::new();
        let dir = tempdir().unwrap();
        let writer = WriteFileTool::new(dir.path().to_path_buf());
        writer
            .execute(
                "tc1",
                serde_json::json!({"path": "src/demo.txt", "content": "hello\nworld"}),
                cancel.clone(),
                None,
            )
            .await
            .unwrap();

        let reader = ReadFileTool::new(dir.path().to_path_buf());
        let result = reader
            .execute(
                "tc2",
                serde_json::json!({"path": "src/demo.txt", "start_line": 2, "end_line": 2}),
                cancel,
                None,
            )
            .await
            .unwrap();
        assert!(
            result.content[0]
                .extract_text()
                .unwrap()
                .contains("2 | world")
        );
    }

    #[tokio::test]
    async fn search_tool_finds_matches() {
        let cancel = CancellationToken::new();
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("main.rs"),
            "fn main() { println!(\"hello\"); }",
        )
        .await
        .unwrap();

        let tool = SearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(
                "tc3",
                serde_json::json!({"pattern": "println!", "include": "*.rs"}),
                cancel,
                None,
            )
            .await
            .unwrap();
        assert!(
            result.content[0]
                .extract_text()
                .unwrap()
                .contains("main.rs:1")
        );
    }

    #[tokio::test]
    async fn shell_tool_respects_workspace() {
        let cancel = CancellationToken::new();
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("marker.txt"), "ok")
            .await
            .unwrap();
        let tool = ShellTool::new(dir.path().to_path_buf());
        let command = if cfg!(windows) {
            "Get-Content marker.txt"
        } else {
            "cat marker.txt"
        };
        let result = tool
            .execute("tc4", serde_json::json!({"command": command}), cancel, None)
            .await
            .unwrap();
        assert!(result.content[0].extract_text().unwrap().contains("ok"));
    }
}
