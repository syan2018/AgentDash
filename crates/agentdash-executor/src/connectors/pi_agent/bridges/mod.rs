mod anthropic_bridge;
mod openai_codex_responses_bridge;
mod openai_completions_bridge;
mod openai_content;
mod openai_responses_bridge;
mod openai_responses_common;
pub mod provider_registry;
mod sse;

pub(crate) use anthropic_bridge::AnthropicBridge;
pub(crate) use openai_codex_responses_bridge::OpenAiCodexResponsesBridge;
pub(crate) use openai_completions_bridge::OpenAiCompletionsBridge;
pub(crate) use openai_responses_bridge::OpenAiResponsesBridge;

use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{BridgeError, StreamChunk};

/// 各 bridge `stream_complete` 共用的流脚手架：建立 64 容量 channel、`tokio::spawn`
/// 运行 `run`，并在其返回 `Err` 时把错误作为 `StreamChunk::Error` 转发给消费方。
pub(super) fn spawn_bridge_stream<F, Fut>(
    run: F,
) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>
where
    F: FnOnce(Sender<StreamChunk>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), BridgeError>> + Send,
{
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

    tokio::spawn(async move {
        if let Err(error) = run(tx.clone()).await {
            let _ = tx.send(StreamChunk::Error(error)).await;
        }
    });

    Box::pin(ReceiverStream::new(rx))
}

/// 校验 HTTP 响应状态：非 2xx 时读出 body 并组装统一的 `{api_label} 返回 {status}: {body}`
/// 错误。`api_label` 保留各 bridge 既有前缀（如 `"API"`）。
pub(super) async fn check_http_response(
    response: reqwest::Response,
    api_label: &str,
) -> Result<reqwest::Response, BridgeError> {
    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(BridgeError::CompletionFailed(format!(
            "{api_label} 返回 {status}: {body_text}"
        )));
    }
    Ok(response)
}
