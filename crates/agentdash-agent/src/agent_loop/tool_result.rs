use crate::types::{AgentToolResult, ContentPart};

pub(super) fn error_tool_result(message: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: None,
    }
}

pub(super) fn approval_rejected_tool_result(reason: Option<String>) -> AgentToolResult {
    let message = reason
        .clone()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("工具执行未获批准：{value}"))
        .unwrap_or_else(|| "工具执行未获批准".to_string());
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: Some(serde_json::json!({
            "approval_state": "rejected",
            "reason": reason,
        })),
    }
}
