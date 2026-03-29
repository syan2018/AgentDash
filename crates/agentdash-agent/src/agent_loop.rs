/// Agent 循环 — 严格对齐 Pi `agent-loop.ts` 核心语义
///
/// 流程（对齐 Pi `runLoop`）：
/// 1. 将 prompt 追加到 context.messages，为每个 prompt 发出 message_start/end
/// 2. 发出 `agent_start`
/// 3. 循环开始前轮询 steering（用户可能在等待期间输入）
/// 4. 外循环（follow-up 驱动）
///    - 4a. 内循环（tool calls + steering 驱动）：注入 pending messages (steering)，
///      `transform_context` → `convert_to_llm` → Bridge.stream_complete()，
///      发出 message 事件，若有 tool_calls → 执行（sequential / parallel），轮询 steering
///    - 4b. 检查 follow-up → 若有则继续外循环
/// 5. 发出 `agent_end`
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use jsonschema::validator_for;
use tokio_util::sync::CancellationToken;

use crate::bridge::{BridgeRequest, LlmBridge, StreamChunk};
use crate::types::{
    AfterToolCallContext, AfterToolCallInput, AfterToolCallResult, AfterTurnInput, AgentContext,
    AgentError, AgentEvent, AgentMessage, AgentToolResult, AssistantStreamEvent, BeforeStopInput,
    BeforeToolCallContext, BeforeToolCallInput, BeforeToolCallResult, ContentPart,
    DynAgentRuntimeDelegate, DynAgentTool, StopDecision, ToolApprovalOutcome, ToolApprovalRequest,
    ToolCallDecision, ToolCallInfo, ToolExecutionMode, ToolUpdateCallback, TransformContextInput,
};

const DEFAULT_MAX_TURNS: usize = 25;

// ─── 回调类型别名 ───────────────────────────────────────────

/// 上下文变换回调：AgentMessage[] → AgentMessage[]
/// 对齐 Pi `AgentLoopConfig.transformContext`
pub type TransformContextFn = Arc<
    dyn Fn(
            Vec<AgentMessage>,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Vec<AgentMessage>> + Send>>
        + Send
        + Sync,
>;

/// Steering 消息获取回调
/// 对齐 Pi `AgentLoopConfig.getSteeringMessages`
pub type GetMessagesFn = Arc<dyn Fn() -> Vec<AgentMessage> + Send + Sync>;

/// before_tool_call 钩子
/// 对齐 Pi `AgentLoopConfig.beforeToolCall`
pub type BeforeToolCallFn = Arc<
    dyn Fn(
            BeforeToolCallContext<'_>,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Option<BeforeToolCallResult>> + Send + '_>>
        + Send
        + Sync,
>;

/// after_tool_call 钩子
/// 对齐 Pi `AgentLoopConfig.afterToolCall`
pub type AfterToolCallFn = Arc<
    dyn Fn(
            AfterToolCallContext<'_>,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Option<AfterToolCallResult>> + Send + '_>>
        + Send
        + Sync,
>;

/// 工具审批等待回调
pub type AwaitToolApprovalFn = Arc<
    dyn Fn(
            ToolApprovalRequest,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = ToolApprovalOutcome> + Send>>
        + Send
        + Sync,
>;

/// 事件 sink —— 对齐 Pi `runAgentLoop(..., emit, ...)` 的异步事件消费模型。
pub type AgentEventSink =
    Arc<dyn Fn(AgentEvent) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + Sync>;

// ─── AgentLoopConfig ────────────────────────────────────────

/// Agent Loop 配置 — 对齐 Pi `AgentLoopConfig`
pub struct AgentLoopConfig {
    pub max_turns: usize,

    /// 上下文变换管线（每次 LLM 调用前执行）
    /// 对齐 Pi `transformContext`：用于上下文裁剪、外部信息注入等。
    pub transform_context: Option<TransformContextFn>,

    /// 获取 steering 消息
    pub get_steering_messages: Option<GetMessagesFn>,

    /// 获取 follow-up 消息
    pub get_follow_up_messages: Option<GetMessagesFn>,

    /// 工具执行模式
    /// 对齐 Pi `toolExecution`，默认 Parallel。
    pub tool_execution: ToolExecutionMode,

    /// 工具执行前钩子
    /// 对齐 Pi `beforeToolCall`。返回 `block: true` 阻止执行。
    pub before_tool_call: Option<BeforeToolCallFn>,

    /// 工具执行后钩子
    /// 对齐 Pi `afterToolCall`。可覆盖工具执行结果。
    pub after_tool_call: Option<AfterToolCallFn>,

    /// 工具审批等待
    pub await_tool_approval: Option<AwaitToolApprovalFn>,

    /// 统一运行时委托
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_turns: DEFAULT_MAX_TURNS,
            transform_context: None,
            get_steering_messages: None,
            get_follow_up_messages: None,
            tool_execution: ToolExecutionMode::default(),
            before_tool_call: None,
            after_tool_call: None,
            await_tool_approval: None,
            runtime_delegate: None,
        }
    }
}

// ─── 入口函数 ───────────────────────────────────────────────

/// 启动一次完整的 Agent Loop（带新 prompt）
/// 对齐 Pi `agentLoop` / `runAgentLoop`
pub async fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: &mut AgentContext,
    tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut new_messages = prompts.clone();
    emit_event(emit, AgentEvent::AgentStart).await;
    emit_event(emit, AgentEvent::TurnStart).await;
    for prompt in &prompts {
        emit_event(
            emit,
            AgentEvent::MessageStart {
                message: prompt.clone(),
            },
        )
        .await;
        emit_event(
            emit,
            AgentEvent::MessageEnd {
                message: prompt.clone(),
            },
        )
        .await;
    }
    context.messages.extend(prompts);
    run_loop(context, tool_instances, &mut new_messages, config, bridge, emit, &cancel).await
}

/// 从当前上下文继续 Agent Loop（不添加新 prompt）
/// 对齐 Pi `agentLoopContinue` / `runAgentLoopContinue`
pub async fn agent_loop_continue(
    context: &mut AgentContext,
    tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    if context.messages.is_empty() {
        return Err(AgentError::ContinueError(
            "Cannot continue: no messages in context".to_string(),
        ));
    }
    if matches!(
        context.messages.last(),
        Some(AgentMessage::Assistant { .. })
    ) {
        return Err(AgentError::ContinueError(
            "Cannot continue from message role: assistant".to_string(),
        ));
    }

    let mut new_messages = Vec::new();
    emit_event(emit, AgentEvent::AgentStart).await;
    emit_event(emit, AgentEvent::TurnStart).await;
    run_loop(context, tool_instances, &mut new_messages, config, bridge, emit, &cancel).await
}

// ─── 主循环 ─────────────────────────────────────────────────

/// 主循环 — 严格对齐 Pi `runLoop` (agent-loop.ts:155-232)
///
/// 双循环结构：
/// - 外循环：follow-up 驱动（agent 本应停止时检查 follow-up 并继续）
/// - 内循环：tool calls + steering 驱动（有工具调用或 pending 消息时继续）
async fn run_loop(
    context: &mut AgentContext,
    tool_instances: &[DynAgentTool],
    new_messages: &mut Vec<AgentMessage>,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut turn_count: usize = 0;
    let mut first_turn = true;
    let mut pending_messages = poll_steering(config);
    let mut pending_follow_up_messages: Vec<AgentMessage> = Vec::new();

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }

            turn_count += 1;
            if turn_count > config.max_turns {
                return Err(AgentError::MaxTurnsExceeded(config.max_turns));
            }

            if first_turn {
                first_turn = false;
            } else {
                emit_event(emit, AgentEvent::TurnStart).await;
            }

            if !pending_messages.is_empty() {
                for msg in pending_messages.drain(..) {
                    emit_event(
                        emit,
                        AgentEvent::MessageStart {
                            message: msg.clone(),
                        },
                    )
                    .await;
                    emit_event(
                        emit,
                        AgentEvent::MessageEnd {
                            message: msg.clone(),
                        },
                    )
                    .await;
                    context.messages.push(msg.clone());
                    new_messages.push(msg);
                }
            }

            let assistant_message =
                stream_assistant_response(context, config, bridge, emit, cancel)
                    .await?;
            new_messages.push(assistant_message.clone());

            if assistant_message.is_error_or_aborted() {
                emit_event(
                    emit,
                    AgentEvent::TurnEnd {
                        message: assistant_message,
                        tool_results: vec![],
                    },
                )
                .await;
                emit_event(
                    emit,
                    AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    },
                )
                .await;
                return Ok(new_messages.clone());
            }

            let tool_calls = match &assistant_message {
                AgentMessage::Assistant { tool_calls, .. } => tool_calls.clone(),
                _ => vec![],
            };
            has_more_tool_calls = !tool_calls.is_empty();

            let mut tool_results = Vec::new();

            if has_more_tool_calls {
                tool_results = execute_tool_calls(
                    context,
                    tool_instances,
                    &assistant_message,
                    &tool_calls,
                    config,
                    emit,
                    cancel,
                )
                .await?;

                for result in &tool_results {
                    context.messages.push(result.clone());
                    new_messages.push(result.clone());
                }
            }

            emit_event(
                emit,
                AgentEvent::TurnEnd {
                    message: assistant_message.clone(),
                    tool_results: tool_results.clone(),
                },
            )
            .await;

            if let Some(decision) =
                run_after_turn_delegate(config, context, &assistant_message, &tool_results, cancel)
                    .await?
            {
                if !decision.steering.is_empty() {
                    pending_messages.extend(decision.steering);
                }
                if !decision.follow_up.is_empty() {
                    pending_follow_up_messages.extend(decision.follow_up);
                }
            }

            let mut newly_polled_steering = poll_steering(config);
            pending_messages.append(&mut newly_polled_steering);
        }

        if !pending_follow_up_messages.is_empty() {
            pending_messages = std::mem::take(&mut pending_follow_up_messages);
            continue;
        }

        let follow_ups = poll_follow_up(config);
        if !follow_ups.is_empty() {
            pending_messages = follow_ups;
            continue;
        }

        if let Some(stop_decision) = run_before_stop_delegate(config, context, cancel).await? {
            match stop_decision {
                StopDecision::Stop => {}
                StopDecision::Continue {
                    mut steering,
                    mut follow_up,
                    ..
                } => {
                    if steering.is_empty() && follow_up.is_empty() {
                        break;
                    }
                    pending_messages.append(&mut steering);
                    pending_messages.append(&mut follow_up);
                    continue;
                }
            }
        }

        break;
    }

    emit_event(
        emit,
        AgentEvent::AgentEnd {
            messages: new_messages.clone(),
        },
    )
    .await;
    Ok(new_messages.clone())
}

async fn emit_event(emit: &AgentEventSink, event: AgentEvent) {
    emit(event).await;
}

async fn run_after_turn_delegate(
    config: &AgentLoopConfig,
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tool_results: &[AgentMessage],
    cancel: &CancellationToken,
) -> Result<Option<crate::types::TurnControlDecision>, AgentError> {
    let Some(delegate) = config.runtime_delegate.as_ref() else {
        return Ok(None);
    };

    delegate
        .after_turn(
            AfterTurnInput {
                context: context.clone(),
                message: assistant_message.clone(),
                tool_results: tool_results.to_vec(),
            },
            cancel.clone(),
        )
        .await
        .map(Some)
        .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))
}

async fn run_before_stop_delegate(
    config: &AgentLoopConfig,
    context: &AgentContext,
    cancel: &CancellationToken,
) -> Result<Option<StopDecision>, AgentError> {
    let Some(delegate) = config.runtime_delegate.as_ref() else {
        return Ok(None);
    };

    delegate
        .before_stop(
            BeforeStopInput {
                context: context.clone(),
            },
            cancel.clone(),
        )
        .await
        .map(Some)
        .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))
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

async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<AgentMessage, AgentError> {
    // delegate / transform_context 可在发送前裁剪或注入消息
    let messages_for_llm = if let Some(delegate) = config.runtime_delegate.as_ref() {
        delegate
            .transform_context(
                TransformContextInput {
                    context: context.clone(),
                },
                cancel.clone(),
            )
            .await
            .map_err(|error| AgentError::RuntimeDelegate(error.to_string()))?
            .messages
    } else if let Some(ref transform) = config.transform_context {
        transform(context.messages.clone(), cancel.clone()).await
    } else {
        context.messages.clone()
    };

    let request = BridgeRequest {
        system_prompt: Some(context.system_prompt.clone()),
        messages: messages_for_llm,
        tools: context.tools.clone(),
    };

    let mut partial = PartialAssistantState::new();
    let mut stream = bridge.stream_complete(request).await;
    let mut response = None;
    let mut stream_failure = None;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            stream_failure = Some(AgentMessage::error_assistant("Agent run aborted", true));
            break;
        }
        match chunk {
            StreamChunk::TextDelta(text) if !text.is_empty() => {
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
            StreamChunk::ToolCallDelta { id, delta } if !delta.is_empty() => {
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
                    if let Ok(arguments) =
                        serde_json::from_str::<serde_json::Value>(&state.partial_json)
                    {
                        Some((state.index, arguments))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some((tool_index, arguments)) = tool_index
                    && let Some(tc) = partial.tool_calls_mut().get_mut(tool_index)
                {
                    tc.arguments = arguments;
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
                        },
                    },
                )
                .await;
            }
            StreamChunk::ToolCall { info } => {
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
                stream_failure = Some(AgentMessage::error_assistant(error.to_string(), false));
                break;
            }
            _ => {}
        }
    }
    drop(stream);

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

fn compute_suffix(existing: &str, incoming: &str) -> String {
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

// ─── 工具执行 ───────────────────────────────────────────────

/// 工具执行入口 — 对齐 Pi `executeToolCalls` (agent-loop.ts:336-348)
///
/// 根据 `config.tool_execution` 分发到 sequential 或 parallel 实现。
async fn execute_tool_calls(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    match config.tool_execution {
        ToolExecutionMode::Sequential => {
            execute_tool_calls_sequential(
                context,
                tool_instances,
                assistant_message,
                tool_calls,
                config,
                emit,
                cancel,
            )
            .await
        }
        ToolExecutionMode::Parallel => {
            execute_tool_calls_parallel(
                context,
                tool_instances,
                assistant_message,
                tool_calls,
                config,
                emit,
                cancel,
            )
            .await
        }
    }
}

/// 顺序执行 — 对齐 Pi `executeToolCallsSequential` (agent-loop.ts:350-388)
async fn execute_tool_calls_sequential(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    for tc in tool_calls {
        emit_event(
            emit,
            AgentEvent::ToolExecutionStart {
                tool_call_id: tc.id.clone(),
                tool_name: tc.name.clone(),
                args: tc.arguments.clone(),
            },
        )
        .await;

        let preparation = prepare_tool_call(context, tool_instances, assistant_message, tc, config, cancel).await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                results.push(emit_tool_call_outcome(tc, &result, is_error, emit).await);
            }
            ToolCallPreparation::AwaitApproval {
                tool,
                args,
                reason,
                details,
            } => {
                match await_tool_approval(tc, &args, &reason, details, config, emit, cancel).await {
                    ApprovalResolution::Approved => {
                        let executed =
                            execute_prepared_tool_call(tc, &tool, &args, cancel, emit).await;
                        let finalized = finalize_executed_tool_call(
                            context,
                            assistant_message,
                            tc,
                            &args,
                            executed,
                            config,
                            cancel,
                        )
                        .await;
                        results.push(
                            emit_tool_call_outcome(tc, &finalized.result, finalized.is_error, emit)
                                .await,
                        );
                    }
                    ApprovalResolution::Rejected { result } => {
                        results.push(emit_tool_result_message(tc, &result, emit).await);
                    }
                }
            }
            ToolCallPreparation::Prepared { tool, args } => {
                let executed = execute_prepared_tool_call(tc, &tool, &args, cancel, emit).await;
                let finalized = finalize_executed_tool_call(
                    context,
                    assistant_message,
                    tc,
                    &args,
                    executed,
                    config,
                    cancel,
                )
                .await;
                results.push(
                    emit_tool_call_outcome(tc, &finalized.result, finalized.is_error, emit).await,
                );
            }
        }
    }

    Ok(results)
}

/// 并行执行 — 对齐 Pi `executeToolCallsParallel` (agent-loop.ts:390-438)
///
/// 顺序 prepare → 并发 execute → 顺序 finalize + emit
async fn execute_tool_calls_parallel(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    struct PreparedEntry {
        tc: ToolCallInfo,
        tool: DynAgentTool,
        args: serde_json::Value,
    }

    let mut runnable: Vec<PreparedEntry> = Vec::new();

    for tc in tool_calls {
        emit_event(
            emit,
            AgentEvent::ToolExecutionStart {
                tool_call_id: tc.id.clone(),
                tool_name: tc.name.clone(),
                args: tc.arguments.clone(),
            },
        )
        .await;

        let preparation = prepare_tool_call(context, tool_instances, assistant_message, tc, config, cancel).await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                results.push(emit_tool_call_outcome(tc, &result, is_error, emit).await);
            }
            ToolCallPreparation::AwaitApproval {
                tool,
                args,
                reason,
                details,
            } => {
                match await_tool_approval(tc, &args, &reason, details, config, emit, cancel).await {
                    ApprovalResolution::Approved => {
                        runnable.push(PreparedEntry {
                            tc: tc.clone(),
                            tool,
                            args,
                        });
                    }
                    ApprovalResolution::Rejected { result } => {
                        results.push(emit_tool_result_message(tc, &result, emit).await);
                    }
                }
            }
            ToolCallPreparation::Prepared { tool, args } => {
                runnable.push(PreparedEntry {
                    tc: tc.clone(),
                    tool,
                    args,
                });
            }
        }
    }

    // Phase 2: 并发 execute — 对齐 Pi: 每个工具获得独立 on_update 回调
    let handles: Vec<_> = runnable
        .iter()
        .map(|entry| {
            let tool = entry.tool.clone();
            let tc_id = entry.tc.id.clone();
            let args = entry.args.clone();
            let cancel = cancel.clone();
            let on_update = Some(build_on_update(&entry.tc, emit));
            tokio::spawn(async move {
                execute_prepared_tool_call_inner(&tc_id, &tool, &args, cancel, on_update).await
            })
        })
        .collect();

    let executed_results: Vec<ExecutedOutcome> = {
        let mut out = Vec::with_capacity(handles.len());
        for handle in handles {
            out.push(handle.await.unwrap_or(ExecutedOutcome {
                result: error_tool_result("工具执行 task panic"),
                is_error: true,
            }));
        }
        out
    };

    // Phase 3: 顺序 finalize + emit
    for (entry, executed) in runnable.iter().zip(executed_results) {
        let finalized = finalize_executed_tool_call(
            context,
            assistant_message,
            &entry.tc,
            &entry.args,
            executed,
            config,
            cancel,
        )
        .await;
        results.push(
            emit_tool_call_outcome(&entry.tc, &finalized.result, finalized.is_error, emit).await,
        );
    }

    Ok(results)
}

// ─── 三阶段工具执行 ─────────────────────────────────────────

enum ToolCallPreparation {
    /// 立即返回结果（工具不存在、参数无效、被 beforeToolCall 阻止）
    Immediate {
        result: AgentToolResult,
        is_error: bool,
    },
    /// 等待用户审批后再决定是否执行
    AwaitApproval {
        tool: DynAgentTool,
        args: serde_json::Value,
        reason: String,
        details: Option<serde_json::Value>,
    },
    /// 准备就绪，可以执行
    Prepared {
        tool: DynAgentTool,
        args: serde_json::Value,
    },
}

struct ExecutedOutcome {
    result: AgentToolResult,
    is_error: bool,
}

/// Phase 1: prepare — 对齐 Pi `prepareToolCall` (agent-loop.ts:458-507)
///
/// 从 tool_instances 查找工具 → validate → delegate 钩子 → 返回 Prepared / Immediate
async fn prepare_tool_call(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tc: &ToolCallInfo,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
) -> ToolCallPreparation {
    let tool = tool_instances.iter().find(|t| t.name() == tc.name);
    let tool = match tool {
        Some(t) => t.clone(),
        None => {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(format!("Tool {} not found", tc.name)),
                is_error: true,
            };
        }
    };

    let mut args = match validate_tool_call_arguments(&tool, tc) {
        Ok(args) => args,
        Err(error) => {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(error),
                is_error: true,
            };
        }
    };

    if let Some(delegate) = config.runtime_delegate.as_ref() {
        let input = BeforeToolCallInput {
            assistant_message: assistant_message.clone(),
            tool_call: tc.clone(),
            args: args.clone(),
            context: context.clone(),
        };
        let decision = match delegate.before_tool_call(input, cancel.clone()).await {
            Ok(decision) => decision,
            Err(error) => {
                return ToolCallPreparation::Immediate {
                    result: error_tool_result(format!(
                        "runtime delegate before_tool_call 失败: {error}"
                    )),
                    is_error: true,
                };
            }
        };

        match decision {
            ToolCallDecision::Allow => {}
            ToolCallDecision::Deny { reason } => {
                return ToolCallPreparation::Immediate {
                    result: error_tool_result(reason),
                    is_error: true,
                };
            }
            ToolCallDecision::Ask {
                reason,
                args: approval_args,
                details,
            } => {
                if config.await_tool_approval.is_none() {
                    return ToolCallPreparation::Immediate {
                        result: error_tool_result(
                            "runtime delegate 请求审批，但当前 Agent 未配置审批等待能力",
                        ),
                        is_error: true,
                    };
                }
                let args = match approval_args {
                    Some(rewritten) => match validate_tool_arguments(&tool, &tc.name, &rewritten) {
                        Ok(validated) => validated,
                        Err(error) => {
                            return ToolCallPreparation::Immediate {
                                result: error_tool_result(error),
                                is_error: true,
                            };
                        }
                    },
                    None => args,
                };
                return ToolCallPreparation::AwaitApproval {
                    tool,
                    args,
                    reason,
                    details,
                };
            }
            ToolCallDecision::Rewrite {
                args: rewritten, ..
            } => match validate_tool_arguments(&tool, &tc.name, &rewritten) {
                Ok(validated) => args = validated,
                Err(error) => {
                    return ToolCallPreparation::Immediate {
                        result: error_tool_result(error),
                        is_error: true,
                    };
                }
            },
        }
    }

    if let Some(ref hook) = config.before_tool_call {
        let ctx = BeforeToolCallContext {
            assistant_message,
            tool_call: tc,
            args: &args,
            context,
        };
        if let Some(before_result) = hook(ctx, cancel.clone()).await
            && before_result.block
        {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(
                    before_result
                        .reason
                        .unwrap_or_else(|| "Tool execution was blocked".to_string()),
                ),
                is_error: true,
            };
        }
    }

    ToolCallPreparation::Prepared { tool, args }
}

/// Phase 2: execute — 对齐 Pi `executePreparedToolCall` (agent-loop.ts:509-544)
async fn execute_prepared_tool_call(
    tc: &ToolCallInfo,
    tool: &DynAgentTool,
    args: &serde_json::Value,
    cancel: &CancellationToken,
    emit: &AgentEventSink,
) -> ExecutedOutcome {
    let on_update = build_on_update(tc, emit);
    execute_prepared_tool_call_inner(&tc.id, tool, args, cancel.clone(), Some(on_update)).await
}

/// 构建 `on_update` 回调 — 对齐 Pi `executePreparedToolCall` 内联闭包
fn build_on_update(tc: &ToolCallInfo, emit: &AgentEventSink) -> ToolUpdateCallback {
    let emit = emit.clone();
    let tc_id = tc.id.clone();
    let tc_name = tc.name.clone();
    let tc_args = tc.arguments.clone();
    Arc::new(move |partial_result: AgentToolResult| {
        let emit = emit.clone();
        let event = AgentEvent::ToolExecutionUpdate {
            tool_call_id: tc_id.clone(),
            tool_name: tc_name.clone(),
            args: tc_args.clone(),
            partial_result: serde_json::to_value(&partial_result).unwrap_or_default(),
        };
        tokio::spawn(async move {
            emit(event).await;
        });
    })
}

async fn execute_prepared_tool_call_inner(
    tool_call_id: &str,
    tool: &DynAgentTool,
    args: &serde_json::Value,
    cancel: CancellationToken,
    on_update: Option<ToolUpdateCallback>,
) -> ExecutedOutcome {
    match tool
        .execute(tool_call_id, args.clone(), cancel, on_update)
        .await
    {
        Ok(result) => {
            let is_error = result.is_error;
            ExecutedOutcome { result, is_error }
        }
        Err(e) => ExecutedOutcome {
            result: error_tool_result(format!("{e}")),
            is_error: true,
        },
    }
}

/// Phase 3: finalize — 对齐 Pi `finalizeExecutedToolCall` (agent-loop.ts:546-580)
///
/// 调用 afterToolCall 钩子，允许覆盖结果。
async fn finalize_executed_tool_call(
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tc: &ToolCallInfo,
    args: &serde_json::Value,
    executed: ExecutedOutcome,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
) -> ExecutedOutcome {
    let mut result = executed.result;
    let mut is_error = executed.is_error;

    if let Some(delegate) = config.runtime_delegate.as_ref() {
        let input = AfterToolCallInput {
            assistant_message: assistant_message.clone(),
            tool_call: tc.clone(),
            args: args.clone(),
            result: result.clone(),
            is_error,
            context: context.clone(),
        };

        match delegate.after_tool_call(input, cancel.clone()).await {
            Ok(effects) => {
                if let Some(content) = effects.content {
                    result.content = content;
                }
                if let Some(details) = effects.details {
                    result.details = Some(details);
                }
                if let Some(err) = effects.is_error {
                    is_error = err;
                }
            }
            Err(error) => {
                return ExecutedOutcome {
                    result: error_tool_result(format!(
                        "runtime delegate after_tool_call 失败: {error}"
                    )),
                    is_error: true,
                };
            }
        }
    }

    if let Some(ref hook) = config.after_tool_call {
        let ctx = AfterToolCallContext {
            assistant_message,
            tool_call: tc,
            args,
            result: &result,
            is_error,
            context,
        };
        if let Some(after_result) = hook(ctx, cancel.clone()).await {
            if let Some(content) = after_result.content {
                result.content = content;
            }
            if let Some(details) = after_result.details {
                result.details = Some(details);
            }
            if let Some(err) = after_result.is_error {
                is_error = err;
            }
        }
    }

    result.is_error = is_error;
    ExecutedOutcome { result, is_error }
}

// ─── 辅助函数 ───────────────────────────────────────────────

fn validate_tool_call_arguments(
    tool: &DynAgentTool,
    tc: &ToolCallInfo,
) -> Result<serde_json::Value, String> {
    validate_tool_arguments(tool, &tc.name, &tc.arguments)
}

fn validate_tool_arguments(
    tool: &DynAgentTool,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let schema = tool.parameters_schema();
    let validator = validator_for(&schema)
        .map_err(|error| format!("Tool {} schema is invalid: {error}", tool_name))?;
    let errors = validator
        .iter_errors(args)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(args.clone())
    } else {
        Err(format!(
            "Tool {} arguments are invalid: {}",
            tool_name,
            errors.join("; ")
        ))
    }
}

/// 发出工具执行结果事件并构建 ToolResult 消息
/// 对齐 Pi `emitToolCallOutcome` (agent-loop.ts:589-616)
async fn emit_tool_call_outcome(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    is_error: bool,
    emit: &AgentEventSink,
) -> AgentMessage {
    emit_event(
        emit,
        AgentEvent::ToolExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            result: serde_json::to_value(result).unwrap_or_else(|_| {
                serde_json::json!({
                    "content": result.content,
                    "is_error": is_error,
                    "details": result.details,
                })
            }),
            is_error,
        },
    )
    .await;

    emit_tool_result_message(tc, result, emit).await
}

async fn emit_tool_result_message(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    emit: &AgentEventSink,
) -> AgentMessage {
    let tool_result_msg = AgentMessage::tool_result_full(
        &tc.id,
        tc.call_id.clone(),
        Some(tc.name.clone()),
        result.content.clone(),
        result.details.clone(),
        result.is_error,
    );

    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: tool_result_msg.clone(),
        },
    )
    .await;
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: tool_result_msg.clone(),
        },
    )
    .await;

    tool_result_msg
}

fn error_tool_result(message: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: None,
    }
}

enum ApprovalResolution {
    Approved,
    Rejected { result: AgentToolResult },
}

async fn await_tool_approval(
    tc: &ToolCallInfo,
    args: &serde_json::Value,
    reason: &str,
    details: Option<serde_json::Value>,
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> ApprovalResolution {
    emit_event(
        emit,
        AgentEvent::ToolExecutionPendingApproval {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            args: args.clone(),
            reason: reason.to_string(),
            details: details.clone(),
        },
    )
    .await;

    let Some(await_approval) = config.await_tool_approval.as_ref() else {
        return ApprovalResolution::Rejected {
            result: approval_rejected_tool_result(Some(
                "当前 Agent 未配置审批等待能力".to_string(),
            )),
        };
    };

    match await_approval(
        ToolApprovalRequest {
            tool_call: tc.clone(),
            args: args.clone(),
            reason: reason.to_string(),
            details,
        },
        cancel.clone(),
    )
    .await
    {
        ToolApprovalOutcome::Approved => {
            emit_event(
                emit,
                AgentEvent::ToolExecutionApprovalResolved {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    args: args.clone(),
                    approved: true,
                    reason: None,
                },
            )
            .await;
            ApprovalResolution::Approved
        }
        ToolApprovalOutcome::Rejected { reason } => {
            emit_event(
                emit,
                AgentEvent::ToolExecutionApprovalResolved {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    args: args.clone(),
                    approved: false,
                    reason: reason.clone(),
                },
            )
            .await;
            ApprovalResolution::Rejected {
                result: approval_rejected_tool_result(reason),
            }
        }
    }
}

fn approval_rejected_tool_result(reason: Option<String>) -> AgentToolResult {
    let message = reason
        .clone()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("工具执行未获批准：{value}"))
        .unwrap_or_else(|| "工具执行未获批准".to_string());
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: Some(serde_json::json!({
            "approval_state": "rejected",
            "reason": reason,
        })),
    }
}

fn poll_steering(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    config
        .get_steering_messages
        .as_ref()
        .map(|f| f())
        .unwrap_or_default()
}

fn poll_follow_up(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    config
        .get_follow_up_messages
        .as_ref()
        .map(|f| f())
        .unwrap_or_default()
}

