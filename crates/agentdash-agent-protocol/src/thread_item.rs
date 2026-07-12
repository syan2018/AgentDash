//! AgentDash 运行协议类型出口。
//!
//! Codex Protocol 已经覆盖的 item 与状态语义直接从 Codex 导出；AgentDash 只在
//! Codex 没有一等 variant 的地方做加法扩展。

use crate::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use codex::{
    CommandExecutionStatus, DynamicToolCallOutputContentItem, DynamicToolCallStatus,
    McpToolCallStatus, PatchApplyStatus, ThreadItem as CodexThreadItem,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export_to = "agentdash/")]
pub enum AgentDashThreadItem {
    #[ts(type = "ThreadItem")]
    Codex(codex::ThreadItem),
    AgentDash(AgentDashNativeThreadItem),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type", export_to = "agentdash/")]
pub enum AgentDashNativeThreadItem {
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    ShellExec {
        id: String,
        command: String,
        cwd: Option<String>,
        execution_mode: ShellExecExecutionMode,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        aggregated_output: Option<String>,
        exit_code: Option<i32>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsRead {
        id: String,
        path: String,
        offset: Option<usize>,
        limit: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsGrep {
        id: String,
        pattern: String,
        path: Option<String>,
        glob: Option<String>,
        file_type: Option<String>,
        output_mode: Option<String>,
        head_limit: Option<usize>,
        offset: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsGlob {
        id: String,
        pattern: String,
        path: Option<String>,
        max_results: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agentdash/")]
pub enum ShellExecExecutionMode {
    Platform,
    MountExec,
}

impl AgentDashThreadItem {
    pub fn id(&self) -> &str {
        match self {
            AgentDashThreadItem::Codex(item) => codex_item_id(item),
            AgentDashThreadItem::AgentDash(item) => item.id(),
        }
    }

    pub fn as_codex(&self) -> Option<&codex::ThreadItem> {
        match self {
            AgentDashThreadItem::Codex(item) => Some(item),
            AgentDashThreadItem::AgentDash(_) => None,
        }
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        match self {
            AgentDashThreadItem::Codex(item) => match item {
                codex::ThreadItem::DynamicToolCall { id, .. }
                | codex::ThreadItem::CommandExecution { id, .. }
                | codex::ThreadItem::McpToolCall { id, .. }
                | codex::ThreadItem::FileChange { id, .. }
                | codex::ThreadItem::CollabAgentToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            },
            AgentDashThreadItem::AgentDash(item) => Some(item.id()),
        }
    }
}

fn codex_item_id(item: &codex::ThreadItem) -> &str {
    match item {
        codex::ThreadItem::UserMessage { id, .. }
        | codex::ThreadItem::HookPrompt { id, .. }
        | codex::ThreadItem::AgentMessage { id, .. }
        | codex::ThreadItem::Plan { id, .. }
        | codex::ThreadItem::Reasoning { id, .. }
        | codex::ThreadItem::CommandExecution { id, .. }
        | codex::ThreadItem::FileChange { id, .. }
        | codex::ThreadItem::McpToolCall { id, .. }
        | codex::ThreadItem::DynamicToolCall { id, .. }
        | codex::ThreadItem::CollabAgentToolCall { id, .. }
        | codex::ThreadItem::SubAgentActivity { id, .. }
        | codex::ThreadItem::WebSearch { id, .. }
        | codex::ThreadItem::ImageView { id, .. }
        | codex::ThreadItem::Sleep { id, .. }
        | codex::ThreadItem::ImageGeneration { id, .. }
        | codex::ThreadItem::EnteredReviewMode { id, .. }
        | codex::ThreadItem::ExitedReviewMode { id, .. }
        | codex::ThreadItem::ContextCompaction { id, .. } => id,
    }
}

impl AgentDashNativeThreadItem {
    pub fn id(&self) -> &str {
        match self {
            AgentDashNativeThreadItem::ShellExec { id, .. }
            | AgentDashNativeThreadItem::FsRead { id, .. }
            | AgentDashNativeThreadItem::FsGrep { id, .. }
            | AgentDashNativeThreadItem::FsGlob { id, .. } => id,
        }
    }

    pub fn tool_name(&self) -> &'static str {
        match self {
            AgentDashNativeThreadItem::ShellExec { .. } => "shell_exec",
            AgentDashNativeThreadItem::FsRead { .. } => "fs_read",
            AgentDashNativeThreadItem::FsGrep { .. } => "fs_grep",
            AgentDashNativeThreadItem::FsGlob { .. } => "fs_glob",
        }
    }

    pub fn arguments(&self) -> &serde_json::Value {
        match self {
            AgentDashNativeThreadItem::ShellExec { arguments, .. }
            | AgentDashNativeThreadItem::FsRead { arguments, .. }
            | AgentDashNativeThreadItem::FsGrep { arguments, .. }
            | AgentDashNativeThreadItem::FsGlob { arguments, .. } => arguments,
        }
    }

    pub fn status(&self) -> &codex::DynamicToolCallStatus {
        match self {
            AgentDashNativeThreadItem::ShellExec { status, .. }
            | AgentDashNativeThreadItem::FsRead { status, .. }
            | AgentDashNativeThreadItem::FsGrep { status, .. }
            | AgentDashNativeThreadItem::FsGlob { status, .. } => status,
        }
    }

    pub fn content_items(&self) -> Option<&Vec<codex::DynamicToolCallOutputContentItem>> {
        match self {
            AgentDashNativeThreadItem::FsRead { content_items, .. }
            | AgentDashNativeThreadItem::FsGrep { content_items, .. }
            | AgentDashNativeThreadItem::FsGlob { content_items, .. } => content_items.as_ref(),
            AgentDashNativeThreadItem::ShellExec { .. } => None,
        }
    }

    pub fn success(&self) -> Option<bool> {
        match self {
            AgentDashNativeThreadItem::ShellExec { success, .. }
            | AgentDashNativeThreadItem::FsRead { success, .. }
            | AgentDashNativeThreadItem::FsGrep { success, .. }
            | AgentDashNativeThreadItem::FsGlob { success, .. } => *success,
        }
    }

    pub fn shell_output(&self) -> Option<&str> {
        match self {
            AgentDashNativeThreadItem::ShellExec {
                aggregated_output, ..
            } => aggregated_output.as_deref(),
            _ => None,
        }
    }
}

impl From<codex::ThreadItem> for AgentDashThreadItem {
    fn from(value: codex::ThreadItem) -> Self {
        AgentDashThreadItem::Codex(value)
    }
}

impl From<crate::generated::codex_v2::server_notification::ThreadItem> for AgentDashThreadItem {
    fn from(value: crate::generated::codex_v2::server_notification::ThreadItem) -> Self {
        let value = serde_json::to_value(value).expect("generated server item serializes");
        let item = serde_json::from_value(value)
            .expect("generated server item conforms to owned ThreadItem schema");
        AgentDashThreadItem::Codex(item)
    }
}

impl From<AgentDashNativeThreadItem> for AgentDashThreadItem {
    fn from(value: AgentDashNativeThreadItem) -> Self {
        AgentDashThreadItem::AgentDash(value)
    }
}
