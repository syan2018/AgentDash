use agentdash_diagnostics::{Subsystem, diag};
use std::collections::HashMap;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::bridge::{
    BridgeError, BridgeRequest, LlmBridge, ProviderErrorClassification, ProviderRetryPolicy,
    StreamChunk, ToolCallDeltaContent, sleep_for_retry,
};
use crate::types::{
    AgentContext, AgentError, AgentEvent, AgentMessage, AssistantStreamEvent,
    BeforeProviderRequestInput, CompactionFailureInput, ContentPart, DynAgentTool,
    EvaluateCompactionInput, ProviderAttemptPhase, ProviderAttemptStatus,
    ProviderVisibleContextStats, ToolCallInfo, TransformContextInput, estimate_request_tokens,
    now_millis,
};

use super::tool_call::refresh_context_tools;
use super::{AgentEventSink, AgentLoopConfig, emit_event};

pub(super) fn compute_suffix(existing: &str, incoming: &str) -> String {
    if incoming.is_empty() {
        return String::new();
    }
    if existing.is_empty() {
        return incoming.to_string();
    }
    if let Some(suffix) = incoming.strip_prefix(existing) {
        suffix.to_string()
    } else if existing == incoming || existing.ends_with(incoming) {
        String::new()
    } else {
        incoming.to_string()
    }
}

/// 对齐 Pi `streamAssistantResponse` (agent-loop.ts:238-331)
///
/// 流程：transformContext → convertToLlm → Bridge.stream_complete → 事件
struct PartialAssistantState {
    message: AgentMessage,
    added_partial: bool,
    active_text_index: Option<usize>,
    active_reasoning_id: Option<String>,
    reasoning_indices: HashMap<String, usize>,
    tool_calls: HashMap<String, PartialToolCallState>,
}

#[derive(Clone)]
struct PartialToolCallState {
    index: usize,
    partial_json: String,
}

impl Default for PartialAssistantState {
    fn default() -> Self {
        Self {
            message: AgentMessage::assistant(""),
            added_partial: false,
            active_text_index: None,
            active_reasoning_id: None,
            reasoning_indices: HashMap::new(),
            tool_calls: HashMap::new(),
        }
    }
}

impl PartialAssistantState {
    fn new() -> Self {
        Self {
            message: AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: Vec::new(),
                stop_reason: None,
                error_message: None,
                usage: None,
                timestamp: Some(crate::types::now_millis()),
            },
            ..Self::default()
        }
    }

    fn content_mut(&mut self) -> &mut Vec<ContentPart> {
        match &mut self.message {
            AgentMessage::Assistant { content, .. } => content,
            _ => unreachable!(),
        }
    }

    fn tool_calls_mut(&mut self) -> &mut Vec<ToolCallInfo> {
        match &mut self.message {
            AgentMessage::Assistant { tool_calls, .. } => tool_calls,
            _ => unreachable!(),
        }
    }

    fn reasoning_key(id: &Option<String>) -> String {
        id.clone()
            .unwrap_or_else(|| "__default_reasoning".to_string())
    }
}

pub(super) async fn stream_assistant_response(
    context: &mut AgentContext,
    fallback_tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<AgentMessage, AgentError> {
    refresh_context_tools(context, fallback_tool_instances, config);

    // delegate / transform_context 可在发送前裁剪或注入消息。
    //
    // PR 4（04-30-session-pipeline-architecture-refactor）字段重命名：
    // `output.steering_messages` 作为"本轮最终要发给 LLM 的完整消息列表"，
    // 其中已合并原 context.messages + 改写后的 user message + hook 注入 +
    // pending steering/follow-up。字段名从 `messages` 改为 `steering_messages`
    // 以强调语义：静态任务语义应通过 ContextFrame 投递，此字段只承 per-turn
    // 动态 steering。
    let mut messages_for_llm = if config.runtime_delegates.context_transform.is_some() {
        let output = config
            .runtime_delegates
            .transform_context(
                TransformContextInput {
                    context: context.clone(),
                },
                cancel.clone(),
            )
            .await
            .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))?;
        if let Some(reason) = output.blocked {
            return Ok(AgentMessage::error_assistant(
                format!("输入被 Hook 规则阻止: {reason}"),
                false,
            ));
        }
        output.steering_messages
    } else if let Some(ref transform) = config.transform_context {
        transform(context.messages.clone(), cancel.clone()).await
    } else {
        context.messages.clone()
    };
    let message_refs_for_llm =
        align_message_refs(&context.messages, &context.message_refs, &messages_for_llm);

    let mut request = BridgeRequest {
        system_prompt: Some(context.system_prompt.clone()),
        messages: messages_for_llm.clone(),
        tools: context.tools.clone(),
    };
    let mut compaction_context_window = 0_u64;
    let mut compaction_reserve_tokens = 0_u64;

    {
        let draft_stats = provider_visible_stats(&request);
        let params = config
            .runtime_delegates
            .evaluate_compaction(
                EvaluateCompactionInput {
                    context: AgentContext {
                        system_prompt: context.system_prompt.clone(),
                        messages: messages_for_llm.clone(),
                        message_refs: message_refs_for_llm.clone(),
                        tools: context.tools.clone(),
                    },
                    provider_visible: Some(draft_stats.clone()),
                },
                cancel.clone(),
            )
            .await
            .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))?;

        if let Some(params) = params {
            compaction_context_window = params.trigger_stats.context_window;
            compaction_reserve_tokens = params.reserve_tokens;
            if crate::compaction::should_execute_compaction(
                &messages_for_llm,
                &message_refs_for_llm,
                &params,
            ) {
                let item_id = format!("context-compaction-{}", now_millis());
                emit_event(
                    emit,
                    AgentEvent::ContextCompactionStarted {
                        item_id: item_id.clone(),
                    },
                )
                .await;

                match crate::compaction::execute_compaction(
                    &messages_for_llm,
                    &message_refs_for_llm,
                    &params,
                    bridge,
                    cancel,
                )
                .await
                {
                    Ok(Some(result)) => {
                        messages_for_llm = result.messages.clone();
                        context.messages = result.messages.clone();
                        context.message_refs = result.message_refs.clone();
                        request.messages = messages_for_llm.clone();
                        emit_event(
                            emit,
                            AgentEvent::ContextCompacted {
                                item_id,
                                messages: result.messages.clone(),
                                message_refs: result.message_refs.clone(),
                                compacted_until_ref: result.compacted_until_ref.clone(),
                                first_kept_ref: result.first_kept_ref.clone(),
                                newly_compacted_messages: result.newly_compacted_messages,
                            },
                        )
                        .await;
                        config
                            .runtime_delegates
                            .after_compaction(result, cancel.clone())
                            .await
                            .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))?;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let is_cancelled = matches!(error, AgentError::Cancelled);
                        let error_message = error.to_string();
                        emit_event(
                            emit,
                            AgentEvent::ContextCompactionFailed {
                                item_id: item_id.clone(),
                                error: error_message.clone(),
                            },
                        )
                        .await;
                        if !is_cancelled {
                            config
                                .runtime_delegates
                                .after_compaction_failed(
                                    CompactionFailureInput {
                                        item_id,
                                        error: error_message,
                                    },
                                    cancel.clone(),
                                )
                                .await
                                .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))?;
                        }
                        if is_cancelled {
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    // BeforeProviderRequest 观测 hook
    {
        let final_stats = provider_visible_stats(&request);
        let _ = config
            .runtime_delegates
            .on_before_provider_request(
                BeforeProviderRequestInput {
                    system_prompt_len: context.system_prompt.len(),
                    message_count: messages_for_llm.len(),
                    tool_count: context.tools.len(),
                    estimated_input_tokens: final_stats.estimated_input_tokens,
                    context_window: compaction_context_window,
                    reserve_tokens: compaction_reserve_tokens,
                },
                cancel.clone(),
            )
            .await;
    }

    let retry_policy = ProviderRetryPolicy::default();
    let mut partial = PartialAssistantState::new();
    let mut response = None;
    let mut stream_failure = None;
    let mut last_pre_delta_error = None;

    for attempt in 1..=retry_policy.max_attempts {
        emit_provider_status(
            emit,
            ProviderAttemptStatus {
                phase: ProviderAttemptPhase::Connecting,
                attempt,
                max_attempts: retry_policy.max_attempts,
                will_retry: false,
                delay_ms: None,
                reason_code: None,
                message: None,
                provider: None,
                model: None,
            },
        )
        .await;

        let mut stream = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AgentError::Cancelled),
            stream = bridge.stream_complete(request.clone()) => stream,
        };
        emit_provider_status(
            emit,
            ProviderAttemptStatus {
                phase: ProviderAttemptPhase::ConnectedWaitingFirstDelta,
                attempt,
                max_attempts: retry_policy.max_attempts,
                will_retry: false,
                delay_ms: None,
                reason_code: None,
                message: None,
                provider: None,
                model: None,
            },
        )
        .await;
        let mut has_visible_delta = false;
        let mut streaming_status_emitted = false;
        let mut retry_error = None;

        loop {
            let chunk = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    stream_failure = Some(AgentMessage::error_assistant("Agent run aborted", true));
                    break;
                }
                chunk = stream.next() => chunk,
            };
            let Some(chunk) = chunk else {
                break;
            };
            if cancel.is_cancelled() {
                stream_failure = Some(AgentMessage::error_assistant("Agent run aborted", true));
                break;
            }
            match chunk {
                StreamChunk::TextDelta(text) if !text.is_empty() => {
                    mark_visible_delta(
                        emit,
                        &mut has_visible_delta,
                        &mut streaming_status_emitted,
                        attempt,
                        retry_policy.max_attempts,
                    )
                    .await;
                    ensure_partial_started(context, emit, &mut partial).await;
                    end_active_reasoning(context, emit, &mut partial).await;
                    let content_index = if let Some(index) = partial.active_text_index {
                        index
                    } else {
                        let index = partial.content_mut().len();
                        partial.content_mut().push(ContentPart::text(""));
                        partial.active_text_index = Some(index);
                        sync_partial(context, &partial);
                        emit_event(
                            emit,
                            AgentEvent::MessageUpdate {
                                message: partial.message.clone(),
                                event: AssistantStreamEvent::TextStart {
                                    content_index: index,
                                },
                            },
                        )
                        .await;
                        index
                    };

                    if let Some(ContentPart::Text { text: existing }) =
                        partial.content_mut().get_mut(content_index)
                    {
                        existing.push_str(&text);
                    }
                    sync_partial(context, &partial);
                    emit_event(
                        emit,
                        AgentEvent::MessageUpdate {
                            message: partial.message.clone(),
                            event: AssistantStreamEvent::TextDelta {
                                content_index,
                                text,
                            },
                        },
                    )
                    .await;
                }
                StreamChunk::ReasoningDelta {
                    id,
                    text,
                    signature,
                } if !text.is_empty() => {
                    mark_visible_delta(
                        emit,
                        &mut has_visible_delta,
                        &mut streaming_status_emitted,
                        attempt,
                        retry_policy.max_attempts,
                    )
                    .await;
                    ensure_partial_started(context, emit, &mut partial).await;
                    end_active_text(context, emit, &mut partial).await;

                    let reasoning_key = PartialAssistantState::reasoning_key(&id);
                    if partial.active_reasoning_id.as_deref() != Some(reasoning_key.as_str()) {
                        end_active_reasoning(context, emit, &mut partial).await;
                    }

                    let content_index =
                        if let Some(index) = partial.reasoning_indices.get(&reasoning_key) {
                            *index
                        } else {
                            let index = partial.content_mut().len();
                            partial.content_mut().push(ContentPart::reasoning(
                                "",
                                id.clone(),
                                signature.clone(),
                            ));
                            partial
                                .reasoning_indices
                                .insert(reasoning_key.clone(), index);
                            partial.active_reasoning_id = Some(reasoning_key.clone());
                            sync_partial(context, &partial);
                            emit_event(
                                emit,
                                AgentEvent::MessageUpdate {
                                    message: partial.message.clone(),
                                    event: AssistantStreamEvent::ThinkingStart {
                                        content_index: index,
                                        id: id.clone(),
                                    },
                                },
                            )
                            .await;
                            index
                        };

                    let delta = if let Some(ContentPart::Reasoning {
                        text: existing,
                        signature: existing_signature,
                        ..
                    }) = partial.content_mut().get_mut(content_index)
                    {
                        if let Some(sig) = signature.clone() {
                            *existing_signature = Some(sig);
                        }
                        let suffix = compute_suffix(existing, &text);
                        if !suffix.is_empty() {
                            existing.push_str(&suffix);
                        }
                        suffix
                    } else {
                        String::new()
                    };

                    partial.active_reasoning_id = Some(reasoning_key);
                    if !delta.is_empty() {
                        sync_partial(context, &partial);
                        emit_event(
                            emit,
                            AgentEvent::MessageUpdate {
                                message: partial.message.clone(),
                                event: AssistantStreamEvent::ThinkingDelta {
                                    content_index,
                                    id: id.clone(),
                                    text: delta,
                                },
                            },
                        )
                        .await;
                    }
                }
                StreamChunk::ToolCallDelta { id, content } => {
                    mark_visible_delta(
                        emit,
                        &mut has_visible_delta,
                        &mut streaming_status_emitted,
                        attempt,
                        retry_policy.max_attempts,
                    )
                    .await;
                    match content {
                        ToolCallDeltaContent::Name(name) => {
                            ensure_partial_started(context, emit, &mut partial).await;
                            end_active_text(context, emit, &mut partial).await;
                            end_active_reasoning(context, emit, &mut partial).await;
                            let _ = ensure_tool_call_partial(
                                context,
                                emit,
                                &mut partial,
                                &id,
                                Some(name),
                                serde_json::Value::Object(Default::default()),
                            )
                            .await;
                            sync_partial(context, &partial);
                        }
                        ToolCallDeltaContent::Arguments(delta) if !delta.is_empty() => {
                            ensure_partial_started(context, emit, &mut partial).await;
                            end_active_text(context, emit, &mut partial).await;
                            end_active_reasoning(context, emit, &mut partial).await;

                            let (content_index, tool_name) = ensure_tool_call_partial(
                                context,
                                emit,
                                &mut partial,
                                &id,
                                None,
                                serde_json::Value::Object(Default::default()),
                            )
                            .await;

                            let tool_index = if let Some(state) = partial.tool_calls.get_mut(&id) {
                                state.partial_json.push_str(&delta);
                                let draft = state.partial_json.clone();
                                if let Ok(arguments) =
                                    serde_json::from_str::<serde_json::Value>(&state.partial_json)
                                {
                                    Some((state.index, Some(arguments), draft))
                                } else {
                                    Some((state.index, None, draft))
                                }
                            } else {
                                None
                            };
                            let mut current_draft = String::new();
                            let mut is_parseable = false;
                            if let Some((tool_index, arguments, draft)) = tool_index {
                                current_draft = draft;
                                if let Some(arguments) = arguments
                                    && let Some(tc) = partial.tool_calls_mut().get_mut(tool_index)
                                {
                                    is_parseable = true;
                                    tc.arguments = arguments;
                                }
                            }
                            sync_partial(context, &partial);
                            emit_event(
                                emit,
                                AgentEvent::MessageUpdate {
                                    message: partial.message.clone(),
                                    event: AssistantStreamEvent::ToolCallDelta {
                                        content_index,
                                        tool_call_id: id,
                                        name: tool_name,
                                        delta,
                                        draft: current_draft,
                                        is_parseable,
                                    },
                                },
                            )
                            .await;
                        }
                        ToolCallDeltaContent::Arguments(_) => {}
                    }
                }
                StreamChunk::ToolCall { info } => {
                    mark_visible_delta(
                        emit,
                        &mut has_visible_delta,
                        &mut streaming_status_emitted,
                        attempt,
                        retry_policy.max_attempts,
                    )
                    .await;
                    ensure_partial_started(context, emit, &mut partial).await;
                    end_active_text(context, emit, &mut partial).await;
                    end_active_reasoning(context, emit, &mut partial).await;

                    let info_id = info.id.clone();
                    let (content_index, tool_name) = ensure_tool_call_partial(
                        context,
                        emit,
                        &mut partial,
                        &info_id,
                        Some(info.name.clone()),
                        info.arguments.clone(),
                    )
                    .await;

                    let mut should_emit_delta = None;
                    let tool_index = if let Some(state) = partial.tool_calls.get_mut(&info_id) {
                        let serialized = serde_json::to_string(&info.arguments).unwrap_or_default();
                        let suffix = compute_suffix(&state.partial_json, &serialized);
                        state.partial_json = serialized;
                        should_emit_delta = (!suffix.is_empty()).then_some(suffix);
                        Some(state.index)
                    } else {
                        None
                    };
                    if let Some(tool_index) = tool_index
                        && let Some(tc) = partial.tool_calls_mut().get_mut(tool_index)
                    {
                        *tc = info.clone();
                    }

                    sync_partial(context, &partial);
                    if let Some(delta) = should_emit_delta {
                        emit_event(
                            emit,
                            AgentEvent::MessageUpdate {
                                message: partial.message.clone(),
                                event: AssistantStreamEvent::ToolCallDelta {
                                    content_index,
                                    tool_call_id: info_id.clone(),
                                    name: tool_name,
                                    delta,
                                    draft: serde_json::to_string(&info.arguments)
                                        .unwrap_or_default(),
                                    is_parseable: true,
                                },
                            },
                        )
                        .await;
                    }
                    emit_event(
                        emit,
                        AgentEvent::MessageUpdate {
                            message: partial.message.clone(),
                            event: AssistantStreamEvent::ToolCallEnd {
                                content_index,
                                tool_call: info,
                            },
                        },
                    )
                    .await;
                }
                StreamChunk::Done(resp) => {
                    response = Some(resp);
                }
                StreamChunk::Error(error) => {
                    if is_retryable_pre_delta(&error, has_visible_delta) {
                        retry_error = Some(error);
                    } else {
                        let aborted = error.is_aborted();
                        stream_failure =
                            Some(AgentMessage::error_assistant(error.to_string(), aborted));
                    }
                    break;
                }
                _ => {}
            }
        }
        drop(stream);

        if stream_failure.is_some() {
            break;
        }

        if response.is_some() {
            emit_provider_status(
                emit,
                ProviderAttemptStatus {
                    phase: ProviderAttemptPhase::Succeeded,
                    attempt,
                    max_attempts: retry_policy.max_attempts,
                    will_retry: false,
                    delay_ms: None,
                    reason_code: None,
                    message: None,
                    provider: None,
                    model: None,
                },
            )
            .await;
            break;
        }

        if let Some(error) = retry_error {
            let classification = error.classification();
            if attempt < retry_policy.max_attempts {
                let delay_ms =
                    retry_policy.delay_for_attempt(attempt, classification.retry_after_ms);
                emit_retry_scheduled(
                    emit,
                    attempt,
                    retry_policy,
                    delay_ms,
                    &error,
                    &classification,
                )
                .await;
                last_pre_delta_error = Some(error);
                sleep_for_retry(delay_ms, cancel).await?;
                continue;
            }

            emit_retry_exhausted(emit, attempt, retry_policy, &error, &classification).await;
            stream_failure = Some(AgentMessage::error_assistant(error.to_string(), false));
            break;
        }

        let empty_error = BridgeError::EmptyResponse;
        if has_visible_delta {
            stream_failure = Some(AgentMessage::error_assistant(
                empty_error.to_string(),
                false,
            ));
            break;
        }
        let classification = empty_error.classification();
        if attempt < retry_policy.max_attempts {
            let delay_ms = retry_policy.delay_for_attempt(attempt, classification.retry_after_ms);
            emit_retry_scheduled(
                emit,
                attempt,
                retry_policy,
                delay_ms,
                &empty_error,
                &classification,
            )
            .await;
            last_pre_delta_error = Some(empty_error);
            sleep_for_retry(delay_ms, cancel).await?;
            continue;
        }

        emit_retry_exhausted(emit, attempt, retry_policy, &empty_error, &classification).await;
        last_pre_delta_error = Some(empty_error);
        break;
    }

    if stream_failure.is_none()
        && response.is_none()
        && let Some(error) = last_pre_delta_error
    {
        stream_failure = Some(AgentMessage::error_assistant(error.to_string(), false));
    }

    end_active_text(context, emit, &mut partial).await;
    end_active_reasoning(context, emit, &mut partial).await;

    let assistant_message = if let Some(message) = stream_failure {
        message
    } else {
        let response = response.ok_or(crate::bridge::BridgeError::EmptyResponse)?;
        match response.message {
            AgentMessage::Assistant {
                content,
                tool_calls,
                stop_reason,
                error_message,
                ..
            } => AgentMessage::Assistant {
                content,
                tool_calls: tool_calls.clone(),
                stop_reason: stop_reason.or_else(|| {
                    Some(if error_message.is_some() {
                        crate::types::StopReason::Error
                    } else if tool_calls.is_empty() {
                        crate::types::StopReason::Stop
                    } else {
                        crate::types::StopReason::ToolUse
                    })
                }),
                error_message,
                usage: Some(response.usage.clone()),
                timestamp: Some(crate::types::now_millis()),
            },
            other => other,
        }
    };

    if !partial.added_partial {
        emit_event(
            emit,
            AgentEvent::MessageStart {
                message: assistant_message.clone(),
            },
        )
        .await;
        context.messages.push(assistant_message.clone());
        context.message_refs.push(None);
    } else {
        *context
            .messages
            .last_mut()
            .expect("partial must exist in context") = assistant_message.clone();
    }

    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: assistant_message.clone(),
        },
    )
    .await;

    Ok(assistant_message)
}

async fn ensure_partial_started(
    context: &mut AgentContext,
    emit: &AgentEventSink,
    partial: &mut PartialAssistantState,
) {
    if partial.added_partial {
        return;
    }
    context.messages.push(partial.message.clone());
    context.message_refs.push(None);
    partial.added_partial = true;
    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: partial.message.clone(),
        },
    )
    .await;
}

fn sync_partial(context: &mut AgentContext, partial: &PartialAssistantState) {
    if partial.added_partial
        && let Some(last) = context.messages.last_mut()
    {
        *last = partial.message.clone();
    }
}

fn align_message_refs(
    base_messages: &[AgentMessage],
    base_refs: &[Option<crate::types::MessageRef>],
    projected_messages: &[AgentMessage],
) -> Vec<Option<crate::types::MessageRef>> {
    if base_messages.len() != base_refs.len() {
        return vec![None; projected_messages.len()];
    }
    let mut next_base = 0_usize;
    projected_messages
        .iter()
        .map(|message| {
            let matched = base_messages
                .iter()
                .enumerate()
                .skip(next_base)
                .find(|(_, base_message)| *base_message == message)
                .map(|(idx, _)| idx);
            if let Some(idx) = matched {
                next_base = idx.saturating_add(1);
                base_refs[idx].clone()
            } else {
                None
            }
        })
        .collect()
}

fn provider_visible_stats(request: &BridgeRequest) -> ProviderVisibleContextStats {
    ProviderVisibleContextStats {
        system_prompt_len: request.system_prompt.as_deref().map(str::len).unwrap_or(0),
        message_count: request.messages.len(),
        tool_count: request.tools.len(),
        estimated_input_tokens: estimate_request_tokens(
            request.system_prompt.as_deref(),
            &request.messages,
            &request.tools,
        ),
    }
}

fn is_retryable_pre_delta(error: &BridgeError, has_visible_delta: bool) -> bool {
    !has_visible_delta && error.classification().is_retryable_before_visible_delta()
}

async fn mark_visible_delta(
    emit: &AgentEventSink,
    has_visible_delta: &mut bool,
    streaming_status_emitted: &mut bool,
    attempt: u32,
    max_attempts: u32,
) {
    *has_visible_delta = true;
    if *streaming_status_emitted {
        return;
    }
    *streaming_status_emitted = true;
    emit_provider_status(
        emit,
        ProviderAttemptStatus {
            phase: ProviderAttemptPhase::Streaming,
            attempt,
            max_attempts,
            will_retry: false,
            delay_ms: None,
            reason_code: None,
            message: None,
            provider: None,
            model: None,
        },
    )
    .await;
}

async fn emit_provider_status(emit: &AgentEventSink, status: ProviderAttemptStatus) {
    emit_event(emit, AgentEvent::ProviderAttemptStatus { status }).await;
}

async fn emit_retry_scheduled(
    emit: &AgentEventSink,
    attempt: u32,
    retry_policy: ProviderRetryPolicy,
    delay_ms: u64,
    error: &BridgeError,
    classification: &ProviderErrorClassification,
) {
    emit_provider_status(
        emit,
        ProviderAttemptStatus {
            phase: ProviderAttemptPhase::RetryScheduled,
            attempt,
            max_attempts: retry_policy.max_attempts,
            will_retry: true,
            delay_ms: Some(delay_ms),
            reason_code: provider_reason_code(classification),
            message: Some(format!(
                "Reconnecting... {}/{}",
                attempt.saturating_add(1),
                retry_policy.max_attempts
            )),
            provider: None,
            model: None,
        },
    )
    .await;
    diag!(Warn, Subsystem::AgentRun,

        attempt,
        max_attempts = retry_policy.max_attempts,
        delay_ms,
        http_status = classification.http_status,
        provider_code = classification.provider_code.as_deref(),
        error = %error,
        "provider attempt failed before visible delta; retry scheduled"
    );
}

async fn emit_retry_exhausted(
    emit: &AgentEventSink,
    attempt: u32,
    retry_policy: ProviderRetryPolicy,
    error: &BridgeError,
    classification: &ProviderErrorClassification,
) {
    emit_provider_status(
        emit,
        ProviderAttemptStatus {
            phase: ProviderAttemptPhase::Failed,
            attempt,
            max_attempts: retry_policy.max_attempts,
            will_retry: false,
            delay_ms: None,
            reason_code: provider_reason_code(classification),
            message: Some(error.to_string()),
            provider: None,
            model: None,
        },
    )
    .await;
}

fn provider_reason_code(classification: &ProviderErrorClassification) -> Option<String> {
    classification.provider_code.clone().or_else(|| {
        classification
            .http_status
            .map(|status| format!("http_{status}"))
    })
}

async fn end_active_text(
    context: &mut AgentContext,
    emit: &AgentEventSink,
    partial: &mut PartialAssistantState,
) {
    let Some(content_index) = partial.active_text_index.take() else {
        return;
    };
    sync_partial(context, partial);
    let text = match &partial.content_mut()[content_index] {
        ContentPart::Text { text } => text.clone(),
        _ => String::new(),
    };
    emit_event(
        emit,
        AgentEvent::MessageUpdate {
            message: partial.message.clone(),
            event: AssistantStreamEvent::TextEnd {
                content_index,
                text,
            },
        },
    )
    .await;
}

async fn end_active_reasoning(
    context: &mut AgentContext,
    emit: &AgentEventSink,
    partial: &mut PartialAssistantState,
) {
    let Some(reasoning_key) = partial.active_reasoning_id.take() else {
        return;
    };
    let Some(&content_index) = partial.reasoning_indices.get(&reasoning_key) else {
        return;
    };
    sync_partial(context, partial);
    let (id, text, signature) = match &partial.content_mut()[content_index] {
        ContentPart::Reasoning {
            id,
            text,
            signature,
        } => (id.clone(), text.clone(), signature.clone()),
        _ => (None, String::new(), None),
    };
    emit_event(
        emit,
        AgentEvent::MessageUpdate {
            message: partial.message.clone(),
            event: AssistantStreamEvent::ThinkingEnd {
                content_index,
                id,
                text,
                signature,
            },
        },
    )
    .await;
}

async fn ensure_tool_call_partial(
    context: &mut AgentContext,
    emit: &AgentEventSink,
    partial: &mut PartialAssistantState,
    tool_call_id: &str,
    tool_name: Option<String>,
    arguments: serde_json::Value,
) -> (usize, String) {
    if let Some(state) = partial.tool_calls.get(tool_call_id).cloned() {
        if let Some(tool_name) = tool_name
            && !tool_name.is_empty()
        {
            partial.tool_calls_mut()[state.index].name = tool_name;
        }
        let name = partial.tool_calls_mut()[state.index].name.clone();
        return (state.index, name);
    }

    let name = tool_name.unwrap_or_else(|| "pending_tool".to_string());
    let index = partial.tool_calls_mut().len();
    partial.tool_calls_mut().push(ToolCallInfo {
        id: tool_call_id.to_string(),
        call_id: Some(tool_call_id.to_string()),
        name: name.clone(),
        arguments,
    });
    partial.tool_calls.insert(
        tool_call_id.to_string(),
        PartialToolCallState {
            index,
            partial_json: String::new(),
        },
    );
    sync_partial(context, partial);
    emit_event(
        emit,
        AgentEvent::MessageUpdate {
            message: partial.message.clone(),
            event: AssistantStreamEvent::ToolCallStart {
                content_index: index,
                tool_call_id: tool_call_id.to_string(),
                name: name.clone(),
            },
        },
    )
    .await;
    (index, name)
}
