use rig::OneOrMany;
use rig::completion::message::{AssistantContent, Message, Reasoning, Text, UserContent};

use crate::types::{AgentMessage, ContentPart, ToolCallInfo};

/// 默认 convert_to_llm：将 AgentMessage 列表转为 rig::Message 列表。
pub fn default_convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages.iter().filter_map(agent_to_llm).collect()
}

fn agent_to_llm(msg: &AgentMessage) -> Option<Message> {
    match msg {
        AgentMessage::User { content, .. } => {
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
            ..
        } => {
            let mut parts: Vec<AssistantContent> = content
                .iter()
                .filter_map(content_part_to_assistant)
                .collect();

            for tc in tool_calls {
                let call_id = tc.call_id.clone().unwrap_or_else(|| tc.id.clone());
                parts.push(AssistantContent::tool_call_with_call_id(
                    tc.id.clone(),
                    call_id,
                    tc.name.clone(),
                    tc.arguments.clone(),
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
            call_id,
            content,
            ..
        } => {
            let text = content
                .iter()
                .filter_map(ContentPart::extract_text)
                .collect::<Vec<_>>()
                .join("\n");

            Some(Message::tool_result_with_call_id(
                tool_call_id.clone(),
                call_id.clone().or_else(|| Some(tool_call_id.clone())),
                text,
            ))
        }
    }
}

fn content_part_to_user(part: &ContentPart) -> Option<UserContent> {
    match part {
        ContentPart::Text { text } => Some(UserContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } => None,
        ContentPart::Reasoning { .. } => None,
    }
}

fn content_part_to_assistant(part: &ContentPart) -> Option<AssistantContent> {
    match part {
        ContentPart::Text { text } => Some(AssistantContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } => None,
        ContentPart::Reasoning {
            text,
            id,
            signature,
        } => Some(AssistantContent::Reasoning(
            Reasoning::new(text)
                .optional_id(id.clone())
                .with_signature(signature.clone()),
        )),
    }
}

/// 从 rig 的 AssistantContent 列表构建 AgentMessage::Assistant。
pub fn assistant_from_llm_content(content: &[AssistantContent]) -> AgentMessage {
    let mut parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in content {
        match item {
            AssistantContent::Text(t) => {
                parts.push(ContentPart::text(&t.text));
            }
            AssistantContent::ToolCall(tc) => {
                tool_calls.push(ToolCallInfo {
                    id: tc.id.clone(),
                    call_id: tc.call_id.clone().or_else(|| Some(tc.id.clone())),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                });
            }
            AssistantContent::Reasoning(reasoning) => {
                let text = reasoning.reasoning.join("");
                parts.push(ContentPart::reasoning(
                    text,
                    reasoning.id.clone(),
                    reasoning.signature.clone(),
                ));
            }
        }
    }

    AgentMessage::Assistant {
        content: parts,
        tool_calls,
        stop_reason: None,
        error_message: None,
        usage: None,
        timestamp: Some(crate::types::now_millis()),
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
    fn convert_assistant_with_tool_calls_and_reasoning() {
        let content = vec![
            AssistantContent::Reasoning(
                Reasoning::new("思考中")
                    .with_id("r1".to_string())
                    .with_signature(Some("sig".to_string())),
            ),
            AssistantContent::text("让我帮你查一下"),
            AssistantContent::tool_call("tc_1", "search", serde_json::json!({"query": "rust"})),
        ];

        let msg = assistant_from_llm_content(&content);
        match &msg {
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                assert_eq!(content.len(), 2);
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "search");
                assert!(matches!(
                    &content[0],
                    ContentPart::Reasoning { text, .. } if text == "思考中"
                ));
            }
            _ => panic!("应该是 Assistant 消息"),
        }
    }
}
