/// RigBridge — 基于 rig-core 的 LlmBridge 实现
///
/// 将 `agentdash-agent` 的 `BridgeRequest`/`BridgeResponse` 与 rig-core 的
/// `CompletionModel` API 桥接。所有 rig 类型的引用都限制在此模块中。
use std::pin::Pin;

use async_trait::async_trait;
use rig::OneOrMany;
use rig::completion::message::{AssistantContent, Message, Reasoning, Text, UserContent};
use rig::completion::request::GetTokenUsage;
use rig::completion::{CompletionModel, CompletionRequest, Usage};
use rig::streaming::StreamedAssistantContent;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{BridgeError, BridgeRequest, BridgeResponse, LlmBridge, StreamChunk};
use agentdash_agent::types::{AgentMessage, ContentPart, TokenUsage, ToolCallInfo, now_millis};

// ─── RigBridge ──────────────────────────────────────────────

pub struct RigBridge<M: CompletionModel> {
    model: M,
}

impl<M: CompletionModel> RigBridge<M> {
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
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
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
            use futures::StreamExt;

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
                        info: ToolCallInfo {
                            id: tool_call.id.clone(),
                            call_id: tool_call.call_id.clone().or(Some(tool_call.id)),
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
                    raw_content: extract_content_parts(&raw_content),
                    usage: from_rig_usage(&usage),
                }))
                .await;
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

// ─── rig request 构建 ───────────────────────────────────────

fn build_rig_request(request: &BridgeRequest) -> Result<CompletionRequest, BridgeError> {
    let llm_messages = convert_to_llm(&request.messages);

    let mut full_messages = Vec::new();
    if let Some(ref sp) = request.system_prompt
        && !sp.is_empty()
    {
        full_messages.push(Message::user(format!("[System Instructions]\n{sp}")));
    }
    full_messages.extend(llm_messages);

    let chat_history = if full_messages.is_empty() {
        OneOrMany::one(Message::user(""))
    } else {
        OneOrMany::many(full_messages)
            .map_err(|e| BridgeError::RequestBuildFailed(format!("消息列表构建失败: {e}")))?
    };

    let tools: Vec<rig::completion::ToolDefinition> = request
        .tools
        .iter()
        .map(|t| rig::completion::ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect();

    Ok(CompletionRequest {
        preamble: None,
        chat_history,
        documents: vec![],
        tools,
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
    })
}

// ─── AgentMessage ↔ rig::Message 转换 ──────────────────────

pub fn convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages.iter().filter_map(agent_to_llm).collect()
}

fn agent_to_llm(msg: &AgentMessage) -> Option<Message> {
    match msg {
        AgentMessage::User { content, .. } => {
            let parts: Vec<UserContent> = content.iter().filter_map(content_part_to_user).collect();
            if parts.is_empty() {
                return None;
            }
            Some(Message::User {
                content: OneOrMany::many(parts).unwrap_or_else(|_| unreachable!()),
            })
        }
        AgentMessage::Assistant {
            content,
            tool_calls,
            ..
        } => {
            let mut parts: Vec<AssistantContent> = content
                .iter()
                .filter_map(content_part_to_assistant)
                .collect();

            for tc in tool_calls {
                let call_id = tc.call_id.clone().unwrap_or_else(|| tc.id.clone());
                parts.push(AssistantContent::tool_call_with_call_id(
                    tc.id.clone(),
                    call_id,
                    tc.name.clone(),
                    tc.arguments.clone(),
                ));
            }

            if parts.is_empty() {
                return None;
            }
            Some(Message::Assistant {
                id: None,
                content: OneOrMany::many(parts).unwrap_or_else(|_| unreachable!()),
            })
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

            Some(Message::tool_result_with_call_id(
                tool_call_id.clone(),
                call_id.clone().or_else(|| Some(tool_call_id.clone())),
                text,
            ))
        }
    }
}

fn content_part_to_user(part: &ContentPart) -> Option<UserContent> {
    match part {
        ContentPart::Text { text } => Some(UserContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } | ContentPart::Reasoning { .. } => None,
    }
}

fn content_part_to_assistant(part: &ContentPart) -> Option<AssistantContent> {
    match part {
        ContentPart::Text { text } => Some(AssistantContent::Text(Text { text: text.clone() })),
        ContentPart::Image { .. } => None,
        ContentPart::Reasoning {
            text,
            id,
            signature,
        } => Some(AssistantContent::Reasoning(
            Reasoning::new(text)
                .optional_id(id.clone())
                .with_signature(signature.clone()),
        )),
    }
}

pub fn assistant_from_llm_content(content: &[AssistantContent]) -> AgentMessage {
    let mut parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in content {
        match item {
            AssistantContent::Text(t) => {
                parts.push(ContentPart::text(&t.text));
            }
            AssistantContent::ToolCall(tc) => {
                tool_calls.push(ToolCallInfo {
                    id: tc.id.clone(),
                    call_id: tc.call_id.clone().or_else(|| Some(tc.id.clone())),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                });
            }
            AssistantContent::Reasoning(reasoning) => {
                let text = reasoning.reasoning.join("");
                parts.push(ContentPart::reasoning(
                    text,
                    reasoning.id.clone(),
                    reasoning.signature.clone(),
                ));
            }
        }
    }

    AgentMessage::Assistant {
        content: parts,
        tool_calls,
        stop_reason: None,
        error_message: None,
        usage: None,
        timestamp: Some(now_millis()),
    }
}

// ─── 辅助转换 ───────────────────────────────────────────────

fn extract_content_parts(content: &[AssistantContent]) -> Vec<ContentPart> {
    content
        .iter()
        .filter_map(|item| match item {
            AssistantContent::Text(t) => Some(ContentPart::text(&t.text)),
            AssistantContent::Reasoning(r) => Some(ContentPart::reasoning(
                r.reasoning.join(""),
                r.id.clone(),
                r.signature.clone(),
            )),
            AssistantContent::ToolCall(_) => None,
        })
        .collect()
}

fn from_rig_usage(usage: &Usage) -> TokenUsage {
    TokenUsage {
        input: usage.input_tokens,
        output: usage.output_tokens,
    }
}
