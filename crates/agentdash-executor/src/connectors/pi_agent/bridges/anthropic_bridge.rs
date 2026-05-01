/// Anthropic Messages API 直连 Bridge
///
/// 对标 pi-mono `anthropic.ts`，直接走 `/v1/messages` SSE 流。
/// 不依赖任何 LLM SDK，仅使用 reqwest + serde_json。
use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, ToolCallDeltaContent,
};
use agentdash_agent::types::{AgentMessage, ContentPart, TokenUsage, ToolCallInfo, now_millis};

use super::sse::SseParser;

const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u64 = 16384;

pub struct AnthropicBridge {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
}

impl AnthropicBridge {
    pub fn new(api_key: &str, model_id: &str, base_url: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url
                .unwrap_or("https://api.anthropic.com")
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.to_string(),
            model_id: model_id.to_string(),
        }
    }
}

#[async_trait]
impl LlmBridge for AnthropicBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let client = self.client.clone();
        let url = format!("{}/v1/messages", self.base_url);
        let api_key = self.api_key.clone();
        let model_id = self.model_id.clone();

        tokio::spawn(async move {
            if let Err(e) = run_stream(&client, &url, &api_key, &model_id, &request, &tx).await {
                let _ = tx.send(StreamChunk::Error(e)).await;
            }
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

// ─── 流处理主函数 ──────────────────────────────────────────

async fn run_stream(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model_id: &str,
    request: &BridgeRequest,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let body = build_request_body(model_id, request);

    let mut req_builder = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION);

    // 启用 interleaved thinking + fine-grained tool streaming
    let beta_features = "interleaved-thinking-2025-05-14";
    req_builder = req_builder.header("anthropic-beta", beta_features);

    let response = req_builder
        .body(
            serde_json::to_string(&body)
                .map_err(|e| BridgeError::RequestBuildFailed(e.to_string()))?,
        )
        .send()
        .await
        .map_err(|e| BridgeError::CompletionFailed(format!("HTTP 请求失败: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(BridgeError::CompletionFailed(format!(
            "API 返回 {status}: {body_text}"
        )));
    }

    let mut parser = SseParser::new();
    let mut state = StreamState::default();
    let mut response = response;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| BridgeError::CompletionFailed(format!("读取响应流失败: {e}")))?
    {
        let text = String::from_utf8_lossy(&chunk);
        for event in parser.feed(&text) {
            let event_type = event.event.as_deref().unwrap_or("");
            if event_type == "error" {
                return Err(BridgeError::CompletionFailed(event.data.clone()));
            }
            if !is_message_event(event_type) {
                continue;
            }
            process_anthropic_event(event_type, &event.data, &mut state, tx).await?;
        }
    }

    let message = state.into_agent_message();
    let content_parts: Vec<ContentPart> = match &message {
        AgentMessage::Assistant { content, .. } => content.clone(),
        _ => vec![],
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

fn is_message_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "message_start"
            | "message_delta"
            | "message_stop"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
    )
}

// ─── 请求体构建 ──────────────────────────────────────────────

fn build_request_body(model_id: &str, request: &BridgeRequest) -> serde_json::Value {
    let messages = convert_messages(request);
    let mut body = serde_json::json!({
        "model": model_id,
        "messages": messages,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "stream": true,
    });

    if let Some(ref sp) = request.system_prompt {
        if !sp.is_empty() {
            body["system"] = serde_json::Value::String(sp.clone());
        }
    }

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                let mut tool = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                });
                // 为每个工具启用 eager input streaming
                tool["eager_input_streaming"] = serde_json::json!({"type": "enabled"});
                tool
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    // 启用 extended thinking (adaptive)
    body["thinking"] = serde_json::json!({
        "type": "enabled",
        "effort": "high",
    });

    body
}

fn convert_messages(request: &BridgeRequest) -> Vec<serde_json::Value> {
    use agentdash_agent::types::StopReason;

    let mut messages: Vec<serde_json::Value> = Vec::new();

    for msg in &request.messages {
        match msg {
            AgentMessage::User { content, .. } => {
                let parts: Vec<serde_json::Value> = content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => {
                            Some(serde_json::json!({ "type": "text", "text": text }))
                        }
                        ContentPart::Image { mime_type, data } => Some(serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": mime_type,
                                "data": data,
                            }
                        })),
                        ContentPart::Reasoning { .. } => None,
                    })
                    .collect();
                if !parts.is_empty() {
                    messages.push(serde_json::json!({ "role": "user", "content": parts }));
                }
            }
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            } => {
                continue;
            }
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let mut parts: Vec<serde_json::Value> = Vec::new();

                for part in content {
                    match part {
                        ContentPart::Text { text } => {
                            if !text.is_empty() {
                                parts.push(serde_json::json!({ "type": "text", "text": text }));
                            }
                        }
                        ContentPart::Reasoning {
                            text, signature, ..
                        } => {
                            // 使用 signature 作为 opaque thinking token 回传 Anthropic
                            if let Some(sig) = signature {
                                parts.push(serde_json::json!({
                                    "type": "thinking",
                                    "thinking": text,
                                    "signature": sig,
                                }));
                            } else if !text.is_empty() {
                                parts.push(serde_json::json!({
                                    "type": "thinking",
                                    "thinking": text,
                                    "signature": "",
                                }));
                            }
                        }
                        ContentPart::Image { .. } => {}
                    }
                }

                for tc in tool_calls {
                    parts.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": tc.arguments,
                    }));
                }

                if !parts.is_empty() {
                    messages.push(serde_json::json!({ "role": "assistant", "content": parts }));
                }
            }
            AgentMessage::ToolResult {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": text,
                    "is_error": is_error,
                });
                // Anthropic: tool_result 必须在 user 消息内
                if let Some(last) = messages.last_mut() {
                    if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                            arr.push(result);
                            continue;
                        }
                    }
                }
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": [result],
                }));
            }
            AgentMessage::CompactionSummary { summary, .. } => {
                if !summary.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": format!("<summary>\n{summary}\n</summary>"),
                    }));
                }
            }
        }
    }

    messages
}

// ─── SSE 事件处理 ────────────────────────────────────────────

/// 当前正在构建中的 tool_use block
struct PendingToolUse {
    id: String,
    name: String,
    input_json_buf: String,
}

#[derive(Default)]
struct StreamState {
    content_parts: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    usage: TokenUsage,
    /// 当前活跃的 content block 类型
    current_block_type: Option<String>,
    /// 文本累积
    text_buf: String,
    /// thinking 累积
    thinking_buf: String,
    thinking_signature: Option<String>,
    /// tool_use 累积
    pending_tool: Option<PendingToolUse>,
}

impl StreamState {
    fn finish_text(&mut self) {
        if !self.text_buf.is_empty() {
            self.content_parts
                .push(ContentPart::text(std::mem::take(&mut self.text_buf)));
        }
    }

    fn finish_thinking(&mut self) {
        if !self.thinking_buf.is_empty() {
            self.content_parts.push(ContentPart::reasoning(
                std::mem::take(&mut self.thinking_buf),
                None,
                self.thinking_signature.take(),
            ));
        }
    }

    fn finish_tool_use(&mut self) {
        if let Some(tool) = self.pending_tool.take() {
            let input = serde_json::from_str(&tool.input_json_buf)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            self.tool_calls.push(ToolCallInfo {
                id: tool.id.clone(),
                call_id: Some(tool.id),
                name: tool.name,
                arguments: input,
            });
        }
    }

    fn into_agent_message(mut self) -> AgentMessage {
        self.finish_text();
        self.finish_thinking();
        self.finish_tool_use();
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

async fn process_anthropic_event(
    event_type: &str,
    data: &str,
    state: &mut StreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| BridgeError::CompletionFailed(format!("Anthropic JSON 解析失败: {e}")))?;

    match event_type {
        "message_start" => {
            if let Some(usage) = json.get("message").and_then(|m| m.get("usage")) {
                state.usage.input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
        }

        "content_block_start" => {
            let block = json
                .get("content_block")
                .unwrap_or(&serde_json::Value::Null);
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match block_type {
                "text" => {
                    state.finish_thinking();
                    state.finish_tool_use();
                    state.current_block_type = Some("text".into());
                }
                "thinking" => {
                    state.finish_text();
                    state.finish_tool_use();
                    state.current_block_type = Some("thinking".into());
                }
                "tool_use" => {
                    state.finish_text();
                    state.finish_thinking();
                    state.finish_tool_use();
                    state.current_block_type = Some("tool_use".into());

                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: id.clone(),
                            content: ToolCallDeltaContent::Name(name.clone()),
                        })
                        .await;

                    state.pending_tool = Some(PendingToolUse {
                        id,
                        name,
                        input_json_buf: String::new(),
                    });
                }
                _ => {
                    state.current_block_type = None;
                }
            }
        }

        "content_block_delta" => {
            let delta = json.get("delta").unwrap_or(&serde_json::Value::Null);
            let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match delta_type {
                "text_delta" => {
                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                        state.text_buf.push_str(text);
                        let _ = tx.send(StreamChunk::TextDelta(text.to_string())).await;
                    }
                }
                "thinking_delta" => {
                    if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                        state.thinking_buf.push_str(thinking);
                        let _ = tx
                            .send(StreamChunk::ReasoningDelta {
                                id: None,
                                text: thinking.to_string(),
                                signature: None,
                            })
                            .await;
                    }
                }
                "input_json_delta" => {
                    if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                        if let Some(ref mut tool) = state.pending_tool {
                            tool.input_json_buf.push_str(partial);
                            let _ = tx
                                .send(StreamChunk::ToolCallDelta {
                                    id: tool.id.clone(),
                                    content: ToolCallDeltaContent::Arguments(partial.to_string()),
                                })
                                .await;
                        }
                    }
                }
                "signature_delta" => {
                    if let Some(sig) = delta.get("signature").and_then(|s| s.as_str()) {
                        match &mut state.thinking_signature {
                            Some(existing) => existing.push_str(sig),
                            None => state.thinking_signature = Some(sig.to_string()),
                        }
                    }
                }
                _ => {}
            }
        }

        "content_block_stop" => {
            match state.current_block_type.as_deref() {
                Some("text") => state.finish_text(),
                Some("thinking") => state.finish_thinking(),
                Some("tool_use") => state.finish_tool_use(),
                _ => {}
            }
            state.current_block_type = None;
        }

        "message_delta" => {
            if let Some(usage) = json.get("usage") {
                state.usage.output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
        }

        "message_stop" => {
            // 流结束
        }

        _ => {}
    }

    Ok(())
}
