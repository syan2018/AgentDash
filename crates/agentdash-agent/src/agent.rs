/// Agent 结构体 — 面向使用者的高层封装
///
/// 严格对齐 Pi `Agent` 类 (agent.ts:116-612)
///
/// 管理 Agent 的完整生命周期：
/// - 统一状态 (`AgentState`) — 对齐 Pi `Agent._state`
/// - 事件订阅（broadcast 多订阅者）— 对齐 Pi `Agent.listeners`
/// - 事件驱动状态同步 — 对齐 Pi `Agent._processLoopEvent`
/// - Steering / Follow-up 队列（支持 all / one-at-a-time 出队模式）
/// - prompt / continue 入口
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::{
    self, AfterToolCallFn, AgentEventSink, AgentLoopConfig, BeforeToolCallFn, ConvertToLlmFn,
    TransformContextFn,
};
use crate::bridge::LlmBridge;
use crate::event_stream::{self, EventReceiver};
use crate::types::{
    AgentContext, AgentError, AgentEvent, AgentMessage, AgentState, DynAgentTool, ThinkingLevel,
    ToolExecutionMode,
};

// ─── QueueMode ──────────────────────────────────────────────

/// 队列出队模式 — 对齐 Pi `steeringMode` / `followUpMode`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum QueueMode {
    /// 一次取出全部消息
    All,
    /// 一次只取出一条
    #[default]
    OneAtATime,
}

// ─── AgentConfig ────────────────────────────────────────────

/// Agent 配置 — 对齐 Pi `AgentOptions`
pub struct AgentConfig {
    pub system_prompt: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub max_turns: usize,

    /// 思考/推理级别 — 对齐 Pi `AgentState.thinkingLevel`
    pub thinking_level: ThinkingLevel,

    /// 消息格式转换回调
    pub convert_to_llm: Option<ConvertToLlmFn>,

    /// 上下文变换管线
    pub transform_context: Option<TransformContextFn>,

    /// Steering 出队模式（对齐 Pi `steeringMode`）
    pub steering_mode: QueueMode,

    /// Follow-up 出队模式（对齐 Pi `followUpMode`）
    pub follow_up_mode: QueueMode,

    /// 工具执行模式
    pub tool_execution: ToolExecutionMode,

    /// 工具执行前钩子
    pub before_tool_call: Option<BeforeToolCallFn>,

    /// 工具执行后钩子
    pub after_tool_call: Option<AfterToolCallFn>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            temperature: None,
            max_tokens: Some(8192),
            max_turns: 25,
            thinking_level: ThinkingLevel::default(),
            convert_to_llm: None,
            transform_context: None,
            steering_mode: QueueMode::default(),
            follow_up_mode: QueueMode::default(),
            tool_execution: ToolExecutionMode::default(),
            before_tool_call: None,
            after_tool_call: None,
        }
    }
}

// ─── Agent ──────────────────────────────────────────────────

/// Agent — Pi 风格的 Agent Runtime
///
/// 对齐 Pi `Agent` 类 (agent.ts:116)
///
/// 核心设计：
/// - `AgentState` 统一状态（对齐 Pi `_state`）— 通过 `Arc<Mutex>` 跨 task 共享
/// - 事件驱动状态同步（对齐 Pi `_processLoopEvent`）— spawned task 中处理
/// - broadcast 事件通道（对齐 Pi `listeners` 多订阅者模型）
pub struct Agent {
    config: AgentConfig,
    bridge: Arc<dyn LlmBridge>,
    /// 统一运行时状态 — 对齐 Pi `Agent._state`
    state: Arc<Mutex<AgentState>>,
    steering_queue: Arc<Mutex<Vec<AgentMessage>>>,
    follow_up_queue: Arc<Mutex<Vec<AgentMessage>>>,
    cancel: Option<CancellationToken>,
    idle_notify: Arc<Notify>,
    /// 持久化事件发送端 — 对齐 Pi `Agent.listeners` 多订阅者模型
    persistent_event_tx: Option<event_stream::EventSender<AgentEvent>>,
}

impl Agent {
    pub fn new(bridge: Arc<dyn LlmBridge>, config: AgentConfig) -> Self {
        let state = AgentState {
            system_prompt: config.system_prompt.clone(),
            thinking_level: config.thinking_level,
            ..AgentState::new()
        };
        Self {
            config,
            bridge,
            state: Arc::new(Mutex::new(state)),
            steering_queue: Arc::new(Mutex::new(Vec::new())),
            follow_up_queue: Arc::new(Mutex::new(Vec::new())),
            cancel: None,
            idle_notify: Arc::new(Notify::new()),
            persistent_event_tx: None,
        }
    }

    // ── Tool 管理 ──

    pub fn add_tool(&mut self, tool: DynAgentTool) {
        // 不阻塞 — 只在非运行期间调用
        let state = self.state.try_lock();
        if let Ok(mut s) = state {
            s.tools.push(tool);
        }
    }

    pub fn set_tools(&mut self, tools: Vec<DynAgentTool>) {
        if let Ok(mut s) = self.state.try_lock() {
            s.tools = tools;
        }
    }

    // ── State 访问 ──

    /// 获取当前 AgentState 快照 — 对齐 Pi `Agent.state`
    pub async fn state(&self) -> AgentState {
        self.state.lock().await.clone()
    }

    /// 同步版本（非 async 上下文使用，可能失败）
    pub fn try_state(&self) -> Option<AgentState> {
        self.state.try_lock().ok().map(|s| s.clone())
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let prompt = prompt.into();
        self.config.system_prompt = prompt.clone();
        if let Ok(mut s) = self.state.try_lock() {
            s.system_prompt = prompt;
        }
    }

    pub async fn messages(&self) -> Vec<AgentMessage> {
        self.state.lock().await.messages.clone()
    }

    pub async fn replace_messages(&self, messages: Vec<AgentMessage>) {
        self.state.lock().await.messages = messages;
    }

    pub async fn clear_messages(&self) {
        self.state.lock().await.messages.clear();
    }

    pub fn set_thinking_level(&mut self, level: ThinkingLevel) {
        self.config.thinking_level = level;
        if let Ok(mut s) = self.state.try_lock() {
            s.thinking_level = level;
        }
    }

    /// 订阅 Agent 事件流 — 对齐 Pi `Agent.subscribe(fn)`
    ///
    /// 返回一个 `EventReceiver`，可作为 `Stream` 消费。
    /// 支持多次调用创建多个独立订阅者。
    pub fn subscribe(&mut self) -> event_stream::EventReceiver<AgentEvent> {
        if self.persistent_event_tx.is_none() {
            let (tx, _) = event_stream::event_channel();
            self.persistent_event_tx = Some(tx);
        }
        self.persistent_event_tx.as_ref().unwrap().subscribe()
    }

    // ── Queue Mode 配置 ──

    pub fn set_steering_mode(&mut self, mode: QueueMode) {
        self.config.steering_mode = mode;
    }

    pub fn set_follow_up_mode(&mut self, mode: QueueMode) {
        self.config.follow_up_mode = mode;
    }

    // ── Steering / Follow-up 队列 ──

    /// 向 steering 队列推入消息 — 对齐 Pi `steer(m)`
    pub async fn steer(&self, msg: AgentMessage) {
        self.steering_queue.lock().await.push(msg);
    }

    /// 向 follow-up 队列推入消息 — 对齐 Pi `followUp(m)`
    pub async fn follow_up(&self, msg: AgentMessage) {
        self.follow_up_queue.lock().await.push(msg);
    }

    /// 对齐 Pi `clearSteeringQueue()`
    pub async fn clear_steering_queue(&self) {
        self.steering_queue.lock().await.clear();
    }

    /// 对齐 Pi `clearFollowUpQueue()`
    pub async fn clear_follow_up_queue(&self) {
        self.follow_up_queue.lock().await.clear();
    }

    /// 对齐 Pi `clearAllQueues()`
    pub async fn clear_all_queues(&self) {
        self.steering_queue.lock().await.clear();
        self.follow_up_queue.lock().await.clear();
    }

    /// 对齐 Pi `hasQueuedMessages()`
    pub async fn has_queued_messages(&self) -> bool {
        !self.steering_queue.lock().await.is_empty()
            || !self.follow_up_queue.lock().await.is_empty()
    }

    // ── 取消 ──

    /// 取消当前正在执行的 agent loop — 对齐 Pi `abort()`
    pub fn abort(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }

    // ── 等待空闲 ──

    /// 等待当前 agent loop 完成 — 对齐 Pi `waitForIdle()`
    pub async fn wait_for_idle(&self) {
        let is_streaming = self.state.lock().await.is_streaming;
        if is_streaming {
            self.idle_notify.notified().await;
        }
    }

    // ── 重置 ──

    /// 重置全部状态 — 对齐 Pi `reset()`
    pub async fn reset(&mut self) {
        self.abort();
        {
            let is_streaming = self.state.lock().await.is_streaming;
            if is_streaming {
                self.idle_notify.notified().await;
            }
        }
        {
            let mut s = self.state.lock().await;
            s.messages.clear();
            s.is_streaming = false;
            s.stream_message = None;
            s.pending_tool_calls.clear();
            s.error = None;
        }
        self.steering_queue.lock().await.clear();
        self.follow_up_queue.lock().await.clear();
        self.cancel = None;
    }

    // ── 执行入口 ──

    /// 发起一次 prompt 并运行 agent loop — 对齐 Pi `prompt(message)`
    ///
    /// 返回事件接收流和一个 JoinHandle（loop 在后台 task 中运行）。
    pub fn prompt(
        &mut self,
        input: impl Into<AgentMessage>,
    ) -> Result<
        (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
        ),
        AgentError,
    > {
        self.run_loop(Some(vec![input.into()]), RunLoopOptions::default())
    }

    /// 从当前消息历史继续运行 agent loop — 对齐 Pi `continue()`
    pub fn continue_loop(
        &mut self,
    ) -> Result<
        (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
        ),
        AgentError,
    > {
        self.ensure_not_running("Agent is already processing. Wait for completion before continuing.")?;

        let state = self
            .try_state()
            .ok_or_else(|| AgentError::InvalidState("Agent state is temporarily unavailable".to_string()))?;
        if state.messages.is_empty() {
            return Err(AgentError::ContinueError("No messages to continue from".to_string()));
        }

        if matches!(state.messages.last(), Some(AgentMessage::Assistant { .. })) {
            let queued_steering =
                try_dequeue_messages(&self.steering_queue, self.config.steering_mode)?;
            if !queued_steering.is_empty() {
                return self.run_loop(
                    Some(queued_steering),
                    RunLoopOptions {
                        skip_initial_steering_poll: true,
                    },
                );
            }

            let queued_follow_up =
                try_dequeue_messages(&self.follow_up_queue, self.config.follow_up_mode)?;
            if !queued_follow_up.is_empty() {
                return self.run_loop(Some(queued_follow_up), RunLoopOptions::default());
            }

            return Err(AgentError::ContinueError(
                "Cannot continue from message role: assistant".to_string(),
            ));
        }

        self.run_loop(None, RunLoopOptions::default())
    }

    fn run_loop(
        &mut self,
        prompts: Option<Vec<AgentMessage>>,
        options: RunLoopOptions,
    ) -> Result<
        (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
        ),
        AgentError,
    > {
        self.ensure_not_running(
            "Agent is already processing a prompt. Use steer() or follow_up() to queue messages, or wait for completion.",
        )?;

        let cancel = CancellationToken::new();
        self.cancel = Some(cancel.clone());

        // 如果存在持久化事件通道，复用它（支持多订阅者）；否则创建临时通道
        let (event_tx, event_rx) = if let Some(ref ptx) = self.persistent_event_tx {
            let rx = ptx.subscribe();
            (ptx.clone(), rx)
        } else {
            event_stream::event_channel()
        };

        let bridge = self.bridge.clone();
        let steering_queue = self.steering_queue.clone();
        let follow_up_queue = self.follow_up_queue.clone();
        let steering_mode = self.config.steering_mode;
        let follow_up_mode = self.config.follow_up_mode;
        let idle_notify = self.idle_notify.clone();
        let state = self.state.clone();
        let event_sink = build_event_sink(event_tx.clone(), state.clone());
        let skip_initial_steering_poll =
            Arc::new(AtomicBool::new(options.skip_initial_steering_poll));

        // 从 state 中取出构建 context 所需数据
        let (system_prompt, messages, tools) = {
            // 同步获取 — run_loop 在非 async 上下文调用
            let s = self
                .state
                .try_lock()
                .expect("Agent state lock should not be contended at prompt() time");
            (s.system_prompt.clone(), s.messages.clone(), s.tools.clone())
        };

        // 标记为 streaming
        if let Ok(mut s) = self.state.try_lock() {
            s.is_streaming = true;
            s.stream_message = None;
            s.error = None;
        }

        let mut context = AgentContext {
            system_prompt,
            messages,
            tools,
        };

        let config = AgentLoopConfig {
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            max_turns: self.config.max_turns,
            convert_to_llm: self.config.convert_to_llm.clone(),
            transform_context: self.config.transform_context.clone(),
            tool_execution: self.config.tool_execution,
            before_tool_call: self.config.before_tool_call.clone(),
            after_tool_call: self.config.after_tool_call.clone(),
            get_steering_messages: Some(Arc::new(move || {
                if skip_initial_steering_poll.swap(false, Ordering::SeqCst) {
                    return Vec::new();
                }
                dequeue_messages(&steering_queue, steering_mode)
            })),
            get_follow_up_messages: Some(Arc::new(move || {
                dequeue_messages(&follow_up_queue, follow_up_mode)
            })),
        };

        let handle = tokio::spawn(async move {
            let result = match prompts {
                Some(prompts) => {
                    agent_loop::agent_loop(
                        prompts,
                        &mut context,
                        &config,
                        bridge.as_ref(),
                        &event_sink,
                        cancel.clone(),
                    )
                    .await
                }
                None => {
                    agent_loop::agent_loop_continue(
                        &mut context,
                        &config,
                        bridge.as_ref(),
                        &event_sink,
                        cancel.clone(),
                    )
                    .await
                }
            };

            let result = match result {
                Ok(messages) => Ok(messages),
                Err(error) => {
                    let error_message =
                        AgentMessage::error_assistant(error.to_string(), cancel.is_cancelled());
                    event_sink(
                        AgentEvent::MessageStart {
                            message: error_message.clone(),
                        },
                    )
                    .await;
                    event_sink(
                        AgentEvent::MessageEnd {
                            message: error_message.clone(),
                        },
                    )
                    .await;
                    event_sink(
                        AgentEvent::TurnEnd {
                            message: error_message.clone(),
                            tool_results: vec![],
                        },
                    )
                    .await;
                    event_sink(
                        AgentEvent::AgentEnd {
                            messages: vec![error_message.clone()],
                        },
                    )
                    .await;
                    Ok(vec![error_message])
                }
            };

            {
                let mut s = state.lock().await;
                s.is_streaming = false;
                s.stream_message = None;
                s.pending_tool_calls.clear();
            }

            idle_notify.notify_waiters();
            result
        });

        Ok((event_rx, handle))
    }

    fn ensure_not_running(&self, message: &str) -> Result<(), AgentError> {
        match self.state.try_lock() {
            Ok(state) if state.is_streaming => Err(AgentError::InvalidState(message.to_string())),
            Ok(_) => Ok(()),
            Err(_) => Err(AgentError::InvalidState(
                "Agent state is temporarily unavailable".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RunLoopOptions {
    skip_initial_steering_poll: bool,
}

fn build_event_sink(
    sender: event_stream::EventSender<AgentEvent>,
    state: Arc<Mutex<AgentState>>,
) -> AgentEventSink {
    Arc::new(move |event: AgentEvent| {
        let sender = sender.clone();
        let state = state.clone();
        Box::pin(async move {
            process_event(state.as_ref(), &event).await;
            sender.send(event);
        })
    })
}

/// 按模式从队列中出队消息 — 对齐 Pi `dequeueSteeringMessages` / `dequeueFollowUpMessages`
fn dequeue_messages(
    queue: &Arc<Mutex<Vec<AgentMessage>>>,
    mode: QueueMode,
) -> Vec<AgentMessage> {
    queue
        .try_lock()
        .ok()
        .map(|mut q| dequeue_messages_inner(&mut q, mode))
        .unwrap_or_default()
}

fn try_dequeue_messages(
    queue: &Arc<Mutex<Vec<AgentMessage>>>,
    mode: QueueMode,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut q = queue.try_lock().map_err(|_| {
        AgentError::InvalidState("Agent queue is temporarily unavailable".to_string())
    })?;
    Ok(dequeue_messages_inner(&mut q, mode))
}

fn dequeue_messages_inner(queue: &mut Vec<AgentMessage>, mode: QueueMode) -> Vec<AgentMessage> {
    match mode {
        QueueMode::All => queue.drain(..).collect(),
        QueueMode::OneAtATime => {
            if queue.is_empty() {
                vec![]
            } else {
                vec![queue.remove(0)]
            }
        }
    }
}

// ─── 事件驱动状态同步 ──────────────────────────────────────

/// 处理单个事件并同步更新 AgentState — 对齐 Pi `Agent._processLoopEvent` (agent.ts:458-500)
///
/// 此函数可由外部调用者（如 `pi_agent.rs`）在消费事件流时使用，
/// 也可在 Agent 内部的事件转发循环中使用。
pub async fn process_event(state: &Mutex<AgentState>, event: &AgentEvent) {
    let mut s = state.lock().await;
    match event {
        AgentEvent::MessageStart { message } => {
            s.stream_message = Some(message.clone());
        }
        AgentEvent::MessageUpdate { message, .. } => {
            s.stream_message = Some(message.clone());
        }
        AgentEvent::MessageEnd { message } => {
            s.stream_message = None;
            s.messages.push(message.clone());
        }
        AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
            s.pending_tool_calls.insert(tool_call_id.clone());
        }
        AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
            s.pending_tool_calls.remove(tool_call_id);
        }
        AgentEvent::TurnEnd { message, .. } => {
            if let AgentMessage::Assistant { error_message: Some(err), .. } = message {
                s.error = Some(err.clone());
            }
        }
        AgentEvent::AgentEnd { .. } => {
            s.is_streaming = false;
            s.stream_message = None;
        }
        _ => {}
    }
}

// ─── AgentMessage 便利转换 ──────────────────────────────────

impl From<String> for AgentMessage {
    fn from(text: String) -> Self {
        AgentMessage::user(text)
    }
}

impl From<&str> for AgentMessage {
    fn from(text: &str) -> Self {
        AgentMessage::user(text)
    }
}
