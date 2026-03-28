//! 云端↔本机 WebSocket 中继协议消息类型
//!
//! 所有消息遵循统一信封格式，通过 `type` 字段区分消息种类。
//! 详见 docs/relay-protocol.md

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::RelayError;

// ─── 消息信封 ───────────────────────────────────────────────

/// 中继协议消息（顶层信封）
///
/// 通过 `type` 字段自动路由到具体变体。
/// 云端和本机共享同一枚举，按发送方向区分使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RelayMessage {
    // ── 注册 ──
    /// 本机 → 云端：连接后的第一条消息
    #[serde(rename = "register")]
    Register {
        id: String,
        payload: RegisterPayload,
    },

    /// 云端 → 本机：注册确认
    #[serde(rename = "register_ack")]
    RegisterAck {
        id: String,
        payload: RegisterAckPayload,
    },

    // ── 心跳 ──
    /// 云端 → 本机
    #[serde(rename = "ping")]
    Ping { id: String, payload: PingPayload },

    /// 本机 → 云端
    #[serde(rename = "pong")]
    Pong { id: String, payload: PongPayload },

    // ── 命令（云端 → 本机）──
    /// 执行第三方 Agent prompt
    #[serde(rename = "command.prompt")]
    CommandPrompt {
        id: String,
        payload: Box<CommandPromptPayload>,
    },

    /// 取消执行
    #[serde(rename = "command.cancel")]
    CommandCancel {
        id: String,
        payload: CommandCancelPayload,
    },

    /// 查询本机第三方能力
    #[serde(rename = "command.discover")]
    CommandDiscover { id: String, payload: EmptyPayload },

    /// 查询执行器选项（流式）
    #[serde(rename = "command.discover_options")]
    CommandDiscoverOptions {
        id: String,
        payload: CommandDiscoverOptionsPayload,
    },

    /// 检测 Git 信息
    #[serde(rename = "command.workspace_detect_git")]
    CommandWorkspaceDetectGit {
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    },

    /// 浏览本地目录（盘符列表 / 子目录列表）
    #[serde(rename = "command.browse_directory")]
    CommandBrowseDirectory {
        id: String,
        payload: CommandBrowseDirectoryPayload,
    },

    // ── PiAgent Tool Call 命令（云端 → 本机）──
    /// 读取文件
    #[serde(rename = "command.tool.file_read")]
    CommandToolFileRead {
        id: String,
        payload: ToolFileReadPayload,
    },

    /// 写入文件
    #[serde(rename = "command.tool.file_write")]
    CommandToolFileWrite {
        id: String,
        payload: ToolFileWritePayload,
    },

    /// 执行 Shell 命令
    #[serde(rename = "command.tool.shell_exec")]
    CommandToolShellExec {
        id: String,
        payload: ToolShellExecPayload,
    },

    /// 列出目录内容
    #[serde(rename = "command.tool.file_list")]
    CommandToolFileList {
        id: String,
        payload: ToolFileListPayload,
    },

    /// 文本内容搜索（优先使用 ripgrep）
    #[serde(rename = "command.tool.search")]
    CommandToolSearch {
        id: String,
        payload: ToolSearchPayload,
    },

    // ── 响应（本机 → 云端）──
    #[serde(rename = "response.prompt")]
    ResponsePrompt {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponsePromptPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.cancel")]
    ResponseCancel {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseCancelPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.discover")]
    ResponseDiscover {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseDiscoverPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.workspace_detect_git")]
    ResponseWorkspaceDetectGit {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseWorkspaceDetectGitPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.browse_directory")]
    ResponseBrowseDirectory {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseBrowseDirectoryPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    // ── PiAgent Tool Call 响应 ──
    #[serde(rename = "response.tool.file_read")]
    ResponseToolFileRead {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileReadResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.file_write")]
    ResponseToolFileWrite {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileWriteResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.shell_exec")]
    ResponseToolShellExec {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolShellExecResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.file_list")]
    ResponseToolFileList {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileListResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.search")]
    ResponseToolSearch {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolSearchResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    // ── 事件（本机 → 云端）──
    /// 能力变更通知
    #[serde(rename = "event.capabilities_changed")]
    EventCapabilitiesChanged {
        id: String,
        payload: CapabilitiesPayload,
    },

    /// ACP 会话通知（最高频消息）
    #[serde(rename = "event.session_notification")]
    EventSessionNotification {
        id: String,
        payload: SessionNotificationPayload,
    },

    /// 执行状态变更
    #[serde(rename = "event.session_state_changed")]
    EventSessionStateChanged {
        id: String,
        payload: SessionStateChangedPayload,
    },

    /// 执行器选项发现 patch（流式）
    #[serde(rename = "event.discover_options_patch")]
    EventDiscoverOptionsPatch {
        id: String,
        payload: DiscoverOptionsPatchPayload,
    },

    // ── 通用错误 ──
    #[serde(rename = "error")]
    Error { id: String, error: RelayError },
}

impl RelayMessage {
    /// 获取消息 ID（所有变体都有）
    pub fn id(&self) -> &str {
        match self {
            Self::Register { id, .. }
            | Self::RegisterAck { id, .. }
            | Self::Ping { id, .. }
            | Self::Pong { id, .. }
            | Self::CommandPrompt { id, .. }
            | Self::CommandCancel { id, .. }
            | Self::CommandDiscover { id, .. }
            | Self::CommandDiscoverOptions { id, .. }
            | Self::CommandWorkspaceDetectGit { id, .. }
            | Self::CommandBrowseDirectory { id, .. }
            | Self::CommandToolFileRead { id, .. }
            | Self::CommandToolFileWrite { id, .. }
            | Self::CommandToolShellExec { id, .. }
            | Self::CommandToolFileList { id, .. }
            | Self::CommandToolSearch { id, .. }
            | Self::ResponsePrompt { id, .. }
            | Self::ResponseCancel { id, .. }
            | Self::ResponseDiscover { id, .. }
            | Self::ResponseWorkspaceDetectGit { id, .. }
            | Self::ResponseBrowseDirectory { id, .. }
            | Self::ResponseToolFileRead { id, .. }
            | Self::ResponseToolFileWrite { id, .. }
            | Self::ResponseToolShellExec { id, .. }
            | Self::ResponseToolFileList { id, .. }
            | Self::ResponseToolSearch { id, .. }
            | Self::EventCapabilitiesChanged { id, .. }
            | Self::EventSessionNotification { id, .. }
            | Self::EventSessionStateChanged { id, .. }
            | Self::EventDiscoverOptionsPatch { id, .. }
            | Self::Error { id, .. } => id,
        }
    }

    /// 生成唯一消息 ID
    pub fn new_id(prefix: &str) -> String {
        let ts = chrono::Utc::now().timestamp_millis();
        let rand: u32 = rand_u32();
        format!("{prefix}-{ts}-{rand:08x}")
    }
}

fn rand_u32() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish() as u32
}

// ─── Payload 定义 ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyPayload {}

// ── 注册 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPayload {
    pub backend_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: CapabilitiesPayload,
    pub accessible_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAckPayload {
    pub backend_id: String,
    pub status: String,
    pub server_time: i64,
}

// ── 心跳 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingPayload {
    pub server_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongPayload {
    pub client_time: i64,
}

// ── 能力 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesPayload {
    pub executors: Vec<ExecutorInfoRelay>,
    #[serde(default)]
    pub supports_cancel: bool,
    #[serde(default)]
    pub supports_discover_options: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorInfoRelay {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default = "default_true")]
    pub available: bool,
}

fn default_true() -> bool {
    true
}

// ── command.prompt ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPromptPayload {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_blocks: Option<serde_json::Value>,
    pub workspace_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<ExecutorConfigRelay>,
    #[serde(default)]
    pub mcp_servers: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfigRelay {
    pub executor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
}

// ── command.cancel ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCancelPayload {
    pub session_id: String,
}

// ── command.discover_options ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDiscoverOptionsPayload {
    pub executor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandWorkspaceDetectGitPayload {
    /// 待检测的 workspace 根目录。
    /// 本机必须先校验它位于 accessible_roots 内，不能把它当任意文件系统路径使用。
    pub path: String,
}

// ── command.browse_directory ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandBrowseDirectoryPayload {
    /// 要浏览的路径。为空或 None 时返回盘符列表（Windows）或根目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

// ── PiAgent tool call payloads ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileWritePayload {
    pub call_id: String,
    pub path: String,
    pub content: String,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolShellExecPayload {
    pub call_id: String,
    pub command: String,
    /// shell 允许访问的工作区根目录边界。
    /// 若未提供 `cwd`，执行器默认在该目录下启动命令。
    pub workspace_root: String,
    /// 可选执行目录。
    /// 当前约定：
    /// - 允许为空，此时回退到 `workspace_root`
    /// - 相对路径相对于 `workspace_root` 解析
    /// - 绝对路径必须仍位于 `workspace_root` / accessible_roots 边界内
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileListPayload {
    pub call_id: String,
    pub path: String,
    pub workspace_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchPayload {
    pub call_id: String,
    pub workspace_root: String,
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

// ─── 响应 Payload ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePromptPayload {
    pub turn_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCancelPayload {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseDiscoverPayload {
    pub executors: Vec<ExecutorInfoRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntryRelay {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    #[serde(default)]
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWorkspaceDetectGitPayload {
    pub is_git: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

// ── browse_directory 响应 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseBrowseDirectoryPayload {
    /// 当前浏览的绝对路径（若为根则为空字符串）
    pub current_path: String,
    pub entries: Vec<BrowseDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseDirectoryEntry {
    pub name: String,
    /// 完整绝对路径
    pub path: String,
    pub is_dir: bool,
}

// ── PiAgent tool call 响应 ──

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
pub struct ToolFileWriteResponse {
    pub call_id: String,
    pub status: String,
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

// ─── 事件 Payload ──────────────────────────────────────────

/// ACP SessionNotification 透传（不解析内部结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionNotificationPayload {
    pub session_id: String,
    /// 完整的 ACP SessionNotification JSON，云端透传不解析
    pub notification: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateChangedPayload {
    pub session_id: String,
    pub turn_id: String,
    /// started | completed | failed | cancelled
    pub state: SessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Started,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverOptionsPatchPayload {
    pub request_id: String,
    pub patch: serde_json::Value,
    #[serde(default)]
    pub done: bool,
}
