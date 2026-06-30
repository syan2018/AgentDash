use std::sync::Arc;

pub type ToolResultCacheWriter = Arc<dyn Fn(ToolResultCacheWrite) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadableBodyKind {
    Tool,
    Command,
}

impl ReadableBodyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool_result",
            Self::Command => "command_result",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableToolResultRef {
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: ReadableBodyKind,
    pub item_id: String,
    pub lifecycle_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableTerminalRef {
    pub raw_terminal_id: String,
    pub terminal_alias: String,
    pub metadata_path: String,
    pub log_path: String,
    pub lifecycle_path: String,
}

pub trait ToolResultAddressProvider: Send + Sync {
    fn tool_result_ref(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        tool_name: &str,
    ) -> ReadableToolResultRef;
}

#[derive(Clone)]
pub struct ToolResultRefContext {
    pub session_id: String,
    pub raw_turn_id: String,
    pub address_provider: Arc<dyn ToolResultAddressProvider>,
    pub cache_writer: Option<ToolResultCacheWriter>,
}

#[derive(Debug, Clone)]
pub struct ToolResultCacheWrite {
    pub session_id: String,
    pub item_id: String,
    pub lifecycle_path: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: String,
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub tool_name: String,
    pub text: String,
    pub original_bytes: usize,
}

pub fn stable_tool_result_item_id(turn_id: &str, tool_call_id: &str) -> String {
    format!("{turn_id}:{tool_call_id}")
}

pub fn ephemeral_tool_result_ref(raw_tool_call_id: &str, tool_name: &str) -> ReadableToolResultRef {
    let body_kind = if tool_name == "shell_exec" {
        ReadableBodyKind::Command
    } else {
        ReadableBodyKind::Tool
    };
    let turn_alias = "turn_001".to_string();
    let body_alias = match body_kind {
        ReadableBodyKind::Tool => "tool_001",
        ReadableBodyKind::Command => "cmd_001",
    }
    .to_string();
    let item_id = format!("{turn_alias}:{body_alias}");
    let lifecycle_path =
        format!("lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt");
    ReadableToolResultRef {
        raw_turn_id: "turn".to_string(),
        raw_tool_call_id: raw_tool_call_id.to_string(),
        turn_alias,
        body_alias,
        body_kind,
        item_id,
        lifecycle_path,
    }
}
