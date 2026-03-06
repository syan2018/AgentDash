/// Agent 循环 — 复刻 Pi 的 `agentLoop` 核心语义
///
/// 流程：
/// 1. 将 prompt 追加到 context.messages
/// 2. 发出 `agent_start` 事件
/// 3. 外层循环（处理 follow-up）
///    a. 发出 `turn_start`
///    b. 可选 `transform_context` 管线
///    c. `convert_to_llm` → rig::Message[]
///    d. 调用 Bridge.complete()
///    e. 发出 `message_start` / `message_end`
///    f. 若有 tool_calls → 执行工具 → 追加结果 → 继续内循环
///    g. 若无 tool_calls → 检查 follow-up 队列 → 决定是否继续外循环
///    h. 发出 `turn_end`
/// 4. 发出 `agent_end`
use futures::StreamExt;
use rig::completion::ToolDefinition;
use tokio_util::sync::CancellationToken;

use crate::bridge::{BridgeRequest, LlmBridge, StreamChunk};
use crate::event_stream::EventSender;
use crate::types::{
    AgentContext, AgentError, AgentEvent, AgentMessage, AgentToolResult, ContentPart, DynAgentTool,
    ToolCallInfo,
};

const DEFAULT_MAX_TURNS: usize = 25;

/// Agent Loop 配置
pub struct AgentLoopConfig {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub max_turns: usize,
    /// 获取 steering 消息（循环中每次工具执行后调用，若非空则跳过剩余工具并继续下一轮）
    pub get_steering_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    /// 获取 follow-up 消息（循环结束前调用，若非空则继续下一轮）
    pub get_follow_up_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            max_tokens: Some(8192),
            max_turns: DEFAULT_MAX_TURNS,
            get_steering_messages: None,
            get_follow_up_messages: None,
        }
    }
}

/// 启动一次完整的 Agent Loop（带新 prompt）
pub async fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    context.messages.extend(prompts);
    run_loop(context, config, bridge, events, cancel).await
}

/// 从当前上下文继续 Agent Loop（不添加新 prompt）
pub async fn agent_loop_continue(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    events: &EventSender<AgentEvent>,
    cancel: CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    run_loop(context, config, bridge, events, cancel).await
}

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

    'outer: loop {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        turn_count += 1;
        if turn_count > config.max_turns {
            return Err(AgentError::MaxTurnsExceeded(config.max_turns));
        }

        events.send(AgentEvent::TurnStart);

        // ─── 调用 LLM ──────────────────────────────────
        let request = BridgeRequest {
            system_prompt: Some(context.system_prompt.clone()),
            messages: context.messages.clone(),
            tools: tool_definitions.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
        };

        events.send(AgentEvent::MessageStart);

        // 流式调用 LLM — 逐 chunk 发送 MessageDelta，最终聚合完整响应
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
        let assistant_message = response.message.clone();

        events.send(AgentEvent::MessageEnd {
            message: assistant_message.clone(),
        });

        context.messages.push(assistant_message.clone());

        // ─── 提取 tool_calls ────────────────────────────
        let tool_calls = match &assistant_message {
            AgentMessage::Assistant { tool_calls, .. } => tool_calls.clone(),
            _ => vec![],
        };

        if tool_calls.is_empty() {
            // 没有工具调用 → 检查 follow-up
            events.send(AgentEvent::TurnEnd);

            if let Some(ref get_follow_up) = config.get_follow_up_messages {
                let follow_ups = get_follow_up();
                if !follow_ups.is_empty() {
                    context.messages.extend(follow_ups);
                    continue 'outer;
                }
            }

            break 'outer;
        }

        // ─── 执行工具调用 ───────────────────────────────
        let tool_results = execute_tool_calls(
            &context.tools,
            &tool_calls,
            events,
            config.get_steering_messages.as_deref(),
            &cancel,
        )
        .await?;

        context.messages.extend(tool_results.results);

        if let Some(steering) = tool_results.steering_messages {
            context.messages.extend(steering);
        }

        events.send(AgentEvent::TurnEnd);
    }

    events.send(AgentEvent::AgentEnd {
        messages: context.messages.clone(),
    });

    Ok(context.messages.clone())
}

struct ToolExecutionResult {
    results: Vec<AgentMessage>,
    steering_messages: Option<Vec<AgentMessage>>,
}

async fn execute_tool_calls(
    tools: &[DynAgentTool],
    tool_calls: &[ToolCallInfo],
    events: &EventSender<AgentEvent>,
    get_steering: Option<&(dyn Fn() -> Vec<AgentMessage> + Send + Sync)>,
    cancel: &CancellationToken,
) -> Result<ToolExecutionResult, AgentError> {
    let mut results = Vec::new();
    let tool_map: std::collections::HashMap<&str, &DynAgentTool> =
        tools.iter().map(|t| (t.name(), t)).collect();

    for tc in tool_calls {
        if cancel.is_cancelled() {
            results.push(skip_tool_call(tc, "Agent 已取消"));
            continue;
        }

        events.send(AgentEvent::ToolExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            args: tc.arguments.clone(),
        });

        let result = if let Some(tool) = tool_map.get(tc.name.as_str()) {
            match tool.execute(&tc.id, tc.arguments.clone()).await {
                Ok(result) => result,
                Err(e) => AgentToolResult {
                    content: vec![ContentPart::text(format!("工具执行错误: {e}"))],
                    is_error: true,
                },
            }
        } else {
            AgentToolResult {
                content: vec![ContentPart::text(format!("未知工具: {}", tc.name))],
                is_error: true,
            }
        };

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
            is_error: result.is_error,
        });

        results.push(AgentMessage::tool_result(&tc.id, &result_text, result.is_error));

        // steering 检查：每个工具执行后轮询 steering 队列
        if let Some(get_steering) = get_steering {
            let steering = get_steering();
            if !steering.is_empty() {
                // 跳过剩余工具，将 steering 消息注入
                return Ok(ToolExecutionResult {
                    results,
                    steering_messages: Some(steering),
                });
            }
        }
    }

    Ok(ToolExecutionResult {
        results,
        steering_messages: None,
    })
}

fn skip_tool_call(tc: &ToolCallInfo, reason: &str) -> AgentMessage {
    AgentMessage::tool_result(&tc.id, format!("[已跳过] {reason}"), true)
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
