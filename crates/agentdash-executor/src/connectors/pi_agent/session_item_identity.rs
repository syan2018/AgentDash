use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agentdash_agent::{
    AgentMessage, ReadableBodyKind, ReadableTerminalRef, ReadableToolResultRef,
    ToolResultAddressProvider,
};
use agentdash_spi::RestoredSessionState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SessionItemIdentityWatermark {
    pub turn: usize,
    pub tool: usize,
    pub command: usize,
    pub terminal: usize,
}

#[derive(Debug, Default)]
pub(super) struct SessionItemIdentity {
    inner: RwLock<SessionItemIdentityState>,
}

#[derive(Debug, Default)]
struct SessionItemIdentityState {
    turn_aliases: HashMap<String, String>,
    body_aliases: HashMap<ReadableBodyAliasKey, String>,
    terminal_aliases: HashMap<String, String>,
    watermark: SessionItemIdentityWatermark,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReadableBodyAliasKey {
    kind: ReadableBodyKind,
    raw_tool_call_id: String,
}

impl SessionItemIdentity {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn observe_restored_state(&self, restored_state: Option<&RestoredSessionState>) {
        let Some(restored_state) = restored_state else {
            return;
        };
        for message in &restored_state.messages {
            self.observe_message(message);
        }
    }

    pub(super) fn observe_tool_result_item_id(&self, item_id: &str) {
        let Some((turn_index, body_kind, body_index)) = parse_tool_result_item_id(item_id) else {
            return;
        };
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.watermark.turn = state.watermark.turn.max(turn_index);
        match body_kind {
            ReadableBodyKind::Tool => {
                state.watermark.tool = state.watermark.tool.max(body_index);
            }
            ReadableBodyKind::Command => {
                state.watermark.command = state.watermark.command.max(body_index);
            }
        }
    }

    pub(super) fn terminal_ref(&self, raw_terminal_id: &str) -> ReadableTerminalRef {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let terminal_alias = state.terminal_alias(raw_terminal_id);
        ReadableTerminalRef {
            raw_terminal_id: raw_terminal_id.to_string(),
            metadata_path: format!("session/terminal/{terminal_alias}.metadata.json"),
            log_path: format!("session/terminal/{terminal_alias}.log"),
            lifecycle_path: format!("lifecycle://session/terminal/{terminal_alias}.log"),
            terminal_alias,
        }
    }

    #[cfg(test)]
    pub(super) fn watermark(&self) -> SessionItemIdentityWatermark {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .watermark
    }

    fn observe_message(&self, message: &AgentMessage) {
        match message {
            AgentMessage::Assistant { tool_calls, .. } => {
                for tool_call in tool_calls {
                    self.observe_tool_result_item_id(&tool_call.id);
                }
            }
            AgentMessage::ToolResult {
                tool_call_id,
                details,
                ..
            } => {
                self.observe_tool_result_item_id(tool_call_id);
                if let Some(details) = details {
                    self.observe_tool_result_details(details);
                }
            }
            _ => {}
        }
    }

    fn observe_tool_result_details(&self, details: &serde_json::Value) {
        if let Some(item_id) = details
            .get("readable_ref")
            .and_then(|value| value.get("item_id"))
            .and_then(serde_json::Value::as_str)
        {
            self.observe_tool_result_item_id(item_id);
        }
        if let Some(item_id) = details
            .get("lifecycle_path")
            .and_then(serde_json::Value::as_str)
            .and_then(tool_result_item_id_from_lifecycle_path)
        {
            self.observe_tool_result_item_id(&item_id);
        }
    }
}

impl ToolResultAddressProvider for SessionItemIdentity {
    fn tool_result_ref(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        tool_name: &str,
    ) -> ReadableToolResultRef {
        let kind = readable_body_kind_for_tool_name(tool_name);
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let turn_alias = state.turn_alias(raw_turn_id);
        let body_alias = state.body_alias(kind, raw_tool_call_id);
        let item_id = readable_tool_result_item_id(&turn_alias, &body_alias);
        let lifecycle_path = readable_tool_result_lifecycle_path(&turn_alias, &body_alias);
        ReadableToolResultRef {
            raw_turn_id: raw_turn_id.to_string(),
            raw_tool_call_id: raw_tool_call_id.to_string(),
            turn_alias,
            body_alias,
            body_kind: kind,
            item_id,
            lifecycle_path,
        }
    }
}

impl SessionItemIdentityState {
    fn turn_alias(&mut self, raw_turn_id: &str) -> String {
        if let Some(alias) = self.turn_aliases.get(raw_turn_id) {
            return alias.clone();
        }
        self.watermark.turn += 1;
        let alias = format_readable_alias("turn", self.watermark.turn);
        self.turn_aliases
            .insert(raw_turn_id.to_string(), alias.clone());
        alias
    }

    fn body_alias(&mut self, kind: ReadableBodyKind, raw_tool_call_id: &str) -> String {
        let key = ReadableBodyAliasKey {
            kind,
            raw_tool_call_id: raw_tool_call_id.to_string(),
        };
        if let Some(alias) = self.body_aliases.get(&key) {
            return alias.clone();
        }
        let next = match kind {
            ReadableBodyKind::Tool => {
                self.watermark.tool += 1;
                self.watermark.tool
            }
            ReadableBodyKind::Command => {
                self.watermark.command += 1;
                self.watermark.command
            }
        };
        let alias = format_readable_alias(readable_body_alias_prefix(kind), next);
        self.body_aliases.insert(key, alias.clone());
        alias
    }

    fn terminal_alias(&mut self, raw_terminal_id: &str) -> String {
        if let Some(alias) = self.terminal_aliases.get(raw_terminal_id) {
            return alias.clone();
        }
        self.watermark.terminal += 1;
        let alias = format_readable_alias("term", self.watermark.terminal);
        self.terminal_aliases
            .insert(raw_terminal_id.to_string(), alias.clone());
        alias
    }
}

fn readable_body_kind_for_tool_name(tool_name: &str) -> ReadableBodyKind {
    if tool_name == "shell_exec" {
        ReadableBodyKind::Command
    } else {
        ReadableBodyKind::Tool
    }
}

fn readable_body_alias_prefix(kind: ReadableBodyKind) -> &'static str {
    match kind {
        ReadableBodyKind::Tool => "tool",
        ReadableBodyKind::Command => "cmd",
    }
}

fn format_readable_alias(prefix: &str, index: usize) -> String {
    if index < 1000 {
        format!("{prefix}_{index:03}")
    } else {
        format!("{prefix}_{index}")
    }
}

fn parse_readable_alias(alias: &str, prefix: &str) -> Option<usize> {
    let suffix = alias
        .strip_prefix(prefix)?
        .strip_prefix('_')
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))?;
    let index = suffix.parse::<usize>().ok()?;
    (index > 0).then_some(index)
}

fn parse_tool_result_item_id(item_id: &str) -> Option<(usize, ReadableBodyKind, usize)> {
    let (turn_alias, body_alias) = item_id.split_once(':')?;
    let turn_index = parse_readable_alias(turn_alias, "turn")?;
    if let Some(body_index) = parse_readable_alias(body_alias, "tool") {
        return Some((turn_index, ReadableBodyKind::Tool, body_index));
    }
    if let Some(body_index) = parse_readable_alias(body_alias, "cmd") {
        return Some((turn_index, ReadableBodyKind::Command, body_index));
    }
    None
}

fn readable_tool_result_item_id(turn_alias: &str, body_alias: &str) -> String {
    format!("{turn_alias}:{body_alias}")
}

fn readable_tool_result_lifecycle_path(turn_alias: &str, body_alias: &str) -> String {
    format!("lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt")
}

fn tool_result_item_id_from_lifecycle_path(path: &str) -> Option<String> {
    let remainder = path
        .strip_prefix("lifecycle://session/tool-results/")?
        .strip_suffix("/result.txt")?;
    let mut parts = remainder.split('/');
    let turn_alias = parts.next()?;
    let body_alias = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{turn_alias}:{body_alias}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_state_advances_tool_and_command_watermarks() {
        let identity = SessionItemIdentity::new();
        let restored_state = RestoredSessionState {
            messages: vec![
                AgentMessage::Assistant {
                    content: Vec::new(),
                    tool_calls: vec![agentdash_agent::ToolCallInfo {
                        id: "turn_001:tool_004".to_string(),
                        call_id: None,
                        name: "fs_read".to_string(),
                        arguments: serde_json::json!({}),
                    }],
                    stop_reason: None,
                    error_message: None,
                    usage: None,
                    timestamp: None,
                },
                AgentMessage::ToolResult {
                    tool_call_id: "legacy-raw-tool-call-id".to_string(),
                    call_id: None,
                    tool_name: Some("shell_exec".to_string()),
                    content: Vec::new(),
                    details: Some(serde_json::json!({
                        "readable_ref": {
                            "item_id": "turn_002:cmd_002"
                        },
                        "lifecycle_path": "lifecycle://session/tool-results/turn_002/cmd_002/result.txt"
                    })),
                    is_error: false,
                    timestamp: None,
                },
            ],
            message_refs: Vec::new(),
        };

        identity.observe_restored_state(Some(&restored_state));

        let tool_ref = identity.tool_result_ref("raw-turn-new", "raw-tool-new", "fs_read");
        assert_eq!(tool_ref.item_id, "turn_003:tool_005");

        let command_ref = identity.tool_result_ref("raw-turn-new", "raw-cmd-new", "shell_exec");
        assert_eq!(command_ref.item_id, "turn_003:cmd_003");
    }

    #[test]
    fn terminal_ref_uses_session_identity_watermark() {
        let identity = SessionItemIdentity::new();

        let first = identity.terminal_ref("terminal-raw-1");
        assert_eq!(first.terminal_alias, "term_001");
        assert_eq!(
            first.metadata_path,
            "session/terminal/term_001.metadata.json"
        );
        assert_eq!(first.log_path, "session/terminal/term_001.log");
        assert_eq!(
            first.lifecycle_path,
            "lifecycle://session/terminal/term_001.log"
        );

        let same = identity.terminal_ref("terminal-raw-1");
        assert_eq!(same.terminal_alias, "term_001");

        let second = identity.terminal_ref("terminal-raw-2");
        assert_eq!(second.terminal_alias, "term_002");
        assert_eq!(identity.watermark().terminal, 2);
    }
}
