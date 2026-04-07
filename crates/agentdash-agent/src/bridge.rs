/// LLM 桥接层 — Streaming-first 架构
///
/// 设计参考 pi-mono/packages/ai：
/// - 所有 LLM 调用默认走 streaming
/// - `StreamChunk` 对应 pi-mono 的 `AssistantMessageEvent`
///
/// `LlmBridge` trait 定义在此；具体实现（如 `RigBridge`）在 `agentdash-executor` 中。
use std::pin::Pin;

use async_trait::async_trait;
use futures::StreamExt;
use thiserror::Error;

use crate::types::{AgentMessage, ContentPart, TokenUsage, ToolCallInfo, ToolDefinition};

// ─── 流式 Chunk 类型 ────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ToolCallDeltaContent {
    Name(String),
    Arguments(String),
}

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
        content: ToolCallDeltaContent,
    },
    ToolCall {
        info: ToolCallInfo,
    },
    Done(BridgeResponse),
    Error(BridgeError),
}

// ─── Bridge 协议 ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BridgeRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone)]
pub struct BridgeResponse {
    pub message: AgentMessage,
    pub raw_content: Vec<ContentPart>,
    pub usage: TokenUsage,
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

/// LLM 桥接层 trait — 具体实现（RigBridge 等）在 executor 层
#[async_trait]
pub trait LlmBridge: Send + Sync {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>;

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
