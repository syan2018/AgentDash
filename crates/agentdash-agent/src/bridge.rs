/// LLM 桥接层 — Streaming-first 架构
///
/// 设计参考 pi-mono/packages/ai：
/// - 所有 LLM 调用默认走 streaming
/// - `StreamChunk` 对应 pi-mono 的 `AssistantMessageEvent`
/// - `complete()` 通过消费 `stream_complete()` 实现
///
/// Rig 的 `CompletionModel` 提供 `stream()` API，内部通过 channel 将
/// Rig 的 `StreamedAssistantContent` → 我们的 `StreamChunk` 逐个推送。
use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rig::OneOrMany;
use rig::completion::message::AssistantContent;
use rig::completion::request::GetTokenUsage;
use rig::completion::{CompletionModel, CompletionRequest, Usage};
use rig::streaming::StreamedAssistantContent;
use thiserror::Error;
use tokio_stream::wrappers::ReceiverStream;

use crate::convert::{assistant_from_llm_content, default_convert_to_llm};
use crate::types::AgentMessage;

// ─── 流式 Chunk 类型 ────────────────────────────────────────

/// LLM 流式输出的 chunk 单元（对标 pi-mono `AssistantMessageEvent`）
#[derive(Debug, Clone)]
pub enum StreamChunk {
    TextDelta(String),
    ReasoningDelta {
        id: Option<String>,
        text: String,
        signature: Option<String>,
    },
    ToolCallDelta {
        id: String,
        delta: String,
    },
    ToolCall {
        info: crate::types::ToolCallInfo,
    },
    /// 流结束，附带聚合后的完整响应
    Done(BridgeResponse),
    Error(BridgeError),
}

// ─── Bridge 协议 ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BridgeRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<rig::completion::ToolDefinition>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    /// 预转换好的 LLM 消息（由 AgentLoop 的 convert_to_llm 管线生成）。
    /// 若为 Some，Bridge 应优先使用此字段而非自行转换 `messages`。
    pub llm_messages: Option<Vec<rig::completion::Message>>,
}

#[derive(Debug, Clone)]
pub struct BridgeResponse {
    pub message: AgentMessage,
    pub raw_content: Vec<AssistantContent>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Error)]
pub enum BridgeError {
    #[error("LLM 调用失败: {0}")]
    CompletionFailed(String),
    #[error("LLM 响应为空")]
    EmptyResponse,
    #[error("请求构建失败: {0}")]
    RequestBuildFailed(String),
}

/// LLM 桥接层 trait
#[async_trait]
pub trait LlmBridge: Send + Sync {
    /// 流式补全 — 返回 StreamChunk 流，最后一个 chunk 为 Done 或 Error
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;

    /// 收集式补全 — 消费完整流后返回聚合响应（默认实现）
    async fn complete(&self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        let mut stream = self.stream_complete(request).await;
        let mut result: Option<BridgeResponse> = None;
        let mut last_error: Option<BridgeError> = None;

        while let Some(chunk) = stream.next().await {
            match chunk {
                StreamChunk::Done(resp) => {
                    result = Some(resp);
                }
                StreamChunk::Error(e) => {
                    last_error = Some(e);
                }
                _ => {}
            }
        }

        if let Some(err) = last_error {
            return Err(err);
        }
        result.ok_or(BridgeError::EmptyResponse)
    }
}

// ─── Rig 实现 ───────────────────────────────────────────────

pub struct RigBridge<M: CompletionModel> {
    model: M,
}

impl<M: CompletionModel> RigBridge<M> {
    pub fn new(model: M) -> Self {
        Self { model }
    }
}

fn build_rig_request(request: &BridgeRequest) -> Result<CompletionRequest, BridgeError> {
    // 优先使用 AgentLoop 预转换好的 LLM 消息
    let llm_messages = match &request.llm_messages {
        Some(msgs) => msgs.clone(),
        None => default_convert_to_llm(&request.messages),
    };

    // 某些 OpenAI 兼容端点不支持 system role，以 user 消息注入
    let mut full_messages = Vec::new();
    if let Some(ref sp) = request.system_prompt
        && !sp.is_empty()
    {
        full_messages.push(rig::completion::Message::user(format!(
            "[System Instructions]\n{sp}"
        )));
    }
    full_messages.extend(llm_messages);

    let chat_history = if full_messages.is_empty() {
        OneOrMany::one(rig::completion::Message::user(""))
    } else {
        OneOrMany::many(full_messages)
            .map_err(|e| BridgeError::RequestBuildFailed(format!("消息列表构建失败: {e}")))?
    };

    Ok(CompletionRequest {
        preamble: None,
        chat_history,
        documents: vec![],
        tools: request.tools.clone(),
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        tool_choice: None,
        additional_params: None,
    })
}

#[async_trait]
impl<M> LlmBridge for RigBridge<M>
where
    M: CompletionModel + Send + Sync + 'static,
    M::Response: Send + Sync,
{
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>> {
        let rig_request = match build_rig_request(&request) {
            Ok(r) => r,
            Err(e) => {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                let _ = tx.send(StreamChunk::Error(e)).await;
                return Box::pin(ReceiverStream::new(rx));
            }
        };

        let mut rig_stream = match self.model.stream(rig_request).await {
            Ok(s) => s,
            Err(e) => {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                let _ = tx
                    .send(StreamChunk::Error(BridgeError::CompletionFailed(
                        e.to_string(),
                    )))
                    .await;
                return Box::pin(ReceiverStream::new(rx));
            }
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        tokio::spawn(async move {
            while let Some(chunk) = rig_stream.next().await {
                let sc = match chunk {
                    Ok(StreamedAssistantContent::Text(t)) => StreamChunk::TextDelta(t.text),
                    Ok(StreamedAssistantContent::Reasoning(reasoning)) => {
                        StreamChunk::ReasoningDelta {
                            id: reasoning.id.clone(),
                            text: reasoning.reasoning.join(""),
                            signature: reasoning.signature.clone(),
                        }
                    }
                    Ok(StreamedAssistantContent::ToolCall(tool_call)) => StreamChunk::ToolCall {
                        info: crate::types::ToolCallInfo {
                            id: tool_call.id.clone(),
                            call_id: tool_call.call_id.clone().or_else(|| Some(tool_call.id)),
                            name: tool_call.function.name.clone(),
                            arguments: tool_call.function.arguments.clone(),
                        },
                    },
                    Ok(StreamedAssistantContent::ToolCallDelta { id, delta }) => {
                        StreamChunk::ToolCallDelta { id, delta }
                    }
                    Ok(StreamedAssistantContent::Final(_)) => continue,
                    Err(e) => {
                        let _ = tx
                            .send(StreamChunk::Error(BridgeError::CompletionFailed(
                                e.to_string(),
                            )))
                            .await;
                        return;
                    }
                };
                if tx.send(sc).await.is_err() {
                    return;
                }
            }

            // Rig 的 StreamingCompletionResponse 在 Stream 结束时自动聚合 choice
            let raw_content: Vec<AssistantContent> =
                rig_stream.choice.clone().into_iter().collect();

            if raw_content.is_empty() {
                let _ = tx
                    .send(StreamChunk::Error(BridgeError::EmptyResponse))
                    .await;
                return;
            }

            let message = assistant_from_llm_content(&raw_content);
            let usage = rig_stream
                .response
                .as_ref()
                .and_then(|r| r.token_usage())
                .unwrap_or_default();

            let _ = tx
                .send(StreamChunk::Done(BridgeResponse {
                    message,
                    raw_content,
                    usage,
                }))
                .await;
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_request_builds_correctly() {
        let req = BridgeRequest {
            system_prompt: Some("你是一个助手".into()),
            messages: vec![AgentMessage::user("你好")],
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(4096),
            llm_messages: None,
        };
        assert!(req.system_prompt.is_some());
        assert_eq!(req.messages.len(), 1);
    }
}
