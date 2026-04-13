//! LLM 驱动的会话标题自动生成实现。

use std::sync::Arc;

use agentdash_agent::{AgentMessage, BridgeRequest, ContentPart, LlmBridge};
use agentdash_application::session::SessionTitleGenerator;
use async_trait::async_trait;

const TITLE_SYSTEM_PROMPT: &str = "\
你是一个标题生成器。根据用户发送的首条消息，生成一个简短的会话标题。

要求：
- 不超过 15 个字
- 只输出标题文本，不要加引号、标点或任何额外说明
- 使用中文";

pub struct LlmTitleGenerator {
    bridge: Arc<dyn LlmBridge>,
}

impl LlmTitleGenerator {
    pub fn new(bridge: Arc<dyn LlmBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait]
impl SessionTitleGenerator for LlmTitleGenerator {
    async fn generate_title(&self, user_prompt: &str) -> Result<String, String> {
        let request = BridgeRequest {
            system_prompt: Some(TITLE_SYSTEM_PROMPT.to_string()),
            messages: vec![AgentMessage::user(user_prompt)],
            tools: vec![],
        };

        let response = self
            .bridge
            .complete(request)
            .await
            .map_err(|e| format!("LLM 调用失败: {e}"))?;

        let text: String = response
            .raw_content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        let title = text.trim().to_string();
        if title.is_empty() {
            return Err("LLM 返回了空内容".to_string());
        }
        Ok(title)
    }
}
