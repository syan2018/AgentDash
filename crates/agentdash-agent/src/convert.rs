/// AgentMessage ↔ rig::Message 双向转换
///
/// 这是设计文档中 `convert_to_llm` / `convert_from_llm` 管线的实现。
/// AgentMessage 是面向会话的消息层，rig::Message 是面向模型的消息层。
use rig::completion::message::{
    AssistantContent, Message, Text, ToolCall, ToolFunction, ToolResult, ToolResultContent,
    UserContent,
};
use rig::OneOrMany;

use crate::types::{AgentMessage, ContentPart, ToolCallInfo};

/// 默认 convert_to_llm：将 AgentMessage 列表转为 rig::Message 列表
pub fn default_convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages.iter().filter_map(agent_to_llm).collect()
}

fn agent_to_llm(msg: &AgentMessage) -> Option<Message> {
    match msg {
        AgentMessage::User { content } => {
            let parts: Vec<UserContent> = content.iter().filter_map(content_part_to_user).collect();
            if parts.is_empty() {
                return None;
            }
            Some(Message::User {
                content: OneOrMany::many(parts).unwrap_or_else(|_| unreachable!()),
            })
        }
        AgentMessage::Assistant {
            content,
            tool_calls,
        } => {
            let mut parts: Vec<AssistantContent> =
                content.iter().filter_map(content_part_to_assistant).collect();

            for tc in tool_calls {
                parts.push(AssistantContent::ToolCall(
                    ToolCall::new(
                        tc.id.clone(),
                        ToolFunction::new(tc.name.clone(), tc.arguments.clone()),
                    )
                    .with_call_id(tc.id.clone()),
                ));
            }

            if parts.is_empty() {
                return None;
            }
            Some(Message::Assistant {
                id: None,
                content: OneOrMany::many(parts).unwrap_or_else(|_| unreachable!()),
            })
        }
        AgentMessage::ToolResult {
            tool_call_id,
            content,
            ..
        } => {
            let text = content
                .iter()
                .filter_map(ContentPart::extract_text)
                .collect::<Vec<_>>()
                .join("\n");

            Some(Message::User {
                content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                    id: tool_call_id.clone(),
                    call_id: Some(tool_call_id.clone()),
                    content: OneOrMany::one(ToolResultContent::text(text)),
                })),
            })
        }
    }
}

fn content_part_to_user(part: &ContentPart) -> Option<UserContent> {
    match part {
        ContentPart::Text { text } => Some(UserContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } => None,
    }
}

fn content_part_to_assistant(part: &ContentPart) -> Option<AssistantContent> {
    match part {
        ContentPart::Text { text } => Some(AssistantContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } => None,
    }
}

/// 从 rig 的 AssistantContent 列表构建 AgentMessage::Assistant
pub fn assistant_from_llm_content(content: &[AssistantContent]) -> AgentMessage {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in content {
        match item {
            AssistantContent::Text(t) => {
                text_parts.push(ContentPart::text(&t.text));
            }
            AssistantContent::ToolCall(tc) => {
                tool_calls.push(ToolCallInfo {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                });
            }
            AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {}
        }
    }

    AgentMessage::Assistant {
        content: text_parts,
        tool_calls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_user_message() {
        let msg = AgentMessage::user("你好世界");
        let llm = default_convert_to_llm(&[msg]);
        assert_eq!(llm.len(), 1);
        match &llm[0] {
            Message::User { content } => {
                let first = content.first();
                assert!(matches!(first, UserContent::Text(t) if t.text == "你好世界"));
            }
            _ => panic!("应该是 User 消息"),
        }
    }

    #[test]
    fn convert_assistant_with_tool_calls() {
        let content = vec![
            AssistantContent::text("让我帮你查一下"),
            AssistantContent::ToolCall(ToolCall::new(
                "tc_1".to_string(),
                ToolFunction::new("search".to_string(), serde_json::json!({"query": "rust"})),
            )),
        ];

        let msg = assistant_from_llm_content(&content);
        match &msg {
            AgentMessage::Assistant {
                content,
                tool_calls,
            } => {
                assert_eq!(content.len(), 1);
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "search");
            }
            _ => panic!("应该是 Assistant 消息"),
        }
    }
}
