//! 云端↔本机 WebSocket 中继协议消息类型
//!
//! 所有消息遵循统一信封格式，通过 `type` 字段区分消息种类。
//! 详见 docs/relay-protocol.md

use serde::{Deserialize, Serialize};

use crate::error::RelayError;

pub mod discovery;
pub mod extension_runtime;
pub mod handshake;
pub mod mcp;
pub mod prompt;
pub mod session_event;
pub mod terminal;
pub mod tool;
pub mod vfs_materialization;
pub mod workspace;

pub use discovery::*;
pub use extension_runtime::*;
pub use handshake::*;
pub use mcp::*;
pub use prompt::*;
pub use session_event::*;
pub use terminal::*;
pub use tool::*;
pub use vfs_materialization::*;
pub use workspace::*;

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

    /// 向运行中的第三方 Agent 注入 steer 消息
    #[serde(rename = "command.steer")]
    CommandSteer {
        id: String,
        payload: CommandSteerPayload,
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

    /// 通用工作空间探测
    #[serde(rename = "command.workspace_detect")]
    CommandWorkspaceDetect {
        id: String,
        payload: CommandWorkspaceDetectPayload,
    },

    /// 检测 Git 信息
    #[serde(rename = "command.workspace_detect_git")]
    CommandWorkspaceDetectGit {
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    },

    /// 按 Workspace identity 反向发现本机候选目录
    #[serde(rename = "command.workspace_discover_by_identity")]
    CommandWorkspaceDiscoverByIdentity {
        id: String,
        payload: CommandWorkspaceDiscoverByIdentityPayload,
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

    /// 读取文件 bytes，用于图片等浏览器资产加载
    #[serde(rename = "command.tool.file_read_binary")]
    CommandToolFileReadBinary {
        id: String,
        payload: ToolFileReadPayload,
    },

    /// 写入文件
    #[serde(rename = "command.tool.file_write")]
    CommandToolFileWrite {
        id: String,
        payload: ToolFileWritePayload,
    },

    /// 删除文件
    #[serde(rename = "command.tool.file_delete")]
    CommandToolFileDelete {
        id: String,
        payload: ToolFileDeletePayload,
    },

    /// 重命名文件
    #[serde(rename = "command.tool.file_rename")]
    CommandToolFileRename {
        id: String,
        payload: ToolFileRenamePayload,
    },

    /// 以 apply_patch 语法批量修改文件
    #[serde(rename = "command.tool.apply_patch")]
    CommandToolApplyPatch {
        id: String,
        payload: ToolApplyPatchPayload,
    },

    /// 执行 Shell 命令
    #[serde(rename = "command.tool.shell_exec")]
    CommandToolShellExec {
        id: String,
        payload: ToolShellExecPayload,
    },

    #[serde(rename = "command.tool.shell_read")]
    CommandToolShellRead {
        id: String,
        payload: ToolShellReadPayload,
    },

    #[serde(rename = "command.tool.shell_input")]
    CommandToolShellInput {
        id: String,
        payload: ToolShellInputPayload,
    },

    #[serde(rename = "command.tool.shell_terminate")]
    CommandToolShellTerminate {
        id: String,
        payload: ToolShellTerminatePayload,
    },

    /// 将云端 VFS 资源物化到本机 backend 的 session cache / working copy
    #[serde(rename = "command.vfs.materialize")]
    CommandVfsMaterialize {
        id: String,
        payload: Box<VfsMaterializePayload>,
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

    #[serde(rename = "response.steer")]
    ResponseSteer {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseSteerPayload>,
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

    #[serde(rename = "response.workspace_detect")]
    ResponseWorkspaceDetect {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseWorkspaceDetectPayload>,
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

    #[serde(rename = "response.workspace_discover_by_identity")]
    ResponseWorkspaceDiscoverByIdentity {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseWorkspaceDiscoverByIdentityPayload>,
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

    #[serde(rename = "response.tool.file_read_binary")]
    ResponseToolFileReadBinary {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileReadBinaryResponse>,
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

    #[serde(rename = "response.tool.file_delete")]
    ResponseToolFileDelete {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileDeleteResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.file_rename")]
    ResponseToolFileRename {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolFileRenameResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.apply_patch")]
    ResponseToolApplyPatch {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolApplyPatchResponse>,
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

    #[serde(rename = "response.tool.shell_read")]
    ResponseToolShellRead {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolShellReadResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.shell_input")]
    ResponseToolShellInput {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolShellInputResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.tool.shell_terminate")]
    ResponseToolShellTerminate {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ToolShellTerminateResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.vfs.materialize")]
    ResponseVfsMaterialize {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<VfsMaterializeResponse>,
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

    // ── MCP Relay 命令（云端 → 本机）──
    /// 一次性 probe：临时连接指定 transport 并探测工具列表（不入池）
    #[serde(rename = "command.mcp_probe_transport")]
    CommandMcpProbeTransport {
        id: String,
        payload: CommandMcpProbeTransportPayload,
    },

    /// 列举本机 MCP server 提供的工具
    #[serde(rename = "command.mcp_list_tools")]
    CommandMcpListTools {
        id: String,
        payload: CommandMcpListToolsPayload,
    },

    /// 调用本机 MCP server 的工具
    #[serde(rename = "command.mcp_call_tool")]
    CommandMcpCallTool {
        id: String,
        payload: CommandMcpCallToolPayload,
    },

    /// 关闭本机 MCP server 连接（可选，会话结束清理）
    #[serde(rename = "command.mcp_close")]
    CommandMcpClose {
        id: String,
        payload: CommandMcpClosePayload,
    },

    /// 调用本机 TS Extension Host runtime action
    #[serde(rename = "command.extension_action_invoke")]
    CommandExtensionActionInvoke {
        id: String,
        payload: CommandExtensionActionInvokePayload,
    },

    /// 调用本机 TS Extension Host protocol channel
    #[serde(rename = "command.extension_channel_invoke")]
    CommandExtensionChannelInvoke {
        id: String,
        payload: CommandExtensionChannelInvokePayload,
    },

    // ── MCP Relay 响应（本机 → 云端）──
    #[serde(rename = "response.mcp_probe_transport")]
    ResponseMcpProbeTransport {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseMcpProbeTransportPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.mcp_list_tools")]
    ResponseMcpListTools {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseMcpListToolsPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.mcp_call_tool")]
    ResponseMcpCallTool {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseMcpCallToolPayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.mcp_close")]
    ResponseMcpClose {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseMcpClosePayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.extension_action_invoke")]
    ResponseExtensionActionInvoke {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseExtensionActionInvokePayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.extension_channel_invoke")]
    ResponseExtensionChannelInvoke {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<ResponseExtensionChannelInvokePayload>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    // ── 串行 Shell 流式输出事件（本机 → 云端）──
    /// shell_exec 执行期间的增量输出
    #[serde(rename = "event.tool.shell_output")]
    EventToolShellOutput {
        id: String,
        payload: ToolShellOutputPayload,
    },

    // ── 交互式终端命令（云端 → 本机）──
    #[serde(rename = "command.terminal.spawn")]
    CommandTerminalSpawn {
        id: String,
        payload: TerminalSpawnPayload,
    },

    #[serde(rename = "command.terminal.input")]
    CommandTerminalInput {
        id: String,
        payload: TerminalInputPayload,
    },

    #[serde(rename = "command.terminal.resize")]
    CommandTerminalResize {
        id: String,
        payload: TerminalResizePayload,
    },

    #[serde(rename = "command.terminal.kill")]
    CommandTerminalKill {
        id: String,
        payload: TerminalKillPayload,
    },

    // ── 交互式终端响应（本机 → 云端）──
    #[serde(rename = "response.terminal.spawn")]
    ResponseTerminalSpawn {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<TerminalSpawnResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.terminal.input")]
    ResponseTerminalInput {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<TerminalInputResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.terminal.resize")]
    ResponseTerminalResize {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<TerminalResizeResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    #[serde(rename = "response.terminal.kill")]
    ResponseTerminalKill {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<TerminalKillResponse>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RelayError>,
    },

    // ── 交互式终端事件（本机 → 云端）──
    #[serde(rename = "event.terminal.output")]
    EventTerminalOutput {
        id: String,
        payload: TerminalOutputPayload,
    },

    #[serde(rename = "event.terminal.state_changed")]
    EventTerminalStateChanged {
        id: String,
        payload: TerminalStateChangedPayload,
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
            | Self::CommandSteer { id, .. }
            | Self::CommandDiscover { id, .. }
            | Self::CommandDiscoverOptions { id, .. }
            | Self::CommandWorkspaceDetect { id, .. }
            | Self::CommandWorkspaceDetectGit { id, .. }
            | Self::CommandWorkspaceDiscoverByIdentity { id, .. }
            | Self::CommandBrowseDirectory { id, .. }
            | Self::CommandToolFileRead { id, .. }
            | Self::CommandToolFileReadBinary { id, .. }
            | Self::CommandToolFileWrite { id, .. }
            | Self::CommandToolFileDelete { id, .. }
            | Self::CommandToolFileRename { id, .. }
            | Self::CommandToolApplyPatch { id, .. }
            | Self::CommandToolShellExec { id, .. }
            | Self::CommandToolShellRead { id, .. }
            | Self::CommandToolShellInput { id, .. }
            | Self::CommandToolShellTerminate { id, .. }
            | Self::CommandVfsMaterialize { id, .. }
            | Self::CommandToolFileList { id, .. }
            | Self::CommandToolSearch { id, .. }
            | Self::ResponsePrompt { id, .. }
            | Self::ResponseCancel { id, .. }
            | Self::ResponseSteer { id, .. }
            | Self::ResponseDiscover { id, .. }
            | Self::ResponseWorkspaceDetect { id, .. }
            | Self::ResponseWorkspaceDetectGit { id, .. }
            | Self::ResponseWorkspaceDiscoverByIdentity { id, .. }
            | Self::ResponseBrowseDirectory { id, .. }
            | Self::ResponseToolFileRead { id, .. }
            | Self::ResponseToolFileReadBinary { id, .. }
            | Self::ResponseToolFileWrite { id, .. }
            | Self::ResponseToolFileDelete { id, .. }
            | Self::ResponseToolFileRename { id, .. }
            | Self::ResponseToolApplyPatch { id, .. }
            | Self::ResponseToolShellExec { id, .. }
            | Self::ResponseToolShellRead { id, .. }
            | Self::ResponseToolShellInput { id, .. }
            | Self::ResponseToolShellTerminate { id, .. }
            | Self::ResponseVfsMaterialize { id, .. }
            | Self::ResponseToolFileList { id, .. }
            | Self::ResponseToolSearch { id, .. }
            | Self::CommandMcpProbeTransport { id, .. }
            | Self::CommandMcpListTools { id, .. }
            | Self::CommandMcpCallTool { id, .. }
            | Self::CommandMcpClose { id, .. }
            | Self::CommandExtensionActionInvoke { id, .. }
            | Self::CommandExtensionChannelInvoke { id, .. }
            | Self::ResponseMcpProbeTransport { id, .. }
            | Self::ResponseMcpListTools { id, .. }
            | Self::ResponseMcpCallTool { id, .. }
            | Self::ResponseMcpClose { id, .. }
            | Self::ResponseExtensionActionInvoke { id, .. }
            | Self::ResponseExtensionChannelInvoke { id, .. }
            | Self::EventCapabilitiesChanged { id, .. }
            | Self::EventSessionNotification { id, .. }
            | Self::EventSessionStateChanged { id, .. }
            | Self::EventDiscoverOptionsPatch { id, .. }
            | Self::EventToolShellOutput { id, .. }
            | Self::CommandTerminalSpawn { id, .. }
            | Self::CommandTerminalInput { id, .. }
            | Self::CommandTerminalResize { id, .. }
            | Self::CommandTerminalKill { id, .. }
            | Self::ResponseTerminalSpawn { id, .. }
            | Self::ResponseTerminalInput { id, .. }
            | Self::ResponseTerminalResize { id, .. }
            | Self::ResponseTerminalKill { id, .. }
            | Self::EventTerminalOutput { id, .. }
            | Self::EventTerminalStateChanged { id, .. }
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
#[cfg(test)]
mod tests {
    use super::CommandPromptPayload;
    use super::*;
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use std::path::PathBuf;

    #[test]
    fn command_prompt_payload_requires_mount_root_ref() {
        let payload: CommandPromptPayload = serde_json::from_value(serde_json::json!({
            "session_id": "s1",
            "input": [
                {
                    "type": "text",
                    "text": "hello",
                    "text_elements": []
                }
            ],
            "mount_root_ref": "/workspace/repo"
        }))
        .expect("payload should deserialize");
        assert_eq!(payload.mount_root_ref, "/workspace/repo");
    }

    #[test]
    fn command_prompt_payload_serializes_mount_root_ref() {
        let payload = CommandPromptPayload {
            session_id: "s1".to_string(),
            follow_up_session_id: None,
            input: agentdash_agent_protocol::text_user_input_blocks("hello"),
            mount_root_ref: "/new/workspace".to_string(),
            workspace_identity_kind: None,
            workspace_identity_payload: None,
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
            mcp_servers: Vec::new(),
        };

        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["mount_root_ref"], "/new/workspace");
        assert_eq!(value["input"][0]["type"], "text");
        let old_raw_field = ["prompt", "blocks"].join("_");
        assert!(
            !value
                .as_object()
                .expect("payload object")
                .contains_key(&old_raw_field)
        );
    }

    #[test]
    fn command_prompt_payload_roundtrips_typed_user_input() {
        let input = vec![
            codex::UserInput::Text {
                text: "请分析这张图".to_string(),
                text_elements: Vec::new(),
            },
            codex::UserInput::Image {
                detail: None,
                url: "data:image/png;base64,AAAA".to_string(),
            },
            codex::UserInput::LocalImage {
                detail: None,
                path: PathBuf::from("assets/local.png"),
            },
            codex::UserInput::Skill {
                name: "reviewer".to_string(),
                path: PathBuf::from("skills/reviewer/SKILL.md"),
            },
            codex::UserInput::Mention {
                name: "main.rs".to_string(),
                path: "file://src/main.rs".to_string(),
            },
        ];
        let payload = CommandPromptPayload {
            session_id: "s1".to_string(),
            follow_up_session_id: Some("s0".to_string()),
            input: input.clone(),
            mount_root_ref: "/workspace/repo".to_string(),
            workspace_identity_kind: None,
            workspace_identity_payload: None,
            working_dir: Some("crates/app".to_string()),
            env: std::collections::HashMap::new(),
            executor_config: None,
            mcp_servers: Vec::new(),
        };

        let value = serde_json::to_value(&payload).expect("payload should serialize");
        assert!(value.get("input").is_some());
        let old_raw_field = ["prompt", "blocks"].join("_");
        assert!(value.get(&old_raw_field).is_none());

        let decoded: CommandPromptPayload =
            serde_json::from_value(value).expect("payload should deserialize");
        assert_eq!(decoded.input, input);
        assert_eq!(decoded.follow_up_session_id.as_deref(), Some("s0"));
    }

    #[test]
    fn command_prompt_payload_rejects_legacy_prompt_and_workspace_root() {
        let error = serde_json::from_value::<CommandPromptPayload>(serde_json::json!({
            "session_id": "s1",
            "prompt": "old text prompt",
            "workspace_root": "/workspace"
        }))
        .expect_err("legacy prompt/workspace_root fields should be rejected");

        let message = error.to_string();
        assert!(message.contains("prompt") || message.contains("workspace_root"));
    }

    #[test]
    fn mcp_probe_transport_command_roundtrip() {
        let msg = RelayMessage::CommandMcpProbeTransport {
            id: "probe-1".to_string(),
            payload: CommandMcpProbeTransportPayload {
                transport: McpTransportConfigRelay::Stdio {
                    command: "npx".to_string(),
                    args: vec!["@mcp/server".to_string()],
                    env: vec![],
                    cwd: None,
                },
            },
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "command.mcp_probe_transport");
        assert_eq!(json["payload"]["transport"]["type"], "stdio");
        assert_eq!(json["payload"]["transport"]["command"], "npx");

        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(deser.id(), "probe-1");
    }

    #[test]
    fn mcp_probe_transport_response_ok_roundtrip() {
        let msg = RelayMessage::ResponseMcpProbeTransport {
            id: "probe-1".to_string(),
            payload: Some(ResponseMcpProbeTransportPayload {
                status: "ok".to_string(),
                latency_ms: Some(123),
                tools: Some(vec![McpToolInfoRelay {
                    name: "read_file".to_string(),
                    description: "read a file".to_string(),
                    parameters_schema: serde_json::json!({}),
                }]),
                error: None,
            }),
            error: None,
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "response.mcp_probe_transport");
        assert_eq!(json["payload"]["status"], "ok");
        assert_eq!(json["payload"]["tools"][0]["name"], "read_file");
    }

    #[test]
    fn mcp_probe_transport_response_error_roundtrip() {
        let msg = RelayMessage::ResponseMcpProbeTransport {
            id: "probe-2".to_string(),
            payload: Some(ResponseMcpProbeTransportPayload {
                status: "error".to_string(),
                latency_ms: None,
                tools: None,
                error: Some("进程启动失败".to_string()),
            }),
            error: None,
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["payload"]["status"], "error");
        assert_eq!(json["payload"]["error"], "进程启动失败");
    }

    #[test]
    fn mcp_list_and_call_commands_carry_resolved_server_declaration() {
        let server = McpServerRelay {
            name: "p4-tools".to_string(),
            transport: McpTransportConfigRelay::Http {
                url: "http://127.0.0.1:8999/mcp?p4_client=demo".to_string(),
                headers: vec![McpHttpHeaderRelay {
                    name: "x-p4-client".to_string(),
                    value: "demo".to_string(),
                }],
            },
        };
        let list = RelayMessage::CommandMcpListTools {
            id: "mcp-list-1".to_string(),
            payload: CommandMcpListToolsPayload {
                server: server.clone(),
            },
        };
        let json = serde_json::to_value(&list).expect("serialize list");
        assert_eq!(json["type"], "command.mcp_list_tools");
        assert_eq!(json["payload"]["server"]["name"], "p4-tools");
        assert_eq!(json["payload"]["server"]["transport"]["type"], "http");
        assert_eq!(
            json["payload"]["server"]["transport"]["headers"][0]["value"],
            "demo"
        );

        let call = RelayMessage::CommandMcpCallTool {
            id: "mcp-call-1".to_string(),
            payload: CommandMcpCallToolPayload {
                server,
                tool_name: "workspace_status".to_string(),
                arguments: Some(serde_json::Map::from_iter([(
                    "detail".to_string(),
                    serde_json::json!(true),
                )])),
            },
        };
        let json = serde_json::to_value(&call).expect("serialize call");
        assert_eq!(json["type"], "command.mcp_call_tool");
        assert_eq!(json["payload"]["server"]["name"], "p4-tools");
        assert_eq!(json["payload"]["tool_name"], "workspace_status");

        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize call");
        assert_eq!(deser.id(), "mcp-call-1");
    }

    #[test]
    fn extension_action_invoke_roundtrip() {
        let msg = RelayMessage::CommandExtensionActionInvoke {
            id: "ext-1".to_string(),
            payload: CommandExtensionActionInvokePayload {
                extension_key: "local-hello".to_string(),
                extension_id: "local-hello".to_string(),
                action_key: "local-hello.profile".to_string(),
                project_id: "project-1".to_string(),
                session_id: "session-1".to_string(),
                input: serde_json::json!({ "verbose": true }),
                package_artifact: Some(ExtensionPackageArtifactRelay {
                    artifact_id: "artifact-1".to_string(),
                    archive_digest:
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                            .to_string(),
                }),
                runtime_extensions: vec![],
                workspace: Some(ExtensionInvocationWorkspaceRelay {
                    mount_id: "main".to_string(),
                    root_ref: "D:/Workspaces/demo".to_string(),
                }),
                trace_id: "trace-1".to_string(),
                invocation_id: "rtinv-1".to_string(),
            },
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "command.extension_action_invoke");
        assert_eq!(json["payload"]["action_key"], "local-hello.profile");
        assert_eq!(json["payload"]["workspace"]["mount_id"], "main");
        assert!(
            !json["payload"]
                .as_object()
                .expect("payload object")
                .contains_key("backend_id"),
            "extension action relay payload must not carry backend_id; routing owns the target"
        );

        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(deser.id(), "ext-1");
    }

    #[test]
    fn extension_action_response_roundtrip() {
        let msg = RelayMessage::ResponseExtensionActionInvoke {
            id: "ext-1".to_string(),
            payload: Some(ResponseExtensionActionInvokePayload {
                extension_key: "local-hello".to_string(),
                extension_id: "local-hello".to_string(),
                action_key: "local-hello.profile".to_string(),
                output: serde_json::json!({ "backend_id": "backend-1" }),
                metadata: serde_json::Map::from_iter([(
                    "trace_id".to_string(),
                    serde_json::json!("trace-1"),
                )]),
            }),
            error: None,
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "response.extension_action_invoke");
        assert_eq!(json["payload"]["metadata"]["trace_id"], "trace-1");
    }

    #[test]
    fn extension_channel_invoke_roundtrip() {
        let msg = RelayMessage::CommandExtensionChannelInvoke {
            id: "ext-channel-1".to_string(),
            payload: CommandExtensionChannelInvokePayload {
                provider_extension_key: "protocol-demo".to_string(),
                provider_extension_id: "protocol-demo".to_string(),
                channel_key: "protocol-demo.api".to_string(),
                method: "echo".to_string(),
                project_id: "project-1".to_string(),
                session_id: "session-1".to_string(),
                input: serde_json::json!({ "text": "hello" }),
                package_artifact: ExtensionPackageArtifactRelay {
                    artifact_id: "artifact-1".to_string(),
                    archive_digest:
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                            .to_string(),
                },
                consumer: ExtensionChannelConsumerRelay {
                    kind: "extension_panel".to_string(),
                    extension_key: Some("protocol-demo".to_string()),
                    extension_id: Some("protocol-demo".to_string()),
                    dependency_alias: Some("self".to_string()),
                },
                workspace: Some(ExtensionInvocationWorkspaceRelay {
                    mount_id: "main".to_string(),
                    root_ref: "D:/Workspaces/demo".to_string(),
                }),
                trace_id: "trace-1".to_string(),
                invocation_id: "rtinv-1".to_string(),
            },
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "command.extension_channel_invoke");
        assert_eq!(json["payload"]["channel_key"], "protocol-demo.api");
        assert_eq!(json["payload"]["method"], "echo");
        assert_eq!(
            json["payload"]["workspace"]["root_ref"],
            "D:/Workspaces/demo"
        );
        assert!(
            !json["payload"]
                .as_object()
                .expect("payload object")
                .contains_key("backend_id"),
            "extension channel relay payload must not carry backend_id; routing owns the target"
        );

        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(deser.id(), "ext-channel-1");
    }

    #[test]
    fn extension_channel_response_roundtrip() {
        let msg = RelayMessage::ResponseExtensionChannelInvoke {
            id: "ext-channel-1".to_string(),
            payload: Some(ResponseExtensionChannelInvokePayload {
                provider_extension_key: "protocol-demo".to_string(),
                provider_extension_id: "protocol-demo".to_string(),
                channel_key: "protocol-demo.api".to_string(),
                method: "echo".to_string(),
                output: serde_json::json!({ "ok": true }),
                metadata: serde_json::Map::from_iter([(
                    "trace_id".to_string(),
                    serde_json::json!("trace-1"),
                )]),
            }),
            error: None,
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "response.extension_channel_invoke");
        assert_eq!(json["payload"]["metadata"]["trace_id"], "trace-1");
    }

    #[test]
    fn vfs_materialize_command_roundtrip() {
        let msg = RelayMessage::CommandVfsMaterialize {
            id: "mat-1".to_string(),
            payload: Box::new(VfsMaterializePayload {
                session_id: "session-1".to_string(),
                turn_id: Some("turn-1".to_string()),
                tool_call_id: Some("tool-1".to_string()),
                plan_id: "plan-1".to_string(),
                plan_kind: MaterializationPlanKind::SkillResourceSet,
                source_uri: "skill-assets://skills/reviewer/scripts/check.sh".to_string(),
                root_uri: "skill-assets://skills/reviewer".to_string(),
                mount_id: "skill-assets".to_string(),
                provider: "skill_asset_fs".to_string(),
                primary_relative_path: "scripts/check.sh".to_string(),
                target_kind: MaterializationTargetKind::File,
                access_mode: MaterializationAccessMode::ReadOnly,
                entries: vec![VfsMaterializeEntry {
                    relative_path: "scripts/check.sh".to_string(),
                    content: VfsMaterializeContent::Utf8Text {
                        text: "echo ok\n".to_string(),
                    },
                    digest: "sha256:test".to_string(),
                    size_bytes: 8,
                    mime_hint: Some("text/x-shellscript".to_string()),
                    executable_hint: true,
                }],
                cache_scope: MaterializationCacheScope::Public,
                ttl_ms: Some(60_000),
            }),
        };

        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "command.vfs.materialize");
        assert_eq!(json["payload"]["plan_kind"], "skill_resource_set");
        assert_eq!(json["payload"]["cache_scope"], "public");
        assert_eq!(
            json["payload"]["entries"][0]["content"]["encoding"],
            "utf8_text"
        );

        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(deser.id(), "mat-1");
    }

    #[test]
    fn vfs_materialize_response_roundtrip() {
        let msg = RelayMessage::ResponseVfsMaterialize {
            id: "mat-1".to_string(),
            payload: Some(VfsMaterializeResponse {
                source_uri: "skill-assets://skills/reviewer/scripts/check.sh".to_string(),
                local_root_path: "/tmp/agentdash/materialized/session-1/plan-1".to_string(),
                primary_local_path: "/tmp/agentdash/materialized/session-1/plan-1/scripts/check.sh"
                    .to_string(),
                primary_local_url: None,
                access_mode: MaterializationAccessMode::ReadOnly,
                manifest_digest: "sha256:manifest".to_string(),
                total_size_bytes: 8,
                entry_count: 1,
                dirty: false,
                cache_hit: false,
            }),
            error: None,
        };

        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "response.vfs.materialize");
        assert_eq!(json["payload"]["entry_count"], 1);
        let deser: RelayMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(deser.id(), "mat-1");
    }

    #[test]
    fn shell_session_commands_roundtrip() {
        let start = RelayMessage::CommandToolShellExec {
            id: "shell-start-1".to_string(),
            payload: ToolShellExecPayload {
                call_id: "call-1".to_string(),
                command: "cargo check".to_string(),
                terminal_id: Some("term-1".to_string()),
                mount_root_ref: "D:/workspace".to_string(),
                cwd: Some("crates/agentdash-local".to_string()),
                timeout_ms: None,
                yield_time_ms: Some(750),
                max_output_bytes: Some(65_536),
                tty: false,
                cols: None,
                rows: None,
            },
        };
        let json = serde_json::to_value(&start).expect("serialize start");
        assert_eq!(json["type"], "command.tool.shell_exec");
        assert_eq!(json["payload"]["yield_time_ms"], 750);
        assert_eq!(json["payload"]["max_output_bytes"], 65_536);
        assert_eq!(json["payload"]["terminal_id"], "term-1");
        let decoded: RelayMessage = serde_json::from_value(json).expect("deserialize start");
        assert_eq!(decoded.id(), "shell-start-1");

        let read = RelayMessage::CommandToolShellRead {
            id: "shell-read-1".to_string(),
            payload: ToolShellReadPayload {
                session_id: "shell-1".to_string(),
                after_seq: Some(3),
                wait_ms: Some(1_000),
                max_bytes: Some(8_192),
            },
        };
        let json = serde_json::to_value(&read).expect("serialize read");
        assert_eq!(json["type"], "command.tool.shell_read");
        assert_eq!(json["payload"]["after_seq"], 3);
        let decoded: RelayMessage = serde_json::from_value(json).expect("deserialize read");
        assert_eq!(decoded.id(), "shell-read-1");
    }

    #[test]
    fn shell_running_response_carries_session_seq_and_truncation() {
        let msg = RelayMessage::ResponseToolShellExec {
            id: "shell-start-1".to_string(),
            payload: Some(ToolShellExecResponse {
                call_id: "call-1".to_string(),
                session_id: "shell-1".to_string(),
                terminal_id: Some("shell-1".to_string()),
                state: ToolShellSessionState::Running,
                exit_code: None,
                stdout: "ready\n".to_string(),
                stderr: String::new(),
                pty: String::new(),
                chunks: vec![ToolShellOutputChunk {
                    seq: 0,
                    stream: ShellOutputStream::Stdout,
                    data: "ready\n".to_string(),
                }],
                next_seq: 1,
                truncation: ToolShellTruncationInfo {
                    truncated: true,
                    omitted_bytes: 1024,
                    omitted_chunks: 2,
                    omitted_tokens_estimate: Some(256),
                },
            }),
            error: None,
        };
        let json = serde_json::to_value(&msg).expect("serialize response");
        assert_eq!(json["type"], "response.tool.shell_exec");
        assert_eq!(json["payload"]["state"], "running");
        assert_eq!(json["payload"]["next_seq"], 1);
        assert_eq!(json["payload"]["truncation"]["truncated"], true);

        let decoded: RelayMessage = serde_json::from_value(json).expect("deserialize response");
        assert_eq!(decoded.id(), "shell-start-1");
    }
}
