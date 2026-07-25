/// LLM 桥接层 — Streaming-first 架构
///
/// 设计参考 pi-mono/packages/ai：
/// - 所有 LLM 调用默认走 streaming
/// - `StreamChunk` 对应 pi-mono 的 `AssistantMessageEvent`
///
/// `LlmBridge` trait 定义在此；具体实现由 LLM Provider adapter 提供。
use std::pin::Pin;
use std::time::Duration;

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
    pub thinking_level: Option<crate::ThinkingLevel>,
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
    #[error("{message}")]
    Provider {
        message: String,
        classification: ProviderErrorClassification,
        provider: Option<String>,
        model: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    Retryable,
    Fatal,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderErrorClassification {
    pub kind: ProviderErrorKind,
    pub http_status: Option<u16>,
    pub provider_code: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub safe_to_retry_before_visible_delta: bool,
}

impl ProviderErrorClassification {
    pub fn retryable() -> Self {
        Self {
            kind: ProviderErrorKind::Retryable,
            http_status: None,
            provider_code: None,
            retry_after_ms: None,
            safe_to_retry_before_visible_delta: true,
        }
    }

    pub fn fatal() -> Self {
        Self {
            kind: ProviderErrorKind::Fatal,
            http_status: None,
            provider_code: None,
            retry_after_ms: None,
            safe_to_retry_before_visible_delta: false,
        }
    }

    pub fn aborted() -> Self {
        Self {
            kind: ProviderErrorKind::Aborted,
            http_status: None,
            provider_code: Some("aborted".to_string()),
            retry_after_ms: None,
            safe_to_retry_before_visible_delta: false,
        }
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_provider_code(mut self, code: impl Into<String>) -> Self {
        self.provider_code = Some(code.into());
        self
    }

    pub fn with_retry_after_ms(mut self, delay_ms: u64) -> Self {
        self.retry_after_ms = Some(delay_ms);
        self
    }

    pub fn safe_to_retry_before_visible_delta(mut self, safe: bool) -> Self {
        self.safe_to_retry_before_visible_delta = safe;
        self
    }

    pub fn is_retryable_before_visible_delta(&self) -> bool {
        self.kind == ProviderErrorKind::Retryable && self.safe_to_retry_before_visible_delta
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderRetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl ProviderRetryPolicy {
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
    pub const DEFAULT_BASE_DELAY_MS: u64 = 1_000;
    pub const DEFAULT_MAX_DELAY_MS: u64 = 60_000;

    pub fn delay_for_attempt(
        self,
        completed_attempt: u32,
        provider_retry_after_ms: Option<u64>,
    ) -> u64 {
        let backoff_power = completed_attempt.saturating_sub(1).min(20);
        let backoff = self
            .base_delay_ms
            .saturating_mul(2_u64.saturating_pow(backoff_power));
        provider_retry_after_ms
            .unwrap_or(backoff)
            .min(self.max_delay_ms)
    }
}

impl Default for ProviderRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: Self::DEFAULT_MAX_ATTEMPTS,
            base_delay_ms: Self::DEFAULT_BASE_DELAY_MS,
            max_delay_ms: Self::DEFAULT_MAX_DELAY_MS,
        }
    }
}

impl BridgeError {
    pub fn provider(
        message: impl Into<String>,
        classification: ProviderErrorClassification,
    ) -> Self {
        Self::Provider {
            message: message.into(),
            classification,
            provider: None,
            model: None,
        }
    }

    pub fn with_provider_context(
        self,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        match self {
            Self::Provider {
                message,
                classification,
                ..
            } => Self::Provider {
                message,
                classification,
                provider: Some(provider.into()),
                model: Some(model.into()),
            },
            other => other,
        }
    }

    pub fn classification(&self) -> ProviderErrorClassification {
        match self {
            Self::Provider { classification, .. } => classification.clone(),
            Self::RequestBuildFailed(message) => classify_legacy_error(message, false),
            Self::CompletionFailed(message) => classify_legacy_error(message, true),
            Self::EmptyResponse => {
                ProviderErrorClassification::retryable().with_provider_code("empty_response")
            }
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.classification().kind == ProviderErrorKind::Aborted
    }

    pub fn provider_label(&self) -> Option<&str> {
        match self {
            Self::Provider { provider, .. } => provider.as_deref(),
            _ => None,
        }
    }

    pub fn model_id(&self) -> Option<&str> {
        match self {
            Self::Provider { model, .. } => model.as_deref(),
            _ => None,
        }
    }
}

fn classify_legacy_error(message: &str, completion_failure: bool) -> ProviderErrorClassification {
    let lower = message.to_ascii_lowercase();
    if lower.contains("aborted") || lower.contains("cancelled") || lower.contains("canceled") {
        return ProviderErrorClassification::aborted();
    }
    if !completion_failure {
        return ProviderErrorClassification::fatal().with_provider_code("request_build_failed");
    }

    let retryable_http = ["429", "500", "502", "503", "504"]
        .iter()
        .any(|status| lower.contains(status));
    let retryable_text = [
        "rate limit",
        "ratelimit",
        "overloaded",
        "service unavailable",
        "temporarily unavailable",
        "timeout",
        "timed out",
        "connection reset",
        "connection refused",
        "connection closed",
        "connection aborted",
        "network error",
        "fetch failed",
        "reqwest",
        "stream disconnected",
        "stream ended",
        "empty response",
        "读取响应流失败",
        "http 请求失败",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    if retryable_http || retryable_text {
        ProviderErrorClassification::retryable().with_provider_code("transient_provider_error")
    } else {
        ProviderErrorClassification::fatal().with_provider_code("provider_error")
    }
}

pub async fn sleep_for_retry(
    delay_ms: u64,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), crate::types::AgentError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(crate::types::AgentError::Cancelled),
        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => Ok(()),
    }
}

/// LLM 桥接层 trait — 具体实现（OpenAI/Anthropic bridge 等）在 executor 层
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
