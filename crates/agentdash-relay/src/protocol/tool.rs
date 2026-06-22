use serde::{Deserialize, Serialize};

use super::workspace::FileEntryRelay;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    /// 0-based 起始行号；省略 = 从头读。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// 行数上限；省略 = 读到 EOF。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFileWritePayload {
    pub call_id: String,
    pub path: String,
    pub content: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFileDeletePayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFileRenamePayload {
    pub call_id: String,
    pub from_path: String,
    pub to_path: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolApplyPatchPayload {
    pub call_id: String,
    pub patch: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// - 执行目录必须仍位于 `mount_root_ref` 表达的当前 workspace root 边界内
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 单次 shell_start 等待首包输出/终态的窗口；到期后进程继续由本机 runtime 持有。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yield_time_ms: Option<u64>,
    /// retained output buffer 的每 session 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<usize>,
    /// 使用 PTY 执行；省略或 false 时使用 stdout/stderr pipe。
    #[serde(default, skip_serializing_if = "is_false")]
    pub tty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolShellReadPayload {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolShellInputPayload {
    pub session_id: String,
    /// 空字符串表示 poll/read wait，不向 stdin 写入字节。
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolShellTerminatePayload {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFileListPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSearchPayload {
    pub call_id: String,
    pub mount_root_ref: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub is_regex: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_glob: Option<String>,
    pub max_results: usize,
    pub context_lines: usize,
    /// `false` ⇒ smart-case；`true` ⇒ 严格大小写。
    pub case_sensitive: bool,
    /// `true` ⇒ pattern `.` 跨行 + `^/$` 匹配每行（ripgrep `--multiline
    /// --multiline-dotall`）。
    pub multiline: bool,
    /// `-B` 等价；与 `context_lines` 同时设置时取 max。
    pub before_lines: usize,
    /// `-A` 等价。
    pub after_lines: usize,
}

fn default_utf8() -> String {
    "utf-8".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolShellSessionState {
    Starting,
    Running,
    Completed,
    Failed,
    TimedOut,
    Killed,
    Lost,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolShellTerminateStatus {
    Killed,
    AlreadyExited,
    UnknownSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolShellTruncationInfo {
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub omitted_bytes: usize,
    #[serde(default)]
    pub omitted_chunks: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_tokens_estimate: Option<usize>,
}

impl ToolShellTruncationInfo {
    pub fn is_empty(&self) -> bool {
        !self.truncated
            && self.omitted_bytes == 0
            && self.omitted_chunks == 0
            && self.omitted_tokens_estimate.is_none()
    }

    pub fn merge(&self, other: &Self) -> Self {
        let omitted_bytes = self.omitted_bytes.saturating_add(other.omitted_bytes);
        let omitted_chunks = self.omitted_chunks.saturating_add(other.omitted_chunks);
        let omitted_tokens_estimate =
            match (self.omitted_tokens_estimate, other.omitted_tokens_estimate) {
                (Some(left), Some(right)) => Some(left.saturating_add(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            };
        Self {
            truncated: self.truncated || other.truncated,
            omitted_bytes,
            omitted_chunks,
            omitted_tokens_estimate,
        }
    }
}

pub const LIVE_OUTPUT_EVENT_MAX_BYTES: usize = 64 * 1024;

pub fn truncate_live_output_text(
    text: &str,
    max_bytes: usize,
) -> (String, ToolShellTruncationInfo) {
    let max_bytes = max_bytes.max(1);
    if text.len() <= max_bytes {
        return (text.to_string(), ToolShellTruncationInfo::default());
    }

    let end = text
        .char_indices()
        .map(|(idx, ch)| idx + ch.len_utf8())
        .take_while(|end| *end <= max_bytes)
        .last()
        .unwrap_or(0);
    let bounded = text[..end].to_string();
    let omitted_bytes = text.len().saturating_sub(bounded.len());
    (
        bounded,
        ToolShellTruncationInfo {
            truncated: true,
            omitted_bytes,
            omitted_chunks: 1,
            omitted_tokens_estimate: None,
        },
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellOutputChunk {
    pub seq: u64,
    pub stream: ShellOutputStream,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellExecResponse {
    pub call_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    pub state: ToolShellSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub pty: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<ToolShellOutputChunk>,
    pub next_seq: u64,
    #[serde(default)]
    pub truncation: ToolShellTruncationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellReadResponse {
    pub session_id: String,
    pub state: ToolShellSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub chunks: Vec<ToolShellOutputChunk>,
    pub next_seq: u64,
    #[serde(default)]
    pub truncation: ToolShellTruncationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellInputResponse {
    pub session_id: String,
    pub accepted: bool,
    pub stdin_closed: bool,
    pub read: ToolShellReadResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellTerminateResponse {
    pub session_id: String,
    pub status: ToolShellTerminateStatus,
    pub state: ToolShellSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
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
    #[serde(default, skip_serializing_if = "ToolShellTruncationInfo::is_empty")]
    pub truncation: ToolShellTruncationInfo,
}

impl ToolShellOutputPayload {
    pub fn bounded(mut self, max_bytes: usize) -> Self {
        let (delta, truncation) = truncate_live_output_text(&self.delta, max_bytes);
        self.delta = delta;
        self.truncation = self.truncation.merge(&truncation);
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShellOutputStream {
    Stdout,
    Stderr,
    Pty,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_file_read_payload_offset_limit_round_trip() {
        let payload = ToolFileReadPayload {
            call_id: "c1".to_string(),
            path: "src/main.rs".to_string(),
            mount_root_ref: "main".to_string(),
            offset: Some(10),
            limit: Some(50),
        };
        let json_value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(json_value["offset"], 10);
        assert_eq!(json_value["limit"], 50);
        let decoded: ToolFileReadPayload = serde_json::from_value(json_value).expect("deserialize");
        assert_eq!(decoded.offset, Some(10));
        assert_eq!(decoded.limit, Some(50));
    }

    #[test]
    fn tool_file_read_payload_without_range_uses_full_file() {
        let value = json!({
            "call_id": "c1",
            "path": "src/main.rs",
            "mount_root_ref": "main"
        });
        let decoded: ToolFileReadPayload = serde_json::from_value(value).expect("deserialize");
        assert!(decoded.offset.is_none());
        assert!(decoded.limit.is_none());
    }

    #[test]
    fn tool_file_read_payload_rejects_legacy_workspace_root() {
        let legacy = json!({
            "call_id": "c1",
            "path": "src/main.rs",
            "workspace_root": "/workspace"
        });
        let error = serde_json::from_value::<ToolFileReadPayload>(legacy)
            .expect_err("workspace_root is not part of the current payload");
        assert!(error.to_string().contains("workspace_root"));
    }

    #[test]
    fn tool_file_read_payload_requires_mount_root_ref() {
        let missing_mount = json!({
            "call_id": "c1",
            "path": "src/main.rs"
        });
        let error = serde_json::from_value::<ToolFileReadPayload>(missing_mount)
            .expect_err("mount_root_ref is required");
        assert!(error.to_string().contains("mount_root_ref"));
    }

    #[test]
    fn tool_file_read_payload_omits_offset_limit_when_none() {
        let payload = ToolFileReadPayload {
            call_id: "c1".to_string(),
            path: "x".to_string(),
            mount_root_ref: "m".to_string(),
            offset: None,
            limit: None,
        };
        let s = serde_json::to_string(&payload).expect("serialize");
        // skip_serializing_if = Option::is_none ⇒ JSON 不应含这两个 key。
        assert!(!s.contains("\"offset\""));
        assert!(!s.contains("\"limit\""));
    }

    #[test]
    fn shell_output_payload_defaults_missing_truncation() {
        let payload: ToolShellOutputPayload = serde_json::from_value(json!({
            "call_id": "call-1",
            "delta": "ok\n",
            "stream": "stdout"
        }))
        .expect("payload should deserialize");

        assert!(!payload.truncation.truncated);
        assert_eq!(payload.truncation.omitted_bytes, 0);
    }

    #[test]
    fn live_output_payload_bounded_is_utf8_safe() {
        let payload = ToolShellOutputPayload {
            call_id: "call-1".to_string(),
            delta: "好".repeat(10),
            stream: ShellOutputStream::Stdout,
            truncation: ToolShellTruncationInfo::default(),
        }
        .bounded(7);

        assert!(payload.delta.is_char_boundary(payload.delta.len()));
        assert!(payload.delta.len() <= 7);
        assert!(payload.truncation.truncated);
        assert!(payload.truncation.omitted_bytes > 0);
    }

    #[test]
    fn tool_search_payload_grep_fields_round_trip() {
        let payload = ToolSearchPayload {
            call_id: "c2".to_string(),
            mount_root_ref: "main".to_string(),
            query: "needle".to_string(),
            path: Some("src".to_string()),
            is_regex: true,
            include_glob: Some("*.rs".to_string()),
            max_results: 100,
            context_lines: 1,
            case_sensitive: false,
            multiline: true,
            before_lines: 2,
            after_lines: 3,
        };
        let json_value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(json_value["case_sensitive"], false);
        assert_eq!(json_value["multiline"], true);
        assert_eq!(json_value["before_lines"], 2);
        assert_eq!(json_value["after_lines"], 3);
        let decoded: ToolSearchPayload = serde_json::from_value(json_value).expect("deserialize");
        assert!(!decoded.case_sensitive);
        assert!(decoded.multiline);
        assert_eq!(decoded.before_lines, 2);
        assert_eq!(decoded.after_lines, 3);
    }

    #[test]
    fn tool_search_payload_rejects_missing_current_options() {
        let missing_options = json!({
            "call_id": "c1",
            "mount_root_ref": "m",
            "query": "x",
            "max_results": 50
        });
        let error = serde_json::from_value::<ToolSearchPayload>(missing_options)
            .expect_err("search options are required in current payload");
        assert!(error.to_string().contains("is_regex"));
    }

    #[test]
    fn tool_search_payload_rejects_unknown_legacy_workspace_root() {
        let legacy = json!({
            "call_id": "c1",
            "mount_root_ref": "m",
            "workspace_root": "/workspace",
            "query": "x",
            "is_regex": false,
            "max_results": 50,
            "context_lines": 0,
            "case_sensitive": true,
            "multiline": false,
            "before_lines": 0,
            "after_lines": 0
        });
        let error = serde_json::from_value::<ToolSearchPayload>(legacy)
            .expect_err("unknown workspace_root should be rejected");
        assert!(error.to_string().contains("workspace_root"));
    }
}
