use agentdash_agent_core::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, ProviderErrorClassification, StreamChunk,
    ToolCallDeltaContent,
};
use agentdash_agent_core::types::{
    AgentMessage, ContentPart, StopReason, TokenUsage, ToolCallInfo, now_millis,
};
use futures::{Stream, StreamExt};

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
    response: reqwest::Response,
    read_error_context: &str,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let chunks = futures::stream::try_unfold(response, |mut response| async move {
        let chunk = response.chunk().await?;
        Ok(chunk.map(|chunk| (chunk, response)))
    });

    process_responses_chunks(chunks, tx, |error| {
        super::provider_stream_read_error(read_error_context, error)
    })
    .await
}

async fn process_responses_chunks<S, B, E, F>(
    chunks: S,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
    map_read_error: F,
) -> Result<(), BridgeError>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
    F: Fn(E) -> BridgeError,
{
    futures::pin_mut!(chunks);
    let mut parser = SseParser::new();
    let mut state = ResponsesStreamState::default();

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(&map_read_error)?;
        let text = String::from_utf8_lossy(chunk.as_ref());
        for event in parser.feed(&text) {
            if process_responses_sse_event(&event, &mut state, tx).await?
                == ResponsesEventDisposition::Completed
            {
                return send_responses_done(state, tx).await;
            }
        }
    }
    if let Some(trailing) = parser.flush()
        && process_responses_sse_event(&trailing, &mut state, tx).await?
            == ResponsesEventDisposition::Completed
    {
        return send_responses_done(state, tx).await;
    }

    Err(BridgeError::provider(
        "Responses stream ended before response.completed",
        ProviderErrorClassification::retryable().with_provider_code("stream_disconnected"),
    ))
}

async fn send_responses_done(
    state: ResponsesStreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponsesEventDisposition {
    Continue,
    Completed,
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
) -> Result<ResponsesEventDisposition, BridgeError> {
    let event_type = sse.event.as_deref();
    let data: serde_json::Value = match serde_json::from_str(&sse.data) {
        Ok(value) => value,
        Err(error) if is_responses_protocol_event(event_type) => {
            return Err(BridgeError::provider(
                format!(
                    "Responses SSE event {} contains invalid JSON: {error}",
                    event_type.unwrap_or_default()
                ),
                ProviderErrorClassification::fatal().with_provider_code("response_decode_error"),
            ));
        }
        // Responses keepalives and transport sentinels do not carry a protocol event name.
        Err(_) => return Ok(ResponsesEventDisposition::Continue),
    };
    let event_type = event_type.unwrap_or("");

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
                state.usage.cache_read_input = cached;
                state.usage.output = usage
                    .get("output_tokens")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
            }
            return Ok(ResponsesEventDisposition::Completed);
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
            return Err(super::provider_event_error(
                message.to_string(),
                Some(&sse.data),
            ));
        }
        _ => {}
    }

    Ok(ResponsesEventDisposition::Continue)
}

fn is_responses_protocol_event(event_type: Option<&str>) -> bool {
    event_type
        .is_some_and(|event_type| event_type == "error" || event_type.starts_with("response."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_core::bridge::ProviderErrorKind;
    use agentdash_agent_core::types::ToolDefinition;
    use std::time::Duration;

    async fn collect_chunks(mut rx: tokio::sync::mpsc::Receiver<StreamChunk>) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }
        chunks
    }

    fn fixture_read_error(message: &str) -> BridgeError {
        BridgeError::provider(
            message,
            ProviderErrorClassification::retryable().with_provider_code("fixture_decoder"),
        )
    }

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

    #[tokio::test]
    async fn response_completed_stops_before_trailing_decoder_error_and_emits_done_once() {
        let terminal_chunk = concat!(
            "event: response.output_item.added\n",
            "data: {\"item\":{\"type\":\"message\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"done\"}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"usage\":{\"input_tokens\":99,\"output_tokens\":99}}}\n\n",
        )
        .as_bytes()
        .to_vec();
        let chunks = futures::stream::iter(vec![
            Ok::<_, &'static str>(terminal_chunk),
            Err("late decoder error"),
        ]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        process_responses_chunks(chunks, &tx, |_| {
            panic!("transport must not be read after response.completed")
        })
        .await
        .expect("protocol terminal completes the stream");
        drop(tx);

        let emitted = collect_chunks(rx).await;
        assert_eq!(
            emitted
                .iter()
                .filter(|chunk| matches!(chunk, StreamChunk::Done(_)))
                .count(),
            1
        );
        assert!(
            emitted
                .iter()
                .any(|chunk| matches!(chunk, StreamChunk::TextDelta(delta) if delta == "done"))
        );
        let done = emitted
            .iter()
            .find_map(|chunk| match chunk {
                StreamChunk::Done(response) => Some(response),
                _ => None,
            })
            .expect("done response");
        assert_eq!(done.usage.input, 1);
        assert_eq!(done.usage.output, 1);
    }

    #[tokio::test]
    async fn response_completed_finishes_even_when_transport_remains_open() {
        let terminal_chunk = concat!(
            "event: response.output_item.added\n",
            "data: {\"item\":{\"type\":\"message\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"done\"}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"usage\":{}}}\n\n",
        )
        .as_bytes()
        .to_vec();
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(terminal_chunk)])
            .chain(futures::stream::pending::<Result<Vec<u8>, &'static str>>());
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        tokio::time::timeout(
            Duration::from_secs(1),
            process_responses_chunks(chunks, &tx, fixture_read_error),
        )
        .await
        .expect("protocol terminal must not wait for transport EOF")
        .expect("protocol terminal completes the stream");
        drop(tx);

        let emitted = collect_chunks(rx).await;
        assert_eq!(
            emitted
                .iter()
                .filter(|chunk| matches!(chunk, StreamChunk::Done(_)))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn eof_before_response_completed_is_a_stream_disconnect() {
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(
            concat!(
                "event: response.output_item.added\n",
                "data: {\"item\":{\"type\":\"message\"}}\n\n",
                "event: response.output_text.delta\n",
                "data: {\"delta\":\"partial\"}\n\n",
            )
            .as_bytes()
            .to_vec(),
        )]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let error = process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect_err("EOF without protocol terminal must fail");
        drop(tx);

        let BridgeError::Provider {
            message,
            classification,
            ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert_eq!(message, "Responses stream ended before response.completed");
        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("stream_disconnected")
        );
        let emitted = collect_chunks(rx).await;
        assert!(
            emitted
                .iter()
                .any(|chunk| matches!(chunk, StreamChunk::TextDelta(delta) if delta == "partial"))
        );
        assert!(
            emitted
                .iter()
                .all(|chunk| !matches!(chunk, StreamChunk::Done(_)))
        );
    }

    #[tokio::test]
    async fn decoder_error_before_response_completed_is_preserved() {
        let chunks = futures::stream::iter(vec![
            Ok::<_, &'static str>(
                concat!(
                    "event: response.output_item.added\n",
                    "data: {\"item\":{\"type\":\"message\"}}\n\n",
                    "event: response.output_text.delta\n",
                    "data: {\"delta\":\"partial\"}\n\n",
                )
                .as_bytes()
                .to_vec(),
            ),
            Err("decoder failed"),
        ]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let error = process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect_err("decoder error before protocol terminal must fail");
        drop(tx);

        let BridgeError::Provider {
            message,
            classification,
            ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert_eq!(message, "decoder failed");
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("fixture_decoder")
        );
        let emitted = collect_chunks(rx).await;
        assert!(
            emitted
                .iter()
                .all(|chunk| !matches!(chunk, StreamChunk::Done(_)))
        );
    }

    #[tokio::test]
    async fn malformed_named_response_event_is_a_decode_error() {
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(
            b"event: response.completed\ndata: {not-json}\n\n".to_vec(),
        )]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let error = process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect_err("malformed protocol event must fail at its decode boundary");
        drop(tx);

        let BridgeError::Provider {
            message,
            classification,
            ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert!(
            message.starts_with("Responses SSE event response.completed contains invalid JSON:")
        );
        assert_eq!(classification.kind, ProviderErrorKind::Fatal);
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("response_decode_error")
        );
        assert!(collect_chunks(rx).await.is_empty());
    }

    #[tokio::test]
    async fn valid_response_failed_keeps_provider_event_error() {
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(
            concat!(
                "event: response.failed\n",
                "data: {\"response\":{\"error\":{\"message\":\"rate limit exceeded\"}}}\n\n",
            )
            .as_bytes()
            .to_vec(),
        )]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let error = process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect_err("response.failed remains a provider event failure");
        drop(tx);

        let BridgeError::Provider {
            message,
            classification,
            ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert_eq!(message, "rate limit exceeded");
        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert!(collect_chunks(rx).await.is_empty());
    }

    #[tokio::test]
    async fn unnamed_transport_sentinels_do_not_mask_a_valid_terminal() {
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(
            concat!(
                "data: [DONE]\n\n",
                "data: keepalive\n\n",
                "event: response.completed\n",
                "data: {\"response\":{\"usage\":{}}}\n\n",
            )
            .as_bytes()
            .to_vec(),
        )]);
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect("unnamed sentinels are outside the Responses event protocol");
        drop(tx);

        let emitted = collect_chunks(rx).await;
        assert_eq!(
            emitted
                .iter()
                .filter(|chunk| matches!(chunk, StreamChunk::Done(_)))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn response_completed_preserves_content_tool_calls_and_usage() {
        let protocol_stream = concat!(
            "event: response.output_item.added\n",
            "data: {\"item\":{\"type\":\"reasoning\"}}\n\n",
            "event: response.reasoning_summary_text.delta\n",
            "data: {\"delta\":\"think\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"item\":{\"type\":\"reasoning\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"item\":{\"type\":\"message\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"hello\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"item\":{\"type\":\"message\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"item\":{\"type\":\"function_call\",\"call_id\":\"call-1\",\"id\":\"item-1\",\"name\":\"fs_read\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"delta\":\"{\\\"path\\\":\\\"a.txt\\\"}\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"item\":{\"type\":\"function_call\",\"arguments\":\"{\\\"path\\\":\\\"a.txt\\\"}\"}}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"usage\":{\"input_tokens\":10,\"input_tokens_details\":{\"cached_tokens\":2},\"output_tokens\":3}}}\n\n",
        );
        let chunks = futures::stream::iter(vec![Ok::<_, &'static str>(
            protocol_stream.as_bytes().to_vec(),
        )]);
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        process_responses_chunks(chunks, &tx, fixture_read_error)
            .await
            .expect("completed response stream");
        drop(tx);

        let emitted = collect_chunks(rx).await;
        assert!(emitted.iter().any(|chunk| matches!(
            chunk,
            StreamChunk::ReasoningDelta { text, .. } if text == "think"
        )));
        assert!(
            emitted
                .iter()
                .any(|chunk| matches!(chunk, StreamChunk::TextDelta(delta) if delta == "hello"))
        );
        assert!(emitted.iter().any(|chunk| matches!(
            chunk,
            StreamChunk::ToolCallDelta {
                id,
                content: ToolCallDeltaContent::Name(name),
            } if id == "call-1|item-1" && name == "fs_read"
        )));

        let done = emitted
            .into_iter()
            .find_map(|chunk| match chunk {
                StreamChunk::Done(response) => Some(response),
                _ => None,
            })
            .expect("done response");
        assert_eq!(
            done.raw_content,
            vec![
                ContentPart::reasoning("think", None, None),
                ContentPart::text("hello"),
            ]
        );
        assert_eq!(
            done.usage,
            TokenUsage {
                input: 8,
                cache_read_input: 2,
                cache_creation_input: 0,
                output: 3,
            }
        );
        let AgentMessage::Assistant {
            content,
            tool_calls,
            usage,
            ..
        } = done.message
        else {
            panic!("expected assistant message");
        };
        assert_eq!(content, done.raw_content);
        assert_eq!(usage, Some(done.usage));
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call-1|item-1");
        assert_eq!(tool_calls[0].call_id.as_deref(), Some("call-1"));
        assert_eq!(tool_calls[0].name, "fs_read");
        assert_eq!(
            tool_calls[0].arguments,
            serde_json::json!({ "path": "a.txt" })
        );
    }
}
