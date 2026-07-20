use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent::{
    AgentMessage, BridgeError, BridgeRequest, ContentPart, LlmBridge, ProviderErrorKind,
    StopReason, StreamChunk, ThinkingLevel, ToolDefinition,
    dash::{
        CompactionId, ContextRevision, DashCompactionRequest, DashCompactionResult, DashCompactor,
        DashCoreError, DashExecutionCallbacks, DashExecutionDependencies, DashExecutionEvent,
        DashFinishReason, DashMessageRole, DashProvider, DashProviderEvent,
        DashProviderEventStream, DashProviderRequest, DashServiceError, DashToolCall,
        DashToolCallbacks, DashToolResult, HistoryEntryId, HistoryPayload,
    },
};
use async_trait::async_trait;
use futures::StreamExt;
use sha2::{Digest, Sha256};

const DEFAULT_RETAINED_CONVERSATION_MESSAGES: usize = 8;

/// Production provider adapter from the provider-neutral `LlmBridge` to the minimal Dash Agent
/// Core provider port.
pub struct BridgeDashProvider {
    bridge: Arc<dyn LlmBridge>,
    thinking_level: Option<ThinkingLevel>,
}

impl BridgeDashProvider {
    pub fn new(bridge: Arc<dyn LlmBridge>, thinking_level: Option<ThinkingLevel>) -> Self {
        Self {
            bridge,
            thinking_level,
        }
    }
}

#[async_trait]
impl DashProvider for BridgeDashProvider {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        let request = bridge_request(request, self.thinking_level)?;
        let stream = self.bridge.stream_complete(request).await;
        Ok(Box::pin(stream.filter_map(|chunk| async move {
            match chunk {
                StreamChunk::TextDelta(delta) => Some(Ok(DashProviderEvent::TextDelta { delta })),
                StreamChunk::ReasoningDelta { text, .. } => {
                    Some(Ok(DashProviderEvent::ReasoningDelta { delta: text }))
                }
                StreamChunk::ToolCall { info } => Some(Ok(DashProviderEvent::ToolCall {
                    call: DashToolCall {
                        call_id: info.call_id.unwrap_or(info.id),
                        name: info.name,
                        arguments: info.arguments,
                    },
                })),
                StreamChunk::Done(response) => {
                    let finish_reason = match &response.message {
                        AgentMessage::Assistant {
                            tool_calls,
                            stop_reason,
                            ..
                        } if !tool_calls.is_empty()
                            || matches!(stop_reason, Some(StopReason::ToolUse)) =>
                        {
                            DashFinishReason::ToolCalls
                        }
                        _ => DashFinishReason::Stop,
                    };
                    Some(Ok(DashProviderEvent::Completed {
                        finish_reason,
                        input_tokens: response.usage.context_input_tokens(),
                        output_tokens: response.usage.output,
                    }))
                }
                StreamChunk::Error(error) => Some(Err(map_bridge_error(error))),
                StreamChunk::ToolCallDelta { .. } => None,
            }
        })))
    }
}

/// Agent-owned LLM compaction implementation. It summarizes the effective prefix and keeps a
/// bounded recent tail identified by a durable history entry coordinate.
pub struct BridgeDashCompactor {
    bridge: Arc<dyn LlmBridge>,
    thinking_level: Option<ThinkingLevel>,
    retained_conversation_messages: usize,
}

impl BridgeDashCompactor {
    pub fn new(bridge: Arc<dyn LlmBridge>, thinking_level: Option<ThinkingLevel>) -> Self {
        Self {
            bridge,
            thinking_level,
            retained_conversation_messages: DEFAULT_RETAINED_CONVERSATION_MESSAGES,
        }
    }

    pub fn with_retained_conversation_messages(mut self, count: usize) -> Self {
        self.retained_conversation_messages = count;
        self
    }
}

#[async_trait]
impl DashCompactor for BridgeDashCompactor {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        let effective = effective_conversation(&request);
        if effective.messages.is_empty() && effective.previous_summary.is_none() {
            return Err(DashServiceError::InvalidState {
                message: "Agent history has no provider-visible content to compact".to_owned(),
            });
        }

        let retained_count = self
            .retained_conversation_messages
            .min(effective.messages.len().saturating_sub(1));
        let cut = effective.messages.len().saturating_sub(retained_count);
        let retained_from = effective
            .messages
            .get(cut)
            .map(|message| message.entry_id.clone());
        let mut summary_messages = Vec::new();
        if let Some(summary) = effective.previous_summary {
            summary_messages.push(AgentMessage::User {
                content: vec![ContentPart::text(format!(
                    "Existing compacted context:\n{summary}"
                ))],
                timestamp: None,
            });
        }
        summary_messages.extend(
            effective.messages[..cut]
                .iter()
                .map(|message| message.message.clone()),
        );
        let response = self
            .bridge
            .complete(BridgeRequest {
                system_prompt: Some(
                    "Summarize the durable Agent conversation context for exact continuation. \
                     Preserve decisions, constraints, unresolved work, tool outcomes, stable \
                     identifiers, and branch-relevant facts. Do not add commentary."
                        .to_owned(),
                ),
                messages: summary_messages,
                tools: Vec::new(),
                thinking_level: self.thinking_level,
            })
            .await
            .map_err(map_compaction_error)?;
        let summary = message_text(&response.message);
        if summary.trim().is_empty() {
            return Err(DashServiceError::InvalidState {
                message: "Dash Agent compactor returned an empty summary".to_owned(),
            });
        }
        let revision = compaction_revision(
            &request.compaction_id,
            &request.source_digest,
            &summary,
            retained_from.as_ref(),
        );
        Ok(DashCompactionResult {
            revision,
            summary,
            retained_from,
        })
    }
}

pub fn bridge_dash_execution_dependencies(
    bridge: Arc<dyn LlmBridge>,
    thinking_level: Option<ThinkingLevel>,
) -> DashExecutionDependencies {
    DashExecutionDependencies {
        provider: Arc::new(BridgeDashProvider::new(bridge.clone(), thinking_level)),
        tools: Arc::new(UnboundDashToolCallbacks),
        callbacks: Arc::new(UnboundDashExecutionCallbacks),
        compactor: Arc::new(BridgeDashCompactor::new(bridge, thinking_level)),
    }
}

struct UnboundDashToolCallbacks;

#[async_trait]
impl DashToolCallbacks for UnboundDashToolCallbacks {
    async fn invoke(
        &self,
        _turn_id: &agentdash_agent::dash::AgentTurnId,
        _call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        Err(DashCoreError::Tool {
            message: "Dash Agent tool callbacks are not bound to an applied surface".to_owned(),
            retryable: false,
        })
    }
}

struct UnboundDashExecutionCallbacks;

#[async_trait]
impl DashExecutionCallbacks for UnboundDashExecutionCallbacks {
    async fn emit(&self, _event: DashExecutionEvent) -> Result<(), DashCoreError> {
        Err(DashCoreError::Callback {
            message: "Dash execution has no source-scoped Complete Agent live sink".to_owned(),
        })
    }
}

struct EffectiveConversation {
    previous_summary: Option<String>,
    messages: Vec<ConversationMessage>,
}

struct ConversationMessage {
    entry_id: HistoryEntryId,
    message: AgentMessage,
}

fn effective_conversation(request: &DashCompactionRequest) -> EffectiveConversation {
    let entries = request.history.entries();
    let mut applied = BTreeMap::new();
    let mut latest = None;
    for (index, entry) in entries.iter().enumerate() {
        match &entry.payload {
            HistoryPayload::CompactionApplied {
                compaction_id,
                summary,
                retained_from,
                ..
            } => {
                applied.insert(
                    compaction_id.clone(),
                    (index, summary.clone(), retained_from.clone()),
                );
            }
            HistoryPayload::CompactionCompleted { compaction_id } => {
                if let Some((applied_index, summary, retained_from)) =
                    applied.get(compaction_id).cloned()
                {
                    latest = Some((index, applied_index, summary, retained_from));
                }
            }
            _ => {}
        }
    }
    let (previous_summary, start) = latest
        .map(
            |(completed_index, _applied_index, summary, retained_from)| {
                let start = retained_from
                    .as_ref()
                    .and_then(|id| entries.iter().position(|entry| &entry.entry_id == id))
                    .unwrap_or(completed_index.saturating_add(1));
                (Some(summary), start)
            },
        )
        .unwrap_or((None, 0));
    let messages = entries[start..]
        .iter()
        .filter_map(|entry| match &entry.payload {
            HistoryPayload::InputAccepted { content, .. } => Some(ConversationMessage {
                entry_id: entry.entry_id.clone(),
                message: AgentMessage::User {
                    content: vec![ContentPart::text(content.clone())],
                    timestamp: None,
                },
            }),
            HistoryPayload::AgentOutput { content, .. } => Some(ConversationMessage {
                entry_id: entry.entry_id.clone(),
                message: AgentMessage::Assistant {
                    content: vec![ContentPart::text(content.clone())],
                    tool_calls: Vec::new(),
                    stop_reason: Some(StopReason::Stop),
                    error_message: None,
                    usage: None,
                    timestamp: None,
                },
            }),
            _ => None,
        })
        .collect();
    EffectiveConversation {
        previous_summary,
        messages,
    }
}

fn bridge_request(
    request: DashProviderRequest,
    thinking_level: Option<ThinkingLevel>,
) -> Result<BridgeRequest, DashCoreError> {
    let messages = request
        .messages
        .into_iter()
        .map(|message| match message.role {
            DashMessageRole::User => Ok(AgentMessage::User {
                content: vec![ContentPart::text(message.content)],
                timestamp: None,
            }),
            DashMessageRole::Assistant => Ok(AgentMessage::Assistant {
                content: vec![ContentPart::text(message.content)],
                tool_calls: Vec::new(),
                stop_reason: None,
                error_message: None,
                usage: None,
                timestamp: None,
            }),
            DashMessageRole::Tool => {
                let Some(tool_call_id) = message.tool_call_id else {
                    return Err(DashCoreError::Provider {
                        code: "provider_transcript_invalid".to_owned(),
                        message: "provider-visible tool result is missing tool_call_id".to_owned(),
                        retryable: false,
                    });
                };
                Ok(AgentMessage::ToolResult {
                    tool_call_id,
                    call_id: None,
                    tool_name: None,
                    content: vec![ContentPart::text(message.content)],
                    details: None,
                    is_error: false,
                    timestamp: None,
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BridgeRequest {
        system_prompt: (!request.system_prompt.trim().is_empty()).then_some(request.system_prompt),
        messages,
        tools: request
            .tools
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            })
            .collect(),
        thinking_level,
    })
}

fn map_bridge_error(error: BridgeError) -> DashCoreError {
    let classification = error.classification();
    let message = error.to_string();
    let normalized = message.to_ascii_lowercase();
    let provider_code = classification
        .provider_code
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if normalized.contains("context length")
        || normalized.contains("context window")
        || normalized.contains("maximum context")
        || provider_code.contains("context_length")
        || provider_code.contains("context_window")
    {
        DashCoreError::ContextOverflow
    } else if classification.kind == ProviderErrorKind::Aborted {
        DashCoreError::Cancelled
    } else {
        DashCoreError::Provider {
            code: if provider_code.is_empty() {
                match classification.kind {
                    ProviderErrorKind::Retryable => "provider_retryable_error",
                    ProviderErrorKind::Fatal => "provider_fatal_error",
                    ProviderErrorKind::Aborted => "provider_aborted",
                }
                .to_owned()
            } else {
                provider_code
            },
            message,
            retryable: classification.kind == ProviderErrorKind::Retryable,
        }
    }
}

fn map_compaction_error(error: BridgeError) -> DashServiceError {
    let classification = error.classification();
    if classification.kind == ProviderErrorKind::Aborted {
        return DashServiceError::Lost {
            message: error.to_string(),
        };
    }
    DashServiceError::Unavailable {
        message: error.to_string(),
        retryable: classification.kind == ProviderErrorKind::Retryable,
    }
}

fn message_text(message: &AgentMessage) -> String {
    match message {
        AgentMessage::Assistant { content, .. }
        | AgentMessage::User { content, .. }
        | AgentMessage::ToolResult { content, .. } => content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => {
                    Some(text.as_str())
                }
                ContentPart::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        AgentMessage::CompactionSummary { summary, .. } => summary.clone(),
    }
}

fn compaction_revision(
    compaction_id: &CompactionId,
    source_digest: &str,
    summary: &str,
    retained_from: Option<&HistoryEntryId>,
) -> ContextRevision {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash.dash-compaction/v1\0");
    hasher.update(compaction_id.0.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_digest.as_bytes());
    hasher.update(b"\0");
    hasher.update(summary.as_bytes());
    hasher.update(b"\0");
    if let Some(retained_from) = retained_from {
        hasher.update(retained_from.0.as_bytes());
    }
    ContextRevision::new(format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::{
        BridgeResponse, TokenUsage,
        dash::{AgentHistory, AgentSessionId, BranchId, HistoryContribution},
    };
    use futures::stream;

    struct FixtureBridge;

    #[async_trait]
    impl LlmBridge for FixtureBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            Box::pin(stream::iter([StreamChunk::Done(BridgeResponse {
                message: AgentMessage::assistant("durable summary"),
                raw_content: vec![ContentPart::text("durable summary")],
                usage: TokenUsage::default(),
            })]))
        }
    }

    #[test]
    fn dash_profile_thinking_level_reaches_provider_request() {
        let request = bridge_request(
            DashProviderRequest {
                system_prompt: "system".to_owned(),
                messages: Vec::new(),
                tools: Vec::new(),
                round: 1,
            },
            Some(ThinkingLevel::High),
        )
        .expect("Dash request should map to bridge request");

        assert_eq!(request.thinking_level, Some(ThinkingLevel::High));
    }

    #[tokio::test]
    async fn compactor_uses_durable_entry_as_retained_boundary() {
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session"), BranchId::new("branch"));
        for (id, payload) in [
            (
                "input-1",
                HistoryPayload::InputAccepted {
                    input_id: "1".to_owned(),
                    content: "first".to_owned(),
                },
            ),
            (
                "input-2",
                HistoryPayload::InputAccepted {
                    input_id: "2".to_owned(),
                    content: "answer".to_owned(),
                },
            ),
            (
                "input-3",
                HistoryPayload::InputAccepted {
                    input_id: "3".to_owned(),
                    content: "second".to_owned(),
                },
            ),
        ] {
            history
                .append(HistoryContribution {
                    entry_id: HistoryEntryId::new(id),
                    payload,
                })
                .unwrap();
        }
        let result = BridgeDashCompactor::new(Arc::new(FixtureBridge), Some(ThinkingLevel::Off))
            .with_retained_conversation_messages(1)
            .compact(DashCompactionRequest {
                compaction_id: CompactionId::new("compact"),
                mode: agentdash_agent::dash::CompactionMode::Manual,
                source_head: history.head().cloned(),
                source_digest: history.digest(),
                history,
            })
            .await
            .unwrap();
        assert_eq!(result.retained_from, Some(HistoryEntryId::new("input-3")));
        assert!(result.revision.0.starts_with("sha256:"));
    }
}
