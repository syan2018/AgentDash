use agentdash_agent_protocol::{AgentDashThreadItem, codex_app_server_protocol as codex};
use agentdash_agent_runtime_contract::{ContextBlock, RuntimeInput};
use agentdash_agent_types::{AgentMessage, ContentPart};

pub(crate) fn inputs_to_message(input: Vec<RuntimeInput>) -> AgentMessage {
    AgentMessage::user_parts(
        input
            .into_iter()
            .map(|input| match input {
                RuntimeInput::Text { text } => ContentPart::text(text),
                RuntimeInput::Image {
                    mime_type,
                    data_url,
                } => ContentPart::image(mime_type, data_url),
                RuntimeInput::FileReference { uri, media_type } => ContentPart::text(format!(
                    "[file_reference uri={uri} media_type={}]",
                    media_type.unwrap_or_default()
                )),
                RuntimeInput::Structured { schema, value } => {
                    ContentPart::text(format!("[structured schema={schema}] {value}"))
                }
            })
            .collect(),
    )
}

pub(crate) fn context_blocks_to_messages(blocks: &[ContextBlock]) -> Vec<AgentMessage> {
    blocks.iter().filter_map(context_block_to_message).collect()
}

fn context_block_to_message(block: &ContextBlock) -> Option<AgentMessage> {
    match block {
        ContextBlock::Instruction { .. } => None,
        ContextBlock::Input { input } => Some(inputs_to_message(input.clone())),
        ContextBlock::CompactionSummary { summary } => {
            Some(AgentMessage::compaction_summary(summary, 0, 0))
        }
        ContextBlock::RuntimeItem { content } => match content.item() {
            AgentDashThreadItem::Codex(codex::ThreadItem::UserMessage { content, .. }) => {
                let text = content
                    .iter()
                    .filter_map(|part| {
                        let value = serde_json::to_value(part).ok()?;
                        value
                            .get("text")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(AgentMessage::user(text))
            }
            AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { text, .. }) => {
                Some(AgentMessage::assistant(text))
            }
            AgentDashThreadItem::Codex(codex::ThreadItem::Reasoning {
                summary, content, ..
            }) => Some(AgentMessage::assistant(
                summary
                    .iter()
                    .chain(content)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
            AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
                tool,
                content_items,
                success,
                ..
            }) if content_items.is_some() => {
                let output = serde_json::to_value(content_items).unwrap_or(serde_json::Value::Null);
                let content = content_items
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| match item {
                        codex::DynamicToolCallOutputContentItem::InputText { text } => {
                            ContentPart::text(text)
                        }
                        codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                            ContentPart::image("image/*", image_url)
                        }
                    })
                    .collect();
                Some(AgentMessage::tool_result_full(
                    format!("restored-{tool}"),
                    None,
                    Some(tool.clone()),
                    content,
                    Some(output.clone()),
                    success == &Some(false),
                ))
            }
            AgentDashThreadItem::Codex(_) | AgentDashThreadItem::AgentDash(_) => None,
        },
    }
}

pub(crate) fn message_text(message: &AgentMessage) -> String {
    message.first_text().unwrap_or_default().to_string()
}
