use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::bridge::LlmBridge;
use crate::types::{
    AfterToolCallContext, AfterToolCallResult, AfterTurnInput, AgentContext, AgentError,
    AgentEvent, AgentMessage, BeforeStopInput, BeforeToolCallContext, BeforeToolCallResult,
    DynAgentRuntimeDelegate, DynAgentTool, StopDecision, ToolApprovalOutcome, ToolApprovalRequest,
    ToolExecutionMode,
};

mod streaming;
mod tool_call;
mod tool_result;

use self::streaming::stream_assistant_response;
use self::tool_call::execute_tool_calls;

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

/// Live 工具获取回调。
///
/// 工具表可能在当前 running turn 内由 workflow phase 切换热更新，因此 LLM
/// 请求和工具执行不能只读 run_loop 启动时的快照。
pub type GetToolsFn = Arc<dyn Fn() -> Vec<DynAgentTool> + Send + Sync>;

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
#[derive(Default)]
pub struct AgentLoopConfig {
    /// 上下文变换管线（每次 LLM 调用前执行）
    /// 对齐 Pi `transformContext`：用于上下文裁剪、外部信息注入等。
    pub transform_context: Option<TransformContextFn>,

    /// 获取 steering 消息
    pub get_steering_messages: Option<GetMessagesFn>,

    /// 获取 follow-up 消息
    pub get_follow_up_messages: Option<GetMessagesFn>,

    /// 获取当前 live 工具表。
    pub get_tools: Option<GetToolsFn>,

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
    run_loop(
        context,
        tool_instances,
        &mut new_messages,
        config,
        bridge,
        emit,
        &cancel,
    )
    .await
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
    run_loop(
        context,
        tool_instances,
        &mut new_messages,
        config,
        bridge,
        emit,
        &cancel,
    )
    .await
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
    let mut first_turn = true;
    let mut pending_messages = poll_steering(config);
    let mut pending_follow_up_messages: Vec<AgentMessage> = Vec::new();

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
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
                stream_assistant_response(context, tool_instances, config, bridge, emit, cancel)
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
                    allow_empty,
                    ..
                } => {
                    if steering.is_empty() && follow_up.is_empty() && !allow_empty {
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
