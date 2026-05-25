use serde::{Deserialize, Serialize};

use super::workspace::FileEntryRelay;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    /// 0-based 起始行号；省略 = 从头读。
    /// 远端 backend 未识别此字段时按"读全文"回退（兼容旧实现）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// 行数上限；省略 = 读到 EOF。
    /// 远端 backend 未识别此字段时按"读全文"回退。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
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
    /// `false` ⇒ smart-case；`true` ⇒ 严格大小写。默认 true（与历史一致）。
    /// 旧远端不识别此字段时反序列化为 default = true，行为不变。
    #[serde(default = "default_case_sensitive")]
    pub case_sensitive: bool,
    /// `true` ⇒ pattern `.` 跨行 + `^/$` 匹配每行（ripgrep `--multiline
    /// --multiline-dotall`）。
    #[serde(default)]
    pub multiline: bool,
    /// `-B` 等价；与 `context_lines` 同时设置时取 max。
    #[serde(default)]
    pub before_lines: usize,
    /// `-A` 等价。
    #[serde(default)]
    pub after_lines: usize,
}

fn default_search_max_results() -> usize {
    50
}

fn default_case_sensitive() -> bool {
    true
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
        let decoded: ToolFileReadPayload =
            serde_json::from_value(json_value).expect("deserialize");
        assert_eq!(decoded.offset, Some(10));
        assert_eq!(decoded.limit, Some(50));
    }

    #[test]
    fn tool_file_read_payload_legacy_json_without_offset_limit() {
        // 旧 JSON（没有 offset/limit 字段）反序列化后两字段为 None。
        let legacy = json!({
            "call_id": "c1",
            "path": "src/main.rs",
            "mount_root_ref": "main"
        });
        let decoded: ToolFileReadPayload = serde_json::from_value(legacy).expect("deserialize");
        assert!(decoded.offset.is_none());
        assert!(decoded.limit.is_none());
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
        assert_eq!(decoded.case_sensitive, false);
        assert!(decoded.multiline);
        assert_eq!(decoded.before_lines, 2);
        assert_eq!(decoded.after_lines, 3);
    }

    #[test]
    fn tool_search_payload_legacy_json_uses_defaults() {
        // 旧 JSON 缺 grep 新字段 ⇒ default 值（case_sensitive=true，其余=0/false）。
        let legacy = json!({
            "call_id": "c1",
            "mount_root_ref": "m",
            "query": "x",
            "max_results": 50
        });
        let decoded: ToolSearchPayload = serde_json::from_value(legacy).expect("deserialize");
        assert!(decoded.case_sensitive);
        assert!(!decoded.multiline);
        assert_eq!(decoded.before_lines, 0);
        assert_eq!(decoded.after_lines, 0);
        assert_eq!(decoded.context_lines, 0);
    }
}
