use serde::{Deserialize, Serialize};

use super::workspace::FileEntryRelay;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileWritePayload {
    pub call_id: String,
    pub path: String,
    pub content: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileDeletePayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileRenamePayload {
    pub call_id: String,
    pub from_path: String,
    pub to_path: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApplyPatchPayload {
    pub call_id: String,
    pub patch: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellExecPayload {
    pub call_id: String,
    pub command: String,
    /// shell 允许访问的工作区根目录边界。
    /// 若未提供 `cwd`，执行器默认在该目录下启动命令。
    pub mount_root_ref: String,
    /// 可选执行目录。
    /// 当前约定：
    /// - 允许为空，此时回退到 `mount_root_ref`
    /// - 相对路径相对于 `mount_root_ref` 解析
    /// - 绝对路径必须仍位于 `mount_root_ref` / accessible_roots 边界内
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileListPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchPayload {
    pub call_id: String,
    pub mount_root_ref: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_glob: Option<String>,
    #[serde(default = "default_search_max_results")]
    pub max_results: usize,
    #[serde(default)]
    pub context_lines: usize,
}

fn default_search_max_results() -> usize {
    50
}
fn default_utf8() -> String {
    "utf-8".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadResponse {
    pub call_id: String,
    pub content: String,
    #[serde(default = "default_utf8")]
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadBinaryResponse {
    pub call_id: String,
    pub data_base64: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileWriteResponse {
    pub call_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileDeleteResponse {
    pub call_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileRenameResponse {
    pub call_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApplyPatchResponse {
    pub call_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellExecResponse {
    pub call_id: String,
    pub exit_code: i32,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileListResponse {
    pub call_id: String,
    pub entries: Vec<FileEntryRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchResponse {
    pub call_id: String,
    pub hits: Vec<SearchHit>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub line_number: usize,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<String>,
}

// ─── 串行 Shell 流式输出 ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellOutputPayload {
    pub call_id: String,
    pub delta: String,
    pub stream: ShellOutputStream,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShellOutputStream {
    Stdout,
    Stderr,
}
