use agentdash_agent_protocol::{AgentDashThreadItem, codex_app_server_protocol as codex};
use agentdash_agent_runtime_contract::{
    ContextBlock, DriverError, RuntimeInput, RuntimeItemContent,
};
use agentdash_agent_types::{AgentMessage, ContentPart};

pub(crate) fn inputs_to_message(input: Vec<RuntimeInput>) -> Result<AgentMessage, DriverError> {
    Ok(AgentMessage::user_parts(
        input
            .into_iter()
            .map(|input| {
                Ok(match input {
                    RuntimeInput::Text { text } => ContentPart::text(text),
                    RuntimeInput::Image {
                        mime_type,
                        data_url,
                    } => ContentPart::image(mime_type, data_url),
                    RuntimeInput::FileReference { .. } | RuntimeInput::Structured { .. } => {
                        return Err(DriverError::Unsupported {
                            reason: "native Agent Core accepts text and image input only"
                                .to_string(),
                        });
                    }
                })
            })
            .collect::<Result<Vec<_>, DriverError>>()?,
    ))
}

pub(crate) fn context_blocks_to_messages(
    blocks: &[ContextBlock],
) -> Result<Vec<AgentMessage>, DriverError> {
    let mut messages = Vec::new();
    for block in blocks {
        if let Some(message) = context_block_to_message(block)? {
            messages.push(message);
        }
    }
    Ok(messages)
}

fn context_block_to_message(block: &ContextBlock) -> Result<Option<AgentMessage>, DriverError> {
    Ok(match block {
        // Instruction blocks are projected through the dedicated system-instruction channel.
        ContextBlock::Instruction { .. } => None,
        ContextBlock::Input { input } => Some(inputs_to_message(input.clone())?),
        ContextBlock::CompactionSummary { summary } => {
            Some(AgentMessage::compaction_summary(summary, 0, 0))
        }
        ContextBlock::RuntimeItem { content } => match content.item() {
            AgentDashThreadItem::Codex(codex::ThreadItem::UserMessage { content, .. }) => {
                let mut parts = Vec::with_capacity(content.len());
                for part in content {
                    parts.push(match part {
                        codex::UserInput::Text { text, .. } => ContentPart::text(text.clone()),
                        codex::UserInput::Image { url, .. } => {
                            ContentPart::image("image/*", url.clone())
                        }
                        codex::UserInput::LocalImage { .. }
                        | codex::UserInput::Skill { .. }
                        | codex::UserInput::Mention { .. } => {
                            return Err(DriverError::Unsupported {
                                reason: "native context replay cannot flatten local image, skill, or mention input"
                                    .to_string(),
                            });
                        }
                    });
                }
                Some(AgentMessage::user_parts(parts))
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
            }) => {
                let (restored_content_items, output) = match content_items {
                    None => (Vec::new(), None),
                    Some(None) => (Vec::new(), Some(serde_json::Value::Null)),
                    Some(Some(items)) => (
                        items.clone(),
                        Some(serde_json::to_value(items).map_err(|error| {
                            DriverError::ProtocolViolation {
                                reason: format!(
                                    "native context dynamic tool content cannot serialize: {error}"
                                ),
                                critical: true,
                            }
                        })?),
                    ),
                };
                let content = restored_content_items
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
                    output,
                    success == &Some(Some(false)),
                ))
            }
            AgentDashThreadItem::Codex(_) | AgentDashThreadItem::AgentDash(_) => {
                return Err(DriverError::Unsupported {
                    reason: "native context replay encountered an unsupported typed thread item"
                        .to_string(),
                });
            }
        },
    })
}

pub(crate) fn message_content(
    message: &AgentMessage,
    item_id: &str,
) -> Result<RuntimeItemContent, DriverError> {
    if let Some(text) = message.first_text() {
        return Ok(RuntimeItemContent::agent_message(item_id, text));
    }
    if let AgentMessage::Assistant { content, .. } = message {
        let reasoning = content
            .iter()
            .map(|part| match part {
                ContentPart::Reasoning { text, .. } => Ok(text.as_str()),
                ContentPart::Text { .. } | ContentPart::Image { .. } => {
                    Err(DriverError::ProtocolViolation {
                        reason: "native Agent Core completed a non-text message with mixed content"
                            .to_string(),
                        critical: true,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !reasoning.is_empty() {
            return Ok(RuntimeItemContent::reasoning(item_id, reasoning.join("\n")));
        }
    }
    Err(DriverError::ProtocolViolation {
        reason: "native Agent Core completed a message without canonical text or reasoning content"
            .to_string(),
        critical: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_input_mapping_rejects_unowned_modalities_without_flattening() {
        for input in [
            RuntimeInput::FileReference {
                uri: "workspace://notes.md".to_string(),
                media_type: Some("text/markdown".to_string()),
            },
            RuntimeInput::Structured {
                schema: "example".to_string(),
                value: serde_json::json!({"value": 1}),
            },
        ] {
            assert!(matches!(
                inputs_to_message(vec![input]),
                Err(DriverError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn native_message_mapping_rejects_missing_text_instead_of_emitting_empty_content() {
        let image_only = AgentMessage::user_parts(vec![ContentPart::image(
            "image/png",
            "data:image/png;base64,AA==",
        )]);

        assert!(matches!(
            message_content(&image_only, "item-1"),
            Err(DriverError::ProtocolViolation { critical: true, .. })
        ));
    }
}
