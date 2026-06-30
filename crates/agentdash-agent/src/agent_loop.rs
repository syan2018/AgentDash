use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use tokio_util::sync::CancellationToken;

use crate::bridge::LlmBridge;
use crate::types::{
    AfterToolCallContext, AfterToolCallResult, AfterTurnInput, AgentContext, AgentError,
    AgentEvent, AgentMessage, AgentRuntimeDelegateSet, BeforeStopInput, BeforeToolCallContext,
    BeforeToolCallResult, DynAgentTool, StopDecision, ToolApprovalOutcome, ToolApprovalRequest,
    ToolExecutionMode,
};

mod streaming;
mod tool_call;
mod tool_result;

use self::streaming::stream_assistant_response;
use self::tool_call::execute_tool_calls;

const MAX_CONSECUTIVE_EMPTY_CONTINUES: usize = 1;

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

pub type ToolResultCacheWriter = Arc<dyn Fn(ToolResultCacheWrite) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadableBodyKind {
    Tool,
    Command,
}

impl ReadableBodyKind {
    pub fn for_tool_name(tool_name: &str) -> Self {
        if tool_name == "shell_exec" {
            Self::Command
        } else {
            Self::Tool
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool_result",
            Self::Command => "command_result",
        }
    }

    fn alias_prefix(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Command => "cmd",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableToolResultRef {
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: ReadableBodyKind,
    pub item_id: String,
    pub lifecycle_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableTerminalRef {
    pub raw_terminal_id: String,
    pub terminal_alias: String,
    pub metadata_path: String,
    pub log_path: String,
    pub lifecycle_path: String,
}

#[derive(Debug, Default)]
pub struct ReadableIdRegistry {
    inner: RwLock<ReadableIdRegistryState>,
}

#[derive(Debug, Default)]
struct ReadableIdRegistryState {
    turn_aliases: HashMap<String, String>,
    body_aliases: HashMap<ReadableBodyAliasKey, String>,
    terminal_aliases: HashMap<String, String>,
    next_turn: usize,
    next_tool: usize,
    next_command: usize,
    next_terminal: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReadableBodyAliasKey {
    kind: ReadableBodyKind,
    raw_tool_call_id: String,
}

impl ReadableIdRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn observe_tool_result_item_id(&self, item_id: &str) {
        let Some((turn_index, body_kind, body_index)) = parse_tool_result_item_id(item_id) else {
            return;
        };
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.next_turn = state.next_turn.max(turn_index);
        match body_kind {
            ReadableBodyKind::Tool => state.next_tool = state.next_tool.max(body_index),
            ReadableBodyKind::Command => state.next_command = state.next_command.max(body_index),
        }
    }

    pub fn tool_result_ref(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        tool_name: &str,
    ) -> ReadableToolResultRef {
        let kind = ReadableBodyKind::for_tool_name(tool_name);
        let mut state = self.inner.write().unwrap();
        let turn_alias = state.turn_alias(raw_turn_id);
        let body_alias = state.body_alias(kind, raw_tool_call_id);
        let item_id = readable_tool_result_item_id(&turn_alias, &body_alias);
        let lifecycle_path = readable_tool_result_lifecycle_path(&turn_alias, &body_alias);
        ReadableToolResultRef {
            raw_turn_id: raw_turn_id.to_string(),
            raw_tool_call_id: raw_tool_call_id.to_string(),
            turn_alias,
            body_alias,
            body_kind: kind,
            item_id,
            lifecycle_path,
        }
    }

    pub fn terminal_ref(&self, raw_terminal_id: &str) -> ReadableTerminalRef {
        let mut state = self.inner.write().unwrap();
        let terminal_alias = state.terminal_alias(raw_terminal_id);
        ReadableTerminalRef {
            raw_terminal_id: raw_terminal_id.to_string(),
            metadata_path: format!("session/terminal/{terminal_alias}.metadata.json"),
            log_path: format!("session/terminal/{terminal_alias}.log"),
            lifecycle_path: format!("lifecycle://session/terminal/{terminal_alias}.log"),
            terminal_alias,
        }
    }
}

impl ReadableIdRegistryState {
    fn turn_alias(&mut self, raw_turn_id: &str) -> String {
        if let Some(alias) = self.turn_aliases.get(raw_turn_id) {
            return alias.clone();
        }
        self.next_turn += 1;
        let alias = format_readable_alias("turn", self.next_turn);
        self.turn_aliases
            .insert(raw_turn_id.to_string(), alias.clone());
        alias
    }

    fn body_alias(&mut self, kind: ReadableBodyKind, raw_tool_call_id: &str) -> String {
        let key = ReadableBodyAliasKey {
            kind,
            raw_tool_call_id: raw_tool_call_id.to_string(),
        };
        if let Some(alias) = self.body_aliases.get(&key) {
            return alias.clone();
        }
        let next = match kind {
            ReadableBodyKind::Tool => {
                self.next_tool += 1;
                self.next_tool
            }
            ReadableBodyKind::Command => {
                self.next_command += 1;
                self.next_command
            }
        };
        let alias = format_readable_alias(kind.alias_prefix(), next);
        self.body_aliases.insert(key, alias.clone());
        alias
    }

    fn terminal_alias(&mut self, raw_terminal_id: &str) -> String {
        if let Some(alias) = self.terminal_aliases.get(raw_terminal_id) {
            return alias.clone();
        }
        self.next_terminal += 1;
        let alias = format_readable_alias("term", self.next_terminal);
        self.terminal_aliases
            .insert(raw_terminal_id.to_string(), alias.clone());
        alias
    }
}

fn format_readable_alias(prefix: &str, index: usize) -> String {
    if index < 1000 {
        format!("{prefix}_{index:03}")
    } else {
        format!("{prefix}_{index}")
    }
}

fn parse_readable_alias(alias: &str, prefix: &str) -> Option<usize> {
    let suffix = alias
        .strip_prefix(prefix)?
        .strip_prefix('_')
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))?;
    let index = suffix.parse::<usize>().ok()?;
    (index > 0).then_some(index)
}

fn parse_tool_result_item_id(item_id: &str) -> Option<(usize, ReadableBodyKind, usize)> {
    let (turn_alias, body_alias) = item_id.split_once(':')?;
    let turn_index = parse_readable_alias(turn_alias, "turn")?;
    if let Some(body_index) = parse_readable_alias(body_alias, "tool") {
        return Some((turn_index, ReadableBodyKind::Tool, body_index));
    }
    if let Some(body_index) = parse_readable_alias(body_alias, "cmd") {
        return Some((turn_index, ReadableBodyKind::Command, body_index));
    }
    None
}

pub fn readable_tool_result_item_id(turn_alias: &str, body_alias: &str) -> String {
    format!("{turn_alias}:{body_alias}")
}

pub fn readable_tool_result_lifecycle_path(turn_alias: &str, body_alias: &str) -> String {
    format!("lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt")
}

#[derive(Clone)]
pub struct ToolResultRefContext {
    pub session_id: String,
    pub raw_turn_id: String,
    pub readable_ids: Arc<ReadableIdRegistry>,
    pub cache_writer: Option<ToolResultCacheWriter>,
}

#[derive(Debug, Clone)]
pub struct ToolResultCacheWrite {
    pub session_id: String,
    pub item_id: String,
    pub lifecycle_path: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub body_kind: String,
    pub raw_turn_id: String,
    pub raw_tool_call_id: String,
    pub tool_name: String,
    pub text: String,
    pub original_bytes: usize,
}

pub fn stable_tool_result_item_id(turn_id: &str, tool_call_id: &str) -> String {
    format!("{turn_id}:{tool_call_id}")
}

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

    /// 显式运行时委托 facet 集合。
    pub runtime_delegates: AgentRuntimeDelegateSet,

    /// 当前 turn 的工具结果引用上下文，用于生成 stable lifecycle path 并写入外部缓存。
    pub tool_result_ref_context: Option<ToolResultRefContext>,
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
    let mut consecutive_empty_continues = 0_usize;

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
                consecutive_empty_continues = 0;
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
                    context.message_refs.push(None);
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
                    context.message_refs.push(None);
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
            consecutive_empty_continues = 0;
            pending_messages = std::mem::take(&mut pending_follow_up_messages);
            continue;
        }

        let follow_ups = poll_follow_up(config);
        if !follow_ups.is_empty() {
            consecutive_empty_continues = 0;
            pending_messages = follow_ups;
            continue;
        }

        if let Some(stop_decision) = run_before_stop_delegate(config, context, cancel).await? {
            match stop_decision {
                StopDecision::Stop => {}
                StopDecision::Continue {
                    mut steering,
                    mut follow_up,
                    reason,
                    allow_empty,
                } => {
                    let is_empty_continue = steering.is_empty() && follow_up.is_empty();
                    if is_empty_continue {
                        if !allow_empty {
                            break;
                        }
                        consecutive_empty_continues = consecutive_empty_continues.saturating_add(1);
                        if consecutive_empty_continues > MAX_CONSECUTIVE_EMPTY_CONTINUES {
                            let reason = reason.as_deref().unwrap_or("unspecified");
                            return Err(AgentError::ContinueError(format!(
                                "空 continuation 连续触发且没有新消息（reason: {reason}）"
                            )));
                        }
                    } else {
                        consecutive_empty_continues = 0;
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
    if config.runtime_delegates.turn_boundary.is_none() {
        return Ok(None);
    }

    config
        .runtime_delegates
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
    if config.runtime_delegates.turn_boundary.is_none() {
        return Ok(None);
    }

    config
        .runtime_delegates
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_id_registry_reserves_history_item_ids_after_restore() {
        let registry = ReadableIdRegistry::new();
        registry.observe_tool_result_item_id("turn_001:tool_004");
        registry.observe_tool_result_item_id("turn_002:cmd_002");

        let tool_ref = registry.tool_result_ref("raw-turn-new", "raw-tool-new", "fs_read");
        assert_eq!(tool_ref.item_id, "turn_003:tool_005");

        let command_ref = registry.tool_result_ref("raw-turn-new", "raw-command-new", "shell_exec");
        assert_eq!(command_ref.item_id, "turn_003:cmd_003");
    }
}
