use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, StreamChunk, ToolCallDeltaContent,
};
use agentdash_agent::types::{
    AgentMessage, ContentPart, StopReason, TokenUsage, ToolCallInfo, now_millis,
};

use super::openai_content::{responses_input_items, responses_tool_result_output};
use super::sse::SseParser;

pub(super) enum ResponsesSystemPromptMode {
    InputMessage,
    Instructions { default_instructions: &'static str },
}

pub(super) struct ResponsesRequestOptions {
    pub system_prompt_mode: ResponsesSystemPromptMode,
    pub tool_strict: Option<bool>,
    pub text: Option<serde_json::Value>,
    pub include: Option<serde_json::Value>,
    pub tool_choice: Option<serde_json::Value>,
    pub parallel_tool_calls: Option<bool>,
}

impl ResponsesRequestOptions {
    pub(super) fn openai_api() -> Self {
        Self {
            system_prompt_mode: ResponsesSystemPromptMode::InputMessage,
            tool_strict: None,
            text: None,
            include: None,
            tool_choice: None,
            parallel_tool_calls: None,
        }
    }

    pub(super) fn codex() -> Self {
        Self {
            system_prompt_mode: ResponsesSystemPromptMode::Instructions {
                default_instructions: "You are a helpful assistant.",
            },
            tool_strict: Some(false),
            text: Some(serde_json::json!({ "verbosity": "low" })),
            include: Some(serde_json::json!(["reasoning.encrypted_content"])),
            tool_choice: Some(serde_json::json!("auto")),
            parallel_tool_calls: Some(true),
        }
    }
}

pub(super) fn build_responses_request_body(
    model_id: &str,
    request: &BridgeRequest,
    options: ResponsesRequestOptions,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model_id,
        "store": false,
        "stream": true,
        "input": convert_responses_input(
            request,
            matches!(options.system_prompt_mode, ResponsesSystemPromptMode::InputMessage),
        ),
    });

    if let ResponsesSystemPromptMode::Instructions {
        default_instructions,
    } = options.system_prompt_mode
    {
        body["instructions"] = serde_json::json!(
            request
                .system_prompt
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or(default_instructions)
        );
    }

    if let Some(text) = options.text {
        body["text"] = text;
    }
    if let Some(include) = options.include {
        body["include"] = include;
    }
    if let Some(tool_choice) = options.tool_choice {
        body["tool_choice"] = tool_choice;
    }
    if let Some(parallel_tool_calls) = options.parallel_tool_calls {
        body["parallel_tool_calls"] = serde_json::json!(parallel_tool_calls);
    }

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|tool| {
                let mut value = serde_json::json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                });
                if let Some(strict) = options.tool_strict {
                    value["strict"] = serde_json::json!(strict);
                }
                value
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    body
}

fn convert_responses_input(
    request: &BridgeRequest,
    include_system_input: bool,
) -> Vec<serde_json::Value> {
    let mut input = Vec::new();

    if include_system_input
        && let Some(ref system_prompt) = request.system_prompt
        && !system_prompt.is_empty()
    {
        input.push(serde_json::json!({ "role": "system", "content": system_prompt }));
    }

    for message in &request.messages {
        match message {
            AgentMessage::User { content, .. } => {
                let parts = responses_input_items(content);
                if !parts.is_empty() {
                    input.push(serde_json::json!({ "role": "user", "content": parts }));
                }
            }
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            } => {}
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    input.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
                        "status": "completed",
                    }));
                }
                for tool_call in tool_calls {
                    let call_id = tool_call.call_id.as_deref().unwrap_or(&tool_call.id);
                    input.push(serde_json::json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": tool_call.name,
                        "arguments": tool_call.arguments.to_string(),
                    }));
                }
            }
            AgentMessage::ToolResult {
                tool_call_id,
                call_id,
                content,
                ..
            } => {
                let id = call_id.as_deref().unwrap_or(tool_call_id);
                let output = responses_tool_result_output(content);
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": output,
                }));
            }
            AgentMessage::CompactionSummary { summary, .. } => {
                if !summary.is_empty() {
                    input.push(serde_json::json!({
                        "role": "user",
                        "content": [{ "type": "input_text", "text": format!("<summary>\n{summary}\n</summary>") }],
                    }));
                }
            }
        }
    }

    input
}

pub(super) async fn process_responses_stream(
    mut response: reqwest::Response,
    read_error_context: &str,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let mut parser = SseParser::new();
    let mut state = ResponsesStreamState::default();

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| BridgeError::CompletionFailed(format!("{read_error_context}: {error}")))?
    {
        let text = String::from_utf8_lossy(&chunk);
        for event in parser.feed(&text) {
            process_responses_sse_event(&event, &mut state, tx).await?;
        }
    }
    if let Some(trailing) = parser.flush() {
        process_responses_sse_event(&trailing, &mut state, tx).await?;
    }

    let message = state.into_agent_message();
    let content_parts = match &message {
        AgentMessage::Assistant { content, .. } => content.clone(),
        _ => Vec::new(),
    };
    let usage = match &message {
        AgentMessage::Assistant { usage, .. } => usage.clone().unwrap_or_default(),
        _ => TokenUsage::default(),
    };

    let _ = tx
        .send(StreamChunk::Done(BridgeResponse {
            message,
            raw_content: content_parts,
            usage,
        }))
        .await;

    Ok(())
}

struct PendingFunctionCall {
    call_id: String,
    item_id: String,
    name: String,
    arguments_buf: String,
}

#[derive(Default)]
struct ResponsesStreamState {
    content_parts: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    usage: TokenUsage,
    text_buf: String,
    reasoning_buf: String,
    pending_fc: Option<PendingFunctionCall>,
}

impl ResponsesStreamState {
    fn finish_current_text(&mut self) {
        if !self.text_buf.is_empty() {
            self.content_parts
                .push(ContentPart::text(std::mem::take(&mut self.text_buf)));
        }
    }

    fn finish_current_reasoning(&mut self) {
        if !self.reasoning_buf.is_empty() {
            self.content_parts.push(ContentPart::reasoning(
                std::mem::take(&mut self.reasoning_buf),
                None,
                None,
            ));
        }
    }

    fn finish_current_fc(&mut self) {
        if let Some(fc) = self.pending_fc.take() {
            let arguments = serde_json::from_str(&fc.arguments_buf)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let combined_id = format!("{}|{}", fc.call_id, fc.item_id);
            self.tool_calls.push(ToolCallInfo {
                id: combined_id,
                call_id: Some(fc.call_id),
                name: fc.name,
                arguments,
            });
        }
    }

    fn into_agent_message(mut self) -> AgentMessage {
        self.finish_current_text();
        self.finish_current_reasoning();
        self.finish_current_fc();
        AgentMessage::Assistant {
            content: self.content_parts,
            tool_calls: self.tool_calls,
            stop_reason: None,
            error_message: None,
            usage: Some(self.usage),
            timestamp: Some(now_millis()),
        }
    }
}

async fn process_responses_sse_event(
    sse: &super::sse::SseEvent,
    state: &mut ResponsesStreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let data: serde_json::Value = match serde_json::from_str(&sse.data) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let event_type = sse.event.as_deref().unwrap_or("");

    match event_type {
        "response.output_item.added" => {
            let item = data.get("item").unwrap_or(&serde_json::Value::Null);
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            match item_type {
                "reasoning" => {
                    state.finish_current_text();
                    state.finish_current_fc();
                }
                "message" => {
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                }
                "function_call" => {
                    state.finish_current_text();
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                    let call_id = item
                        .get("call_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let item_id = item
                        .get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let combined_id = format!("{call_id}|{item_id}");
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: combined_id,
                            content: ToolCallDeltaContent::Name(name.clone()),
                        })
                        .await;
                    state.pending_fc = Some(PendingFunctionCall {
                        call_id,
                        item_id,
                        name,
                        arguments_buf: String::new(),
                    });
                }
                _ => {}
            }
        }
        "response.output_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|value| value.as_str()) {
                state.text_buf.push_str(delta);
                let _ = tx.send(StreamChunk::TextDelta(delta.to_string())).await;
            }
        }
        "response.reasoning_summary_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|value| value.as_str()) {
                state.reasoning_buf.push_str(delta);
                let _ = tx
                    .send(StreamChunk::ReasoningDelta {
                        id: None,
                        text: delta.to_string(),
                        signature: None,
                    })
                    .await;
            }
        }
        "response.function_call_arguments.delta" => {
            if let Some(delta) = data.get("delta").and_then(|value| value.as_str())
                && let Some(ref mut function_call) = state.pending_fc
            {
                function_call.arguments_buf.push_str(delta);
                let combined_id = format!("{}|{}", function_call.call_id, function_call.item_id);
                let _ = tx
                    .send(StreamChunk::ToolCallDelta {
                        id: combined_id,
                        content: ToolCallDeltaContent::Arguments(delta.to_string()),
                    })
                    .await;
            }
        }
        "response.output_item.done" => {
            let item = data.get("item").unwrap_or(&serde_json::Value::Null);
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            match item_type {
                "message" => state.finish_current_text(),
                "reasoning" => state.finish_current_reasoning(),
                "function_call" => {
                    if let Some(ref mut function_call) = state.pending_fc
                        && let Some(arguments) =
                            item.get("arguments").and_then(|value| value.as_str())
                    {
                        function_call.arguments_buf = arguments.to_string();
                    }
                    state.finish_current_fc();
                }
                _ => {}
            }
        }
        "response.completed" => {
            if let Some(usage) = data.get("response").and_then(|value| value.get("usage")) {
                let input_tokens = usage
                    .get("input_tokens")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let cached = usage
                    .get("input_tokens_details")
                    .and_then(|details| details.get("cached_tokens"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                state.usage.input = input_tokens.saturating_sub(cached);
                state.usage.output = usage
                    .get("output_tokens")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
            }
        }
        "response.failed" | "error" => {
            let message = data
                .get("response")
                .and_then(|value| value.get("error"))
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .or_else(|| {
                    data.get("error")
                        .and_then(|value| value.get("message"))
                        .and_then(|value| value.as_str())
                })
                .or_else(|| data.get("message").and_then(|value| value.as_str()))
                .unwrap_or("unknown error");
            return Err(BridgeError::CompletionFailed(message.to_string()));
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::types::ToolDefinition;

    #[test]
    fn openai_body_keeps_system_prompt_as_input_message() {
        let body = build_responses_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: Some("system".to_string()),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![],
            },
            ResponsesRequestOptions::openai_api(),
        );

        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["role"], "system");
        assert!(body.get("instructions").is_none());
    }

    #[test]
    fn codex_body_uses_instructions_and_strict_false_tools() {
        let body = build_responses_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: Some("system".to_string()),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![ToolDefinition {
                    name: "demo_tool".to_string(),
                    description: "Demo tool".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" }
                        },
                        "required": ["value"],
                        "additionalProperties": false
                    }),
                }],
            },
            ResponsesRequestOptions::codex(),
        );

        assert_eq!(body["instructions"], "system");
        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(body["text"]["verbosity"], "low");
        assert_eq!(body["include"][0], "reasoning.encrypted_content");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["tools"][0]["strict"], false);
    }

    #[test]
    fn responses_input_keeps_user_images() {
        let body = build_responses_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: None,
                messages: vec![AgentMessage::User {
                    content: vec![
                        ContentPart::text("看图"),
                        ContentPart::Image {
                            mime_type: "image/png".to_string(),
                            data: "AAECAw==".to_string(),
                        },
                    ],
                    timestamp: None,
                }],
                tools: vec![],
            },
            ResponsesRequestOptions::openai_api(),
        );

        let content = body["input"][0]["content"].as_array().expect("content");
        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(content[1]["type"], "input_image");
        assert_eq!(content[1]["image_url"], "data:image/png;base64,AAECAw==");
    }

    #[test]
    fn responses_tool_result_image_uses_native_function_output_parts() {
        let body = build_responses_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: None,
                messages: vec![AgentMessage::tool_result_full(
                    "tool-1",
                    Some("call-1".to_string()),
                    Some("fs_read".to_string()),
                    vec![
                        ContentPart::text("file: image.png"),
                        ContentPart::Image {
                            mime_type: "image/png".to_string(),
                            data: "AAECAw==".to_string(),
                        },
                    ],
                    None,
                    false,
                )],
                tools: vec![],
            },
            ResponsesRequestOptions::codex(),
        );

        let input = body["input"].as_array().expect("input");
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "function_call_output");
        let output = input[0]["output"].as_array().expect("output parts");
        assert_eq!(output[0]["type"], "input_text");
        assert_eq!(output[0]["text"], "file: image.png");
        assert_eq!(output[1]["type"], "input_image");
        assert_eq!(output[1]["image_url"], "data:image/png;base64,AAECAw==");
    }
}
