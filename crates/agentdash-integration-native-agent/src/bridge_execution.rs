use std::sync::Arc;

use agentdash_agent::{
    AgentMessage, BridgeError, BridgeRequest, ContentPart, ConversationNamer,
    ConversationNamingInput, LlmBridge, ProviderErrorKind, StopReason, StreamChunk, ThinkingLevel,
    ToolDefinition,
    dash::{
        DashCompactionRequest, DashCompactionResult, DashCompactionTurn, DashCompactor,
        DashConversationNamer, DashConversationNamingRequest, DashCoreError,
        DashExecutionCallbacks, DashExecutionDependencies, DashExecutionEvent, DashFinishReason,
        DashMessageRole, DashProvider, DashProviderEvent, DashProviderEventStream,
        DashProviderRequest, DashServiceError, DashToolCall, DashToolCallbacks,
        NoopDashHistoryCallbacks,
    },
};
use async_trait::async_trait;
use futures::{StreamExt, stream};

const DEFAULT_RETAINED_CONVERSATION_MESSAGES: usize = 8;

/// Production provider adapter from the provider-neutral `LlmBridge` to the minimal Dash Agent
/// Core provider port.
pub struct BridgeDashProvider {
    bridge: Arc<dyn LlmBridge>,
    thinking_level: Option<ThinkingLevel>,
    context_window: u64,
}

pub struct BridgeDashConversationNamer {
    namer: ConversationNamer,
}

impl BridgeDashConversationNamer {
    pub fn new(bridge: Arc<dyn LlmBridge>) -> Self {
        Self {
            namer: ConversationNamer::new(bridge),
        }
    }
}

#[async_trait]
impl DashConversationNamer for BridgeDashConversationNamer {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError> {
        let messages = request
            .messages
            .into_iter()
            .filter_map(|message| match message.role {
                DashMessageRole::User => Some(AgentMessage::user(message.content)),
                DashMessageRole::Assistant => Some(AgentMessage::assistant(message.content)),
                DashMessageRole::Tool => None,
            })
            .collect();
        self.namer
            .generate(ConversationNamingInput { messages })
            .await
            .map(|name| name.into_string())
            .map_err(|error| DashServiceError::Unavailable {
                message: error.to_string(),
                retryable: false,
            })
    }
}

impl BridgeDashProvider {
    pub fn new(
        bridge: Arc<dyn LlmBridge>,
        thinking_level: Option<ThinkingLevel>,
        context_window: u64,
    ) -> Self {
        Self {
            bridge,
            thinking_level,
            context_window,
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
        let context_window = self.context_window;
        let stream = self.bridge.stream_complete(request).await;
        Ok(Box::pin(stream.flat_map(move |chunk| {
            let events = match chunk {
                StreamChunk::TextDelta(delta) => {
                    vec![Ok(DashProviderEvent::TextDelta { delta })]
                }
                StreamChunk::ReasoningDelta { text, .. } => {
                    vec![Ok(DashProviderEvent::ReasoningDelta { delta: text })]
                }
                StreamChunk::Done(response) => {
                    let input_tokens = response.usage.context_input_tokens();
                    let output_tokens = response.usage.output;
                    let (tool_calls, stop_reason) = match response.message {
                        AgentMessage::Assistant {
                            tool_calls,
                            stop_reason,
                            ..
                        } => (tool_calls, stop_reason),
                        _ => (Vec::new(), None),
                    };
                    let finish_reason = if !tool_calls.is_empty()
                        || matches!(stop_reason, Some(StopReason::ToolUse))
                    {
                        DashFinishReason::ToolCalls
                    } else {
                        DashFinishReason::Stop
                    };
                    let mut events = tool_calls
                        .into_iter()
                        .map(|info| {
                            Ok(DashProviderEvent::ToolCall {
                                call: DashToolCall {
                                    call_id: info.call_id.unwrap_or(info.id),
                                    name: info.name,
                                    arguments: info.arguments,
                                },
                            })
                        })
                        .collect::<Vec<_>>();
                    events.push(Ok(DashProviderEvent::Completed {
                        finish_reason,
                        input_tokens,
                        output_tokens,
                        context_window,
                    }));
                    events
                }
                StreamChunk::Error(error) => vec![Err(map_bridge_error(error))],
                // The finalized BridgeResponse is the single complete provider-round fact.
                // Incremental tool chunks are observation-only and must not become a second
                // executable tool-call source.
                StreamChunk::ToolCall { .. } | StreamChunk::ToolCallDelta { .. } => Vec::new(),
            };
            stream::iter(events)
        })))
    }
}

/// Executes compaction as a dedicated provider turn over the exact current Dash session context.
pub struct ProviderDashCompactor {
    provider: Arc<dyn DashProvider>,
    retained_conversation_messages: usize,
}

impl ProviderDashCompactor {
    pub fn new(provider: Arc<dyn DashProvider>) -> Self {
        Self {
            provider,
            retained_conversation_messages: DEFAULT_RETAINED_CONVERSATION_MESSAGES,
        }
    }

    pub fn with_retained_conversation_messages(mut self, count: usize) -> Self {
        self.retained_conversation_messages = count;
        self
    }
}

#[async_trait]
impl DashCompactor for ProviderDashCompactor {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        if request.context.history.is_empty() {
            return Err(DashServiceError::InvalidState {
                message: "Agent history has no provider-visible content to compact".to_owned(),
            });
        }

        let retained_count = self
            .retained_conversation_messages
            .min(request.context.history.len().saturating_sub(1));
        let cut = request.context.history.len().saturating_sub(retained_count);
        let retained_from = request.message_entry_ids.get(cut).cloned();
        let retained_start = retained_from
            .as_ref()
            .and_then(|entry_id| {
                request
                    .message_entry_ids
                    .iter()
                    .position(|candidate| candidate == entry_id)
            })
            .unwrap_or(cut);
        let retained_message_count = request.context.history.len().saturating_sub(retained_start);
        let output = DashCompactionTurn {
            context: request.context,
            instruction: format!(
                "Summarize the durable Agent conversation context before the final \
                 {retained_message_count} provider messages. Those final messages will be \
                 retained verbatim. Preserve decisions, constraints, unresolved work, tool \
                 outcomes, stable identifiers, and branch-relevant facts. Do not add commentary."
            ),
        }
        .run(self.provider.as_ref())
        .await
        .map_err(map_compaction_turn_error)?;
        let summary = output.summary;
        Ok(DashCompactionResult {
            summary,
            retained_from,
        })
    }
}

pub fn bridge_dash_execution_dependencies(
    bridge: Arc<dyn LlmBridge>,
    thinking_level: Option<ThinkingLevel>,
    context_window: u64,
) -> DashExecutionDependencies {
    let provider: Arc<dyn DashProvider> = Arc::new(BridgeDashProvider::new(
        bridge.clone(),
        thinking_level,
        context_window,
    ));
    DashExecutionDependencies {
        provider: provider.clone(),
        tools: Arc::new(UnboundDashToolCallbacks),
        callbacks: Arc::new(UnboundDashExecutionCallbacks),
        history_callbacks: Arc::new(NoopDashHistoryCallbacks),
        compactor: Arc::new(ProviderDashCompactor::new(provider)),
        conversation_namer: Arc::new(BridgeDashConversationNamer::new(bridge)),
    }
}

struct UnboundDashToolCallbacks;

#[async_trait]
impl DashToolCallbacks for UnboundDashToolCallbacks {
    async fn invoke(
        &self,
        _turn_id: &agentdash_agent::dash::AgentTurnId,
        _call: DashToolCall,
    ) -> Result<Box<dyn agentdash_agent::dash::DashToolExecutionStream>, DashCoreError> {
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
                tool_calls: message
                    .tool_calls
                    .into_iter()
                    .map(|call| agentdash_agent::ToolCallInfo {
                        id: call.call_id.clone(),
                        call_id: Some(call.call_id),
                        name: call.name,
                        arguments: call.arguments,
                    })
                    .collect(),
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
                    is_error: message.is_error,
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

fn map_compaction_turn_error(error: DashCoreError) -> DashServiceError {
    match error {
        DashCoreError::ProviderStreamDisconnected | DashCoreError::Cancelled => {
            DashServiceError::Lost {
                message: error.to_string(),
            }
        }
        DashCoreError::Provider {
            message, retryable, ..
        } => DashServiceError::Unavailable { message, retryable },
        error => DashServiceError::Core(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::{
        BridgeResponse, TokenUsage, ToolCallDeltaContent, ToolCallInfo,
        dash::{CompactionId, DashCoreContext, DashMessage, HistoryEntryId},
    };
    use futures::stream;

    struct FixtureBridge;
    struct DeltaOnlyToolBridge;

    #[async_trait]
    impl LlmBridge for FixtureBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            Box::pin(stream::iter([
                StreamChunk::TextDelta("durable summary".to_owned()),
                StreamChunk::Done(BridgeResponse {
                    message: AgentMessage::assistant("durable summary"),
                    raw_content: vec![ContentPart::text("durable summary")],
                    usage: TokenUsage::default(),
                }),
            ]))
        }
    }

    #[async_trait]
    impl LlmBridge for DeltaOnlyToolBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            Box::pin(stream::iter([
                StreamChunk::ToolCallDelta {
                    id: "call-1".to_owned(),
                    content: ToolCallDeltaContent::Name("read".to_owned()),
                },
                StreamChunk::ToolCallDelta {
                    id: "call-1".to_owned(),
                    content: ToolCallDeltaContent::Arguments(r#"{"path":"Cargo.toml"}"#.to_owned()),
                },
                StreamChunk::Done(BridgeResponse {
                    message: AgentMessage::Assistant {
                        content: Vec::new(),
                        tool_calls: vec![ToolCallInfo {
                            id: "call-1".to_owned(),
                            call_id: Some("call-1".to_owned()),
                            name: "read".to_owned(),
                            arguments: serde_json::json!({"path": "Cargo.toml"}),
                        }],
                        stop_reason: Some(StopReason::ToolUse),
                        error_message: None,
                        usage: None,
                        timestamp: None,
                    },
                    raw_content: Vec::new(),
                    usage: TokenUsage::default(),
                }),
            ]))
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

    #[test]
    fn accepted_tool_description_and_nested_schema_reach_the_provider_without_reconstruction() {
        let definition = agentdash_agent::dash::DashToolDefinition {
            name: "workspace_module_invoke".to_owned(),
            description: "Invoke one operation exposed by a workspace module.".to_owned(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "module_id": {
                        "type": "string",
                        "description": "Canonical workspace module id"
                    },
                    "input": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Operation-local VFS path"
                            }
                        }
                    }
                },
                "required": ["module_id"]
            }),
            capability_key: "workspace/modules".to_owned(),
            source: "test".to_owned(),
            tool_path: "workspace/modules::workspace_module_invoke".to_owned(),
            context_usage_kind: "system_tools".to_owned(),
            protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
        };
        let expected_schema = definition.input_schema.clone();

        let request = bridge_request(
            DashProviderRequest {
                system_prompt: "system".to_owned(),
                messages: Vec::new(),
                tools: vec![definition],
                round: 1,
            },
            None,
        )
        .expect("Dash request should map to bridge request");

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0].name, "workspace_module_invoke");
        assert_eq!(
            request.tools[0].description,
            "Invoke one operation exposed by a workspace module."
        );
        assert_eq!(request.tools[0].parameters, expected_schema);
        assert_eq!(
            request.tools[0].parameters["properties"]["input"]["properties"]["path"]["description"],
            "Operation-local VFS path"
        );
    }

    #[tokio::test]
    async fn finalized_bridge_response_is_the_complete_tool_call_fact() {
        let provider = BridgeDashProvider::new(Arc::new(DeltaOnlyToolBridge), None, 200_000);
        let events = provider
            .stream(DashProviderRequest {
                system_prompt: String::new(),
                messages: Vec::new(),
                tools: Vec::new(),
                round: 1,
            })
            .await
            .expect("provider stream")
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("provider events");

        assert!(matches!(
            &events[0],
            DashProviderEvent::ToolCall { call }
                if call.call_id == "call-1"
                    && call.name == "read"
                    && call.arguments == serde_json::json!({"path": "Cargo.toml"})
        ));
        assert!(matches!(
            events[1],
            DashProviderEvent::Completed {
                finish_reason: DashFinishReason::ToolCalls,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn compactor_retained_boundary_keeps_a_tool_call_and_result_together() {
        let provider = Arc::new(BridgeDashProvider::new(
            Arc::new(FixtureBridge),
            Some(ThinkingLevel::Off),
            200_000,
        ));
        let result = ProviderDashCompactor::new(provider)
            .with_retained_conversation_messages(1)
            .compact(DashCompactionRequest {
                compaction_id: CompactionId::new("compact"),
                mode: agentdash_agent::dash::CompactionMode::Manual,
                source_head: Some(HistoryEntryId::new("tool-result")),
                source_digest: "source-digest".to_owned(),
                context: DashCoreContext {
                    system_prompt: "system".to_owned(),
                    history: vec![
                        DashMessage {
                            role: DashMessageRole::User,
                            content: "inspect".to_owned(),
                            tool_call_id: None,
                            tool_calls: Vec::new(),
                            is_error: false,
                        },
                        DashMessage {
                            role: DashMessageRole::Assistant,
                            content: String::new(),
                            tool_call_id: None,
                            tool_calls: vec![DashToolCall {
                                call_id: "call-1".to_owned(),
                                name: "read".to_owned(),
                                arguments: serde_json::json!({"path": "Cargo.toml"}),
                            }],
                            is_error: false,
                        },
                        DashMessage {
                            role: DashMessageRole::Tool,
                            content: "manifest".to_owned(),
                            tool_call_id: Some("call-1".to_owned()),
                            tool_calls: Vec::new(),
                            is_error: false,
                        },
                    ],
                    tools: Vec::new(),
                },
                message_entry_ids: ["input-1", "tool-call", "tool-call"]
                    .into_iter()
                    .map(HistoryEntryId::new)
                    .collect(),
            })
            .await
            .unwrap();
        assert_eq!(result.retained_from, Some(HistoryEntryId::new("tool-call")));
        assert_eq!(result.summary, "durable summary");
    }
}
