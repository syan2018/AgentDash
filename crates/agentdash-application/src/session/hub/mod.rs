//! `SessionRuntimeInner` 装配对象与尚待下沉的 session 内部实现。
//!
//! 按能力服务拆分后的剩余范围：
//! - [`facade`]：测试入口与少量 session 内部 helper。
//! - [`factory`]：构造与注入（`new_with_hooks_and_persistence` + `with_*` / `set_*`）。
//! - [`tool_builder`]：runtime tool + 直连/relay MCP 工具发现 + `replace_current_capability_state`。
//! - [`hook_dispatch`]：`emit_session_hook_trigger` / `ensure_hook_session_runtime` /
//!   `collect_runtime_context_update_injections` / `schedule_hook_auto_resume`。
//! - [`runtime_context_transition`]：workflow phase/runtime context transition 的 live
//!   apply、pending 入队与 next-turn 应用。
//!
//! Commit 8 必须继续把 tool / hook / transition / launch / effects 内部实现迁到
//! 具体服务或明确依赖包；本模块最终只保留装配与 ready gate。

use std::{path::PathBuf, sync::Arc};

use super::companion_wait::CompanionWaitRegistry;
use super::construction_provider::SharedSessionConstructionProvider;
use super::persistence::{SessionPersistence, SessionStoreSet};
use super::runtime_registry::SessionRuntimeRegistry;
use super::turn_supervisor::TurnSupervisor;
use crate::context::SharedContextAuditBus;
use agentdash_spi::AgentConnector;
use agentdash_spi::hooks::ExecutionHookProvider;

mod facade;
mod factory;
mod hook_dispatch;
mod runtime_context_transition;
mod tool_builder;

#[cfg(test)]
mod tests;

pub(crate) use hook_dispatch::{HookTriggerDispatchResult, HookTriggerInput};
pub(crate) use runtime_context_transition::{
    LiveRuntimeContextTransitionInput, PendingRuntimeContextApplication,
    PendingRuntimeContextTransitionInput, RuntimeContextTransitionOutcome,
    build_initial_capability_state_frame,
};

#[derive(Clone)]
pub struct SessionRuntimeInner {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    pub(super) runtime_registry: SessionRuntimeRegistry,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) stores: SessionStoreSet,
    pub(super) persistence: Arc<dyn SessionPersistence>,
    pub(crate) vfs_service: Option<Arc<crate::vfs::RelayVfsService>>,
    pub(super) extra_skill_dirs: Vec<PathBuf>,
    pub companion_wait_registry: CompanionWaitRegistry,
    pub(super) title_generator: Option<Arc<dyn super::title_generator::SessionTitleGenerator>>,
    pub(super) terminal_callback:
        Arc<tokio::sync::RwLock<Option<super::post_turn_handler::DynSessionTerminalCallback>>>,
    pub(super) hook_effect_handler_registry: Arc<
        tokio::sync::RwLock<Option<super::post_turn_handler::DynTerminalHookEffectHandlerRegistry>>,
    >,
    /// 将来源 command 构建成与 HTTP 主通道一致的完整 launch request。
    /// Hub 内部的 auto-resume 等场景必须经它补齐 owner/mcp/flow 上下文，
    /// 避免与主通道漂移。用 `Arc<RwLock<...>>` 以便延迟注入（循环依赖场景）。
    pub(super) session_construction_provider:
        Arc<tokio::sync::RwLock<Option<SharedSessionConstructionProvider>>>,
    /// Context Inspector 使用的审计总线。Hub 内部创建 runtime delegate 时需要把它
    /// 传给 hook 链路，记录每轮 HookInjection → ContextFragment 的动态片段。
    pub(super) context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    /// Layer 0 base system prompt（由 factory 从 settings / 常量注入）。
    pub(super) base_system_prompt: String,
    /// Layer 2 用户偏好提示列表（由 factory 从 settings 注入）。
    pub(super) user_preferences: Vec<String>,
    /// 运行时工具构建 provider（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) runtime_tool_provider:
        Option<Arc<dyn agentdash_spi::connector::RuntimeToolProvider>>,
    /// MCP Relay 工具发现 provider（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) mcp_relay_provider: Option<Arc<dyn agentdash_spi::McpRelayProvider>>,
}
