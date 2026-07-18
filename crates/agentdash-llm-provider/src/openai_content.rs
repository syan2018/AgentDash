use agentdash_agent_core::types::ContentPart;

pub(super) fn responses_input_items(content: &[ContentPart]) -> Vec<serde_json::Value> {
    content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => {
                Some(serde_json::json!({ "type": "input_text", "text": text }))
            }
            ContentPart::Image { mime_type, data } => responses_input_image(mime_type, data),
            ContentPart::Reasoning { .. } => None,
        })
        .collect()
}

pub(super) fn chat_user_content(content: &[ContentPart]) -> Option<serde_json::Value> {
    let has_image = content
        .iter()
        .any(|part| matches!(part, ContentPart::Image { .. }));
    if !has_image {
        let text = content
            .iter()
            .filter_map(ContentPart::extract_text)
            .collect::<Vec<_>>()
            .join("\n");
        return (!text.is_empty()).then_some(serde_json::Value::String(text));
    }

    let items = content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => Some(serde_json::json!({
                "type": "text",
                "text": text,
            })),
            ContentPart::Image { mime_type, data } => chat_image_url(mime_type, data),
            ContentPart::Reasoning { .. } => None,
        })
        .collect::<Vec<_>>();
    (!items.is_empty()).then_some(serde_json::Value::Array(items))
}

pub(super) fn tool_result_output_text(content: &[ContentPart]) -> String {
    let text = content
        .iter()
        .filter_map(ContentPart::extract_text)
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty()
        && content
            .iter()
            .any(|part| matches!(part, ContentPart::Image { .. }))
    {
        "[image output attached in following user message]".to_string()
    } else {
        text
    }
}

pub(super) fn responses_tool_result_output(content: &[ContentPart]) -> serde_json::Value {
    let items = responses_input_items(content);
    if items.iter().any(|item| item["type"] == "input_image") {
        serde_json::Value::Array(items)
    } else {
        let text = content
            .iter()
            .filter_map(ContentPart::extract_text)
            .collect::<Vec<_>>()
            .join("\n");
        let has_image = content
            .iter()
            .any(|part| matches!(part, ContentPart::Image { .. }));
        if text.is_empty() && has_image {
            serde_json::Value::String("[image output omitted: invalid image data]".to_string())
        } else {
            serde_json::Value::String(text)
        }
    }
}

pub(super) fn chat_tool_result_image_followup(
    tool_name: Option<&str>,
    content: &[ContentPart],
) -> Option<serde_json::Value> {
    let mut items = content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Image { mime_type, data } => chat_image_url(mime_type, data),
            ContentPart::Text { .. } | ContentPart::Reasoning { .. } => None,
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    items.insert(
        0,
        serde_json::json!({
            "type": "text",
            "text": tool_image_label_text(tool_name),
        }),
    );
    Some(serde_json::json!({ "role": "user", "content": items }))
}

fn tool_image_label_text(tool_name: Option<&str>) -> String {
    let label = tool_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tool_result");
    format!("Image output from tool result: {label}")
}

fn responses_input_image(mime_type: &str, data: &str) -> Option<serde_json::Value> {
    image_data_url(mime_type, data).map(|image_url| {
        serde_json::json!({
            "type": "input_image",
            "image_url": image_url,
        })
    })
}

fn chat_image_url(mime_type: &str, data: &str) -> Option<serde_json::Value> {
    image_data_url(mime_type, data).map(|url| {
        serde_json::json!({
            "type": "image_url",
            "image_url": { "url": url },
        })
    })
}

fn image_data_url(mime_type: &str, data: &str) -> Option<String> {
    let mime_type = mime_type.trim();
    let data = data.trim();
    if mime_type.is_empty() || data.is_empty() {
        return None;
    }
    Some(format!("data:{mime_type};base64,{data}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_input_items_keep_images() {
        let items = responses_input_items(&[
            ContentPart::text("metadata"),
            ContentPart::Image {
                mime_type: "image/png".to_string(),
                data: "AAECAw==".to_string(),
            },
        ]);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "input_text");
        assert_eq!(items[1]["type"], "input_image");
        assert_eq!(items[1]["image_url"], "data:image/png;base64,AAECAw==");
    }

    #[test]
    fn responses_tool_result_output_uses_native_image_array() {
        let output = responses_tool_result_output(&[
            ContentPart::text("metadata"),
            ContentPart::Image {
                mime_type: "image/png".to_string(),
                data: "AAECAw==".to_string(),
            },
        ]);

        let items = output.as_array().expect("output array");
        assert_eq!(items[0]["type"], "input_text");
        assert_eq!(items[0]["text"], "metadata");
        assert_eq!(items[1]["type"], "input_image");
        assert_eq!(items[1]["image_url"], "data:image/png;base64,AAECAw==");
    }

    #[test]
    fn chat_user_content_uses_multimodal_array_when_image_exists() {
        let content = chat_user_content(&[
            ContentPart::text("metadata"),
            ContentPart::Image {
                mime_type: "image/png".to_string(),
                data: "AAECAw==".to_string(),
            },
        ])
        .expect("content");

        let items = content.as_array().expect("array content");
        assert_eq!(items[0]["type"], "text");
        assert_eq!(items[1]["type"], "image_url");
        assert_eq!(
            items[1]["image_url"]["url"],
            "data:image/png;base64,AAECAw=="
        );
    }
}
