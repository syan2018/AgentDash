use std::sync::Arc;

use agent_client_protocol::SessionNotification;
use agentdash_spi::hooks::HookEffect;

/// Session 事件回调 —— 替代 TurnMonitor 的核心抽象。
///
/// 在 `start_prompt` 时由调用方（如 task 执行层）传入，
/// session pipeline 在事件处理中调用：
///
/// - `on_event`：每个 notification 持久化后触发，用于 artifact 跟踪等平台级簿记
/// - `execute_effects`：SessionTerminal hook 评估后触发，
///   将 hook 规则产出的 `HookEffect` 传递给调用方执行领域级副作用
///
/// 与 TurnMonitor 的区别：
/// - 不独立订阅 session 事件流（消除重复消费）
/// - 决策逻辑由 Hook 规则（Workflow）定义，不硬编码在 Rust 中
/// - pipeline 只负责事件传递，不参与领域决策
#[async_trait::async_trait]
pub trait PostTurnHandler: Send + Sync + 'static {
    /// 每个 session notification 持久化后调用。
    /// 用于 artifact 跟踪、session binding 同步等平台级簿记。
    async fn on_event(&self, session_id: &str, notification: &SessionNotification);

    /// 执行 Hook 评估产出的通用副作用。
    ///
    /// `effects` 来自 `HookResolution.effects`，由 Hook 规则（Rhai 脚本）声明，
    /// 实现方按 `effect.kind` 分派到具体的领域逻辑（如 task 状态变更、retry 触发等）。
    async fn execute_effects(&self, session_id: &str, turn_id: &str, effects: &[HookEffect]);

    /// 返回本 handler 能处理的 effect kind 列表。
    /// 用于运行时校验：不在列表中的 effect 会被 pipeline warn 日志记录。
    fn supported_effect_kinds(&self) -> &[&str];
}

pub type DynPostTurnHandler = Arc<dyn PostTurnHandler>;

/// Session 进入终态后的全局回调。
///
/// 与 `PostTurnHandler`（per-session、由调用方传入）不同，
/// `SessionTerminalCallback` 是平台级基础设施，由 `SessionHub` 持有。
/// 典型用途：LifecycleOrchestrator 在 session 终止后评估后继 node 并启动新 session。
#[async_trait::async_trait]
pub trait SessionTerminalCallback: Send + Sync + 'static {
    /// session 完全终止后（hook 评估、effect 执行、running 状态清理之后）调用。
    /// 实现方可安全地创建新 session、修改 lifecycle run 等。
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str);
}

pub type DynSessionTerminalCallback = Arc<dyn SessionTerminalCallback>;
