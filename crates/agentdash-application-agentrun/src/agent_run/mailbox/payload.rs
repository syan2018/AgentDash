use super::*;

pub(super) fn message_input(
    message: &AgentRunMailboxMessage,
) -> Result<Vec<UserInputBlock>, WorkflowApplicationError> {
    let Some(payload) = message.payload_json.clone() else {
        return Err(WorkflowApplicationError::Conflict(format!(
            "mailbox message {} 缺少 payload",
            message.id
        )));
    };
    serde_json::from_value(payload).map_err(|error| {
        WorkflowApplicationError::BadRequest(format!(
            "mailbox message {} payload 无效: {error}",
            message.id
        ))
    })
}

pub(super) fn message_executor_config(
    message: &AgentRunMailboxMessage,
) -> Result<Option<AgentConfig>, WorkflowApplicationError> {
    message
        .executor_config_json
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApplicationError::BadRequest(format!(
                "mailbox message {} executor_config 无效: {error}",
                message.id
            ))
        })
}

pub(super) fn message_launch_planning_input(
    message: &AgentRunMailboxMessage,
) -> Result<agentdash_application_ports::launch::LaunchPlanningInput, WorkflowApplicationError> {
    message
        .launch_planning_input
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApplicationError::BadRequest(format!(
                "mailbox message {} launch_planning_input 无效: {error}",
                message.id
            ))
        })
        .map(Option::unwrap_or_default)
}

pub(super) fn build_input_preview(input: &[UserInputBlock]) -> String {
    input
        .iter()
        .find_map(|block| match block {
            UserInputBlock::Text { text, .. } => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(truncate_preview(trimmed, 80))
                }
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let preview = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!(
            "{}...",
            preview
                .chars()
                .take(max_chars.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        preview
    }
}

pub(super) fn input_has_images(input: &[UserInputBlock]) -> bool {
    input
        .iter()
        .any(|block| matches!(block, UserInputBlock::Image { .. }))
}

pub(super) fn agent_message_to_user_input_blocks(message: &AgentMessage) -> Vec<UserInputBlock> {
    match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_part_to_user_input)
            .collect(),
        AgentMessage::CompactionSummary { summary, .. } => {
            text_to_user_input(summary).into_iter().collect()
        }
    }
}

fn content_part_to_user_input(part: &ContentPart) -> Option<UserInputBlock> {
    match part {
        ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => {
            text_to_user_input(text)
        }
        ContentPart::Image { mime_type, data } => Some(UserInputBlock::Image {
            detail: None,
            url: format!("data:{mime_type};base64,{data}"),
        }),
    }
}

fn text_to_user_input(text: &str) -> Option<UserInputBlock> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(UserInputBlock::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        })
    }
}

pub(super) fn serialization_error(
    label: &'static str,
) -> impl FnOnce(serde_json::Error) -> WorkflowApplicationError {
    move |error| WorkflowApplicationError::BadRequest(format!("{label} 无法序列化: {error}"))
}
