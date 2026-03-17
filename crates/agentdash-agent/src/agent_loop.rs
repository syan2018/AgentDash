/// Agent 循环 — 严格对齐 Pi `agent-loop.ts` 核心语义
///
/// 流程（对齐 Pi `runLoop`）：
/// 1. 将 prompt 追加到 context.messages，为每个 prompt 发出 message_start/end
/// 2. 发出 `agent_start`
/// 3. 循环开始前轮询 steering（用户可能在等待期间输入）
/// 4. 外循环（follow-up 驱动）
///    4a. 内循环（tool calls + steering 驱动）
///        - 注入 pending messages (steering)
///        - `transform_context` → `convert_to_llm` → Bridge.stream_complete()
///        - 发出 message 事件
///        - 若有 tool_calls → 执行（支持 sequential / parallel）
///        - 轮询 steering
///    4b. 检查 follow-up → 若有则继续外循环
/// 5. 发出 `agent_end`
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use rig::completion::ToolDefinition;
use tokio_util::sync::CancellationToken;

use crate::bridge::{BridgeRequest, LlmBridge, StreamChunk};
use crate::event_stream::EventSender;
use crate::types::{
    AfterToolCallContext, AfterToolCallResult, AgentContext, AgentError, AgentEvent, AgentMessage,
    AgentToolResult, BeforeToolCallContext, BeforeToolCallResult, ContentPart, DynAgentTool,
    ToolCallInfo, ToolExecutionMode, ToolUpdateCallback,
};

const DEFAULT_MAX_TURNS: usize = 25;

// ─── 回调类型别名 ───────────────────────────────────────────

/// 消息转换回调：AgentMessage[] → rig::Message[]
/// 对齐 Pi `AgentLoopConfig.convertToLlm`
pub type ConvertToLlmFn = Arc<
    dyn Fn(&[AgentMessage]) -> Vec<rig::completion::Message> + Send + Sync,
>;

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

// ─── AgentLoopConfig ────────────────────────────────────────

/// Agent Loop 配置 — 对齐 Pi `AgentLoopConfig`
pub struct AgentLoopConfig {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub max_turns: usize,

    /// 消息格式转换：AgentMessage[] → LLM Message[]
    /// 对齐 Pi `convertToLlm`。若为 None 则使用 `default_convert_to_llm`。
    pub convert_to_llm: Option<ConvertToLlmFn>,

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
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            max_tokens: Some(8192),
            max_turns: DEFAULT_MAX_TURNS,
            convert_to_llm: None,
            transform_context: None,
            get_steering_messages: None,
            get_follow_up_messages: None,
            tool_execution: ToolExecutionMode::default(),
            before_tool_call: None,
            after_tool_call: None,
        }
    }
}

// ─── 入口函数 ───────────────────────────────────────────────

/// 启动一次完整的 Agent Loop（带新 prompt）
/// 对齐 Pi `agentLoop` / `runAgentLoop`
pub async fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    // 对齐 Pi: 为每个 prompt 发出 message_start/end 事件
    for prompt in &prompts {
        events.send(AgentEvent::MessageStart {
            message: prompt.clone(),
        });
        events.send(AgentEvent::MessageEnd {
            message: prompt.clone(),
        });
    }
    context.messages.extend(prompts);
    run_loop(context, config, bridge, events, cancel).await
}

/// 从当前上下文继续 Agent Loop（不添加新 prompt）
/// 对齐 Pi `agentLoopContinue` / `runAgentLoopContinue`
pub async fn agent_loop_continue(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    // 对齐 Pi: 安全检查 — 消息不能为空，且最后一条不能是 assistant
    if context.messages.is_empty() {
        return Err(AgentError::ContinueError(
            "Cannot continue: no messages in context".to_string(),
        ));
    }
    if matches!(context.messages.last(), Some(AgentMessage::Assistant { .. })) {
        return Err(AgentError::ContinueError(
            "Cannot continue from message role: assistant".to_string(),
        ));
    }
    run_loop(context, config, bridge, events, cancel).await
}

// ─── 主循环 ─────────────────────────────────────────────────

/// 主循环 — 严格对齐 Pi `runLoop` (agent-loop.ts:155-232)
///
/// 双循环结构：
/// - 外循环：follow-up 驱动（agent 本应停止时检查 follow-up 并继续）
/// - 内循环：tool calls + steering 驱动（有工具调用或 pending 消息时继续）
async fn run_loop(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    events.send(AgentEvent::AgentStart);

    let tool_definitions = build_tool_definitions(&context.tools);
    let mut turn_count: usize = 0;
    let mut first_turn = true;

    // 对齐 Pi: 循环开始前轮询 steering（用户可能在等待期间输入）
    let mut pending_messages = poll_steering(config);

    // 外循环：follow-up 驱动
    loop {
        let mut has_more_tool_calls = true;

        // 内循环：tool calls + steering 驱动
        // 对齐 Pi: while (hasMoreToolCalls || pendingMessages.length > 0)
        while has_more_tool_calls || !pending_messages.is_empty() {
            if cancel.is_cancelled() {
                emit_agent_end(context, events);
                return Err(AgentError::Cancelled);
            }

            turn_count += 1;
            if turn_count > config.max_turns {
                emit_agent_end(context, events);
                return Err(AgentError::MaxTurnsExceeded(config.max_turns));
            }

            if !first_turn {
                events.send(AgentEvent::TurnStart);
            } else {
                events.send(AgentEvent::TurnStart);
                first_turn = false;
            }

            // 对齐 Pi: 注入 pending messages (steering 消息)
            if !pending_messages.is_empty() {
                for msg in &pending_messages {
                    events.send(AgentEvent::MessageStart {
                        message: msg.clone(),
                    });
                    events.send(AgentEvent::MessageEnd {
                        message: msg.clone(),
                    });
                    context.messages.push(msg.clone());
                }
                pending_messages.clear();
            }

            // ─── 流式调用 LLM ───────────────────────────────
            let assistant_message = stream_assistant_response(
                context, config, bridge, events, &tool_definitions, &cancel,
            )
            .await?;

            // 对齐 Pi: 检查 stopReason 是否为 error/aborted (agent-loop.ts:194-198)
            if assistant_message.is_error_or_aborted() {
                events.send(AgentEvent::TurnEnd {
                    message: assistant_message,
                    tool_results: vec![],
                });
                emit_agent_end(context, events);
                return Ok(context.messages.clone());
            }

            // 提取 tool_calls
            let tool_calls = match &assistant_message {
                AgentMessage::Assistant { tool_calls, .. } => tool_calls.clone(),
                _ => vec![],
            };
            has_more_tool_calls = !tool_calls.is_empty();

            let mut tool_results = Vec::new();

            if has_more_tool_calls {
                tool_results = execute_tool_calls(
                    context,
                    &assistant_message,
                    &tool_calls,
                    config,
                    events,
                    &cancel,
                )
                .await?;

                for result in &tool_results {
                    context.messages.push(result.clone());
                }
            }

            events.send(AgentEvent::TurnEnd {
                message: assistant_message,
                tool_results,
            });

            // 对齐 Pi: 工具执行后轮询 steering
            pending_messages = poll_steering(config);
        }

        // 对齐 Pi: agent 本应停止，检查 follow-up
        let follow_ups = poll_follow_up(config);
        if !follow_ups.is_empty() {
            pending_messages = follow_ups;
            continue;
        }

        break;
    }

    emit_agent_end(context, events);
    Ok(context.messages.clone())
}

fn emit_agent_end(context: &AgentContext, events: &EventSender<AgentEvent>) {
    events.send(AgentEvent::AgentEnd {
        messages: context.messages.clone(),
    });
}

/// 对齐 Pi `streamAssistantResponse` (agent-loop.ts:238-331)
///
/// 流程：transformContext → convertToLlm → Bridge.stream_complete → 事件
async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    tool_definitions: &[ToolDefinition],
    cancel: &CancellationToken,
) -> Result<AgentMessage, AgentError> {
    // 对齐 Pi: transformContext 管线
    let messages_for_llm = if let Some(ref transform) = config.transform_context {
        transform(context.messages.clone(), cancel.clone()).await
    } else {
        context.messages.clone()
    };

    // 对齐 Pi: convertToLlm
    let llm_messages = if let Some(ref convert) = config.convert_to_llm {
        convert(&messages_for_llm)
    } else {
        crate::convert::default_convert_to_llm(&messages_for_llm)
    };

    let request = BridgeRequest {
        system_prompt: Some(context.system_prompt.clone()),
        messages: context.messages.clone(),
        tools: tool_definitions.to_vec(),
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        llm_messages: Some(llm_messages),
    };

    events.send(AgentEvent::MessageStart {
        message: AgentMessage::assistant(""),
    });

    let mut stream = bridge.stream_complete(request).await;
    let mut response = None;
    let mut stream_error = None;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        match chunk {
            StreamChunk::TextDelta(text) if !text.is_empty() => {
                events.send(AgentEvent::MessageDelta { text });
            }
            StreamChunk::Done(resp) => {
                response = Some(resp);
            }
            StreamChunk::Error(e) => {
                stream_error = Some(e);
            }
            _ => {}
        }
    }
    drop(stream);

    if let Some(e) = stream_error {
        return Err(e.into());
    }
    let response = response.ok_or(crate::bridge::BridgeError::EmptyResponse)?;

    // 对齐 Pi: 将 usage 和 stop_reason 传播到 AgentMessage
    let assistant_message = match response.message {
        AgentMessage::Assistant {
            content,
            tool_calls,
            ..
        } => {
            let has_tool_calls = !tool_calls.is_empty();
            let stop_reason = if has_tool_calls {
                Some(crate::types::StopReason::ToolUse)
            } else {
                Some(crate::types::StopReason::Stop)
            };
            let usage = Some(crate::types::TokenUsage {
                input: response.usage.input_tokens,
                output: response.usage.output_tokens,
            });
            AgentMessage::Assistant {
                content,
                tool_calls,
                stop_reason,
                error_message: None,
                usage,
                timestamp: Some(crate::types::now_millis()),
            }
        }
        other => other,
    };

    events.send(AgentEvent::MessageEnd {
        message: assistant_message.clone(),
    });

    context.messages.push(assistant_message.clone());

    Ok(assistant_message)
}

// ─── 工具执行 ───────────────────────────────────────────────

/// 工具执行入口 — 对齐 Pi `executeToolCalls` (agent-loop.ts:336-348)
///
/// 根据 `config.tool_execution` 分发到 sequential 或 parallel 实现。
async fn execute_tool_calls(
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    events: &EventSender<AgentEvent>,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    match config.tool_execution {
        ToolExecutionMode::Sequential => {
            execute_tool_calls_sequential(
                context,
                assistant_message,
                tool_calls,
                config,
                events,
                cancel,
            )
            .await
        }
        ToolExecutionMode::Parallel => {
            execute_tool_calls_parallel(
                context,
                assistant_message,
                tool_calls,
                config,
                events,
                cancel,
            )
            .await
        }
    }
}

/// 顺序执行 — 对齐 Pi `executeToolCallsSequential` (agent-loop.ts:350-388)
async fn execute_tool_calls_sequential(
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    events: &EventSender<AgentEvent>,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    for tc in tool_calls {
        events.send(AgentEvent::ToolExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            args: tc.arguments.clone(),
        });

        let preparation =
            prepare_tool_call(context, assistant_message, tc, config, cancel).await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                results.push(
                    emit_tool_call_outcome(tc, &result, is_error, events),
                );
            }
            ToolCallPreparation::Prepared { tool, args } => {
                let executed = execute_prepared_tool_call(tc, &tool, &args, cancel, events).await;
                let finalized =
                    finalize_executed_tool_call(context, assistant_message, tc, &args, executed, config, cancel)
                        .await;
                results.push(
                    emit_tool_call_outcome(tc, &finalized.result, finalized.is_error, events),
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
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    events: &EventSender<AgentEvent>,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    struct PreparedEntry {
        tc: ToolCallInfo,
        tool: DynAgentTool,
        args: serde_json::Value,
    }

    let mut runnable: Vec<PreparedEntry> = Vec::new();

    // Phase 1: 顺序 prepare
    for tc in tool_calls {
        events.send(AgentEvent::ToolExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            args: tc.arguments.clone(),
        });

        let preparation =
            prepare_tool_call(context, assistant_message, tc, config, cancel).await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                results.push(
                    emit_tool_call_outcome(tc, &result, is_error, events),
                );
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
            let on_update = Some(build_on_update(&entry.tc, events));
            tokio::spawn(async move { execute_prepared_tool_call_inner(&tc_id, &tool, &args, cancel, on_update).await })
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
    for (entry, executed) in runnable.iter().zip(executed_results.into_iter()) {
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
            emit_tool_call_outcome(&entry.tc, &finalized.result, finalized.is_error, events),
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
/// 查找工具 → beforeToolCall 钩子 → 返回 Prepared 或 Immediate
async fn prepare_tool_call(
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tc: &ToolCallInfo,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
) -> ToolCallPreparation {
    let tool = context.tools.iter().find(|t| t.name() == tc.name);
    let tool = match tool {
        Some(t) => t.clone(),
        None => {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(format!("Tool {} not found", tc.name)),
                is_error: true,
            };
        }
    };

    // 对齐 Pi: beforeToolCall 钩子
    if let Some(ref hook) = config.before_tool_call {
        let ctx = BeforeToolCallContext {
            assistant_message,
            tool_call: tc,
            args: &tc.arguments,
            context,
        };
        if let Some(before_result) = hook(ctx, cancel.clone()).await {
            if before_result.block {
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
    }

    ToolCallPreparation::Prepared {
        tool,
        args: tc.arguments.clone(),
    }
}

/// Phase 2: execute — 对齐 Pi `executePreparedToolCall` (agent-loop.ts:509-544)
async fn execute_prepared_tool_call(
    tc: &ToolCallInfo,
    tool: &DynAgentTool,
    args: &serde_json::Value,
    cancel: &CancellationToken,
    events: &EventSender<AgentEvent>,
) -> ExecutedOutcome {
    let on_update = build_on_update(tc, events);
    execute_prepared_tool_call_inner(&tc.id, tool, args, cancel.clone(), Some(on_update)).await
}

/// 构建 `on_update` 回调 — 对齐 Pi `executePreparedToolCall` 内联闭包
fn build_on_update(tc: &ToolCallInfo, events: &EventSender<AgentEvent>) -> ToolUpdateCallback {
    let events = events.clone();
    let tc_id = tc.id.clone();
    let tc_name = tc.name.clone();
    let tc_args = tc.arguments.clone();
    Arc::new(move |partial_result: AgentToolResult| {
        events.send(AgentEvent::ToolExecutionUpdate {
            tool_call_id: tc_id.clone(),
            tool_name: tc_name.clone(),
            args: tc_args.clone(),
            partial_result: serde_json::to_value(&partial_result).unwrap_or_default(),
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
    match tool.execute(tool_call_id, args.clone(), cancel, on_update).await {
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

    ExecutedOutcome { result, is_error }
}

// ─── 辅助函数 ───────────────────────────────────────────────

/// 发出工具执行结果事件并构建 ToolResult 消息
/// 对齐 Pi `emitToolCallOutcome` (agent-loop.ts:589-616)
fn emit_tool_call_outcome(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    is_error: bool,
    events: &EventSender<AgentEvent>,
) -> AgentMessage {
    let result_text = result
        .content
        .iter()
        .filter_map(ContentPart::extract_text)
        .collect::<Vec<_>>()
        .join("\n");

    events.send(AgentEvent::ToolExecutionEnd {
        tool_call_id: tc.id.clone(),
        tool_name: tc.name.clone(),
        result: serde_json::json!({ "text": result_text }),
        is_error,
    });

    let tool_result_msg = AgentMessage::tool_result_full(
        &tc.id,
        tc.call_id.clone(),
        Some(tc.name.clone()),
        &result_text,
        result.details.clone(),
        is_error,
    );

    events.send(AgentEvent::MessageStart {
        message: tool_result_msg.clone(),
    });
    events.send(AgentEvent::MessageEnd {
        message: tool_result_msg.clone(),
    });

    tool_result_msg
}

fn error_tool_result(message: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: None,
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

fn build_tool_definitions(tools: &[DynAgentTool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|t| ToolDefinition {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.parameters_schema(),
        })
        .collect()
}
