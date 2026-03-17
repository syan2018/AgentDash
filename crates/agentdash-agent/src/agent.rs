/// Agent 结构体 — 面向使用者的高层封装
///
/// 严格对齐 Pi `Agent` 类 (agent.ts:116-612)
///
/// 管理 Agent 的完整生命周期：
/// - 状态（system prompt、tools、messages）
/// - 事件订阅（通过 EventReceiver）
/// - Steering / Follow-up 队列（支持 all / one-at-a-time 出队模式）
/// - prompt / continue 入口
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::{
    self, AfterToolCallFn, AgentLoopConfig, BeforeToolCallFn, ConvertToLlmFn, TransformContextFn,
};
use crate::bridge::LlmBridge;
use crate::event_stream::{self, EventReceiver};
use crate::types::{
    AgentContext, AgentError, AgentEvent, AgentMessage, DynAgentTool, ThinkingLevel,
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
/// 每个 Agent 实例持有：
/// - LLM Bridge（通过 trait object 支持多后端）
/// - 工具集合
/// - 消息历史
/// - Steering / Follow-up 队列（带出队模式）
pub struct Agent {
    config: AgentConfig,
    bridge: Arc<dyn LlmBridge>,
    tools: Vec<DynAgentTool>,
    messages: Vec<AgentMessage>,
    steering_queue: Arc<Mutex<Vec<AgentMessage>>>,
    follow_up_queue: Arc<Mutex<Vec<AgentMessage>>>,
    cancel: Option<CancellationToken>,
    /// 对齐 Pi `waitForIdle()` — 等待当前循环完成
    idle_notify: Arc<Notify>,
    is_running: bool,
    /// 持久化事件发送端 — 对齐 Pi `Agent.listeners` 多订阅者模型
    persistent_event_tx: Option<event_stream::EventSender<AgentEvent>>,
}

impl Agent {
    pub fn new(bridge: Arc<dyn LlmBridge>, config: AgentConfig) -> Self {
        Self {
            config,
            bridge,
            tools: Vec::new(),
            messages: Vec::new(),
            steering_queue: Arc::new(Mutex::new(Vec::new())),
            follow_up_queue: Arc::new(Mutex::new(Vec::new())),
            cancel: None,
            idle_notify: Arc::new(Notify::new()),
            is_running: false,
            persistent_event_tx: None,
        }
    }

    // ── Tool 管理 ──

    pub fn add_tool(&mut self, tool: DynAgentTool) {
        self.tools.push(tool);
    }

    pub fn set_tools(&mut self, tools: Vec<DynAgentTool>) {
        self.tools = tools;
    }

    // ── State mutators ──

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.config.system_prompt = prompt.into();
    }

    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    pub fn replace_messages(&mut self, messages: Vec<AgentMessage>) {
        self.messages = messages;
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    pub fn set_thinking_level(&mut self, level: ThinkingLevel) {
        self.config.thinking_level = level;
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
        if self.is_running {
            self.idle_notify.notified().await;
        }
    }

    // ── 重置 ──

    /// 重置全部状态 — 对齐 Pi `reset()`
    pub async fn reset(&mut self) {
        self.abort();
        if self.is_running {
            self.idle_notify.notified().await;
        }
        self.messages.clear();
        self.steering_queue.lock().await.clear();
        self.follow_up_queue.lock().await.clear();
        self.cancel = None;
        self.is_running = false;
    }

    // ── 执行入口 ──

    /// 发起一次 prompt 并运行 agent loop — 对齐 Pi `prompt(message)`
    ///
    /// 返回事件接收流和一个 JoinHandle（loop 在后台 task 中运行）。
    pub fn prompt(
        &mut self,
        input: impl Into<AgentMessage>,
    ) -> (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
    ) {
        self.run_loop(vec![input.into()])
    }

    /// 从当前消息历史继续运行 agent loop — 对齐 Pi `continue()`
    ///
    /// 对齐 Pi 安全检查：消息不能为空，最后一条不能是 assistant。
    pub fn continue_loop(
        &mut self,
    ) -> (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
    ) {
        self.run_loop(vec![])
    }

    fn run_loop(
        &mut self,
        prompts: Vec<AgentMessage>,
    ) -> (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
    ) {
        let cancel = CancellationToken::new();
        self.cancel = Some(cancel.clone());
        self.is_running = true;

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

        let mut context = AgentContext {
            system_prompt: self.config.system_prompt.clone(),
            messages: self.messages.clone(),
            tools: self.tools.clone(),
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
                dequeue_messages(&steering_queue, steering_mode)
            })),
            get_follow_up_messages: Some(Arc::new(move || {
                dequeue_messages(&follow_up_queue, follow_up_mode)
            })),
        };

        let handle = tokio::spawn(async move {
            let result = if prompts.is_empty() {
                agent_loop::agent_loop_continue(
                    &mut context,
                    &config,
                    bridge.as_ref(),
                    &event_tx,
                    cancel,
                )
                .await
            } else {
                agent_loop::agent_loop(
                    prompts,
                    &mut context,
                    &config,
                    bridge.as_ref(),
                    &event_tx,
                    cancel,
                )
                .await
            };
            idle_notify.notify_waiters();
            result
        });

        (event_rx, handle)
    }
}

/// 按模式从队列中出队消息 — 对齐 Pi `dequeueSteeringMessages` / `dequeueFollowUpMessages`
fn dequeue_messages(
    queue: &Arc<Mutex<Vec<AgentMessage>>>,
    mode: QueueMode,
) -> Vec<AgentMessage> {
    queue
        .try_lock()
        .ok()
        .map(|mut q| match mode {
            QueueMode::All => q.drain(..).collect(),
            QueueMode::OneAtATime => {
                if q.is_empty() {
                    vec![]
                } else {
                    vec![q.remove(0)]
                }
            }
        })
        .unwrap_or_default()
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
