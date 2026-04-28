/// OpenAI Responses API 直连 Bridge
///
/// 对标 pi-mono `openai-responses.ts` + `openai-responses-shared.ts`，
/// 直接走 `/v1/responses` SSE 流。不依赖任何 LLM SDK。
use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, ToolCallDeltaContent,
};
use agentdash_agent::types::{AgentMessage, ContentPart, TokenUsage, ToolCallInfo, now_millis};

use super::sse::SseParser;

pub struct OpenAiResponsesBridge {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
}

impl OpenAiResponsesBridge {
    pub fn new(api_key: &str, model_id: &str, base_url: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.to_string(),
            model_id: model_id.to_string(),
        }
    }
}

#[async_trait]
impl LlmBridge for OpenAiResponsesBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let client = self.client.clone();
        let url = format!("{}/responses", self.base_url);
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

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {api_key}"))
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
            process_sse_event(&event, &mut state, tx).await?;
        }
    }
    if let Some(trailing) = parser.flush() {
        process_sse_event(&trailing, &mut state, tx).await?;
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

// ─── 请求体构建 ──────────────────────────────────────────────

fn build_request_body(model_id: &str, request: &BridgeRequest) -> serde_json::Value {
    let input = convert_input(request);
    let mut body = serde_json::json!({
        "model": model_id,
        "input": input,
        "stream": true,
        "store": false,
    });

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    body
}

fn convert_input(request: &BridgeRequest) -> Vec<serde_json::Value> {
    use agentdash_agent::types::StopReason;

    let mut input = Vec::new();

    if let Some(ref sp) = request.system_prompt {
        if !sp.is_empty() {
            input.push(serde_json::json!({ "role": "developer", "content": sp }));
        }
    }

    for msg in &request.messages {
        match msg {
            AgentMessage::User { content, .. } => {
                let parts: Vec<serde_json::Value> = content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => {
                            Some(serde_json::json!({ "type": "input_text", "text": text }))
                        }
                        _ => None,
                    })
                    .collect();
                if !parts.is_empty() {
                    input.push(serde_json::json!({ "role": "user", "content": parts }));
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
                for tc in tool_calls {
                    let call_id = tc.call_id.as_deref().unwrap_or(&tc.id);
                    input.push(serde_json::json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": tc.name,
                        "arguments": tc.arguments.to_string(),
                    }));
                }
            }
            AgentMessage::ToolResult {
                tool_call_id,
                call_id,
                content,
                ..
            } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let id = call_id.as_deref().unwrap_or(tool_call_id);
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": text,
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

// ─── SSE 事件处理 ────────────────────────────────────────────

/// 当前正在构建中的 function_call
struct PendingFunctionCall {
    call_id: String,
    item_id: String,
    name: String,
    arguments_buf: String,
}

#[derive(Default)]
struct StreamState {
    content_parts: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    usage: TokenUsage,
    /// 当前活跃的输出项类型（reasoning / message / function_call）
    current_item_type: Option<String>,
    /// 当前文本累积缓冲
    text_buf: String,
    /// 当前 reasoning 累积缓冲
    reasoning_buf: String,
    /// 当前 function call
    pending_fc: Option<PendingFunctionCall>,
}

impl StreamState {
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

async fn process_sse_event(
    sse: &super::sse::SseEvent,
    state: &mut StreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let event_type = sse.event.as_deref().unwrap_or("");
    let data: serde_json::Value = match serde_json::from_str(&sse.data) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    match event_type {
        "response.output_item.added" => {
            let item_type = data
                .get("item")
                .and_then(|i| i.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");

            match item_type {
                "reasoning" => {
                    state.finish_current_text();
                    state.finish_current_fc();
                    state.current_item_type = Some("reasoning".into());
                }
                "message" => {
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                    state.current_item_type = Some("message".into());
                }
                "function_call" => {
                    state.finish_current_text();
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                    state.current_item_type = Some("function_call".into());

                    let item = data.get("item").unwrap_or(&serde_json::Value::Null);
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let item_id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
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
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                state.text_buf.push_str(delta);
                let _ = tx.send(StreamChunk::TextDelta(delta.to_string())).await;
            }
        }

        "response.reasoning_summary_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
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
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                if let Some(ref mut fc) = state.pending_fc {
                    fc.arguments_buf.push_str(delta);
                    let combined_id = format!("{}|{}", fc.call_id, fc.item_id);
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: combined_id,
                            content: ToolCallDeltaContent::Arguments(delta.to_string()),
                        })
                        .await;
                }
            }
        }

        "response.output_item.done" => {
            let item_type = data
                .get("item")
                .and_then(|i| i.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");

            match item_type {
                "message" => state.finish_current_text(),
                "reasoning" => state.finish_current_reasoning(),
                "function_call" => {
                    if let Some(ref mut fc) = state.pending_fc {
                        if let Some(args_str) = data
                            .get("item")
                            .and_then(|i| i.get("arguments"))
                            .and_then(|a| a.as_str())
                        {
                            fc.arguments_buf = args_str.to_string();
                        }
                    }
                    state.finish_current_fc();
                }
                _ => {}
            }
            state.current_item_type = None;
        }

        "response.completed" => {
            if let Some(response) = data.get("response") {
                if let Some(usage) = response.get("usage") {
                    let input_tokens = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let cached = usage
                        .get("input_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    state.usage.input = input_tokens.saturating_sub(cached);
                    state.usage.output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                }
            }
        }

        "response.failed" | "error" => {
            let msg = data
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .or_else(|| data.get("message").and_then(|m| m.as_str()))
                .unwrap_or("unknown error");
            return Err(BridgeError::CompletionFailed(msg.to_string()));
        }

        _ => {}
    }

    Ok(())
}
