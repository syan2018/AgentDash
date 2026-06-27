use agentdash_agent_types::{AgentMessage, ContentPart, ProjectedTranscript};
use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

/// 把 `ProjectedTranscript` 组装为 `ContextFrame(kind=continuation_context)`。
pub fn build_continuation_context_frame(
    transcript: &ProjectedTranscript,
    owner_context: Option<&str>,
) -> Option<ContextFrame> {
    let owner_context = owner_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let transcript_markdown = build_continuation_transcript_markdown(transcript);
    if transcript_markdown.is_none() && owner_context.is_none() {
        return None;
    }

    let mut rendered_sections = Vec::new();
    if let Some(owner) = owner_context.as_deref() {
        rendered_sections.push(format!("## Owner Context\n\n{owner}"));
    }
    if let Some(transcript_body) = transcript_markdown.as_deref() {
        rendered_sections.push(format!("## Session Continuation\n\n{transcript_body}"));
    }

    let created_at_ms = chrono::Utc::now().timestamp_millis();
    let summary = if transcript.is_empty() {
        "当前会话暂无可恢复的历史事件，已保留 owner 上下文。".to_string()
    } else {
        format!("从会话仓储恢复 {} 条历史消息。", transcript.entries.len())
    };

    Some(ContextFrame {
        id: format!("continuation-context-{created_at_ms}"),
        kind: "continuation_context".to_string(),
        source: RuntimeEventSource::RuntimeContextUpdate,
        phase_node: None,
        apply_mode: None,
        delivery_status: "prepared_for_connector".to_string(),
        delivery_channel: "connector_context".to_string(),
        message_role: "system".to_string(),
        rendered_text: rendered_sections.join("\n\n"),
        sections: vec![ContextFrameSection::ContinuationContext {
            title: "Session Continuation".to_string(),
            summary,
            owner_context,
            transcript_markdown: transcript_markdown.unwrap_or_default(),
        }],
        created_at_ms,
    })
}

fn build_continuation_transcript_markdown(transcript: &ProjectedTranscript) -> Option<String> {
    if transcript.is_empty() {
        return None;
    }

    let mut history_lines = Vec::new();
    history_lines.push(
        "以下内容由 session 仓储事件重建，用于在当前进程缺少 live runtime 时恢复连续会话语义。请将其视为本 session 已经发生过的事实，并在此基础上继续处理新的用户输入。"
            .to_string(),
    );

    history_lines.push(String::new());
    history_lines.push("### Transcript".to_string());
    for entry in &transcript.entries {
        match &entry.message {
            AgentMessage::User { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                history_lines.push("#### 用户".to_string());
                history_lines.push(text);
            }
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let mut text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                if !tool_calls.is_empty() {
                    let tool_lines = tool_calls
                        .iter()
                        .map(|tool_call| {
                            format!(
                                "- {}({})",
                                tool_call.name,
                                continuation_json_preview(&tool_call.arguments)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str("工具调用：\n");
                    text.push_str(&tool_lines);
                }
                history_lines.push("#### 助手".to_string());
                history_lines.push(text);
            }
            AgentMessage::ToolResult {
                tool_name,
                content,
                details,
                is_error,
                ..
            } => {
                let mut text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                if text.is_empty()
                    && let Some(details) = details.as_ref()
                {
                    text = continuation_json_preview(details);
                }
                if let Some(summary) = details.as_ref().and_then(render_truncation_summary) {
                    if !text.trim().is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(&summary);
                }
                history_lines.push(format!(
                    "#### 工具结果 ({})",
                    tool_name.as_deref().unwrap_or("tool_result")
                ));
                if *is_error {
                    history_lines.push(format!("[error]\n{text}"));
                } else {
                    history_lines.push(text);
                }
            }
            AgentMessage::CompactionSummary { summary, .. } => {
                history_lines.push("#### 历史摘要".to_string());
                history_lines.push(summary.clone());
            }
        }
        history_lines.push(String::new());
    }

    Some(history_lines.join("\n"))
}

fn continuation_json_preview(value: &serde_json::Value) -> String {
    const MAX_LEN: usize = 320;
    let rendered = value.to_string();
    if rendered.len() <= MAX_LEN {
        rendered
    } else {
        let shortened: String = rendered.chars().take(MAX_LEN).collect();
        format!("{shortened}...")
    }
}

fn render_truncation_summary(details: &serde_json::Value) -> Option<String> {
    let truncation = find_truncation_metadata(details)?;
    let truncated = truncation
        .get("truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !truncated {
        return None;
    }

    let lifecycle_path = details
        .get("lifecycle_path")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let policy = truncation
        .get("policy")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let original_bytes = truncation
        .get("original_bytes")
        .and_then(serde_json::Value::as_u64);
    let inline_bytes = truncation
        .get("inline_bytes")
        .and_then(serde_json::Value::as_u64);
    let omitted_bytes = truncation
        .get("omitted_bytes")
        .and_then(serde_json::Value::as_u64);

    let mut lines = Vec::new();
    lines.push("[tool result truncated]".to_string());
    if let Some(path) = lifecycle_path {
        lines.push(format!("lifecycle_path: {path}"));
    }
    if let Some(policy) = policy {
        lines.push(format!("policy: {policy}"));
    }
    if let Some(bytes) = original_bytes {
        lines.push(format!("original_bytes: {bytes}"));
    }
    if let Some(bytes) = inline_bytes {
        lines.push(format!("inline_bytes: {bytes}"));
    }
    if let Some(bytes) = omitted_bytes {
        lines.push(format!("omitted_bytes: {bytes}"));
    }
    Some(lines.join("\n"))
}

fn find_truncation_metadata(value: &serde_json::Value) -> Option<&serde_json::Value> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(truncation) = object.get("truncation")
                && truncation.is_object()
            {
                return Some(truncation);
            }
            object.values().find_map(find_truncation_metadata)
        }
        serde_json::Value::Array(values) => values.iter().find_map(find_truncation_metadata),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_summary_uses_details_without_result_body() {
        let details = serde_json::json!({
            "lifecycle_path": "lifecycle://session/tool-results/turn_001/tool_001/result.txt",
            "truncation": {
                "truncated": true,
                "original_bytes": 123456,
                "inline_bytes": 4096,
                "omitted_bytes": 119360,
                "policy": "head_tail"
            }
        });

        let summary = render_truncation_summary(&details).expect("summary");

        assert!(summary.contains("lifecycle://session/tool-results/turn_001/tool_001/result.txt"));
        assert!(summary.contains("policy: head_tail"));
        assert!(summary.contains("original_bytes: 123456"));
        assert!(!summary.contains("result.txt body"));
    }
}
