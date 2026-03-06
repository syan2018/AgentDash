/// LLM 桥接层 — Runtime 与 LLM 提供方之间的隔离层
///
/// 设计原则：Agent Runtime 不直接调用 Rig，通过 Bridge trait 隔离。
/// 这使得运行时可以对接不同的 LLM 后端（Rig、直连 HTTP、Mock 等）。
use async_trait::async_trait;
use rig::completion::message::AssistantContent;
use rig::completion::{CompletionModel, CompletionRequest, Usage};
use rig::OneOrMany;
use thiserror::Error;

use crate::types::AgentMessage;
use crate::convert::{assistant_from_llm_content, default_convert_to_llm};

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
    /// 模型返回的 assistant 消息（已转换为 AgentMessage）
    pub message: AgentMessage,
    /// 原始 AssistantContent 列表（用于判断是否有 tool_calls 等）
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
/// 泛型 `M` 可以是 `rig::providers::anthropic::CompletionModel` 或其他
/// 实现了 `rig::completion::CompletionModel` trait 的类型。
pub struct RigBridge<M>
where
    M: CompletionModel,
{
    model: M,
}

impl<M> RigBridge<M>
where
    M: CompletionModel,
{
    pub fn new(model: M) -> Self {
        Self { model }
    }
}

#[async_trait]
impl<M> LlmBridge for RigBridge<M>
where
    M: CompletionModel + Send + Sync + 'static,
    M::Response: Send + Sync,
{
    async fn complete(&self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        let llm_messages = default_convert_to_llm(&request.messages);

        let chat_history = if llm_messages.is_empty() {
            OneOrMany::one(rig::completion::Message::user(""))
        } else {
            OneOrMany::many(llm_messages)
                .map_err(|e| BridgeError::RequestBuildFailed(format!("消息列表构建失败: {e}")))?
        };

        let rig_request = CompletionRequest {
            model: None,
            preamble: request.system_prompt,
            chat_history,
            documents: vec![],
            tools: request.tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };

        let response = self
            .model
            .completion(rig_request)
            .await
            .map_err(|e| BridgeError::CompletionFailed(e.to_string()))?;

        let raw_content: Vec<AssistantContent> = response.choice.into_iter().collect();
        if raw_content.is_empty() {
            return Err(BridgeError::EmptyResponse);
        }

        let message = assistant_from_llm_content(&raw_content);

        Ok(BridgeResponse {
            message,
            raw_content,
            usage: response.usage,
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
