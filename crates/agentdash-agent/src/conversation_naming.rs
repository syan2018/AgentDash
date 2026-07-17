use std::sync::Arc;

use thiserror::Error;

use crate::{AgentMessage, BridgeError, BridgeRequest, LlmBridge};

const MAX_CONVERSATION_NAME_CHARS: usize = 22;
const NAMING_SYSTEM_PROMPT: &str = "根据用户本轮目标和助手最终回答生成一个简洁、可区分的会话标题。只输出标题，不要解释，不要使用 Markdown。";

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationNamingInput {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationName(String);

impl ConversationName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Error)]
pub enum ConversationNamingError {
    #[error("会话命名输入必须包含消息")]
    EmptyInput,
    #[error("会话命名模型没有返回有效标题")]
    InvalidOutput,
    #[error(transparent)]
    Bridge(#[from] BridgeError),
}

#[derive(Clone)]
pub struct ConversationNamer {
    bridge: Arc<dyn LlmBridge>,
}

impl ConversationNamer {
    pub fn new(bridge: Arc<dyn LlmBridge>) -> Self {
        Self { bridge }
    }

    pub async fn generate(
        &self,
        input: ConversationNamingInput,
    ) -> Result<ConversationName, ConversationNamingError> {
        if input.messages.is_empty() {
            return Err(ConversationNamingError::EmptyInput);
        }
        let response = self
            .bridge
            .complete(BridgeRequest {
                system_prompt: Some(NAMING_SYSTEM_PROMPT.to_string()),
                messages: input.messages,
                tools: Vec::new(),
            })
            .await?;
        normalize_conversation_name(
            response
                .message
                .first_text()
                .ok_or(ConversationNamingError::InvalidOutput)?,
        )
    }
}

fn normalize_conversation_name(value: &str) -> Result<ConversationName, ConversationNamingError> {
    let mut value = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    value = value.trim().to_string();
    while value.starts_with('#') {
        value.remove(0);
        value = value.trim_start().to_string();
    }
    loop {
        let stripped = strip_wrapping_pair(&value, "**", "**")
            .or_else(|| strip_wrapping_pair(&value, "\"", "\""))
            .or_else(|| strip_wrapping_pair(&value, "'", "'"))
            .or_else(|| strip_wrapping_pair(&value, "`", "`"))
            .or_else(|| strip_wrapping_pair(&value, "“", "”"))
            .or_else(|| strip_wrapping_pair(&value, "‘", "’"));
        match stripped {
            Some(stripped) if stripped != value => value = stripped,
            _ => break,
        }
    }
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = value
        .chars()
        .take(MAX_CONVERSATION_NAME_CHARS)
        .collect::<String>();
    let value = value.trim();
    if value.is_empty() {
        return Err(ConversationNamingError::InvalidOutput);
    }
    Ok(ConversationName(value.to_string()))
}

fn strip_wrapping_pair(value: &str, open: &str, close: &str) -> Option<String> {
    value
        .strip_prefix(open)
        .and_then(|value| value.strip_suffix(close))
        .map(str::trim)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures::stream;

    use super::*;
    use crate::{BridgeResponse, ContentPart, StreamChunk, TokenUsage};

    struct FixedBridge {
        response: Result<String, BridgeError>,
    }

    #[async_trait::async_trait]
    impl LlmBridge for FixedBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            match &self.response {
                Ok(value) => Box::pin(stream::iter(vec![StreamChunk::Done(BridgeResponse {
                    message: AgentMessage::assistant(value.clone()),
                    raw_content: vec![ContentPart::text(value.clone())],
                    usage: TokenUsage::default(),
                })])),
                Err(error) => Box::pin(stream::iter(vec![StreamChunk::Error(error.clone())])),
            }
        }
    }

    fn namer(response: Result<&str, BridgeError>) -> ConversationNamer {
        ConversationNamer::new(Arc::new(FixedBridge {
            response: response.map(str::to_string),
        }))
    }

    #[tokio::test]
    async fn normalizes_markdown_quotes_lines_and_unicode_length() {
        let name = namer(Ok("  ## “跨层\n会话名称治理与自动刷新验证”  "))
            .generate(ConversationNamingInput {
                messages: vec![
                    AgentMessage::user("修复会话名称"),
                    AgentMessage::assistant("已经完成"),
                ],
            })
            .await
            .expect("normalized name");
        assert_eq!(name.as_str(), "跨层 会话名称治理与自动刷新验证");
        assert!(name.as_str().chars().count() <= MAX_CONVERSATION_NAME_CHARS);
    }

    #[tokio::test]
    async fn rejects_empty_output_and_propagates_bridge_failure() {
        assert!(matches!(
            namer(Ok(" \n "))
                .generate(ConversationNamingInput {
                    messages: vec![AgentMessage::user("input")],
                })
                .await,
            Err(ConversationNamingError::InvalidOutput)
        ));
        assert!(matches!(
            namer(Err(BridgeError::CompletionFailed("failed".to_string())))
                .generate(ConversationNamingInput {
                    messages: vec![AgentMessage::user("input")],
                })
                .await,
            Err(ConversationNamingError::Bridge(_))
        ));
    }

    #[test]
    fn truncates_by_unicode_scalar_count() {
        let name = normalize_conversation_name("一二三四五六七八九十一二三四五六七八九十一二三四")
            .expect("long title is normalized");
        assert_eq!(name.as_str().chars().count(), MAX_CONVERSATION_NAME_CHARS);
        assert_eq!(
            name.as_str(),
            "一二三四五六七八九十一二三四五六七八九十一二"
        );
    }

    #[test]
    fn removes_nested_markdown_and_quote_wrappers() {
        assert_eq!(
            normalize_conversation_name("**“会话名称”**")
                .expect("nested wrappers")
                .as_str(),
            "会话名称"
        );
    }

    #[test]
    fn rejects_empty_wrapped_output() {
        for value in ["\"\"", "''", "“”", "‘’", "``", "****", "** **"] {
            assert!(
                matches!(
                    normalize_conversation_name(value),
                    Err(ConversationNamingError::InvalidOutput)
                ),
                "{value:?} must not become a conversation name"
            );
        }
    }
}
