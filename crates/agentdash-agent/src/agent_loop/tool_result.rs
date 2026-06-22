#![allow(dead_code)]

use crate::types::{AgentToolResult, ContentPart};

pub(super) const AGENT_TOOL_RESULT_FINAL_INLINE_BYTE_CAP: usize = 64 * 1024;
pub(super) const AGENT_TOOL_RESULT_UPDATE_INLINE_BYTE_CAP: usize = 8 * 1024;
pub(super) const AGENT_TOOL_RESULT_TRUNCATION_POLICY: &str = "head_tail";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentToolResultInlineKind {
    Final,
    Update,
}

impl AgentToolResultInlineKind {
    pub(super) fn inline_byte_cap(self) -> usize {
        match self {
            Self::Final => AGENT_TOOL_RESULT_FINAL_INLINE_BYTE_CAP,
            Self::Update => AGENT_TOOL_RESULT_UPDATE_INLINE_BYTE_CAP,
        }
    }
}

#[derive(Debug)]
pub(super) struct AgentToolResultCacheWrite<'a> {
    pub item_id: &'a str,
    pub lifecycle_path: &'a str,
    pub text: &'a str,
    pub original_bytes: usize,
}

pub(super) fn lifecycle_path_for_tool_result(item_id: &str) -> String {
    format!("lifecycle://session/tool-results/{item_id}/result.txt")
}

pub(super) fn bound_agent_tool_result_text<F>(
    result: &AgentToolResult,
    item_id: &str,
    inline_kind: AgentToolResultInlineKind,
    mut write_cache: F,
) -> AgentToolResult
where
    F: FnMut(AgentToolResultCacheWrite<'_>),
{
    let cap = inline_kind.inline_byte_cap();
    let Some(boundable_content) = boundable_tool_result_content(result, cap) else {
        return result.clone();
    };

    let original_text = boundable_content.text;
    let original_bytes = original_text.len();
    let lifecycle_path = lifecycle_path_for_tool_result(item_id);
    write_cache(AgentToolResultCacheWrite {
        item_id,
        lifecycle_path: &lifecycle_path,
        text: &original_text,
        original_bytes,
    });

    let bounded_text = bounded_tool_result_text(&original_text, cap, &lifecycle_path);
    let inline_bytes = bounded_text.len();
    let omitted_bytes = original_bytes.saturating_sub(inline_bytes);
    let details = merge_truncation_details(
        result.details.clone(),
        &lifecycle_path,
        original_bytes,
        inline_bytes,
        omitted_bytes,
    );
    let content = if boundable_content.replace_all_content {
        vec![ContentPart::text(bounded_text)]
    } else {
        replace_text_content_with_preview(&result.content, bounded_text)
    };

    AgentToolResult {
        content,
        is_error: result.is_error,
        details: Some(details),
    }
}

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

fn joined_text_content(content: &[ContentPart]) -> Option<String> {
    let mut text_parts = content.iter().filter_map(ContentPart::extract_text);
    let first = text_parts.next()?;
    let mut joined = first.to_string();
    for text in text_parts {
        joined.push('\n');
        joined.push_str(text);
    }
    Some(joined)
}

struct BoundableToolResultContent {
    text: String,
    replace_all_content: bool,
}

fn boundable_tool_result_content(
    result: &AgentToolResult,
    cap: usize,
) -> Option<BoundableToolResultContent> {
    let joined_text = joined_text_content(&result.content);
    let text_bytes = joined_text.as_ref().map_or(0, String::len);
    let has_non_text_content = result
        .content
        .iter()
        .any(|part| !matches!(part, ContentPart::Text { .. }));
    let serialized_content = if has_non_text_content {
        serde_json::to_string(&result.content).ok()
    } else {
        None
    };
    let serialized_bytes = serialized_content.as_ref().map_or(0, String::len);

    if text_bytes <= cap && serialized_bytes <= cap {
        return None;
    }

    if has_non_text_content && serialized_bytes > cap {
        return Some(BoundableToolResultContent {
            text: serialized_content.unwrap_or_else(|| {
                format!(
                    "[non-text tool result exceeded inline cap: {serialized_bytes} serialized bytes]"
                )
            }),
            replace_all_content: true,
        });
    }

    joined_text.map(|text| BoundableToolResultContent {
        text,
        replace_all_content: false,
    })
}

fn replace_text_content_with_preview(content: &[ContentPart], preview: String) -> Vec<ContentPart> {
    let mut replaced_text = false;
    let mut bounded = Vec::with_capacity(content.len());
    for part in content {
        match part {
            ContentPart::Text { .. } if !replaced_text => {
                bounded.push(ContentPart::text(preview.clone()));
                replaced_text = true;
            }
            ContentPart::Text { .. } => {}
            other => bounded.push(other.clone()),
        }
    }

    if replaced_text {
        bounded
    } else {
        vec![ContentPart::text(preview)]
    }
}

fn bounded_tool_result_text(original: &str, cap: usize, lifecycle_path: &str) -> String {
    let header = format!(
        "[tool result truncated]\nlifecycle_path: {lifecycle_path}\npolicy: {AGENT_TOOL_RESULT_TRUNCATION_POLICY}\n\n",
    );
    if cap <= header.len() {
        return utf8_prefix_at_most(&header, cap).to_string();
    }

    let body_cap = cap - header.len();
    let omitted_hint = format!(
        "\n[... omitted {} bytes ...]\n",
        original.len().saturating_sub(body_cap)
    );
    let body = utf8_head_tail_preview(original, body_cap, &omitted_hint);
    let mut bounded = header;
    bounded.push_str(&body);
    if bounded.len() > cap {
        bounded.truncate(previous_char_boundary(&bounded, cap));
    }
    bounded
}

fn utf8_head_tail_preview(original: &str, cap: usize, omitted_hint: &str) -> String {
    if original.len() <= cap {
        return original.to_string();
    }
    if cap == 0 {
        return String::new();
    }
    if omitted_hint.len() >= cap {
        return utf8_prefix_at_most(original, cap).to_string();
    }

    let available = cap - omitted_hint.len();
    let head_cap = available / 2;
    let tail_cap = available - head_cap;
    let head = utf8_prefix_at_most(original, head_cap);
    let tail = utf8_suffix_at_most(original, tail_cap);
    let mut preview = String::with_capacity(head.len() + omitted_hint.len() + tail.len());
    preview.push_str(head);
    preview.push_str(omitted_hint);
    preview.push_str(tail);
    preview
}

fn utf8_prefix_at_most(value: &str, max_bytes: usize) -> &str {
    let end = previous_char_boundary(value, max_bytes.min(value.len()));
    &value[..end]
}

fn utf8_suffix_at_most(value: &str, max_bytes: usize) -> &str {
    if max_bytes >= value.len() {
        return value;
    }
    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}

fn previous_char_boundary(value: &str, max_bytes: usize) -> usize {
    let mut boundary = max_bytes.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn merge_truncation_details(
    existing: Option<serde_json::Value>,
    lifecycle_path: &str,
    original_bytes: usize,
    inline_bytes: usize,
    omitted_bytes: usize,
) -> serde_json::Value {
    let truncation = serde_json::json!({
        "truncated": true,
        "original_bytes": original_bytes,
        "inline_bytes": inline_bytes,
        "omitted_bytes": omitted_bytes,
        "policy": AGENT_TOOL_RESULT_TRUNCATION_POLICY,
    });

    match existing {
        Some(serde_json::Value::Object(mut object)) => {
            object.insert("truncation".to_string(), truncation);
            object.insert(
                "lifecycle_path".to_string(),
                serde_json::Value::String(lifecycle_path.to_string()),
            );
            serde_json::Value::Object(object)
        }
        Some(value) => serde_json::json!({
            "original_details": value,
            "truncation": truncation,
            "lifecycle_path": lifecycle_path,
        }),
        None => serde_json::json!({
            "truncation": truncation,
            "lifecycle_path": lifecycle_path,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "AGENTDASH_LARGE_TOOL_RESULT_SENTINEL";

    #[test]
    fn bounded_tool_result_keeps_utf8_and_hides_middle_sentinel() {
        let content = format!("{}{}{}", "前".repeat(40_000), SENTINEL, "尾".repeat(40_000));
        let result = AgentToolResult {
            content: vec![ContentPart::text(content)],
            is_error: false,
            details: Some(serde_json::json!({
                "runtime_trace": { "span_id": "span-1" },
                "approval_state": "approved",
            })),
        };
        let mut cached = None;

        let bounded = bound_agent_tool_result_text(
            &result,
            "item-1",
            AgentToolResultInlineKind::Final,
            |write| {
                cached = Some((write.item_id.to_string(), write.text.to_string()));
            },
        );

        let text = bounded.content[0].extract_text().unwrap();
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
        assert!(text.len() <= AGENT_TOOL_RESULT_FINAL_INLINE_BYTE_CAP);
        assert!(!text.contains(SENTINEL));
        assert_eq!(bounded.is_error, result.is_error);
        assert_eq!(cached.as_ref().unwrap().0, "item-1");
        assert!(cached.as_ref().unwrap().1.contains(SENTINEL));

        let details = bounded.details.as_ref().unwrap();
        assert_eq!(
            details["lifecycle_path"],
            "lifecycle://session/tool-results/item-1/result.txt"
        );
        assert_eq!(details["truncation"]["truncated"], true);
        assert_eq!(
            details["truncation"]["policy"],
            AGENT_TOOL_RESULT_TRUNCATION_POLICY
        );
        assert!(details["truncation"]["original_bytes"].as_u64().unwrap() > 0);
        assert!(details["truncation"]["inline_bytes"].as_u64().unwrap() > 0);
        assert!(details["truncation"]["omitted_bytes"].as_u64().unwrap() > 0);
        assert_eq!(details["runtime_trace"]["span_id"], "span-1");
        assert_eq!(details["approval_state"], "approved");
    }

    #[test]
    fn small_tool_result_is_unchanged_and_not_cached() {
        let result = AgentToolResult {
            content: vec![ContentPart::text("small")],
            is_error: true,
            details: None,
        };
        let mut cache_writes = 0;

        let bounded = bound_agent_tool_result_text(
            &result,
            "item-2",
            AgentToolResultInlineKind::Update,
            |_| {
                cache_writes += 1;
            },
        );

        assert_eq!(bounded, result);
        assert_eq!(cache_writes, 0);
    }

    #[test]
    fn non_object_details_are_preserved() {
        let result = AgentToolResult {
            content: vec![ContentPart::text(
                "x".repeat(AGENT_TOOL_RESULT_UPDATE_INLINE_BYTE_CAP + 1),
            )],
            is_error: false,
            details: Some(serde_json::json!("raw-details")),
        };

        let bounded = bound_agent_tool_result_text(
            &result,
            "item-3",
            AgentToolResultInlineKind::Update,
            |_| {},
        );

        let details = bounded.details.as_ref().unwrap();
        assert_eq!(details["original_details"], "raw-details");
        assert_eq!(details["truncation"]["truncated"], true);
    }

    #[test]
    fn oversized_non_text_content_is_replaced_with_text_preview() {
        let result = AgentToolResult {
            content: vec![ContentPart::image(
                "image/png",
                format!("{}{}{}", "a".repeat(10_000), SENTINEL, "z".repeat(10_000)),
            )],
            is_error: false,
            details: None,
        };

        let bounded = bound_agent_tool_result_text(
            &result,
            "image-result",
            AgentToolResultInlineKind::Update,
            |_| {},
        );

        assert_eq!(bounded.content.len(), 1);
        let text = bounded.content[0].extract_text().unwrap();
        assert!(text.len() <= AGENT_TOOL_RESULT_UPDATE_INLINE_BYTE_CAP);
        assert!(text.contains("tool result truncated"));
        assert!(!matches!(bounded.content[0], ContentPart::Image { .. }));
        assert_eq!(
            bounded.details.as_ref().unwrap()["lifecycle_path"],
            "lifecycle://session/tool-results/image-result/result.txt"
        );
    }
}
