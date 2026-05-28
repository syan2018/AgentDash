use crate::{AgentMessage, ContentPart, ToolDefinition};

pub fn estimate_request_tokens(
    system_prompt: Option<&str>,
    messages: &[AgentMessage],
    tools: &[ToolDefinition],
) -> u64 {
    let system_tokens = system_prompt.map(text_tokens).unwrap_or_default();
    let message_tokens = messages
        .iter()
        .map(estimate_message_tokens)
        .fold(0_u64, u64::saturating_add);
    let tool_tokens = tools
        .iter()
        .map(estimate_tool_tokens)
        .fold(0_u64, u64::saturating_add);
    system_tokens
        .saturating_add(message_tokens)
        .saturating_add(tool_tokens)
}

pub fn estimate_tool_tokens(tool: &ToolDefinition) -> u64 {
    let chars = tool
        .name
        .chars()
        .count()
        .saturating_add(tool.description.chars().count())
        .saturating_add(json_chars(&tool.parameters));
    chars_to_tokens(chars).saturating_add(4)
}

pub fn estimate_message_tokens(message: &AgentMessage) -> u64 {
    match message {
        AgentMessage::User { content, .. } => estimate_content_tokens(content),
        AgentMessage::Assistant {
            content,
            tool_calls,
            error_message,
            ..
        } => {
            let tool_chars = tool_calls.iter().fold(0_usize, |total, call| {
                total
                    .saturating_add(call.id.chars().count())
                    .saturating_add(
                        call.call_id
                            .as_deref()
                            .map(|value| value.chars().count())
                            .unwrap_or_default(),
                    )
                    .saturating_add(call.name.chars().count())
                    .saturating_add(json_chars(&call.arguments))
            });
            estimate_content_tokens(content)
                .saturating_add(chars_to_tokens(tool_chars))
                .saturating_add(
                    error_message
                        .as_deref()
                        .map(text_tokens)
                        .unwrap_or_default(),
                )
        }
        AgentMessage::ToolResult {
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            ..
        } => {
            let metadata_chars = tool_call_id
                .chars()
                .count()
                .saturating_add(
                    call_id
                        .as_deref()
                        .map(|value| value.chars().count())
                        .unwrap_or_default(),
                )
                .saturating_add(
                    tool_name
                        .as_deref()
                        .map(|value| value.chars().count())
                        .unwrap_or_default(),
                )
                .saturating_add(details.as_ref().map(json_chars).unwrap_or_default());
            estimate_content_tokens(content).saturating_add(chars_to_tokens(metadata_chars))
        }
        AgentMessage::CompactionSummary { summary, .. } => text_tokens(summary),
    }
}

pub fn estimate_content_tokens(content: &[ContentPart]) -> u64 {
    let chars = content.iter().fold(0_usize, |total, part| {
        total.saturating_add(match part {
            ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => {
                text.chars().count()
            }
            ContentPart::Image { mime_type, data } => {
                mime_type.chars().count().saturating_add(data.len() / 4)
            }
        })
    });
    chars_to_tokens(chars).saturating_add(4)
}

pub fn text_tokens(value: &str) -> u64 {
    chars_to_tokens(value.chars().count()).saturating_add(4)
}

pub fn chars_to_tokens(chars: usize) -> u64 {
    u64::try_from(chars).unwrap_or(u64::MAX).saturating_add(3) / 4
}

fn json_chars(value: &serde_json::Value) -> usize {
    serde_json::to_string(value)
        .map(|value| value.chars().count())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentMessage;

    #[test]
    fn request_estimate_includes_system_messages_and_tools() {
        let tool = ToolDefinition {
            name: "read_file".to_string(),
            description: "读取文件".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };

        let total =
            estimate_request_tokens(Some("system"), &[AgentMessage::user("hello")], &[tool]);

        assert!(total > estimate_message_tokens(&AgentMessage::user("hello")));
    }
}
