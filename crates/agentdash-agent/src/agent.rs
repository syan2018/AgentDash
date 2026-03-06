/// Agent 结构体 — 面向使用者的高层封装
///
/// 管理 Agent 的完整生命周期：
/// - 状态（system prompt、model、tools、messages）
/// - 事件订阅
/// - Steering / Follow-up 队列
/// - prompt / continue 入口
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::agent_loop::{self, AgentLoopConfig};
use crate::bridge::LlmBridge;
use crate::event_stream::{self, EventReceiver};
use crate::types::{AgentContext, AgentError, AgentEvent, AgentMessage, DynAgentTool};

/// Agent 配置
pub struct AgentConfig {
    pub system_prompt: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub max_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            temperature: None,
            max_tokens: Some(8192),
            max_turns: 25,
        }
    }
}

/// Agent — Pi 风格的 Agent Runtime
///
/// 每个 Agent 实例持有：
/// - LLM Bridge（通过 trait object 支持多后端）
/// - 工具集合
/// - 消息历史
/// - Steering / Follow-up 队列
pub struct Agent {
    config: AgentConfig,
    bridge: Arc<dyn LlmBridge>,
    tools: Vec<DynAgentTool>,
    messages: Vec<AgentMessage>,
    steering_queue: Arc<Mutex<Vec<AgentMessage>>>,
    follow_up_queue: Arc<Mutex<Vec<AgentMessage>>>,
    cancel: Option<CancellationToken>,
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
        }
    }

    pub fn add_tool(&mut self, tool: DynAgentTool) {
        self.tools.push(tool);
    }

    pub fn set_tools(&mut self, tools: Vec<DynAgentTool>) {
        self.tools = tools;
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.config.system_prompt = prompt.into();
    }

    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    pub fn replace_messages(&mut self, messages: Vec<AgentMessage>) {
        self.messages = messages;
    }

    /// 向 steering 队列推入消息（在工具执行间隙被轮询，触发中断）
    pub async fn steer(&self, msg: AgentMessage) {
        self.steering_queue.lock().await.push(msg);
    }

    /// 向 follow-up 队列推入消息（在 agent loop 即将结束时被轮询，触发继续）
    pub async fn follow_up(&self, msg: AgentMessage) {
        self.follow_up_queue.lock().await.push(msg);
    }

    pub async fn clear_queues(&self) {
        self.steering_queue.lock().await.clear();
        self.follow_up_queue.lock().await.clear();
    }

    /// 取消当前正在执行的 agent loop
    pub fn abort(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }

    /// 发起一次 prompt 并运行 agent loop
    ///
    /// 返回事件接收流和一个 JoinHandle（loop 在后台 task 中运行）。
    /// 调用方通过 EventReceiver 消费事件，通过 JoinHandle 等待完成。
    pub fn prompt(
        &mut self,
        input: impl Into<AgentMessage>,
    ) -> (
        EventReceiver<AgentEvent>,
        tokio::task::JoinHandle<Result<Vec<AgentMessage>, AgentError>>,
    ) {
        self.run_loop(vec![input.into()])
    }

    /// 从当前消息历史继续运行 agent loop（不添加新 prompt）
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

        let (event_tx, event_rx) = event_stream::event_channel();

        let bridge = self.bridge.clone();
        let steering_queue = self.steering_queue.clone();
        let follow_up_queue = self.follow_up_queue.clone();

        let mut context = AgentContext {
            system_prompt: self.config.system_prompt.clone(),
            messages: self.messages.clone(),
            tools: self.tools.clone(),
        };

        let config = AgentLoopConfig {
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            max_turns: self.config.max_turns,
            get_steering_messages: Some(Box::new(move || {
                steering_queue
                    .try_lock()
                    .ok()
                    .map(|mut q| q.drain(..).collect())
                    .unwrap_or_default()
            })),
            get_follow_up_messages: Some(Box::new(move || {
                follow_up_queue
                    .try_lock()
                    .ok()
                    .map(|mut q| q.drain(..).collect())
                    .unwrap_or_default()
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

            result
        });

        (event_rx, handle)
    }
}

/// Agent 快速构建 prompt 入口的便利 trait
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
