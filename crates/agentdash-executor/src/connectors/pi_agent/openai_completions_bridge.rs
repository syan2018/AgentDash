/// OpenAI Chat Completions API 直连 Bridge
///
/// 对标 pi-mono `openai-completions.ts`，直接走 `/v1/chat/completions` SSE 流。
/// 不依赖任何 LLM SDK，仅使用 reqwest + serde_json。
use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, ToolCallDeltaContent,
};
use agentdash_agent::types::{AgentMessage, ContentPart, TokenUsage, ToolCallInfo, now_millis};

use super::sse::SseParser;

pub struct OpenAiCompletionsBridge {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
}

impl OpenAiCompletionsBridge {
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
impl LlmBridge for OpenAiCompletionsBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let client = self.client.clone();
        let url = format!("{}/chat/completions", self.base_url);
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
            if event.data == "[DONE]" {
                break;
            }
            process_chunk_event(&event.data, &mut state, tx).await?;
        }
    }

    if let Some(trailing) = parser.flush() {
        if trailing.data != "[DONE]" {
            process_chunk_event(&trailing.data, &mut state, tx).await?;
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

// ─── 请求体构建 ──────────────────────────────────────────────

fn build_request_body(model_id: &str, request: &BridgeRequest) -> serde_json::Value {
    let messages = convert_messages(request);
    let mut body = serde_json::json!({
        "model": model_id,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    body
}

fn convert_messages(request: &BridgeRequest) -> Vec<serde_json::Value> {
    use agentdash_agent::types::StopReason;

    let mut messages = Vec::new();

    if let Some(ref sp) = request.system_prompt {
        if !sp.is_empty() {
            messages.push(serde_json::json!({ "role": "system", "content": sp }));
        }
    }

    for msg in &request.messages {
        match msg {
            AgentMessage::User { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    messages.push(serde_json::json!({ "role": "user", "content": text }));
                }
            }
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            } => {
                // 对齐 pi-mono transform-messages.ts:
                // 跳过 error/aborted 的 assistant 消息，这些不完整的 turn 不应被重放，
                // 否则会导致后续 LLM 请求因格式无效而失败。
                continue;
            }
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let mut msg_obj = serde_json::json!({ "role": "assistant" });

                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    msg_obj["content"] = serde_json::Value::String(text);
                }

                if !tool_calls.is_empty() {
                    let tcs: Vec<serde_json::Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            let call_id = tc.call_id.as_deref().unwrap_or(&tc.id);
                            serde_json::json!({
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                }
                            })
                        })
                        .collect();
                    msg_obj["tool_calls"] = serde_json::Value::Array(tcs);
                }

                messages.push(msg_obj);
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
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": text,
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

// ─── SSE chunk 处理 ──────────────────────────────────────────

#[derive(Default)]
struct StreamState {
    content_parts: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    usage: TokenUsage,
    /// 当前正在累积的 tool call（按 stream index 跟踪）
    pending_tool_calls: Vec<PendingToolCall>,
}

struct PendingToolCall {
    id: String,
    name: String,
    arguments_buf: String,
    stream_index: Option<u32>,
}

impl StreamState {
    fn into_agent_message(mut self) -> AgentMessage {
        self.finalize_pending_tool_calls();
        AgentMessage::Assistant {
            content: self.content_parts,
            tool_calls: self.tool_calls,
            stop_reason: None,
            error_message: None,
            usage: Some(self.usage),
            timestamp: Some(now_millis()),
        }
    }

    fn finalize_pending_tool_calls(&mut self) {
        for ptc in self.pending_tool_calls.drain(..) {
            let arguments = serde_json::from_str(&ptc.arguments_buf)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            self.tool_calls.push(ToolCallInfo {
                id: ptc.id.clone(),
                call_id: Some(ptc.id),
                name: ptc.name,
                arguments,
            });
        }
    }
}

async fn process_chunk_event(
    data: &str,
    state: &mut StreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let chunk: serde_json::Value = serde_json::from_str(data).map_err(|e| {
        BridgeError::CompletionFailed(format!("JSON 解析失败: {e}\nraw: {data}"))
    })?;

    if let Some(usage) = chunk.get("usage").and_then(|u| u.as_object()) {
        if let Some(input) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
            state.usage.input = input;
        }
        if let Some(output) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
            state.usage.output = output;
        }
    }

    let Some(choice) = chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
    else {
        return Ok(());
    };

    let Some(delta) = choice.get("delta") else {
        return Ok(());
    };

    // 文本内容
    if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            state.content_parts.push(ContentPart::text(text));
            let _ = tx.send(StreamChunk::TextDelta(text.to_string())).await;
        }
    }

    // 推理内容 — 兼容 reasoning_content / reasoning 两种字段名
    for field in &["reasoning_content", "reasoning"] {
        if let Some(reasoning) = delta.get(*field).and_then(|v| v.as_str()) {
            if !reasoning.is_empty() {
                state
                    .content_parts
                    .push(ContentPart::reasoning(reasoning, None, None));
                let _ = tx
                    .send(StreamChunk::ReasoningDelta {
                        id: None,
                        text: reasoning.to_string(),
                        signature: None,
                    })
                    .await;
                break;
            }
        }
    }

    // 工具调用增量
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tool_calls {
            let stream_index = tc.get("index").and_then(|v| v.as_u64()).map(|v| v as u32);
            let tc_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let tc_name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tc_args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let existing = state.pending_tool_calls.iter_mut().find(|p| {
                stream_index.is_some() && p.stream_index == stream_index
                    || (!tc_id.is_empty() && p.id == tc_id)
            });

            if let Some(ptc) = existing {
                if !tc_name.is_empty() && ptc.name.is_empty() {
                    ptc.name = tc_name.to_string();
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: ptc.id.clone(),
                            content: ToolCallDeltaContent::Name(tc_name.to_string()),
                        })
                        .await;
                }
                if !tc_args.is_empty() {
                    ptc.arguments_buf.push_str(tc_args);
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: ptc.id.clone(),
                            content: ToolCallDeltaContent::Arguments(tc_args.to_string()),
                        })
                        .await;
                }
            } else {
                let id = if tc_id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    tc_id.to_string()
                };

                let mut ptc = PendingToolCall {
                    id: id.clone(),
                    name: tc_name.to_string(),
                    arguments_buf: String::new(),
                    stream_index,
                };

                if !tc_name.is_empty() {
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: id.clone(),
                            content: ToolCallDeltaContent::Name(tc_name.to_string()),
                        })
                        .await;
                }
                if !tc_args.is_empty() {
                    ptc.arguments_buf.push_str(tc_args);
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: id.clone(),
                            content: ToolCallDeltaContent::Arguments(tc_args.to_string()),
                        })
                        .await;
                }

                state.pending_tool_calls.push(ptc);
            }
        }
    }

    Ok(())
}
