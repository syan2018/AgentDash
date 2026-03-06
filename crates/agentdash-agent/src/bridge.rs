/// LLM 桥接层 — Runtime 与 LLM 提供方之间的隔离层
///
/// 设计参考 pi-mono/packages/ai 的 streaming-first 架构：
/// - pi-mono: Provider → stream() → AssistantMessageEventStream
/// - 本模块: RigBridge → complete() 内部用 stream() → 收集完整响应
///
/// Rig 的 CompletionModel 提供 completion() (同步) 和 stream() (流式) 两种 API，
/// 许多 OpenAI 兼容端点仅支持流式响应，因此 RigBridge 统一使用 stream() 实现。
use async_trait::async_trait;
use futures::StreamExt;
use rig::completion::message::AssistantContent;
use rig::completion::{CompletionModel, CompletionRequest, Usage};
use rig::completion::request::GetTokenUsage;
use rig::OneOrMany;
use thiserror::Error;

use crate::convert::{assistant_from_llm_content, default_convert_to_llm};
use crate::types::AgentMessage;

// ─── Bridge 协议 ────────────────────────────────────────────

/// 桥接请求
#[derive(Debug, Clone)]
pub struct BridgeRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<rig::completion::ToolDefinition>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

/// 桥接响应
#[derive(Debug, Clone)]
pub struct BridgeResponse {
    pub message: AgentMessage,
    pub raw_content: Vec<AssistantContent>,
    pub usage: Usage,
}

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("LLM 调用失败: {0}")]
    CompletionFailed(String),
    #[error("LLM 响应为空")]
    EmptyResponse,
    #[error("请求构建失败: {0}")]
    RequestBuildFailed(String),
}

/// LLM 桥接层 trait — 对象安全，支持 `Arc<dyn LlmBridge>` 使用
#[async_trait]
pub trait LlmBridge: Send + Sync {
    async fn complete(&self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError>;
}

// ─── Rig 实现 ───────────────────────────────────────────────

/// 基于 Rig CompletionModel 的桥接实现
///
/// 内部使用 Rig 的 streaming API 消费 LLM 响应（兼容仅支持流式的端点），
/// 消费完毕后从聚合结果中提取完整的 assistant 内容和 token usage。
pub struct RigBridge<M: CompletionModel> {
    model: M,
}

impl<M: CompletionModel> RigBridge<M> {
    pub fn new(model: M) -> Self {
        Self { model }
    }
}

/// 构建 Rig CompletionRequest（从 BridgeRequest 转换）
fn build_rig_request(request: &BridgeRequest) -> Result<CompletionRequest, BridgeError> {
    let llm_messages = default_convert_to_llm(&request.messages);

    // 某些 OpenAI 兼容端点不支持 system role 消息，
    // 将 system prompt 作为首条 user 消息注入以保证兼容性。
    let mut full_messages = Vec::new();
    if let Some(ref sp) = request.system_prompt {
        if !sp.is_empty() {
            full_messages.push(rig::completion::Message::user(format!(
                "[System Instructions]\n{sp}"
            )));
        }
    }
    full_messages.extend(llm_messages);

    let chat_history = if full_messages.is_empty() {
        OneOrMany::one(rig::completion::Message::user(""))
    } else {
        OneOrMany::many(full_messages)
            .map_err(|e| BridgeError::RequestBuildFailed(format!("消息列表构建失败: {e}")))?
    };

    Ok(CompletionRequest {
        model: None,
        preamble: None,
        chat_history,
        documents: vec![],
        tools: request.tools.clone(),
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
    })
}

#[async_trait]
impl<M> LlmBridge for RigBridge<M>
where
    M: CompletionModel + Send + Sync + 'static,
    M::Response: Send + Sync,
{
    async fn complete(&self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        let rig_request = build_rig_request(&request)?;

        // 使用 Rig 的 streaming API：兼容仅支持流式的端点
        let mut stream = self
            .model
            .stream(rig_request)
            .await
            .map_err(|e| BridgeError::CompletionFailed(e.to_string()))?;

        // 消费所有 chunk，Rig 内部会自动聚合到 stream.choice
        while let Some(chunk) = stream.next().await {
            if let Err(e) = chunk {
                return Err(BridgeError::CompletionFailed(e.to_string()));
            }
        }

        let raw_content: Vec<AssistantContent> = stream.choice.into_iter().collect();
        if raw_content.is_empty() {
            return Err(BridgeError::EmptyResponse);
        }

        let message = assistant_from_llm_content(&raw_content);

        let usage = stream
            .response
            .and_then(|r| r.token_usage())
            .unwrap_or_default();

        Ok(BridgeResponse {
            message,
            raw_content,
            usage,
        })
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
        };
        assert!(req.system_prompt.is_some());
        assert_eq!(req.messages.len(), 1);
    }
}
