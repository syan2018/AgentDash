use agentdash_agent_runtime_contract::{ContextBlock, RuntimeInput, RuntimeItemContent};
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
        ContextBlock::RuntimeItem { content } => match content {
            RuntimeItemContent::UserMessage { input } => Some(inputs_to_message(input.clone())),
            RuntimeItemContent::AgentMessage { text } | RuntimeItemContent::Reasoning { text } => {
                Some(AgentMessage::assistant(text))
            }
            RuntimeItemContent::ToolResult { name, output } => {
                Some(AgentMessage::tool_result_full(
                    format!("restored-{name}"),
                    None,
                    Some(name.clone()),
                    vec![ContentPart::text(output.to_string())],
                    Some(output.clone()),
                    false,
                ))
            }
            RuntimeItemContent::ToolCall { .. }
            | RuntimeItemContent::SystemContextChange { .. }
            | RuntimeItemContent::ContextCompaction { .. }
            | RuntimeItemContent::Plan { .. } => None,
        },
    }
}

pub(crate) fn message_text(message: &AgentMessage) -> String {
    message.first_text().unwrap_or_default().to_string()
}
