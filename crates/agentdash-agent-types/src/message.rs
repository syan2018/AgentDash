use serde::{Deserialize, Serialize};

use crate::content::ContentPart;
use crate::tool::AgentToolResult;

// ─── MessageRef ────────────────────────────────────────────

/// 消息稳定引用 — 对齐 PersistedSessionEvent 的 turn_id + entry_index。
///
/// 用于 compaction cut boundary、restore 对齐、branch lineage 继承。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageRef {
    pub turn_id: String,
    pub entry_index: u32,
}

/// 当前时间戳（毫秒）
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── ToolCallInfo ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    #[serde(default)]
    pub call_id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ─── StopReason ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

// ─── TokenUsage ─────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

// ─── AgentMessage ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentMessage {
    User {
        content: Vec<ContentPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    Assistant {
        content: Vec<ContentPart>,
        #[serde(default)]
        tool_calls: Vec<ToolCallInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<StopReason>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default)]
        call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    CompactionSummary {
        summary: String,
        #[serde(default)]
        tokens_before: u64,
        #[serde(default)]
        messages_compacted: u32,
        /// 精准压缩边界 — 此摘要覆盖到这条消息（含）之前的所有内容。
        /// 优先于 messages_compacted 计数，为 None 时 fallback 到计数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compacted_until_ref: Option<MessageRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
}

impl AgentMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![ContentPart::text(text)],
            timestamp: Some(now_millis()),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant {
            content: vec![ContentPart::text(text)],
            tool_calls: vec![],
            stop_reason: None,
            error_message: None,
            usage: None,
            timestamp: Some(now_millis()),
        }
    }

    pub fn error_assistant(error_message: impl Into<String>, aborted: bool) -> Self {
        let msg = error_message.into();
        Self::Assistant {
            content: vec![ContentPart::text(&msg)],
            tool_calls: vec![],
            stop_reason: Some(if aborted {
                StopReason::Aborted
            } else {
                StopReason::Error
            }),
            error_message: Some(msg),
            usage: Some(TokenUsage::default()),
            timestamp: Some(now_millis()),
        }
    }

    pub fn is_error_or_aborted(&self) -> bool {
        matches!(
            self,
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            }
        )
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id: None,
            tool_name: None,
            content: vec![ContentPart::text(text)],
            details: None,
            is_error,
            timestamp: Some(now_millis()),
        }
    }

    pub fn tool_result_full(
        tool_call_id: impl Into<String>,
        call_id: Option<String>,
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        details: Option<serde_json::Value>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp: Some(now_millis()),
        }
    }

    pub fn compaction_summary(
        summary: impl Into<String>,
        tokens_before: u64,
        messages_compacted: u32,
    ) -> Self {
        Self::CompactionSummary {
            summary: summary.into(),
            tokens_before,
            messages_compacted,
            compacted_until_ref: None,
            timestamp: Some(now_millis()),
        }
    }

    pub fn tool_result_from_agent(
        tool_call_id: impl Into<String>,
        call_id: Option<String>,
        tool_name: Option<String>,
        result: &AgentToolResult,
    ) -> Self {
        Self::tool_result_full(
            tool_call_id,
            call_id,
            tool_name,
            result.content.clone(),
            result.details.clone(),
            result.is_error,
        )
    }

    pub fn first_text(&self) -> Option<&str> {
        match self {
            Self::User { content, .. }
            | Self::Assistant { content, .. }
            | Self::ToolResult { content, .. } => {
                content.iter().find_map(ContentPart::extract_text)
            }
            Self::CompactionSummary { summary, .. } => Some(summary.as_str()),
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Self::User { .. })
    }

    /// Replace all text content parts in a User message.
    pub fn replace_user_text(&mut self, new_text: &str) {
        if let Self::User { content, .. } = self {
            *content = vec![ContentPart::text(new_text)];
        }
    }
}

// ─── AgentMessage 便利转换 ──────────────────────────────────

impl From<String> for AgentMessage {
    fn from(text: String) -> Self {
        AgentMessage::user(text)
    }
}

impl From<&str> for AgentMessage {
    fn from(text: &str) -> Self {
        AgentMessage::user(text)
    }
}
